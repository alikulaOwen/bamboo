---
okf_version: "0.2"
title: "Project Bamboo Operational Changelog"
description: "Chronological audit and evolution log for Project Bamboo specifications, contracts, crates, and OKF documentation."
type: "log"
tags:
  - changelog
  - history
  - protocol
categories:
  - Operations
resource: "docs/index.md"
generated:
  by: "openwiki/0.2"
  at: "2026-09-08T07:58:39Z"
---

<!-- OPENWIKI:START -->
# Project Bamboo Changelog

This document tracks chronological milestones, design upgrades, schema revisions, and smart contract deployments across the Project Bamboo monorepo in accordance with the Open Knowledge Format (OKF v0.2) specifications.

---

### [2026-09-08] - Three-Spoke Protocol Architecture & Monorepo Initialization
* **Author**: Principal Protocol Architect & Web3 Systems Engineer
* **Scope**: Core Repository Infrastructure, Contract Spokes, Rust Crates, OKF Documentation

#### Added
* **Monorepo Build Orchestration**:
  - Configured Moon (`moonrepo`) in [`.moon/workspace.yml`](../.moon/workspace.yml) and [`.moon/toolchains.yml`](../.moon/toolchains.yml) defining topological DAG builds.
  - Quarantined Anchor's `cargo-build-sbf` toolchain in `contracts/solana-escrow` away from host Cargo workspace members.
* **Spoke 1: Base EVM Uniswap v4 Hook**:
  - Implemented `BambooSettlementHook.sol` in [`contracts/uniswap-hook/src/BambooSettlementHook.sol`](../contracts/uniswap-hook/src/BambooSettlementHook.sol).
  - Engineered transient storage (`tstore`/`tload`) delta capture during `afterSwap` for gasless flash accounting.
  - Added periphery settlement router hooks for altcoin conversions.
* **Spoke 2: Solana SVM Escrow**:
  - Implemented Anchor program `bamboo_escrow` in [`contracts/solana-escrow/programs/bamboo-escrow/src/lib.rs`](../contracts/solana-escrow/programs/bamboo-escrow/src/lib.rs).
  - Enforced 0.01 SOL (`10_000_000` lamports) anti-griefing micro-bonds on makers/takers with PDA isolation (`seeds = [b"order", order_id]`).
  - Added support for atomic multi-instruction Jupiter CPI swaps and permissionless timelock refunds.
* **Spoke 3: BNB Chain Native EVM**:
  - Implemented standalone non-custodial escrow contract `BambooBscSettlement.sol` in [`contracts/bnb-escrow/src/BambooBscSettlement.sol`](../contracts/bnb-escrow/src/BambooBscSettlement.sol).
  - Added native BNB (`address(0)`) and BEP-20 (USDC/USDT) deposit handling with direct local fiat off-ramping without wrapped bridge tokens.
* **Host Crates**:
  - `bamboo-core`: Canonical 32-byte order and fiat payload schemas, fee ceiling constant `MAX_HOST_FEE_BPS = 25` (0.25%).
  - `bamboo-evm`: Unified Alloy client bindings for both Base and BNB Chain.
  - `bamboo-solana`: SVM client with PDA derivation and account deserialization.
  - `bamboo-hedging`: Automated 1x delta-neutral short perp execution on Hyperliquid L1 order books.
  - `bamboo-fiat`: Domestic fiat rail adapters for Safaricom M-Pesa, Pix, SEPA Instant, and zkTLS session verifications.
  - `bamboo-relayer`: Axum daemon with in-process 7s RFQ countdown engine, WebSocket feeds, and SQLite WAL double-entry accounting.
* **Spoke 2: Solana SVM Escrow Refactor to Squads v4 Standard**:
  - Re-architected `contracts/solana-escrow/programs/bamboo-escrow` following the **Squads Protocol v4** enterprise pattern.
  - Decomposed monolithic file into modular sub-packages: `constants.rs`, `errors.rs`, `events.rs`, `state/order.rs`, and individual instruction modules (`instructions/lock_order.rs`, `instructions/settle_payout.rs`, `instructions/claim_refund.rs`, `instructions/mod.rs`).
  - Implemented explicit account space calculation (`OrderAccount::LEN`), clear PDA seeds (`SEED_ORDER`, `SEED_VAULT`), and zero-ambiguity client account re-exports.
* **Git Submodule Separation for Contract Spokes**:
  - Decoupled smart contract spokes into 3 dedicated upstream repositories (`alikulaOwen/bamboo-uniswap-hook`, `alikulaOwen/bamboo-solana-escrow`, `alikulaOwen/bamboo-bnb-escrow`).
  - Staged and registered submodules in `.gitmodules` under `contracts/` with isolated toolchains.
* **OpenWiki Knowledge Base**:
  - Standardized all architecture, spoke, relayer, and operational guides under `docs/` using OKF v0.2 metadata and non-destructive sync boundaries.

---

### [2026-09-29] - Pure-Rust SurrealKV, Dynamic MoR Compliance, and Bidirectional Rails
* **Author**: Antigravity Protocol Engineer
* **Scope**: `crates/bamboo-relayer`, `crates/bamboo-fiat`, `crates/bamboo-solana`, Architecture Decision Records (ADRs)

#### Added & Upgraded
* **Embedded SurrealDB Engine (`crates/bamboo-relayer`)**:
  - Upgraded relayer storage layer to pure-Rust Embedded SurrealKV (`surrealdb` with `features = ["kv-surrealkv"]`), removing C-binding dependencies.
  - Implemented ACID double-entry bookkeeping with pre-seeded Chart of Accounts (`ESCROW_RECEIVABLE_USDC`, `FIAT_FLOAT_KES`, `FIAT_FLOAT_BRL`, `FIAT_FLOAT_EUR`, `HOT_CRYPTO_TREASURY_USDC`, `FEE_REVENUE_USDC`, `DISPUTE_WRITEOFF_KES`).
  - Implemented bidirectional order state machine (`FlowDirection::OnRamp` and `FlowDirection::OffRamp`) across 7 order states (`QUOTE_PENDING` $\rightarrow$ `LOCKED` $\rightarrow$ `PROCESSING` $\rightarrow$ `SETTLED` | `DISPUTED` | `REFUNDED`).
  - Replaced Redis with native `LIVE SELECT` changefeed streaming for real-time WebSocket order subscriptions (`/orders/stream`).
* **Dynamic Merchant of Record (MoR) & Compliance Engine (`crates/bamboo-relayer/src/jurisdiction.rs`)**:
  - Added localized corporate subsidiary ring-fencing (`BambooKenyaLtd`, `BambooEuropeCasp`, `BambooBrazilLtda`, `RegulatedPartnerUs`).
  - Built 3-Tier risk verification matrix: Tier 0 (< $150, phone OTP), Tier 1 ($150-$1,000, document KYC/liveness), Tier 2 (> $1,000, EDD/source of funds).
  - Built automated pre-lock KYT wallet screening and sanctioned territory geofencing.
* **Bidirectional Liquidity Rails & YellowCard OTC (`crates/bamboo-fiat`)**:
  - Refactored `LiquidityRailAdapter` into a polymorphic trait with `RailDirection::Ingress` (on-ramp) and `RailDirection::Egress` (off-ramp).
  - Implemented Safaricom Daraja C2B STK Push for ingress and B2C disbursement for egress.
  - Added Pix dynamic QR code generation (ingress) and SPI Key transfers (egress).
  - Added SEPA Instant ISO 20022 `camt.054` credit notifications and `pain.001` payment initiations.
  - Added `YellowCardOtcAdapter` for institutional OTC on/off-ramping across 20+ African jurisdictions.
* **Solana Client Enhancement (`crates/bamboo-solana`)**:
  - Implemented full Anchor SVM instruction builders and account parsers for `lock_order`, `settle_payout`, and `claim_refund` instructions with PDA validation.
* **Architecture Decision Records**:
  - Added [ADR 0004 (SurrealKV)](./decisions/0004-relayer-zero-cloud-surrealkv.md) superseding SQLite WAL.
  - Added [ADR 0006 (MoR Routing & Bidirectional Rails)](./decisions/0006-jurisdiction-mor-and-bidirectional-liquidity.md).
  - Published comprehensive 11-part Bamboo v1 Implementation Plan under `docs/plan/`.

---

### [2026-09-30] - Documentation Refinement, Invariant Alignment, and Verification
* **Author**: Antigravity Assistant
* **Scope**: `README.md`, `docs/`, Wiki Navigation, Invariant Verification

#### Refined
* Replaced root `README.md` stub with comprehensive architecture guide, repository tree, invariants, and build quickstart.
* Synchronized `docs/relayer/unified-host-daemon.md` and `docs/fiat/fiat-adapters-and-zktls.md` to reflect SurrealKV, MoR routing, bidirectional ingress/egress, and YellowCard OTC.
* Verified that all 21 unit tests across all workspace crates (`bamboo-core`, `bamboo-evm`, `bamboo-fiat`, `bamboo-hedging`, `bamboo-relayer`, `bamboo-solana`) compile cleanly and pass with 0 failures.
<!-- OPENWIKI:END -->
