---
okf_version: "0.2"
title: "Hyperliquid Delta-Neutral Clearing Spine"
description: "Specification of the bamboo-hedging clearing spine executing 1x short perps on Hyperliquid L1 to neutralize collateral volatility during fiat verification windows."
type: "architecture"
tags:
  - hedging
  - hyperliquid
  - delta-neutral
  - risk-management
categories:
  - Architecture
resource: "crates/bamboo-hedging/src/lib.rs"
generated:
  by: "openwiki/0.2"
  at: "2026-09-08T07:58:39Z"
---

<!-- OPENWIKI:START -->
# Hyperliquid Delta-Neutral Clearing Spine

During cross-chain fiat off-ramping, an off-chain fiat settlement window occurs between the time crypto collateral is locked in an escrow spoke (Base, Solana, or BNB Chain) and the time fiat arrives in the recipient's bank account. This window typically lasts anywhere from a few seconds (Pix, M-Pesa) to minutes (SEPA Instant).

If volatile tokens (e.g., SOL, BNB, ETH) are deposited as collateral, downward price movements during this settlement window could cause relayer losses or under-collateralized payouts.

Project Bamboo solves this by deploying **`bamboo-hedging`**, a dedicated sub-engine that dispatches automated **1x short perps/spot hedges on Hyperliquid L1 order books**.

```mermaid
graph TD
    subgraph Escrow["Escrow Spoke"]
        L[Collateral Locked in Escrow]
    end

    subgraph Relayer["bamboo-relayer"]
        D[Detect OrderLocked Event]
        Q[Calculate Collateral USD Value]
    end

    subgraph Clearing["Hyperliquid L1 Spine"]
        O[Dispatch 1x Short Perp Hedge]
        H[(Active Delta-Neutral Position)]
        C[Close Short Perp Position]
    end

    subgraph Banking["Domestic Rails"]
        F[Execute Fiat Transfer]
        V[Verify Receipt / Proof]
    end

    L --> D
    D --> Q
    Q --> O
    O --> H
    D --> F
    F --> V
    V --> C
    C --> Escrow
```

---

## Hedging Mechanics

1. **Leverage Calibration**:
   The engine enforces strict **1x leverage** (`default_leverage: 1`). This is purely delta-neutral risk mitigation, eliminating liquidation risk while exactly offsetting collateral price fluctuations:
   $$\Delta_{\text{net}} = \Delta_{\text{collateral}} + \Delta_{\text{short}} = (+1) + (-1) = 0$$

2. **Order Book Execution**:
   Orders are dispatched to Hyperliquid L1's low-latency CLOB (Central Limit Order Book) via signed EIP-712 messages over WebSocket/REST endpoints.

3. **Position Lifecycle**:
   * **`open_collateral_hedge`**: Triggered when `OrderLocked` is parsed from on-chain logs.
   * **`close_collateral_hedge`**: Triggered when either:
     - The domestic fiat rails confirm successful payout (`OrderSettled`), or
     - The escrow expires and a timelock refund is initiated (`OrderRefunded`).

---

## Code Reference
Implemented in [`crates/bamboo-hedging/src/lib.rs`](../../crates/bamboo-hedging/src/lib.rs) via `HyperliquidHedgeEngine` and `HedgePosition`.
<!-- OPENWIKI:END -->
