---
okf_version: "0.2"
title: "ADR 0004 — Zero-Cloud Relayer Daemon: Embedded SurrealKV, ACID Ledger, and Native Changefeeds"
description: "Architecture Decision Record: Structuring the off-chain relayer as a self-hosted single-binary Rust daemon backed by pure-Rust Embedded SurrealKV, ACID double-entry accounting, and native LIVE SELECT changefeeds."
type: "decision"
tags:
  - adr
  - decision
  - relayer
  - rust
  - axum
  - surrealkv
  - surrealdb
  - rfq
  - ledger
categories:
  - Decisions
resource: "crates/bamboo-relayer/src/db.rs"
generated:
  by: "openwiki/0.2"
  at: "2026-09-29T08:30:00Z"
---

<!-- OPENWIKI:START -->
# ADR 0004 — Zero-Cloud Relayer Daemon: Embedded SurrealKV, ACID Ledger, and Native Changefeeds

**Status:** Accepted (Supersedes SQLite WAL specification)  
**Date:** 2026-09-29  
**Area:** Off-Chain Infrastructure (`crates/bamboo-relayer`)  
**Deciders:** Protocol Architecture Team  
**Related Docs:** [Unified Host Daemon](../relayer/unified-host-daemon.md), [Ledger Subsystem](../plan/14-ledger.md)

---

## Context

Decentralized cross-border settlement protocols require a sovereign system of record to enforce atomic state transitions across polymorphic fiat rails and multi-chain smart contracts. Traditional settlement architectures typically introduce critical vulnerabilities:

1. **External Daemon & Cloud Bloat:** Relying on external database daemons (PostgreSQL, MySQL), cloud key-value stores (DynamoDB), or external message brokers (Redis, Kafka). This violates zero-cloud deployment principles, incurs operational costs, and introduces single points of censorship and failure.
2. **C-Dependency Complexities (SQLite):** While SQLite WAL offered a single-file alternative, relying on C-bindings (`rusqlite`) complicates hermetic cross-compilation, lacks native document-graph schema validation, and cannot provide integrated real-time pub/sub streams.
3. **Imbalanced Financial Accounting:** Relying on simple database state flags rather than verifiable double-entry bookkeeping, risking balance drift and reconciliation failures across fiat and crypto float accounts.

---

## Decision

We implement **`bamboo-relayer`** as an autonomous, zero-cloud **single Rust binary** backed by **embedded SurrealDB powered by pure-Rust SurrealKV** (`surrealdb` with `features = ["kv-surrealkv"]`):

### 1. Pure-Rust Embedded Storage Substrate (SurrealKV)
* Compiles into the relayer binary without external DBMS daemons, C-library bindings, or container sidecars.
* Mounts directly to local NVMe/SSD storage (`surrealkv_data`) with native MVCC transactions.
* Namespaced isolation: `namespace: bamboo_protocol`, `database: settlement_core`.

### 2. Strict SCHEMAFULL Bidirectional Order Model
* Enforces strict typing, value assertions, and unique indexes at the database engine layer:
  * `order`: Tracks `order_id` (unique), `flow_direction` (`ON_RAMP`, `OFF_RAMP`), `spoke_chain` (`solana`, `base`, `bsc`), fiat amounts, crypto amounts, maker, and expiry.
  * Status transitions strictly governed by protocol FSM:
    $$\text{QUOTE\_PENDING} \longrightarrow \text{LOCKED} \longrightarrow \text{PROCESSING} \longrightarrow \text{SETTLED} \mid \text{DISPUTED} \mid \text{REFUNDED}$$

### 3. ACID Balanced Double-Entry Ledger
* Invariant: Total debits strictly equal total credits ($\sum \text{Debits} = \sum \text{Credits}$).
* Embedded Chart of Accounts pre-seeded at boot:
  * `ESCROW_RECEIVABLE_USDC`: On-chain locked collateral.
  * `FIAT_FLOAT_KES`: Safaricom M-Pesa B2C working float.
  * `FIAT_FLOAT_BRL`: Central Bank Pix working capital.
  * `FIAT_FLOAT_EUR`: SEPA Instant commercial account.
  * `HOT_CRYPTO_TREASURY_USDC`: Relayer hot clearing pool.
  * `FEE_REVENUE_USDC`: Accrued protocol host fees ($F_{\text{host}} \le 25\text{ bps}$).
  * `DISPUTE_WRITEOFF_KES`: Reserve for unresolved disputes.
* Every transfer atomically commits an immutable `journal_entry` and updates matching `account` balances inside a SurrealQL `BEGIN TRANSACTION ... COMMIT TRANSACTION` block.

### 4. Native `LIVE SELECT` Changefeeds (Zero-Redis Pub/Sub)
* Replaces Redis Pub/Sub with SurrealDB's embedded `LIVE SELECT` changefeed.
* Yields real-time Rust async `Stream` of order modifications streamed directly to client WebSockets (`/orders/stream`).

### 5. In-Memory 7-Second RFQ Countdown Engine
* Exchange rate quotes are delivered via REST (`/rfq/quote`) and WebSocket subscriptions.
* Enforces a deterministic **7-second expiry window** (`RFQ_COUNTDOWN_SECONDS = 7`), bridging DEX price volatility with domestic fiat payout guarantees.

---

## Consequences

### Positive
* **Zero External Daemons:** No PostgreSQL, MySQL, SQLite C-toolchains, or Redis instances required. The relayer runs as a sovereign single binary.
* **Hermetic Pure-Rust Compilation:** Smooth cross-compilation across `x86_64` and `aarch64` targets.
* **Cryptographic & Financial Integrity:** Complete auditability with append-only journal entries and balanced double-entry assertions.
* **Integrated Streaming:** Instant reactive client updates over WebSockets powered by native changefeeds.

### Negative / Tradeoffs
* **Initial Dependency Compilation:** `surrealdb` with `kv-surrealkv` pulls in cryptographic and parser dependencies that increase cold compilation times.
* **Disk I/O Sizing:** Production nodes require standard SSD/NVMe disk write throughput for high-frequency settlement logging.
<!-- OPENWIKI:END -->
