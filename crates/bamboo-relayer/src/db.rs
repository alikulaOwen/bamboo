#![allow(dead_code)]

use chrono::{DateTime, Utc};
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use surrealdb::engine::local::{Db, SurrealKv};
use surrealdb::Surreal;
use thiserror::Error;
use tracing::{info, instrument};

pub type Database = Arc<Surreal<Db>>;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("SurrealDB error: {0}")]
    Surreal(#[from] surrealdb::Error),

    #[error("Order not found: {0}")]
    OrderNotFound(String),

    #[error("Account not found: {0}")]
    AccountNotFound(String),

    #[error("Invalid status transition from {current:?} to {next:?}")]
    InvalidStatusTransition {
        current: OrderStatus,
        next: OrderStatus,
    },

    #[error("Unbalanced double-entry transaction: debit ({debit}) != credit ({credit})")]
    UnbalancedTransaction { debit: f64, credit: f64 },

    #[error("Storage initialization error: {0}")]
    Initialization(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlowDirection {
    OnRamp,  // Fiat Ingress -> Crypto Egress
    OffRamp, // Crypto Ingress -> Fiat Egress
}

impl std::fmt::Display for FlowDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OnRamp => write!(f, "ON_RAMP"),
            Self::OffRamp => write!(f, "OFF_RAMP"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpokeChain {
    Solana,
    Base,
    Bsc,
}

impl std::fmt::Display for SpokeChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Solana => write!(f, "solana"),
            Self::Base => write!(f, "base"),
            Self::Bsc => write!(f, "bsc"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderStatus {
    QuotePending,
    QuoteExpired,
    Locked,
    Processing,
    Settled,
    Disputed,
    Refunded,
}

impl OrderStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::QuoteExpired | Self::Settled | Self::Refunded)
    }

    pub fn can_transition_to(&self, next: Self) -> bool {
        use OrderStatus::*;
        match (self, next) {
            (QuotePending, QuoteExpired) => true,
            (QuotePending, Locked) => true,
            (Locked, Processing) => true,
            (Locked, Refunded) => true,
            (Processing, Settled) => true,
            (Processing, Disputed) => true,
            (Processing, Refunded) => true,
            (Disputed, Settled) => true,
            (Disputed, Refunded) => true,
            _ => false,
        }
    }
}

impl std::fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::QuotePending => write!(f, "QUOTE_PENDING"),
            Self::QuoteExpired => write!(f, "QUOTE_EXPIRED"),
            Self::Locked => write!(f, "LOCKED"),
            Self::Processing => write!(f, "PROCESSING"),
            Self::Settled => write!(f, "SETTLED"),
            Self::Disputed => write!(f, "DISPUTED"),
            Self::Refunded => write!(f, "REFUNDED"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRecord {
    pub id: Option<surrealdb::sql::Thing>,
    pub order_id: String,
    pub flow_direction: FlowDirection,
    pub spoke_chain: SpokeChain,
    pub fiat_rail: String,
    pub fiat_currency: String,
    pub fiat_amount: f64,
    pub crypto_asset: String,
    pub crypto_amount: f64,
    pub status: OrderStatus,
    pub maker: String,
    pub recipient_identifier: String,
    pub fiat_tx_id: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRecord {
    pub id: Option<surrealdb::sql::Thing>,
    pub account_id: String,
    pub description: String,
    pub asset: String,
    pub balance: f64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntryRecord {
    pub id: Option<surrealdb::sql::Thing>,
    pub order_id: String,
    pub debit_account: String,
    pub credit_account: String,
    pub asset: String,
    pub amount: f64,
    pub memo: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CreateOrderParams {
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
    pub validity_seconds: i64,
}

#[derive(Debug, Clone)]
pub struct DoubleEntryTransferParams {
    pub order_id: String,
    pub debit_account: String,
    pub credit_account: String,
    pub asset: String,
    pub amount: f64,
    pub memo: String,
}

pub struct StorageEngine;

impl StorageEngine {
    /// Initializes the pure-Rust embedded SurrealKV database.
    /// Eliminates external daemons, PostgreSQL, and SQLite dependencies.
    #[instrument(skip_all, fields(path = %data_dir))]
    pub async fn init(data_dir: &str) -> Result<Database, DbError> {
        let storage_path = Path::new(data_dir).join("surrealkv_data");
        let path_str = storage_path
            .to_str()
            .ok_or_else(|| DbError::Initialization("Invalid storage directory path".into()))?;

        info!("Mounting embedded SurrealKV engine at {}", path_str);
        let db = Surreal::new::<SurrealKv>(path_str).await?;

        db.use_ns("bamboo_protocol")
            .use_db("settlement_core")
            .await?;

        Self::bootstrap_schema(&db).await?;
        Self::seed_chart_of_accounts(&db).await?;

        info!("SurrealKV storage engine initialized and schemas verified");
        Ok(Arc::new(db))
    }

    /// Enforces strictly typed tables, assertions, and unique indexes inside SurrealKV.
    async fn bootstrap_schema(db: &Surreal<Db>) -> Result<(), DbError> {
        let schema_ql = r#"
            -- 1. Orders Table Definition
            DEFINE TABLE order SCHEMAFULL;
            DEFINE FIELD order_id ON TABLE order TYPE string;
            DEFINE FIELD flow_direction ON TABLE order TYPE string ASSERT $value IN ["ON_RAMP", "OFF_RAMP"];
            DEFINE FIELD spoke_chain ON TABLE order TYPE string ASSERT $value IN ["solana", "base", "bsc"];
            DEFINE FIELD fiat_rail ON TABLE order TYPE string;
            DEFINE FIELD fiat_currency ON TABLE order TYPE string;
            DEFINE FIELD fiat_amount ON TABLE order TYPE number ASSERT $value > 0;
            DEFINE FIELD crypto_asset ON TABLE order TYPE string;
            DEFINE FIELD crypto_amount ON TABLE order TYPE number ASSERT $value > 0;
            DEFINE FIELD status ON TABLE order TYPE string 
                ASSERT $value IN ["QUOTE_PENDING", "QUOTE_EXPIRED", "LOCKED", "PROCESSING", "SETTLED", "DISPUTED", "REFUNDED"];
            DEFINE FIELD maker ON TABLE order TYPE string;
            DEFINE FIELD recipient_identifier ON TABLE order TYPE string;
            DEFINE FIELD fiat_tx_id ON TABLE order TYPE option<string>;
            DEFINE FIELD expires_at ON TABLE order TYPE datetime;
            DEFINE FIELD created_at ON TABLE order TYPE datetime VALUE $before OR time::now();
            DEFINE FIELD updated_at ON TABLE order TYPE datetime VALUE time::now();
            DEFINE INDEX idx_order_id ON TABLE order FIELDS order_id UNIQUE;

            -- 2. Chart of Accounts Table
            DEFINE TABLE account SCHEMAFULL;
            DEFINE FIELD account_id ON TABLE account TYPE string;
            DEFINE FIELD description ON TABLE account TYPE string;
            DEFINE FIELD asset ON TABLE account TYPE string;
            DEFINE FIELD balance ON TABLE account TYPE number;
            DEFINE FIELD updated_at ON TABLE account TYPE datetime VALUE time::now();
            DEFINE INDEX idx_account_id ON TABLE account FIELDS account_id UNIQUE;

            -- 3. Strict Append-Only Journal Entries
            DEFINE TABLE journal_entry SCHEMAFULL;
            DEFINE FIELD order_id ON TABLE journal_entry TYPE string;
            DEFINE FIELD debit_account ON TABLE journal_entry TYPE string;
            DEFINE FIELD credit_account ON TABLE journal_entry TYPE string;
            DEFINE FIELD asset ON TABLE journal_entry TYPE string;
            DEFINE FIELD amount ON TABLE journal_entry TYPE number ASSERT $value > 0;
            DEFINE FIELD memo ON TABLE journal_entry TYPE string;
            DEFINE FIELD timestamp ON TABLE journal_entry TYPE datetime VALUE time::now();
        "#;

        db.query(schema_ql).await?;
        Ok(())
    }

    /// Pre-populates the standard Chart of Accounts required for zero-sum accounting.
    async fn seed_chart_of_accounts(db: &Surreal<Db>) -> Result<(), DbError> {
        let initial_accounts = [
            ("ESCROW_RECEIVABLE_USDC", "Escrowed USDC locked on-chain", "USDC"),
            ("FIAT_FLOAT_KES", "Safaricom M-Pesa B2C Working Float", "KES"),
            ("FIAT_FLOAT_BRL", "Central Bank Pix Working Capital", "BRL"),
            ("FIAT_FLOAT_EUR", "SEPA Instant Commercial Account", "EUR"),
            ("HOT_CRYPTO_TREASURY_USDC", "Relayer Hot Clearing USDC Pool", "USDC"),
            ("FEE_REVENUE_USDC", "Protocol Accrued Host Fees (F_host <= 25bps)", "USDC"),
            ("DISPUTE_WRITEOFF_KES", "Default Reserve for Unresolved Disputes", "KES"),
        ];

        for (acc_id, desc, asset) in initial_accounts {
            let check_sql = "SELECT * FROM account WHERE account_id = $acc_id LIMIT 1";
            let mut response = db.query(check_sql).bind(("acc_id", acc_id)).await?;
            let existing: Vec<AccountRecord> = response.take(0)?;

            if existing.is_empty() {
                let insert_sql = r#"
                    CREATE account CONTENT {
                        account_id: $acc_id,
                        description: $desc,
                        asset: $asset,
                        balance: 0.0,
                        updated_at: time::now()
                    };
                "#;
                db.query(insert_sql)
                    .bind(("acc_id", acc_id))
                    .bind(("desc", desc))
                    .bind(("asset", asset))
                    .await?;
            }
        }

        Ok(())
    }

    /// Registers a new order inside SurrealKV with an initial QuotePending state.
    pub async fn create_order(
        db: &Surreal<Db>,
        params: CreateOrderParams,
    ) -> Result<OrderRecord, DbError> {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(params.validity_seconds);

        let insert_sql = r#"
            CREATE order CONTENT {
                order_id: $order_id,
                flow_direction: $flow_direction,
                spoke_chain: $spoke_chain,
                fiat_rail: $fiat_rail,
                fiat_currency: $fiat_currency,
                fiat_amount: $fiat_amount,
                crypto_asset: $crypto_asset,
                crypto_amount: $crypto_amount,
                status: "QUOTE_PENDING",
                maker: $maker,
                recipient_identifier: $recipient,
                fiat_tx_id: NONE,
                expires_at: <datetime> $expires_at,
                created_at: time::now(),
                updated_at: time::now()
            };
        "#;

        let mut res = db
            .query(insert_sql)
            .bind(("order_id", params.order_id.clone()))
            .bind(("flow_direction", params.flow_direction.to_string()))
            .bind(("spoke_chain", params.spoke_chain.to_string()))
            .bind(("fiat_rail", params.fiat_rail))
            .bind(("fiat_currency", params.fiat_currency))
            .bind(("fiat_amount", params.fiat_amount))
            .bind(("crypto_asset", params.crypto_asset))
            .bind(("crypto_amount", params.crypto_amount))
            .bind(("maker", params.maker))
            .bind(("recipient", params.recipient_identifier))
            .bind(("expires_at", expires_at))
            .await?;

        let created: Option<OrderRecord> = res.take(0)?;
        created.ok_or_else(|| DbError::Initialization("Failed to insert order into SurrealKV".into()))
    }

    /// Queries an order by its canonical 32-byte hexadecimal string.
    pub async fn get_order(db: &Surreal<Db>, order_id: &str) -> Result<Option<OrderRecord>, DbError> {
        let ql = "SELECT * FROM order WHERE order_id = $order_id LIMIT 1";
        let mut res = db.query(ql).bind(("order_id", order_id.to_string())).await?;
        let orders: Vec<OrderRecord> = res.take(0)?;
        Ok(orders.into_iter().next())
    }

    /// Updates order status with strict lifecycle progression validation.
    pub async fn transition_order_status(
        db: &Surreal<Db>,
        order_id: &str,
        next_status: OrderStatus,
        fiat_tx_id: Option<String>,
    ) -> Result<OrderRecord, DbError> {
        let current_order = Self::get_order(db, order_id)
            .await?
            .ok_or_else(|| DbError::OrderNotFound(order_id.to_string()))?;

        if !current_order.status.can_transition_to(next_status) {
            return Err(DbError::InvalidStatusTransition {
                current: current_order.status,
                next: next_status,
            });
        }

        let update_sql = r#"
            UPDATE order SET 
                status = $next_status,
                fiat_tx_id = $fiat_tx_id,
                updated_at = time::now()
            WHERE order_id = $order_id;
        "#;

        let mut res = db
            .query(update_sql)
            .bind(("next_status", next_status.to_string()))
            .bind(("fiat_tx_id", fiat_tx_id.or(current_order.fiat_tx_id)))
            .bind(("order_id", order_id.to_string()))
            .await?;

        let updated: Vec<OrderRecord> = res.take(0)?;
        updated
            .into_iter()
            .next()
            .ok_or_else(|| DbError::OrderNotFound(order_id.to_string()))
    }

    /// Executes an atomic ACID double-entry balanced transaction across the Chart of Accounts.
    /// Invariant: Σ Debits == Σ Credits.
    pub async fn record_balanced_transaction(
        db: &Surreal<Db>,
        params: DoubleEntryTransferParams,
    ) -> Result<(), DbError> {
        if params.amount <= 0.0 {
            return Err(DbError::UnbalancedTransaction {
                debit: params.amount,
                credit: 0.0,
            });
        }

        // SurrealQL atomic transaction block
        let txn_sql = r#"
            BEGIN TRANSACTION;

            -- 1. Insert immutable journal entry
            CREATE journal_entry CONTENT {
                order_id: $order_id,
                debit_account: $debit_account,
                credit_account: $credit_account,
                asset: $asset,
                amount: $amount,
                memo: $memo,
                timestamp: time::now()
            };

            -- 2. Adjust debit account (+ for asset/expense accounts)
            UPDATE account SET 
                balance += $amount,
                updated_at = time::now()
            WHERE account_id = $debit_account;

            -- 3. Adjust credit account (- for asset accounts in disbursement)
            UPDATE account SET 
                balance -= $amount,
                updated_at = time::now()
            WHERE account_id = $credit_account;

            COMMIT TRANSACTION;
        "#;

        db.query(txn_sql)
            .bind(("order_id", params.order_id))
            .bind(("debit_account", params.debit_account))
            .bind(("credit_account", params.credit_account))
            .bind(("asset", params.asset))
            .bind(("amount", params.amount))
            .bind(("memo", params.memo))
            .await?;

        Ok(())
    }

    /// Subscribes directly to real-time order state changes via SurrealDB LIVE SELECT changefeeds.
    /// Pushes instant transitions over WebSockets to client frontends without Redis.
    pub async fn subscribe_order_changes(
        db: &Surreal<Db>,
    ) -> Result<impl Stream<Item = Result<surrealdb::Notification<OrderRecord>, surrealdb::Error>>, DbError> {
        let stream = db.select("order").live().await?;
        Ok(stream)
    }

    /// Retrieves current working float balance for liquidity monitoring.
    pub async fn get_account_balance(db: &Surreal<Db>, account_id: &str) -> Result<f64, DbError> {
        let ql = "SELECT balance FROM account WHERE account_id = $account_id LIMIT 1";
        let mut res = db.query(ql).bind(("account_id", account_id.to_string())).await?;
        
        #[derive(Deserialize)]
        struct BalanceResult {
            balance: f64,
        }

        let records: Vec<BalanceResult> = res.take(0)?;
        records
            .into_iter()
            .next()
            .map(|r| r.balance)
            .ok_or_else(|| DbError::AccountNotFound(account_id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_surrealkv_lifecycle_and_accounting() {
        let dir = tempdir().expect("Failed to create tempdir");
        let db = StorageEngine::init(dir.path().to_str().unwrap())
            .await
            .expect("Engine init failed");

        // 1. Create an On-Ramp Order
        let params = CreateOrderParams {
            order_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".into(),
            flow_direction: FlowDirection::OnRamp,
            spoke_chain: SpokeChain::Solana,
            fiat_rail: "MPESA_DARAYA".into(),
            fiat_currency: "KES".into(),
            fiat_amount: 15_000.0,
            crypto_asset: "USDC".into(),
            crypto_amount: 115.0,
            maker: "8x2...solana".into(),
            recipient_identifier: "+254712345678".into(),
            validity_seconds: 7,
        };

        let order = StorageEngine::create_order(&db, params.clone())
            .await
            .expect("Failed to create order");
        assert_eq!(order.status, OrderStatus::QuotePending);

        // 2. Validate Transition
        let locked_order = StorageEngine::transition_order_status(
            &db,
            &params.order_id,
            OrderStatus::Locked,
            None,
        )
        .await
        .expect("Failed status transition");
        assert_eq!(locked_order.status, OrderStatus::Locked);

        // 3. Test Invalid Transition (Terminal violation)
        let invalid_res = StorageEngine::transition_order_status(
            &db,
            &params.order_id,
            OrderStatus::QuotePending,
            None,
        )
        .await;
        assert!(invalid_res.is_err());

        // 4. Test Double-Entry Transaction
        let transfer = DoubleEntryTransferParams {
            order_id: params.order_id.clone(),
            debit_account: "ESCROW_RECEIVABLE_USDC".into(),
            credit_account: "HOT_CRYPTO_TREASURY_USDC".into(),
            asset: "USDC".into(),
            amount: 115.0,
            memo: "Order collateral locked on Solana spoke".into(),
        };

        StorageEngine::record_balanced_transaction(&db, transfer)
            .await
            .expect("Failed double entry ledger transfer");

        let escrow_balance = StorageEngine::get_account_balance(&db, "ESCROW_RECEIVABLE_USDC")
            .await
            .unwrap();
        let treasury_balance = StorageEngine::get_account_balance(&db, "HOT_CRYPTO_TREASURY_USDC")
            .await
            .unwrap();

        assert_eq!(escrow_balance, 115.0);
        assert_eq!(treasury_balance, -115.0);
    }
}
