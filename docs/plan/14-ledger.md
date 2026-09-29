---
okf_version: "0.2"
title: "Double-Entry Ledger Specification"
description: "Technical specification for bamboo-relayer double-entry bookkeeping, SQLite WAL persistence, chart of accounts, and automated reconciliation."
type: "plan"
tags:
  - plan
  - ledger
  - sqlite-wal
  - accounting
  - double-entry
categories:
  - Plan
resource: "crates/bamboo-relayer/src/db.rs"
generated:
  by: "openwiki/0.2"
  at: "2026-09-29T07:30:00Z"
---

<!-- OPENWIKI:START -->
# Component Specification: Double-Entry Ledger (`bamboo-relayer`)

## 1. Purpose & Responsibilities

The `LedgerDb` module in `bamboo-relayer` enforces cryptographic double-entry bookkeeping across all fiat disbursements and crypto settlements. It provides absolute financial auditability and guarantees that the relayer never leaks capital or experiences untracked balance drift.

### Responsibilities
- **ACID Double-Entry Accounting:** Enforce balanced journal transactions where every debit strictly equals every credit ($\sum \text{Debits} = \sum \text{Credits}$).
- **State Persistence:** Migrate from ephemeral in-memory databases ([`main.rs:142`](../../crates/bamboo-relayer/src/main.rs)) to a persistent, file-backed SQLite database operating in Write-Ahead Logging (WAL) mode.
- **Account Balance Tracking:** Maintain live account balances updated atomically within each journal transaction, resolving the schema omission where `accounts.balance` was never updated.
- **Financial Reconciliation:** Provide automated reconciliation routines matching local ledger balances against Solana on-chain token vault balances and Safaricom M-Pesa statement exports.

---

## 2. Storage Engine Decision: File-Backed SQLite WAL

### Selection: SQLite WAL Mode (`/var/data/bamboo/relayer.db`)
For Bamboo v1, **file-backed SQLite WAL** is selected over PostgreSQL.

| Evaluation Dimension | SQLite WAL | PostgreSQL | Decision Rationale for v1 |
|---|---|---|---|
| **Operational Overhead** | Zero external dependencies; single file; runs inside daemon process. | Requires external daemon, connection pool, backup agent, and RDS/host provisioning. | **SQLite wins:** Preserves the zero-cloud, single-binary architecture mandated by ADR 0004. |
| **Write Performance** | 20,000–50,000 writes/sec with WAL + memory-mapped I/O. | 5,000–10,000 writes/sec (network roundtrip bound). | **SQLite wins:** Exceeds v1 throughput requirements by three orders of magnitude. |
| **Concurrency Model** | Multiple readers, single writer with zero lock contention in WAL mode. | Multi-reader, multi-writer MVCC. | **SQLite adequate:** The relayer runs as a single-instance daemon where writes are serialized through an async worker. |
| **Crash Safety** | Full ACID durability; write-ahead log survives process crashes and power loss. | Full ACID durability with enterprise write replication. | **Tie:** Both guarantee zero corrupted transactions on crash. |

---

## 3. Chart of Accounts & Invariants

### 3.1 Chart of Accounts

| Account ID | Category | Currency / Unit | Description |
|---|---|---|---|
| `ESCROW_RECEIVABLE` | Asset | USDC (atomic units) | Collateral locked in Solana escrow vaults awaiting finalization. |
| `MPESA_FLOAT` | Asset | KES (integer) | Working fiat capital held in Safaricom B2C shortcode account. |
| `RELAYER_USDC_HOT` | Asset | USDC (atomic units) | Settled crypto collateral received into relayer hot wallet. |
| `HOST_FEE_REVENUE` | Revenue | USDC (atomic units) | Cumulative 20–25 bps protocol fees collected by relayer. |
| `DISPUTE_WRITEOFF` | Expense | KES (integer) | Payout losses incurred from disputed or uncollectible orders. |
| `EQUITY_CAPITAL` | Equity | Multi-currency | Initial capital injected by the reference relayer operator. |

### 3.2 Fundamental Accounting Invariants
1. **Zero-Sum Transaction Balance:**
   Every financial transaction comprises at least two postings whose amounts satisfy:
   $$\sum_{i} \text{Debit}_i - \sum_{j} \text{Credit}_j = 0$$
2. **Float Solvency:**
   $$\text{Balance}(\text{MPESA\_FLOAT}) \ge 0$$
   The relayer rejects new quotes if available M-Pesa float is insufficient to settle the requested fiat volume.
3. **Atomic Account Balance Updates:**
   Inserting a journal entry MUST update the corresponding `accounts.balance` column within the same database transaction.

---

## 4. Database Schema (SQLite DDL)

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;

-- Financial Accounts
CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    currency TEXT NOT NULL, -- 'USDC' or 'KES'
    balance INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

-- Balanced Journal Transactions
CREATE TABLE IF NOT EXISTS journal_transactions (
    id TEXT PRIMARY KEY,
    order_id TEXT,
    timestamp INTEGER NOT NULL,
    description TEXT NOT NULL
);

-- Individual Debit / Credit Postings
CREATE TABLE IF NOT EXISTS journal_postings (
    id TEXT PRIMARY KEY,
    transaction_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    entry_type TEXT NOT NULL CHECK(entry_type IN ('DEBIT', 'CREDIT')),
    amount INTEGER NOT NULL CHECK(amount > 0),
    FOREIGN KEY(transaction_id) REFERENCES journal_transactions(id) ON DELETE CASCADE,
    FOREIGN KEY(account_id) REFERENCES accounts(id)
);

-- Order State Mirror
CREATE TABLE IF NOT EXISTS orders (
    order_id TEXT PRIMARY KEY,
    maker TEXT NOT NULL,
    token_mint TEXT NOT NULL,
    crypto_amount INTEGER NOT NULL,
    fiat_amount INTEGER NOT NULL,
    fiat_recipient_hash TEXT NOT NULL,
    recipient_phone TEXT NOT NULL,
    salt BLOB NOT NULL,
    host_fee_bps INTEGER NOT NULL,
    expiry_timestamp INTEGER NOT NULL,
    mpesa_receipt TEXT,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Indexes for Rapid Replay and Audit
CREATE INDEX IF NOT EXISTS idx_postings_tx ON journal_postings(transaction_id);
CREATE INDEX IF NOT EXISTS idx_postings_account ON journal_postings(account_id);
CREATE INDEX IF NOT EXISTS idx_orders_status ON orders(status);
```

---

## 5. Lifecycle Accounting Entries

### Scenario A: Order Locked On-Chain
When an `OrderLocked` event is confirmed on Solana:
*Memorandum entry recording off-chain collateral commitment.*

### Scenario B: M-Pesa Disbursement Confirmed
When Safaricom confirms KES transfer via callback:
```
Transaction: M-Pesa Payout Disbursed (Order e5beec6e...)
  DEBIT   ESCROW_RECEIVABLE_KES       KES 13,050
  CREDIT  MPESA_FLOAT                 KES 13,050
```

### Scenario C: Solana Settlement Finalized
When `finalize()` completes on Solana, transferring 100 USDC to relayer:
```
Transaction: Escrow Finalized (Order e5beec6e...)
  DEBIT   RELAYER_USDC_HOT            100,000,000 (100.00 USDC)
  CREDIT  ESCROW_RECEIVABLE           99,800,000  (99.80 USDC Payout)
  CREDIT  HOST_FEE_REVENUE            200,000     (0.20 USDC Fee)
```

### Scenario D: Dispute Loss (Maker Disputed After Relayer Paid Fiat)
If an order is disputed and funds return to the maker upon timeout:
```
Transaction: Dispute Write-Off (Order e5beec6e...)
  DEBIT   DISPUTE_WRITEOFF            KES 13,050
  CREDIT  ESCROW_RECEIVABLE_KES       KES 13,050
```

---

## 6. Daily Reconciliation & Disaster Recovery

### 6.1 Three-Way Automated Reconciliation
Every 24 hours at 00:00 UTC, the relayer runs an automated three-way check:
1. **On-Chain Escrows vs. Ledger:** Sum of active `ESCROW_RECEIVABLE` accounts must equal the token balance of all un-settled `token_vault` accounts on Solana.
2. **Daraja Utility vs. Ledger:** Call Safaricom's Account Balance API (`POST /mpesa/accountbalance/v1/query`) and confirm `WorkingAccountAvailableFunds` matches `MPESA_FLOAT.balance` within a tolerance of pending in-flight transactions.
3. **Hot Wallet vs. Ledger:** Query the relayer's Solana USDC token account and verify it matches `RELAYER_USDC_HOT.balance`.

### 6.2 Backup & Restore Procedure
- **Live WAL Snapshot:** Backed up every 6 hours using SQLite's online backup API:
  ```bash
  sqlite3 /var/data/bamboo/relayer.db ".backup /var/backups/bamboo/relayer_$(date +%s).bak"
  ```
- **Disaster Recovery:** Restoring from backup requires replaying Solana finalized transactions since the backup timestamp to restore full synchronization.

<!-- OPENWIKI:END -->
