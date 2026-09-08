mod db;
mod rfq;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use bamboo_core::MAX_HOST_FEE_BPS;
use db::LedgerDb;
use rfq::{RfqEngine, RfqQuoteRequest, RfqQuoteResponse, RFQ_COUNTDOWN_SECONDS};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

#[derive(Clone)]
pub struct AppState {
    pub db: LedgerDb,
    pub rfq: RfqEngine,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    max_host_fee_bps: u16,
    rfq_countdown_secs: u64,
}

#[derive(Deserialize)]
struct SettleOrderRequest {
    order_id: String,
    payout_amount: u64,
    host_fee: u64,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        max_host_fee_bps: MAX_HOST_FEE_BPS,
        rfq_countdown_secs: RFQ_COUNTDOWN_SECONDS,
    })
}

async fn create_quote(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RfqQuoteRequest>,
) -> Result<Json<RfqQuoteResponse>, StatusCode> {
    let quote = state.rfq.generate_quote(&req);
    Ok(Json(quote))
}

async fn settle_order(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SettleOrderRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut order_id_bytes = [0u8; 32];
    hex::decode_to_slice(&payload.order_id, &mut order_id_bytes)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    state
        .db
        .record_settlement_double_entry(&order_id_bytes, payload.payout_amount, payload.host_fee)
        .map_err(|e| {
            error!("Failed to record settlement: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(serde_json::json!({
        "status": "settled",
        "order_id": payload.order_id,
        "payout": payload.payout_amount,
        "fee": payload.host_fee
    })))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: Arc<AppState>) {
    info!("WebSocket client connected to RFQ live query channel");
    let mut interval = tokio::time::interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let heartbeat = serde_json::json!({
                    "type": "heartbeat",
                    "timestamp": chrono::Utc::now().timestamp(),
                    "rfq_countdown_seconds": RFQ_COUNTDOWN_SECONDS,
                });
                if socket.send(Message::Text(heartbeat.to_string())).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(req) = serde_json::from_str::<RfqQuoteRequest>(&text) {
                            let quote = state.rfq.generate_quote(&req);
                            let resp = serde_json::json!({
                                "type": "quote",
                                "data": quote
                            });
                            let _ = socket.send(Message::Text(resp.to_string())).await;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/rfq/quote", post(create_quote))
        .route("/rfq/ws", get(ws_handler))
        .route("/orders/settle", post(settle_order))
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    info!("Starting Project Bamboo Unified Host Daemon (bamboo-relayer)");
    info!("Protocol Fee Ceiling: {} bps (0.25%)", MAX_HOST_FEE_BPS);

    let db = LedgerDb::new_in_memory()?;
    let rfq = RfqEngine::new();
    let state = Arc::new(AppState { db, rfq });

    let app = create_router(state);
    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    info!("Bamboo relayer daemon listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_health_endpoint() {
        let db = LedgerDb::new_in_memory().unwrap();
        let rfq = RfqEngine::new();
        let state = Arc::new(AppState { db, rfq });
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
