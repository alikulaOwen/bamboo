---
okf_version: "0.2"
title: "ADR 0004 — Zero-Cloud Relayer Daemon: Axum, SQLite WAL, and 7-Second RFQ Engine"
description: "Architecture Decision Record: Structuring the off-chain relayer as a self-hosted single-binary Rust daemon with SQLite WAL double-entry accounting and an in-memory 7-second RFQ quoter."
type: "decision"
tags:
  - adr
  - decision
  - relayer
  - rust
  - axum
  - sqlite
  - rfq
categories:
  - Decisions
resource: "crates/bamboo-relayer/src/main.rs"
generated:
  by: "openwiki/0.2"
  at: "2026-09-16T12:35:00Z"
---

<!-- OPENWIKI:START -->
# ADR 0004 — Zero-Cloud Relayer Daemon: Axum, SQLite WAL, and 7-Second RFQ Engine

**Status:** Accepted  
**Date:** 2026-09-08  
**Area:** Off-Chain Infrastructure (`crates/bamboo-relayer`)  
**Deciders:** Core Protocol Team  
**Related Docs:** [Unified Host Daemon](../relayer/unified-host-daemon.md)

---

> [!NOTE]
> **Status: v1 differs — see [docs/plan/](../plan/00-overview.md)**  
> Bamboo v1 uses a single reference relayer operated by the protocol team, not a permissionless marketplace. Storage is exclusively file-backed SQLite in WAL mode; SurrealDB is not used. See [13-relayer-daemon.md](../plan/13-relayer-daemon.md) and [14-ledger.md](../plan/14-ledger.md).

## Context

Decentralized routing and settlement protocols often succumb to two infrastructure failures:
1. **Cloud-Native Complexity Bloat:** Requiring Kubernetes clusters, external managed databases (RDS, DynamoDB), distributed message brokers (Kafka), and proprietary cloud services. This prevents permissionless node running, creates high operational costs, and introduces single points of censorship.
2. **Imbalanced Financial Accounting:** Relying on simple database flags (`status = "completed"`) rather than cryptographic double-entry bookkeeping, leading to phantom balance leaks and reconciliation failures across fiat and crypto ledgers.

---

## Decision

We implement **`bamboo-relayer`** as an autonomous, zero-cloud **single Rust binary** powered by Axum and embedded storage:

1. **Zero-Cloud Single Binary Deployment:**
   * Compiles to a single static binary with no external daemon dependencies.
   * Can run on any basic Linux server, Raspberry Pi, or local merchant machine with under 256MB of RAM.

2. **In-Memory 7-Second RFQ Countdown Engine:**
   * Exchange rate quotes are delivered via REST (`/rfq/quote`) and streamed over WebSockets (`/rfq/ws`).
   * Enforces a deterministic **7-second expiry window** (`RFQ_COUNTDOWN_SECONDS = 7`), perfectly bridging the gap between volatile DEX prices and domestic fiat payout guarantees.

3. **ACID Double-Entry Bookkeeping in SQLite WAL Mode:**
   * Financial ledger entries are persisted locally in SQLite configured with Write-Ahead Logging:
     ```sql
     PRAGMA journal_mode = WAL;
     PRAGMA synchronous = NORMAL;
     ```
   * Every settlement transaction requires a balanced journal entry where total debits strictly equal total credits ($\sum \text{Debits} = \sum \text{Credits}$), recording:
     * `ESCROW_VAULT` (debit)
     * `SETTLEMENT_PAYOUT` (credit)
     * `HOST_FEE_POOL` (credit, capped at $\le 25\text{ bps}$)

4. **Multi-Chain Event Dispatch Loop:**
   * Embedded asynchronous workers listen concurrently to Base (Alloy WebSocket), Solana (Solana PubSub WebSocket), and BSC (Alloy WebSocket), triggering instant delta hedges and domestic banking calls upon log emission.

---

## Consequences

### Positive
* **Permissionless Node Operation:** Anyone can run a Bamboo relayer node without a cloud subscription.
* **Flawless Financial Integrity:** Double-entry accounting ensures no relayer can inadvertently double-spend or lose track of collateral payouts.
* **Extreme Throughput:** In-process RFQ state and SQLite WAL eliminate network roundtrips for quotation and matching.

### Negative / Tradeoffs
* **Local Storage Reliability:** Node operators must manage backups of their local SQLite WAL database files.
* **Vertical Scaling Limits:** A single SQLite instance is bound to single-node disk I/O (though WAL handles up to 50,000 writes/second, far exceeding early protocol demands).
<!-- OPENWIKI:END -->
