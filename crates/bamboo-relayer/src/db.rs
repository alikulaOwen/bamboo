use anyhow::Result;
use bamboo_core::UnifiedEscrowOrder;
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct LedgerDb {
    conn: Arc<Mutex<Connection>>,
}

#[allow(dead_code)]
impl LedgerDb {
    pub fn new_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.init()?;
        Ok(db)
    }

    pub fn new(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        // Enable WAL mode for high-concurrency ACID transactions
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS accounts (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                balance INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS journal_entries (
                id TEXT PRIMARY KEY,
                order_id TEXT NOT NULL,
                debit_account TEXT NOT NULL,
                credit_account TEXT NOT NULL,
                amount INTEGER NOT NULL,
                timestamp INTEGER NOT NULL,
                description TEXT NOT NULL,
                FOREIGN KEY(debit_account) REFERENCES accounts(id),
                FOREIGN KEY(credit_account) REFERENCES accounts(id)
            );

            CREATE TABLE IF NOT EXISTS escrow_orders (
                order_id TEXT PRIMARY KEY,
                chain TEXT NOT NULL,
                maker TEXT NOT NULL,
                token TEXT NOT NULL,
                amount INTEGER NOT NULL,
                fiat_recipient_hash TEXT NOT NULL,
                assigned_host TEXT NOT NULL,
                host_fee_bps INTEGER NOT NULL,
                expiry_timestamp INTEGER NOT NULL,
                status TEXT NOT NULL
            );
            ",
        )?;

        // Ensure default system accounts exist
        let mut check_stmt = conn.prepare("SELECT count(*) FROM accounts WHERE id = ?1")?;
        let count: i64 = check_stmt.query_row(params!["ESCROW_VAULT"], |row| row.get(0))?;
        if count == 0 {
            conn.execute(
                "INSERT INTO accounts (id, name, balance) VALUES (?1, ?2, 0), (?3, ?4, 0), (?5, ?6, 0)",
                params![
                    "ESCROW_VAULT", "Escrow Holding Vault",
                    "RELAYER_FEE_POOL", "Protocol Relayer Host Fee Pool",
                    "SETTLEMENT_PAYOUT", "Settlement Payout Account"
                ],
            )?;
        }

        Ok(())
    }

    pub fn record_order(&self, order: &UnifiedEscrowOrder) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO escrow_orders 
            (order_id, chain, maker, token, amount, fiat_recipient_hash, assigned_host, host_fee_bps, expiry_timestamp, status)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                hex::encode(order.order_id),
                order.chain.to_string(),
                order.maker,
                order.token_mint_or_address,
                order.amount as i64,
                hex::encode(order.fiat_recipient_hash),
                order.assigned_host,
                order.host_fee_bps as i64,
                order.expiry_timestamp,
                format!("{:?}", order.status),
            ],
        )?;
        Ok(())
    }

    pub fn record_settlement_double_entry(
        &self,
        order_id: &[u8; 32],
        payout_amount: u64,
        host_fee: u64,
    ) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        let order_hex = hex::encode(order_id);
        let now = chrono::Utc::now().timestamp();

        // 1. Debit Escrow Vault, Credit Settlement Payout
        let tx1_id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO journal_entries (id, order_id, debit_account, credit_account, amount, timestamp, description)
             VALUES (?1, ?2, 'ESCROW_VAULT', 'SETTLEMENT_PAYOUT', ?3, ?4, 'Escrow payout to maker recipient')",
            params![tx1_id, order_hex, payout_amount as i64, now],
        )?;

        // 2. If host fee > 0, Debit Escrow Vault, Credit Relayer Fee Pool
        if host_fee > 0 {
            let tx2_id = uuid::Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO journal_entries (id, order_id, debit_account, credit_account, amount, timestamp, description)
                 VALUES (?1, ?2, 'ESCROW_VAULT', 'RELAYER_FEE_POOL', ?3, ?4, 'Host relayer fee collection')",
                params![tx2_id, order_hex, host_fee as i64, now],
            )?;
        }

        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bamboo_core::{ChainEndpoint, OrderStatus};

    #[test]
    fn test_sqlite_wal_and_double_entry() {
        let db = LedgerDb::new_in_memory().unwrap();
        let order = UnifiedEscrowOrder {
            order_id: [10u8; 32],
            chain: ChainEndpoint::Bsc,
            maker: "0xMaker".into(),
            token_mint_or_address: "0xToken".into(),
            amount: 1_000_000,
            fiat_recipient_hash: [11u8; 32],
            assigned_host: "0xHost".into(),
            host_fee_bps: 25,
            expiry_timestamp: 1800000000,
            status: OrderStatus::Locked,
        };

        db.record_order(&order).unwrap();
        let fee = order.calculate_host_fee();
        let payout = order.calculate_payout_amount();

        assert!(db.record_settlement_double_entry(&order.order_id, payout, fee).is_ok());
    }
}
