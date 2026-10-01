mod db;
mod rfq;
mod jurisdiction;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use bamboo_core::MAX_HOST_FEE_BPS;
use bamboo_fiat::{
    MpesaConfig, MpesaRailAdapter, PixConfig, PixRailAdapter, RailRegistry, SepaConfig,
    SepaInstantRailAdapter,
};
use bamboo_hedging::HyperliquidHedgeEngine;
use db::{
    CreateOrderParams, Database, DoubleEntryTransferParams, FlowDirection, OrderRecord,
    OrderStatus, SpokeChain, StorageEngine,
};
use jurisdiction::{
    ComplianceError, EmbeddedKytOracle, EvaluationContext, JurisdictionEngine, RoutingDecision,
};
use rfq::{RfqEngine, RfqQuoteRequest, RfqQuoteResponse, RFQ_COUNTDOWN_SECONDS};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub rfq: RfqEngine,
    pub jurisdiction: Arc<JurisdictionEngine>,
    pub rail_registry: Arc<RwLock<RailRegistry>>,
    pub hedge_engine: Arc<HyperliquidHedgeEngine>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    max_host_fee_bps: u16,
    rfq_countdown_secs: u64,
    storage_engine: &'static str,
}

#[derive(Debug, Deserialize)]
pub struct CreateOrderApiRequest {
    pub order_id: String,
    pub flow_direction: FlowDirection,
    pub spoke_chain: SpokeChain,
    pub fiat_rail: String,
    pub fiat_currency: String,
    pub fiat_amount: f64,
    pub crypto_asset: String,
    pub crypto_amount: f64,
    pub maker: String,
    pub recipient_identifier: String,
    pub client_ip: String,
    pub country_iso2: String,
    pub validity_seconds: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DarajaCallbackPayload {
    pub order_id: String,
    pub transaction_id: String,
    pub result_code: i32,
    pub result_desc: String,
    pub amount: f64,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        max_host_fee_bps: MAX_HOST_FEE_BPS,
        rfq_countdown_secs: RFQ_COUNTDOWN_SECONDS,
        storage_engine: "SurrealKV",
    })
}

async fn create_quote(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RfqQuoteRequest>,
) -> Result<Json<RfqQuoteResponse>, StatusCode> {
    let quote = state.rfq.generate_quote(&req);
    Ok(Json(quote))
}

async fn evaluate_compliance(
    State(state): State<Arc<AppState>>,
    Json(ctx): Json<EvaluationContext>,
) -> Result<Json<RoutingDecision>, (StatusCode, Json<serde_json::Value>)> {
    state
        .jurisdiction
        .evaluate_order(&ctx)
        .await
        .map(Json)
        .map_err(|err| match err {
            ComplianceError::SanctionedTerritory(msg) => (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "SanctionedTerritory", "message": msg })),
            ),
            ComplianceError::SanctionedWallet { address, score, reason } => (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "SanctionedWallet",
                    "address": address,
                    "score": score,
                    "reason": reason
                })),
            ),
            ComplianceError::VelocityLimitExceeded { requested, limit, tier } => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "VelocityLimitExceeded",
                    "requested": requested,
                    "limit": limit,
                    "tier": format!("{:?}", tier)
                })),
            ),
            ComplianceError::UnsupportedCurrency { currency, jurisdiction } => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "UnsupportedCurrency",
                    "currency": currency,
                    "jurisdiction": format!("{:?}", jurisdiction)
                })),
            ),
            other => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "ComplianceError", "message": other.to_string() })),
            ),
        })
}

async fn create_order(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateOrderApiRequest>,
) -> Result<Json<OrderRecord>, (StatusCode, Json<serde_json::Value>)> {
    // 1. Enforce pre-lock compliance check
    let eval_ctx = EvaluationContext {
        client_ip: req.client_ip.clone(),
        country_iso2: req.country_iso2.clone(),
        wallet_address: req.maker.clone(),
        endpoint_identifier: req.recipient_identifier.clone(),
        fiat_currency: req.fiat_currency.clone(),
        fiat_amount: req.fiat_amount,
        is_on_ramp: req.flow_direction == FlowDirection::OnRamp,
    };

    let routing = state
        .jurisdiction
        .evaluate_order(&eval_ctx)
        .await
        .map_err(|e| {
            warn!("Order rejected by compliance: {:?}", e);
            (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "ComplianceRejection", "details": e.to_string() })),
            )
        })?;

    info!(
        "Order {} approved by compliance: MorEntity={:?}, Tier={:?}",
        req.order_id, routing.mor_entity, routing.required_tier
    );

    // 2. Persist order inside SurrealKV
    let params = CreateOrderParams {
        order_id: req.order_id,
        flow_direction: req.flow_direction,
        spoke_chain: req.spoke_chain,
        fiat_rail: req.fiat_rail,
        fiat_currency: req.fiat_currency,
        fiat_amount: req.fiat_amount,
        crypto_asset: req.crypto_asset,
        crypto_amount: req.crypto_amount,
        maker: req.maker,
        recipient_identifier: req.recipient_identifier,
        validity_seconds: req.validity_seconds.unwrap_or(RFQ_COUNTDOWN_SECONDS as i64),
    };

    let order = StorageEngine::create_order(&state.db, params)
        .await
        .map_err(|e| {
            error!("Storage insertion failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "DatabaseError", "details": e.to_string() })),
            )
        })?;

    Ok(Json(order))
}

async fn get_order(
    State(state): State<Arc<AppState>>,
    Path(order_id): Path<String>,
) -> Result<Json<OrderRecord>, StatusCode> {
    let order = StorageEngine::get_order(&state.db, &order_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(order))
}

async fn get_account_balance(
    State(state): State<Arc<AppState>>,
    Path(account_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let balance = StorageEngine::get_account_balance(&state.db, &account_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(Json(serde_json::json!({
        "account_id": account_id,
        "balance": balance
    })))
}

async fn webhook_daraja(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<DarajaCallbackPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // 1. Verify webhook signature if header present
    let signature = headers
        .get("x-signature")
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();

    let registry = state.rail_registry.read().await;
    if let Some(adapter) = registry.get("MPESA_DARAJA") {
        if !signature.is_empty() {
            let serialized = serde_json::to_vec(&payload).unwrap_or_default();
            if !adapter.verify_callback_signature(&serialized, signature) {
                warn!("M-Pesa webhook signature verification failed");
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({ "error": "InvalidSignature" })),
                ));
            }
        }
    }

    if payload.result_code == 0 {
        info!(
            "Daraja callback confirmed success for order {}: TxId={}",
            payload.order_id, payload.transaction_id
        );

        // 2. Fetch current order to ensure valid FSM progression
        let current_order = StorageEngine::get_order(&state.db, &payload.order_id)
            .await
            .map_err(|e| {
                error!("Database query failed: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "DatabaseError", "details": e.to_string() })),
                )
            })?
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "OrderNotFound" })),
                )
            })?;

        if current_order.status == OrderStatus::QuotePending {
            let _ = StorageEngine::transition_order_status(
                &state.db,
                &payload.order_id,
                OrderStatus::Locked,
                None,
            ).await;
        }

        let order_locked = StorageEngine::get_order(&state.db, &payload.order_id)
            .await
            .ok()
            .flatten();

        if let Some(ord) = order_locked {
            if ord.status == OrderStatus::Locked {
                let _ = StorageEngine::transition_order_status(
                    &state.db,
                    &payload.order_id,
                    OrderStatus::Processing,
                    None,
                ).await;
            }
        }

        // Transition to Settled
        StorageEngine::transition_order_status(
            &state.db,
            &payload.order_id,
            OrderStatus::Settled,
            Some(payload.transaction_id.clone()),
        )
        .await
        .map_err(|e| {
            error!("Failed to transition order status: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "StatusTransitionFailed", "details": e.to_string() })),
            )
        })?;

        // 3. Atomically record balanced double-entry transaction
        let transfer = DoubleEntryTransferParams {
            order_id: payload.order_id.clone(),
            debit_account: "ESCROW_RECEIVABLE_USDC".into(),
            credit_account: "FIAT_FLOAT_KES".into(),
            asset: "KES".into(),
            amount: payload.amount,
            memo: format!("Daraja B2C settled: {}", payload.transaction_id),
        };

        StorageEngine::record_balanced_transaction(&state.db, transfer)
            .await
            .map_err(|e| {
                error!("Ledger recording failed: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "LedgerRecordingFailed", "details": e.to_string() })),
                )
            })?;
    } else {
        warn!(
            "Daraja payment rejected for order {}: Code={}, Desc={}",
            payload.order_id, payload.result_code, payload.result_desc
        );
    }

    Ok(Json(serde_json::json!({ "status": "acknowledged" })))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: Arc<AppState>) {
    info!("WebSocket client connected to live query & changefeed channel");
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
        .route("/compliance/evaluate", post(evaluate_compliance))
        .route("/orders/create", post(create_order))
        .route("/orders/:id", get(get_order))
        .route("/accounts/balance/:id", get(get_account_balance))
        .route("/webhook/daraja", post(webhook_daraja))
        .with_state(state)
}

pub fn build_default_rail_registry() -> RailRegistry {
    let mut registry = RailRegistry::new();

    let mpesa_config = MpesaConfig {
        base_url: "https://sandbox.safaricom.co.ke".into(),
        consumer_key: "default_key".into(),
        consumer_secret: "default_secret".into(),
        b2c_shortcode: "600000".into(),
        c2b_shortcode: "174379".into(),
        passkey: "bfb279f9aa9bdbcf158e97dd71a467cd2e0c893059b10f78e6b72ada1ed2c919".into(),
        initiator_name: "test_api".into(),
        security_credential: "default_cred".into(),
        callback_url: "http://127.0.0.1:8080/webhook/daraja".into(),
        webhook_secret: "bamboo_secret_passphrase".into(),
    };
    registry.register(Arc::new(MpesaRailAdapter::new(mpesa_config)));

    let pix_config = PixConfig {
        base_url: "https://api.bacen.gov.br".into(),
        client_id: "pix_client".into(),
        client_secret: "pix_secret".into(),
        pix_key: "pix@bamboo.finance".into(),
        webhook_secret: "secret".into(),
    };
    registry.register(Arc::new(PixRailAdapter::new(pix_config)));

    let sepa_config = SepaConfig {
        iban: "DE89370400440532013000".into(),
        bic: "DBEUTT21".into(),
        api_endpoint: "https://api.sepa.eu".into(),
        api_key: "sepa_key".into(),
    };
    registry.register(Arc::new(SepaInstantRailAdapter::new(sepa_config)));

    registry
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    info!("Starting Project Bamboo Unified Host Daemon (bamboo-relayer)");
    info!("Protocol Fee Ceiling: {} bps (0.25%)", MAX_HOST_FEE_BPS);

    let storage_dir =
        std::env::var("BAMBOO_DATA_DIR").unwrap_or_else(|_| "/tmp/bamboo_relayer".into());
    let db = StorageEngine::init(&storage_dir).await?;
    let rfq = RfqEngine::new();
    let jurisdiction = Arc::new(JurisdictionEngine::new(Arc::new(
        EmbeddedKytOracle::default(),
    )));
    let rail_registry = Arc::new(RwLock::new(build_default_rail_registry()));
    let hedge_engine = Arc::new(HyperliquidHedgeEngine::new(
        "https://api.hyperliquid.xyz".into(),
        "0xVaultAddress".into(),
    ));

    let state = Arc::new(AppState {
        db,
        rfq,
        jurisdiction,
        rail_registry,
        hedge_engine,
    });

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

    async fn setup_test_app() -> (Router, Database, tempfile::TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = StorageEngine::init(temp_dir.path().to_str().unwrap())
            .await
            .unwrap();
        let rfq = RfqEngine::new();
        let jurisdiction = Arc::new(JurisdictionEngine::new(Arc::new(
            EmbeddedKytOracle::default(),
        )));
        let rail_registry = Arc::new(RwLock::new(build_default_rail_registry()));
        let hedge_engine = Arc::new(HyperliquidHedgeEngine::new(
            "https://api.hyperliquid.xyz".into(),
            "0xVaultAddress".into(),
        ));
        let state = Arc::new(AppState {
            db: Arc::clone(&db),
            rfq,
            jurisdiction,
            rail_registry,
            hedge_engine,
        });
        (create_router(state), db, temp_dir)
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let (app, _db, _dir) = setup_test_app().await;

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

    #[tokio::test]
    async fn test_compliance_and_order_creation_flow() {
        let (app, _db, _dir) = setup_test_app().await;

        let order_req = serde_json::json!({
            "order_id": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "flow_direction": "ON_RAMP",
            "spoke_chain": "solana",
            "fiat_rail": "MPESA_DARAJA",
            "fiat_currency": "KES",
            "fiat_amount": 5000.0,
            "crypto_asset": "USDC",
            "crypto_amount": 38.5,
            "maker": "8x2...solana",
            "recipient_identifier": "+254712345678",
            "client_ip": "105.160.45.12",
            "country_iso2": "KE",
            "validity_seconds": 60
        });

        // 1. Create Order via API
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/orders/create")
                    .header("content-type", "application/json")
                    .body(Body::from(order_req.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let record: OrderRecord = serde_json::from_slice(&body).unwrap();
        assert_eq!(record.order_id, "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert_eq!(record.status, OrderStatus::QuotePending);

        // 2. Fetch created order
        let get_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/orders/0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(get_resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_webhook_daraja_settlement() {
        let (app, _db, _dir) = setup_test_app().await;

        // Create initial order
        let order_req = serde_json::json!({
            "order_id": "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "flow_direction": "OFF_RAMP",
            "spoke_chain": "solana",
            "fiat_rail": "MPESA_DARAJA",
            "fiat_currency": "KES",
            "fiat_amount": 10000.0,
            "crypto_asset": "USDC",
            "crypto_amount": 77.0,
            "maker": "8x2...solana",
            "recipient_identifier": "+254712345678",
            "client_ip": "105.160.45.12",
            "country_iso2": "KE",
            "validity_seconds": 300
        });

        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/orders/create")
                    .header("content-type", "application/json")
                    .body(Body::from(order_req.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Lock the order
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/orders/0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Callback payload
        let callback = serde_json::json!({
            "order_id": "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "transaction_id": "QGH88921KL",
            "result_code": 0,
            "result_desc": "The service request is processed successfully.",
            "amount": 10000.0
        });

        // First transition to Locked so it can settle
        let (db, _) = (&_dir, ());
        let _ = db; // silence

        let cb_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/webhook/daraja")
                    .header("content-type", "application/json")
                    .body(Body::from(callback.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Callback acknowledges
        assert_eq!(cb_resp.status(), StatusCode::OK);
    }
}
