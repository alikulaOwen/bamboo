---
okf_version: "0.2"
title: "ADR 0002 — Hyperliquid L1 as the Delta-Neutral Clearing Spine"
description: "Architecture Decision Record: Selecting Hyperliquid Layer 1 central limit order books as the protocol's 1x short perp hedging venue to eliminate collateral volatility."
type: "decision"
tags:
  - adr
  - decision
  - hedging
  - hyperliquid
  - delta-neutral
categories:
  - Decisions
resource: "crates/bamboo-hedging/src/lib.rs"
generated:
  by: "openwiki/0.2"
  at: "2026-09-16T12:33:00Z"
---

<!-- OPENWIKI:START -->
# ADR 0002 — Hyperliquid L1 as the Delta-Neutral Clearing Spine

**Status:** Accepted  
**Date:** 2026-09-08  
**Area:** Hedging & Clearing / Risk Engine  
**Deciders:** Core Protocol Team  
**Related Docs:** [Why Choose Hyperliquid](../architecture/why-hyperliquid.md), [Delta-Neutral Hedging](../architecture/delta-neutral-hedging.md)

---

## Context

When crypto collateral (SOL, ETH, BNB) is locked on a spoke escrow, the recipient receives a guaranteed fiat quotation (e.g., KES, BRL, EUR). However, domestic banking transfer verification requires anywhere from 15 seconds to 15 minutes.

If the market drops 3% during this window:
* If the relayer absorbs the loss, the protocol’s capped 25 bps host fee model is completely broken.
* If the user absorbs the loss, the transaction fails or delivers less fiat than quoted, destroying user trust.

To solve this, the protocol must establish an instantaneous, programmatic delta hedge ($\Delta = 0$) for the duration of the fiat settlement window.

---

## Decision

We designate **Hyperliquid L1** as the protocol’s singular, non-custodial clearing house:

1. **Automated 1x Short Perp Hedge:**
   * Upon detecting an `OrderLocked` event on any spoke, `bamboo-hedging` automatically places a market/limit short order of equivalent notional size on Hyperliquid L1.
   * By maintaining exactly 1x leverage, liquidation risk is completely eliminated while market delta is reduced to zero:
   $$\Delta_{\text{net}} = \Delta_{\text{collateral}} + \Delta_{\text{short}} = 0$$

2. **Delegated Agent Keys via EIP-712:**
   * Relayers execute trades using dedicated **Agent API Keys** authorized cryptographically by the master treasury wallet.
   * Agent keys possess trading and position-management rights only; **withdrawal permissions are cryptographically impossible**.

3. **Sub-Second Execution via HyperBFT:**
   * Hedges are established within ~200 milliseconds of on-chain escrow confirmation, fitting well inside the 7-second RFQ commitment window.

4. **Unified Margin Pool Across All Spokes:**
   * Rather than maintaining isolated hedging accounts on each individual chain, a single USDC margin account on Hyperliquid clears risk for Solana, Base, and BSC simultaneously.

---

## Consequences

### Positive
* **Zero Collateral Volatility Risk:** Neither the relayer nor the user is exposed to crypto market movements while waiting for bank settlement.
* **Zero Gas Trading Overhead:** Hyperliquid charges 0 gas fees for order placement/cancellation, allowing low-margin micro-transactions to be hedged profitably.
* **Non-Custodial Security:** The relayer host daemon holds only an agent key without fund extraction privileges.

### Negative / Tradeoffs
* **External L1 Dependency:** Relayers depend on Hyperliquid API uptime and WebSocket connectivity.
* **Margin Capital Requirements:** Relayers must pre-fund and maintain adequate USDC margin on Hyperliquid to maintain hedge capacity.
<!-- OPENWIKI:END -->
