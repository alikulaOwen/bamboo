---
okf_version: "0.2"
title: "Spoke 2: Solana SVM Anchor Escrow"
description: "Technical specification for bamboo_escrow on Solana: PDA order isolation, 0.01 SOL anti-griefing micro-bonds, and atomic Jupiter CPI routing."
type: "concept"
tags:
  - spoke
  - solana
  - svm
  - anchor
  - pda
  - anti-griefing
categories:
  - Smart Contracts
resource: "contracts/solana-escrow/programs/bamboo-escrow/src/lib.rs"
generated:
  by: "openwiki/0.2"
  at: "2026-09-08T07:58:39Z"
---

<!-- OPENWIKI:START -->
# Spoke 2: Solana SVM (Anchor Escrow)

> [!NOTE]
> **Status: v1 differs — see [docs/plan/](../plan/00-overview.md)**  
> Bamboo v1 uses maker-seeded PDAs `[b"order", maker, nonce]` (not caller-supplied `order_id` alone) to eliminate order squatting, and enforces a challenge window ($W$) before finalization. See [10-solana-escrow.md](../plan/10-solana-escrow.md).

The Solana spoke is implemented using the **Anchor framework** in [`contracts/solana-escrow/programs/bamboo-escrow/src/lib.rs`](../../contracts/solana-escrow/programs/bamboo-escrow/src/lib.rs). It provides micro-second settlement finality and high-frequency order handling on the Solana Sealevel runtime.

```mermaid
graph TD
    subgraph PDA["Order PDA Isolation"]
        O["seeds = [b'order', order_id]"]
        V["Token Vault: seeds = [b'vault', order_id]"]
    end

    Maker["Maker / Off-Ramp User"] -->|lock_order: SPL Tokens| V
    Maker -->|lock_order: 0.01 SOL Micro-Bond| O
    Relayer["bamboo-relayer"] -->|settle_payout: Fiat Proof| O
    O -->|Disburse SPL Payout| Recipient["Designated Recipient Token Account"]
    O -->|Disburse Host Fee (<= 25 bps)| RelayerToken["Relayer Token Account"]
    O -->|Refund 0.01 SOL Bond| Maker
```

---

## Architectural Invariants

### 1. PDA Order Isolation
Every escrow is isolated within its own dedicated Program Derived Address (PDA) derived using seeds:
```rust
#[account(
    init,
    payer = maker,
    space = 8 + OrderAccount::LEN,
    seeds = [b"order", order_id.as_ref()],
    bump
)]
pub order: Account<'info, OrderAccount>,
```
This guarantees complete state isolation and eliminates cross-order re-entrancy vectors.

### 2. Anti-Griefing Micro-Bond (0.01 SOL)
To prevent malicious takers from locking orders and wasting relayer liquidity without intent to settle, the program mandates an anti-griefing micro-bond:
* **Amount**: `10_000_000` lamports (0.01 SOL).
* Held directly inside the order PDA account balance.
* **Release Condition**: Returned in full to the maker upon successful settlement (`settle_payout`) or permissionless expiry refund (`claim_refund`).

### 3. Atomic Multi-Instruction Jupiter CPI Swaps
Because Solana transactions can compose up to dozens of atomic instructions, users can execute:
1. `Jupiter::route_swap` (swaps any SPL token into USDC/USDT)
2. `bamboo_escrow::lock_order` (locks the resulting USDC into escrow)
All inside a single atomic Solana transaction. If either instruction fails, the entire transaction rolls back.

### 4. Squads v4 Standard Code Architecture
The program adheres strictly to the **Squads Protocol v4** enterprise architecture standard:
* **Decoupled Instructions**: Each instruction handler (`lock_order`, `settle_payout`, `claim_refund`) lives in its own dedicated module under `src/instructions/` with explicit `Accounts` validation structs and handler logic.
* **Encapsulated State Layout**: State definitions, discriminators, and space allocations (`OrderAccount::LEN`) reside under `src/state/order.rs` with dedicated helper methods (`calculate_split`, `is_locked`, `is_expired`).
* **Semantic Constants & Errors**: Program seeds (`SEED_ORDER`, `SEED_VAULT`) and fee parameters are centrally defined in `src/constants.rs`; custom error variants are documented in `src/errors.rs`.
* **Zero Ambiguity Exports**: `src/instructions/mod.rs` re-exports all validation structs and client account definitions (`__client_accounts_*`), while `src/lib.rs` acts purely as a thin routing layer.

```text
programs/bamboo-escrow/src/
├── lib.rs              # Anchor entry point & #[program] routing
├── constants.rs        # PDA seeds, fee ceilings, and bond constants
├── errors.rs           # Squads v4 custom error codes
├── events.rs           # On-chain emitted event structs
├── state/
│   ├── mod.rs          # State module re-exports
│   └── order.rs        # OrderAccount layout, space calculation & split logic
└── instructions/
    ├── mod.rs          # Instruction router & client accounts re-exports
    ├── lock_order.rs   # LockOrder accounts & process_lock_order
    ├── settle_payout.rs# SettlePayout accounts & process_settle_payout
    └── claim_refund.rs # ClaimRefund accounts & process_claim_refund
```

### 5. Quarantined Toolchain
To avoid compilation conflicts between host x86_64 Cargo builds and Solana's eBPF/SBF bytecode compiler (`cargo-build-sbf`), the Anchor program lives in an independent workspace ([`contracts/solana-escrow/Cargo.toml`](../../contracts/solana-escrow/Cargo.toml)) and is executed via `anchor build`.
<!-- OPENWIKI:END -->
