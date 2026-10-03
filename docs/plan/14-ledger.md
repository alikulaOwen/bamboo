---
okf_version: "0.2"
title: "Double-Entry Ledger Specification (Embedded SurrealKV)"
description: "Technical specification for bamboo-relayer double-entry bookkeeping, pure-Rust Embedded SurrealKV persistence, SCHEMAFULL schema, and automated three-way reconciliation."
type: "plan"
tags:
  - plan
  - ledger
  - surrealkv
  - surrealdb
  - accounting
  - double-entry
categories:
  - Plan
resource: "crates/bamboo-relayer/src/db.rs"
generated:
  by: "openwiki/0.2"
  at: "2026-10-02T06:18:00Z"
---

<!-- OPENWIKI:START -->
# Component Specification: Double-Entry Ledger (`bamboo-relayer`)

## 1. Purpose & Responsibilities

The `StorageEngine` and ledger module in `bamboo-relayer` enforces cryptographic double-entry bookkeeping across all fiat disbursements and crypto settlements. It provides absolute financial auditability and guarantees that the relayer never leaks capital or experiences untracked balance drift.

### Responsibilities
- **ACID Double-Entry Accounting:** Enforce balanced journal transactions where every debit strictly equals every credit ($\sum \text{Debits} = \sum \text{Credits}$).
- **Zero-Cloud Embedded Persistence:** Backed by pure-Rust **Embedded SurrealKV** (`crates/bamboo-relayer/src/db.rs`), completely eliminating external DBMS daemons, PostgreSQL, and SQLite C-bindings (`rusqlite`) per [ADR 0004](../../docs/decisions/0004-relayer-zero-cloud-surrealkv.md).
- **Exact Integer Atomic Units:** Strictly store all monetary amounts as integer atomic units (`int` micro-USDC, lamports, KES cents) to eliminate floating-point rounding errors.
- **Idempotency Invariant:** Compound index on `(order_id, memo)` guarantees exactly-once execution for every order state transition.
- **Financial Reconciliation:** Provide automated reconciliation routines matching local ledger balances against Solana on-chain token vault balances and Safaricom M-Pesa statement exports.

---

## 2. Storage Engine Architecture: Pure-Rust Embedded SurrealKV

### Selection: Embedded SurrealKV (`surrealkv_data`)
Per [ADR 0004](../../docs/decisions/0004-relayer-zero-cloud-surrealkv.md), **Embedded SurrealKV** supersedes SQLite WAL.

| Evaluation Dimension | Embedded SurrealKV | SQLite WAL | PostgreSQL | Decision Rationale |
|---|---|---|---|---|
| **Language & Dependencies** | **100% Pure Rust** (`surrealdb` with `kv-surrealkv`). | C-library bindings (`rusqlite`). | External C/C++ daemon. | **SurrealKV wins:** Hermetic compilation into single static Rust binary. |
| **Operational Overhead** | **Zero external dependencies**; mounts directly to NVMe SSD. | Zero external dependencies; single file. | External connection pools, container sidecars, backups. | **SurrealKV wins:** Zero cloud bloat, sovereign host execution. |
| **Real-Time Streaming** | **Native `LIVE SELECT`** changefeeds for order updates. | Polling / custom triggers required. | Requires `LISTEN/NOTIFY` or external Kafka/Redis. | **SurrealKV wins:** Instant push notifications to WebSocket clients. |
| **Financial Integrity** | **Engine-level `SCHEMAFULL`** assertions and integer atomic types. | Table constraints, dynamic types. | Strong relational schema. | **SurrealKV wins:** Schema enforcement with document-graph flexibility. |

---

## 3. Chart of Accounts & Invariants

### 3.1 Chart of Accounts

| Account ID | Category | Currency / Unit | Description |
|---|---|---|---|
| `asset:escrow:solana_vault` | Asset | USDC (micro-units, 6 dec) | Collateral locked in Solana escrow vaults awaiting finalization. |
| `asset:fiat:mpesa_b2c_float` | Asset | KES (cents, 2 dec) | Working fiat capital held in Safaricom B2C shortcode account. |
| `asset:relayer:usdc_hot_wallet` | Asset | USDC (micro-units, 6 dec) | Settled crypto collateral received into relayer hot wallet. |
| `revenue:protocol:host_fee` | Revenue | USDC (micro-units, 6 dec) | Cumulative protocol host fees collected by relayer ($\le 100\text{ bps}$). |
| `expense:fiat:safaricom_tariff` | Expense | KES (cents, 2 dec) | Safaricom B2C bulk payment business charges (KES 5–13). |
| `expense:tax:kenya_excise_duty` | Expense | USDC / KES | 10% statutory excise tax under Kenya Finance Act 2025. |
| `equity:reserve:fraud_loss_reserve` | Equity | USDC (micro-units, 6 dec) | Loss reserve seeded via founder capital contribution ($2,000 T0). |

### 3.2 Fundamental Accounting Invariants
1. **Zero-Sum Transaction Balance:**
   Every financial transaction comprises postings whose amounts satisfy:
   $$\sum \text{Debits} - \sum \text{Credits} = 0$$
2. **Float Solvency:**
   $$\text{Balance}(\text{asset:fiat:mpesa_b2c_float}) \ge 0$$
   The relayer daemon rejects new quotes if liquid float is below safe threshold ($F_{\text{min}}$).
3. **Atomic Balance Updates:**
   Journal postings and account balance updates are committed atomically within an ACID transaction block.

---

## 4. SurrealQL Database Schema (`SCHEMAFULL`)

```surrealql
-- 1. Orders Table Definition
DEFINE TABLE order SCHEMAFULL;
DEFINE FIELD order_id ON TABLE order TYPE string;
DEFINE FIELD flow_direction ON TABLE order TYPE string ASSERT $value IN ["ON_RAMP", "OFF_RAMP"];
DEFINE FIELD spoke_chain ON TABLE order TYPE string ASSERT $value IN ["solana", "base", "bsc"];
DEFINE FIELD fiat_rail ON TABLE order TYPE string;
DEFINE FIELD fiat_currency ON TABLE order TYPE string;
DEFINE FIELD fiat_amount_atomic ON TABLE order TYPE int ASSERT $value > 0;
DEFINE FIELD crypto_asset ON TABLE order TYPE string;
DEFINE FIELD crypto_amount_atomic ON TABLE order TYPE int ASSERT $value > 0;
DEFINE FIELD host_fee_bps ON TABLE order TYPE int ASSERT $value <= 100;
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
DEFINE FIELD balance_atomic ON TABLE account TYPE int;
DEFINE FIELD updated_at ON TABLE account TYPE datetime VALUE time::now();
DEFINE INDEX idx_account_id ON TABLE account FIELDS account_id UNIQUE;

-- 3. Double-Entry Journal Entries Table
DEFINE TABLE journal_entry SCHEMAFULL;
DEFINE FIELD order_id ON TABLE journal_entry TYPE string;
DEFINE FIELD debit_account ON TABLE journal_entry TYPE string;
DEFINE FIELD credit_account ON TABLE journal_entry TYPE string;
DEFINE FIELD asset ON TABLE journal_entry TYPE string;
DEFINE FIELD amount_atomic ON TABLE journal_entry TYPE int ASSERT $value > 0;
DEFINE FIELD memo ON TABLE journal_entry TYPE string;
DEFINE FIELD timestamp ON TABLE journal_entry TYPE datetime VALUE time::now();
-- Idempotency constraint: guarantees exactly-once finalization per event
DEFINE INDEX idx_order_event ON TABLE journal_entry FIELDS order_id, memo UNIQUE;
```

---

## 5. Lifecycle Accounting Entries ($100 Ticket Benchmark)

### 5.1 Event: Fiat Disbursement (M-Pesa Sent)
When Safaricom confirms KES 13,050 payout:
```text
BEGIN TRANSACTION;
  -- Record Disbursement
  CREATE journal_entry CONTENT {
      order_id: $order_id,
      debit_account: "expense:fiat:mpesa_disbursement",
      credit_account: "asset:fiat:mpesa_b2c_float",
      asset: "KES",
      amount_atomic: 1305000, -- KES 13,050.00
      memo: "DISBURSEMENT_PAYOUT"
  };
  -- Record Safaricom Tariff
  CREATE journal_entry CONTENT {
      order_id: $order_id,
      debit_account: "expense:fiat:safaricom_tariff",
      credit_account: "asset:fiat:mpesa_b2c_float",
      asset: "KES",
      amount_atomic: 1100, -- KES 11.00
      memo: "SAFARICOM_B2C_TARIFF"
  };
  UPDATE account SET balance_atomic -= 1306100 WHERE account_id = "asset:fiat:mpesa_b2c_float";
COMMIT TRANSACTION;
```

### 5.2 Event: Settlement Finalized On-Chain
When `finalize()` succeeds on Solana (100 USDC received, 75 bps host fee):
```text
BEGIN TRANSACTION;
  CREATE journal_entry CONTENT {
      order_id: $order_id,
      debit_account: "asset:relayer:usdc_hot_wallet",
      credit_account: "asset:escrow:solana_vault",
      asset: "USDC",
      amount_atomic: 100000000, -- 100.00 USDC
      memo: "ESCROW_FINALIZED"
  };
  UPDATE account SET balance_atomic += 100000000 WHERE account_id = "asset:relayer:usdc_hot_wallet";
  UPDATE account SET balance_atomic -= 100000000 WHERE account_id = "asset:escrow:solana_vault";
COMMIT TRANSACTION;
```

---

## 6. Three-Way Automated Reconciliation

Every 24 hours at 00:00 UTC, the relayer runs an automated three-way check:
1. **On-Chain Escrows vs. SurrealKV:** Sum of active `asset:escrow:solana_vault` balances must equal the token balance of all un-settled `token_vault` accounts on Solana.
2. **Daraja Utility vs. SurrealKV:** Call Safaricom's Account Balance API (`POST /mpesa/accountbalance/v1/query`) and confirm `WorkingAccountAvailableFunds` matches `asset:fiat:mpesa_b2c_float` within tolerance of in-flight disbursements.
3. **Hot Wallet vs. SurrealKV:** Query the relayer's Solana USDC token account and verify it matches `asset:relayer:usdc_hot_wallet`.

<!-- OPENWIKI:END -->
