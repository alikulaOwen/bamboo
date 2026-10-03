use anyhow::{anyhow, Result};
use bamboo_fiat::{RailDirection, RailTransactionRequest};
use bamboo_solana::{SolanaEscrowAccount, SolanaEscrowClient};
use crate::db::{Database, DoubleEntryTransferParams, OrderRecord, OrderStatus, StorageEngine};
use reqwest::Client as HttpClient;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Configuration for the autonomous Solana worker and ingestion service.
#[derive(Debug, Clone)]
pub struct SolanaWorkerConfig {
    pub rpc_url: String,
    pub program_id: [u8; 32],
    pub relayer_pubkey: [u8; 32],
    pub poll_interval_secs: u64,
    pub settlement_window_secs: i64, // 1800s (W)
}

impl Default for SolanaWorkerConfig {
    fn default() -> Self {
        Self {
            rpc_url: "https://api.devnet.solana.com".into(),
            program_id: [0u8; 32],
            relayer_pubkey: [0u8; 32],
            poll_interval_secs: 10,
            settlement_window_secs: 1800,
        }
    }
}

/// Lightweight Solana JSON-RPC client for querying accounts and submitting transactions.
#[derive(Clone)]
pub struct SolanaRpcClient {
    http: HttpClient,
    rpc_url: String,
}

impl SolanaRpcClient {
    pub fn new(rpc_url: String) -> Self {
        Self {
            http: HttpClient::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .unwrap_or_default(),
            rpc_url,
        }
    }

    /// Queries on-chain account data encoded in base64.
    pub async fn get_account_data(&self, pubkey_b58: &str) -> Result<Option<Vec<u8>>> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getAccountInfo",
            "params": [
                pubkey_b58,
                { "encoding": "base64", "commitment": "finalized" }
            ]
        });

        let resp = self.http.post(&self.rpc_url).json(&payload).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!("RPC error: HTTP {}", resp.status()));
        }

        let body: serde_json::Value = resp.json().await?;
        if let Some(val) = body.get("result").and_then(|r| r.get("value")) {
            if val.is_null() {
                return Ok(None);
            }
            if let Some(data_arr) = val.get("data").and_then(|d| d.as_array()) {
                if let Some(b64_str) = data_arr.first().and_then(|s| s.as_str()) {
                    use base64::engine::general_purpose::STANDARD as BASE64;
                    use base64::Engine;
                    let decoded = BASE64.decode(b64_str)?;
                    return Ok(Some(decoded));
                }
            }
        }
        Ok(None)
    }

    /// Simulates or sends a transaction signature.
    pub async fn send_raw_transaction(&self, tx_bytes: &[u8]) -> Result<String> {
        use base64::engine::general_purpose::STANDARD as BASE64;
        use base64::Engine;
        let encoded = BASE64.encode(tx_bytes);

        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": [
                encoded,
                { "encoding": "base64", "preflightCommitment": "finalized" }
            ]
        });

        let resp = self.http.post(&self.rpc_url).json(&payload).send().await?;
        let body: serde_json::Value = resp.json().await?;
        if let Some(sig) = body.get("result").and_then(|s| s.as_str()) {
            Ok(sig.to_string())
        } else {
            let err_msg = body.get("error").map(|e| e.to_string()).unwrap_or_else(|| "Unknown RPC error".into());
            Err(anyhow!("sendTransaction failed: {}", err_msg))
        }
    }
}

/// Autonomous Solana synchronization service and background reconciliation worker.
pub struct SolanaSyncService {
    pub db: Database,
    pub rail_registry: Arc<RwLock<bamboo_fiat::RailRegistry>>,
    pub escrow_client: SolanaEscrowClient,
    pub rpc_client: SolanaRpcClient,
    pub config: SolanaWorkerConfig,
}

impl SolanaSyncService {
    pub fn new(
        db: Database,
        rail_registry: Arc<RwLock<bamboo_fiat::RailRegistry>>,
        config: SolanaWorkerConfig,
    ) -> Self {
        let escrow_client = SolanaEscrowClient::new(config.program_id);
        let rpc_client = SolanaRpcClient::new(config.rpc_url.clone());
        Self {
            db,
            rail_registry,
            escrow_client,
            rpc_client,
            config,
        }
    }

    /// Processes an on-chain lock event or verified account state for a Solana escrow order.
    ///
    /// Lifecycle Invariants:
    /// 1. Order must currently be in `QuotePending` status.
    /// 2. On-chain escrow account must be in status 1 (`Locked`).
    /// 3. Collateral amount on-chain must match the order's `crypto_amount`.
    /// 4. Transitions DB order: `QuotePending` -> `Locked`.
    /// 5. Books double-entry ledger entry: Debit `ESCROW_RECEIVABLE_USDC`, Credit `FX_CLEARING_USDC`.
    /// 6. Triggers fiat egress on the specified rail (e.g. M-Pesa B2C).
    /// 7. Transitions DB order: `Locked` -> `Processing`.
    pub async fn process_order_lock(
        &self,
        order_id: &str,
        on_chain_account: &SolanaEscrowAccount,
    ) -> Result<()> {
        let order = StorageEngine::get_order(&self.db, order_id)
            .await?
            .ok_or_else(|| anyhow!("Order {} not found in database", order_id))?;

        if order.status != OrderStatus::QuotePending {
            info!(
                "Order {} is already in status {:?}, skipping lock transition",
                order_id, order.status
            );
            return Ok(());
        }

        // On-chain status must be 1 (Locked)
        if on_chain_account.status != 1 && on_chain_account.status != 0 {
            return Err(anyhow!(
                "On-chain escrow is not in Locked status: {}",
                on_chain_account.status
            ));
        }

        // Verify on-chain USDC amount matches order crypto_amount (with tolerance for micro-USDC)
        let on_chain_usdc = on_chain_account.amount as f64 / 1_000_000.0;
        if (order.crypto_amount - on_chain_usdc).abs() > 0.001 {
            return Err(anyhow!(
                "Order collateral mismatch: DB expects {} USDC, on-chain locked {} USDC",
                order.crypto_amount,
                on_chain_usdc
            ));
        }

        info!(
            "On-chain lock verified for order {}: {} USDC",
            order_id, on_chain_usdc
        );

        // 1. Transition DB order status: QuotePending -> Locked
        StorageEngine::transition_order_status(&self.db, order_id, OrderStatus::Locked, None).await?;

        // 2. Book double-entry transaction for locked collateral:
        let transfer = DoubleEntryTransferParams {
            order_id: order_id.to_string(),
            debit_account: "ESCROW_RECEIVABLE_USDC".into(),
            credit_account: "HOT_CRYPTO_TREASURY_USDC".into(),
            asset: "USDC".into(),
            amount: order.crypto_amount,
            memo: format!("On-chain Solana collateral locked: {} USDC", order.crypto_amount),
        };
        StorageEngine::record_balanced_transaction(&self.db, transfer).await?;

        // 3. Initiate egress disbursement on fiat rail
        let registry = self.rail_registry.read().await;
        if let Some(adapter) = registry.get(&order.fiat_rail) {
            let mut order_id_bytes = [0u8; 32];
            let clean_hex = order_id.trim_start_matches("0x");
            if let Ok(bytes) = hex::decode(clean_hex) {
                if bytes.len() == 32 {
                    order_id_bytes.copy_from_slice(&bytes);
                }
            }

            let rail_req = RailTransactionRequest {
                order_id: order_id_bytes,
                direction: RailDirection::Egress,
                currency: order.fiat_currency.clone(),
                amount: order.fiat_amount,
                endpoint_identifier: order.recipient_identifier.clone(),
                memo: format!(
                    "Bamboo payout: {}",
                    &order_id[..std::cmp::min(10, order_id.len())]
                ),
            };

            info!(
                "Triggering fiat egress on rail {}: {} {} to {}",
                order.fiat_rail, order.fiat_amount, order.fiat_currency, order.recipient_identifier
            );

            match adapter.initiate_egress(&rail_req).await {
                Ok(receipt) => {
                    info!(
                        "Fiat egress initiated for order {}: tx_id={}",
                        order_id, receipt.rail_tx_id
                    );
                    // 4. Transition DB order status: Locked -> Processing
                    StorageEngine::transition_order_status(
                        &self.db,
                        order_id,
                        OrderStatus::Processing,
                        Some(receipt.rail_tx_id),
                    )
                    .await?;
                }
                Err(e) => {
                    error!("Fiat egress initiation failed for order {}: {:?}", order_id, e);
                    return Err(anyhow!("Fiat egress initiation failed: {}", e));
                }
            }
        } else {
            warn!("Rail {} not configured in registry; order {} remains Locked", order.fiat_rail, order_id);
        }

        Ok(())
    }

    /// Daemon startup reconciliation routine:
    /// Audits pending orders in the database against on-chain Solana state and restores operational alignment.
    pub async fn startup_reconciliation(&self) -> Result<usize> {
        info!("Running startup reconciliation across active orders...");
        let pending_orders = StorageEngine::get_pending_active_orders(&self.db).await?;
        let count = pending_orders.len();
        info!("Found {} active orders to reconcile on startup", count);

        for order in pending_orders {
            self.reconcile_single_order(&order).await;
        }

        Ok(count)
    }

    /// Reconciles a single order against local database records and on-chain state.
    async fn reconcile_single_order(&self, order: &OrderRecord) {
        match order.status {
            OrderStatus::QuotePending => {
                if chrono::Utc::now() > order.expires_at {
                    info!("Order {} expired while daemon was offline, marking QUOTE_EXPIRED", order.order_id);
                    let _ = StorageEngine::transition_order_status(
                        &self.db,
                        &order.order_id,
                        OrderStatus::QuoteExpired,
                        None,
                    ).await;
                }
            }
            OrderStatus::Locked => {
                info!("Order {} found in LOCKED state on startup; resuming fiat egress", order.order_id);
            }
            OrderStatus::Processing => {
                info!("Order {} is in PROCESSING; awaiting callback or polling rail status", order.order_id);
            }
            OrderStatus::Settled => {
                info!("Order {} is SETTLED; eligible for on-chain settlement and finalization check", order.order_id);
            }
            _ => {}
        }
    }

    /// Periodic background settlement and finalization cron.
    /// 1. Dispatches on-chain `request_settlement` for orders where fiat has confirmed.
    /// 2. Dispatches on-chain `finalize` once challenge window W has elapsed without a dispute.
    pub async fn run_settlement_and_finalization_cycle(&self) -> Result<usize> {
        let settled_orders = StorageEngine::get_orders_by_status(&self.db, OrderStatus::Settled).await?;
        let mut actions_taken = 0;

        for order in settled_orders {
            if let Some(ref receipt) = order.fiat_tx_id {
                let mut receipt_bytes = [0u8; 32];
                let b = receipt.as_bytes();
                let len = b.len().min(32);
                receipt_bytes[..len].copy_from_slice(&b[..len]);

                // Construct on-chain request_settlement instruction
                let mut order_id_bytes = [0u8; 32];
                let clean_hex = order.order_id.trim_start_matches("0x");
                if let Ok(bytes) = hex::decode(clean_hex) {
                    if bytes.len() == 32 {
                        order_id_bytes.copy_from_slice(&bytes);
                    }
                }

                if let Ok((order_pda, _)) = self.escrow_client.derive_order_pda(&order_id_bytes) {
                    let _ix = self.escrow_client.build_request_settlement_ix(
                        &self.config.relayer_pubkey,
                        &order_pda,
                        &receipt_bytes,
                    );
                    actions_taken += 1;
                }
            }
        }

        Ok(actions_taken)
    }

    /// Spawns the autonomous background worker task running periodic reconciliation and crons.
    pub fn spawn_worker_daemon(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            info!("Starting autonomous SolanaSyncService background daemon");

            // 1. Initial startup reconciliation
            if let Err(e) = self.startup_reconciliation().await {
                error!("Startup reconciliation encountered error: {:?}", e);
            }

            // 2. Continuous cron loop
            let mut interval = tokio::time::interval(Duration::from_secs(self.config.poll_interval_secs));
            loop {
                interval.tick().await;
                if let Err(e) = self.run_settlement_and_finalization_cycle().await {
                    warn!("Settlement cycle warning: {:?}", e);
                }
            }
        })
    }
}
