---
okf_version: "0.2"
title: "ADR 0003 — Solana Spoke: Squads Protocol v4 Standard & Anti-Griefing Micro-Bonds"
description: "Architecture Decision Record: Engineering the Solana SVM escrow spoke following Squads Protocol v4 modular architecture, isolated order PDAs, and 0.01 SOL anti-griefing bonds."
type: "decision"
tags:
  - adr
  - decision
  - solana
  - anchor
  - squads-v4
  - micro-bonds
categories:
  - Decisions
resource: "contracts/solana-escrow/programs/bamboo-escrow/src/lib.rs"
generated:
  by: "openwiki/0.2"
  at: "2026-09-16T12:34:00Z"
---

<!-- OPENWIKI:START -->
# ADR 0003 — Solana Spoke: Squads Protocol v4 Standard & Anti-Griefing Micro-Bonds

**Status:** Accepted  
**Date:** 2026-09-08  
**Area:** Solana SVM Smart Contracts (`contracts/solana-escrow`)  
**Deciders:** Core Protocol Team  
**Related Docs:** [Solana SVM Spoke](../spokes/solana-svm.md)

---

> [!NOTE]
> **Status: v1 differs — see [docs/plan/](../plan/00-overview.md)**  
> Bamboo v1 uses an Anchor program on Solana with 0.01 SOL anti-griefing bonds, but "Squads v4" refers only to organizational conventions, not on-chain Squads CPI integration. See [10-solana-escrow.md](../plan/10-solana-escrow.md).

## Context

On Solana, high-throughput escrow systems frequently suffer from two critical architectural vulnerabilities:
1. **Monolithic Program Anti-Patterns:** Programs that lump state, validation, instruction logic, and error handling into a single `lib.rs` file. This pattern impairs auditability, creates compute-unit (CU) bloat, and complicates multi-instruction composability.
2. **Order Book Spam & Griefing Attacks:** In an off-ramping system, an attacker could create hundreds of empty or un-actionable escrow orders on-chain for negligible Solana gas fees ($0.0002). This would force off-chain relayers to lock up working fiat capital and open unnecessary hedging positions on Hyperliquid, causing financial Denial of Service (DoS).

---

## Decision

We engineer **`contracts/solana-escrow`** strictly conforming to the **Squads Protocol v4 Enterprise Architecture Standard**, incorporating isolated PDA state and economic anti-griefing bonds:

1. **Squads Protocol v4 Modular Layout:**
   * **Decoupled Instructions:** Handlers are strictly isolated into dedicated files (`lock_order.rs`, `settle_payout.rs`, `claim_refund.rs`).
   * **Encapsulated Account Structs:** Account layouts (`state/order.rs`) feature deterministic byte sizing (`OrderAccount::LEN`) and helper methods (`calculate_fee_split`).
   * **Dedicated Constants & Errors:** Centralized seed definitions (`constants.rs`) and custom error codes (`errors.rs`).

2. **Isolated Order PDAs:**
   Each order lives in its own Program Derived Address derived deterministically from:
   $$\text{seeds} = [\text{"order"}, \text{order\_id}]$$
   This guarantees that high-concurrency order creation never creates state lock contention on a single shared account, enabling massive parallel execution across Solana's Sealevel runtime.

3. **Anti-Griefing Micro-Bond (`0.01 SOL`):**
   * Any maker locking an order must deposit a refundable bond of `0.01 SOL` (`10_000_000` lamports) into the order PDA.
   * **Refund Mechanism:** When the order settles normally (`settle_payout`) or expires past the timelock (`claim_refund`), the 0.01 SOL bond is automatically returned to the maker.
   * **Economic Deterrent:** Spamming 1,000 fake orders requires tying up 10 SOL in bonded escrow, rendering spam attacks economically unviable while imposing zero net cost on honest users.

4. **Atomic Jupiter DEX CPI Composability:**
   * The program is structured to allow atomic multi-instruction transaction bundles: a user can swap arbitrary SPL tokens (e.g., JUP, BONK, RAY) into SOL or USDC via Jupiter CPI in instruction 1, and immediately lock the proceeds into `bamboo_escrow` in instruction 2 within the same atomic transaction.

---

## Consequences

### Positive
* **Institutional Auditability:** Following Squads v4 standards makes the contract immediately recognizable to Solana security auditors (Neodyme, OtterSec, Kudelski).
* **DoS Immunity:** The 0.01 SOL micro-bond completely disincentivizes fake order generation.
* **Maximum Parallelism:** Order PDAs avoid write-lock bottlenecks on Solana validator threads.

### Negative / Tradeoffs
* **Initial Capital Requirement:** Users must hold at least 0.01 SOL in their wallet to post the temporary anti-griefing bond.
<!-- OPENWIKI:END -->
