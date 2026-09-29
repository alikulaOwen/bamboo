---
okf_version: "0.2"
title: "Project Bamboo Protocol Glossary"
description: "Authoritative dictionary of protocol terms, architectural primitives, and financial mechanics conforming to Open Knowledge Format (OKF v0.2)."
type: "glossary"
tags:
  - glossary
  - terminology
  - definitions
  - protocol
categories:
  - Reference
resource: "crates/bamboo-core/src/lib.rs"
generated:
  by: "openwiki/0.2"
  at: "2026-09-16T12:58:00Z"
---

<!-- OPENWIKI:START -->
# Project Bamboo – Protocol Glossary

This glossary defines the canonical technical, cryptographic, and financial terms used across Project Bamboo’s smart contracts, off-chain relayers, and documentation.

---

### Anti-Griefing Micro-Bond
A refundable security deposit of `0.01 SOL` (`10_000_000` lamports) required by the Solana escrow program ([`bamboo_escrow`](./spokes/solana-svm.md)) upon order creation. The bond is held in the isolated order PDA and automatically returned to the maker upon order settlement or timelock expiry. Its purpose is to make Denial-of-Service (DoS) and order spam economically unviable.

### Clearing Spine
The centralized risk-neutralization layer of Project Bamboo, implemented by [`crates/bamboo-hedging`](./architecture/delta-neutral-hedging.md) on **Hyperliquid L1**. It absorbs the price volatility of collateral during the asynchronous fiat verification window by dispatching automated 1x short perpetual positions.

### Delta-Neutral Hedge ($\Delta = 0$)
A position structure where the net directional price sensitivity of the collateral is exactly zero. When collateral of value $V$ is locked in an escrow spoke, a short perpetual position of identical notional size $-V$ is opened on Hyperliquid. Any gain or loss in the spot price of the collateral is perfectly cancelled by an equal and opposite loss or gain in the perpetual position.

### Escrow Spoke
An execution layer deployed natively on a host blockchain that locks user crypto collateral in non-custodial smart contracts. Bamboo maintains three spokes:
*   **Base Spoke:** A Uniswap v4 Hook ([`BambooSettlementHook.sol`](./spokes/base-uniswap-v4.md)).
*   **Solana Spoke:** An Anchor SVM program ([`bamboo_escrow`](./spokes/solana-svm.md)).
*   **BNB Chain Spoke:** A native BEP-20/BNB escrow ([`BambooBscSettlement.sol`](./spokes/bnb-chain.md)).

### Fiat Recipient Salt Hash (`fiat_recipient_hash`)
A privacy-preserving 32-byte hash representing the recipient's off-chain banking destination (e.g., M-Pesa phone number, Pix key, or IBAN) on public blockchains:
$$\text{fiat\_recipient\_hash} = \text{SHA256}("BAMBOO\_FIAT\_V1" \parallel \text{recipient\_id} \parallel \text{salt})$$
Only parties possessing the private salt can verify the recipient's identity, preventing public surveillance of payment recipients.

### Flash Accounting (Uniswap v4)
A core capability of Uniswap v4 where pool balances are tracked as transient net deltas during a transaction rather than transferring tokens between individual pool contracts on each swap. Bamboo’s Base hook leverages flash accounting to lock swap proceeds directly into escrow without intermediate transfers.

### HyperBFT
The custom, high-performance Byzantine Fault Tolerant consensus algorithm powering Hyperliquid L1. It enables sub-second block times (~200ms) with deterministic instant finality, allowing Bamboo to execute hedges inside its 7-second RFQ window.

### Jupiter CPI
Cross-Program Invocation into the Jupiter DEX aggregator on Solana. It allows users to swap any arbitrary SPL token (e.g., JUP, BONK) into SOL or USDC and lock it into `bamboo_escrow` in a single atomic transaction.

### Order PDA (Program Derived Address)
A deterministic Solana account address derived from the program ID and seed parameters:
$$\text{seeds} = [\text{b"order"}, \text{order\_id}]$$
Because each order occupies its own unique PDA, transactions never compete for write-locks on a shared global state, enabling high-parallelism execution via Solana’s Sealevel runtime.

### Protocol Fee Ceiling (`MAX_HOST_FEE_BPS`)
An immutable on-chain invariant hardcoded across all three smart contract spokes capping relayer compensation at **25 basis points (0.25%)**:
$$\text{host\_fee\_bps} \le 25$$
Any order submitted with a fee higher than 25 bps reverts unconditionally with `HostFeeCeilingExceeded`.

### RFQ Countdown Engine (7 Seconds)
The in-memory pricing engine inside `bamboo-relayer` that delivers guaranteed crypto-to-fiat conversion quotes. Quotes have a deterministic 7-second time-to-live (`RFQ_COUNTDOWN_SECONDS = 7`), protecting relayers from stale pricing while providing sufficient time for makers to sign transactions.

### Squads Protocol v4 Standard
An enterprise-grade smart contract architectural pattern for Solana programs developed by Squads Labs. It mandates decoupled instruction handlers, encapsulated state accounts with deterministic size constants (`LEN`), centralized PDA seeds, and structured error modules. Bamboo’s Solana spoke is engineered strictly to this standard.

### Transient Storage (EIP-1153)
EVM opcodes (`TSTORE` and `TLOAD`) introduced in the Cancun hard fork that store data temporarily during transaction execution and wipe it upon completion. Bamboo’s Base Uniswap v4 hook uses transient storage to pass order parameters between swap hooks at near-zero gas cost.

### Write-Ahead Logging (WAL Mode)
A database concurrency configuration for SQLite used in `bamboo-relayer`:
```sql
PRAGMA journal_mode = WAL;
```
WAL mode allows multiple concurrent readers to query order status without blocking concurrent writes from event listeners, providing ACID double-entry accounting at up to 50,000 operations per second.

### zkTLS (Zero-Knowledge Transport Layer Security)
A cryptographic protocol that allows a web client to prove to a third party that it received specific data over an encrypted HTTPS connection (such as a successful bank transfer receipt from an online banking portal) without revealing the user's private credentials, account numbers, or balances.
<!-- OPENWIKI:END -->
