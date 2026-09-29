---
okf_version: "0.2"
title: "ADR 0001 — Three-Spoke Cross-Chain Architecture Without Bridges"
description: "Architecture Decision Record: Selecting a three-spoke architecture (Base Uniswap v4, Solana Anchor SVM, and BNB Chain native EVM) instead of cross-chain bridges or single-chain deployment."
type: "decision"
tags:
  - adr
  - decision
  - architecture
  - cross-chain
  - three-spoke
categories:
  - Decisions
resource: "crates/bamboo-core/src/lib.rs"
generated:
  by: "openwiki/0.2"
  at: "2026-09-16T12:31:00Z"
---

<!-- OPENWIKI:START -->
# ADR 0001 — Three-Spoke Cross-Chain Architecture Without Bridges

**Status:** Accepted  
**Date:** 2026-09-08  
**Area:** Architecture / Cross-Chain Settlement  
**Deciders:** Core Protocol Team  

---

## Context

Most cross-chain financial applications force users into one of two suboptimal patterns:
1. **Single-Chain Centralization:** Deploying on a single chain (e.g., Ethereum or Solana only) and forcing users on other chains to manually bridge their assets before accessing off-ramping liquidity.
2. **Lock-and-Mint / Wrapped Bridges:** Using bridge protocols (e.g., Wormhole, LayerZero, Axelar) that lock native tokens on Chain A and issue synthetic wrapped tokens (e.g., `whUSDC`) on Chain B.

Bridges represent the single largest attack vector in Web3 history (responsible for over $2.5B in cumulative exploit losses). Furthermore, wrapped tokens introduce severe slippage, unwrap friction, and smart contract counterparty risk.

Meanwhile, emerging market crypto volume is strictly distributed across three specific ecosystems:
*   **Solana:** High-speed, retail DeFi and micro-payments.
*   **Base:** Low-cost Ethereum L2 ecosystem with deep Uniswap v4 liquidity.
*   **BNB Smart Chain (BSC):** The dominant retail and P2P off-ramping chain across Africa, Latin America, and Southeast Asia.

---

## Decision

We reject cross-chain asset bridging and lock-and-mint wrappers. 
Instead, we implement a **Three-Spoke Hub-and-Spoke Settlement Architecture** where each spoke operates natively on its host chain:

1. **Spoke 1: Base (Uniswap v4 Hook):**
   * Implemented in [`contracts/uniswap-hook/src/BambooSettlementHook.sol`](../../contracts/uniswap-hook/src/BambooSettlementHook.sol).
   * Intercepts swaps natively at the pool level, utilizing transient storage (`TSTORE`/`TLOAD`) and flash accounting to route crypto directly into escrow without intermediate wallet hops.

2. **Spoke 2: Solana (Anchor SVM Escrow):**
   * Implemented in [`contracts/solana-escrow/programs/bamboo-escrow`](../../contracts/solana-escrow/programs/bamboo-escrow).
   * Operates via isolated Program Derived Address (PDA) order accounts, enforces anti-griefing micro-bonds (0.01 SOL), and integrates with Jupiter DEX swaps.

3. **Spoke 3: BNB Smart Chain (Native BEP-20 / BNB Escrow):**
   * Implemented in [`contracts/bnb-escrow/src/BambooBscSettlement.sol`](../../contracts/bnb-escrow/src/BambooBscSettlement.sol).
   * Optimized for low-overhead retail off-ramping of native BNB and BEP-20 USDT directly against local cash merchants.

4. **Off-Chain Relayer Coordination (`bamboo-relayer`):**
   * A stateless, zero-cloud Rust daemon monitors event logs across all three spokes concurrently using `alloy` (EVM) and `solana-client` (SVM), matching order hashes deterministically.

---

## Consequences

### Positive
* **Zero Bridge Risk:** No wrapped tokens, no custodial bridge validators, and no multi-chain minting keys.
* **Native User Experience:** A user with SOL transacts on Solana; a user on Base swaps on Base; a user on BSC locks BEP-20. No cross-chain bridging steps are required.
* **Failure Isolation:** An outage or exploit on one spoke does not compromise the liquidity or state of the other two spokes.

### Negative / Tradeoffs
* **Triplicate Smart Contract Audits:** Requires maintaining and auditing two distinct smart contract languages/frameworks (Solidity for Base/BSC and Rust/Anchor for Solana).
* **Multi-Chain Node Monitoring:** The relayer daemon must maintain reliable RPC subscriptions across Solana, Base, and BSC simultaneously.
<!-- OPENWIKI:END -->
