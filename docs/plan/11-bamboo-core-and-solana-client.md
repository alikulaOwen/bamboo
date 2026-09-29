---
okf_version: "0.2"
title: "Bamboo Core & Solana Client Specification"
description: "Specification for bamboo-core canonical preimage order-ID derivation, shared state types, and bamboo-solana client decoding."
type: "plan"
tags:
  - plan
  - core
  - solana-client
  - order-id
  - hashing
categories:
  - Plan
resource: "crates/bamboo-core/src/lib.rs"
generated:
  by: "openwiki/0.2"
  at: "2026-09-29T07:30:00Z"
---

<!-- OPENWIKI:START -->
# Component Specification: Bamboo Core & Solana Client

## 1. Purpose & Responsibilities

`bamboo-core` defines protocol-wide canonical data types, cryptographic hashing functions, and state transitions shared across all off-chain daemons. `bamboo-solana` provides a client library for deriving program PDAs, decoding Anchor account byte layouts, and ingesting on-chain events.

### Responsibilities
- **Canonical Preimage Specification:** Standardize order ID generation so that off-chain clients, the relayer daemon, and the on-chain Solana smart contract compute identical 32-byte hashes byte-for-byte.
- **Fail-Closed Type Validation:** Enforce protocol fee ceilings ($\le 25\text{ bps}$), bond minimums, and status byte constraints at the serialization boundary.
- **Anchor Account Deserialization:** Parse raw Solana account data buffers, verifying the 8-byte Anchor discriminator before decoding struct fields.
- **PDA & Bump Calculation:** Provide verified, off-curve Program Derived Address generation mirroring Solana’s `Pubkey::find_program_address`.

---

## 2. Shared Types (`bamboo-core`)

```rust
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_HOST_FEE_BPS: u16 = 25; // 0.25% protocol maximum
pub const BPS_DIVISOR: u64 = 10_000;
pub const ANTI_GRIEFING_BOND_LAMPORTS: u64 = 10_000_000; // 0.01 SOL

#[derive(Error, Debug, PartialEq, Eq)]
pub enum BambooCoreError {
    #[error("Host fee ceiling exceeded: {0} bps > {MAX_HOST_FEE_BPS} bps")]
    FeeCeilingExceeded(u16),

    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidStateTransition { from: OrderStatus, to: OrderStatus },

    #[error("Invalid order parameters: {0}")]
    InvalidOrder(String),
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChainEndpoint {
    Solana,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrderStatus {
    Locked = 1,
    SettlementRequested = 2,
    Disputed = 3,
    Finalized = 4,
    Refunded = 5,
}

impl OrderStatus {
    pub fn can_transition_to(&self, next: OrderStatus) -> bool {
        match (self, next) {
            (OrderStatus::Locked, OrderStatus::SettlementRequested) => true,
            (OrderStatus::Locked, OrderStatus::Refunded) => true,
            (OrderStatus::SettlementRequested, OrderStatus::Disputed) => true,
            (OrderStatus::SettlementRequested, OrderStatus::Finalized) => true,
            (OrderStatus::Disputed, OrderStatus::Refunded) => true,
            _ => false,
        }
    }
}
```

---

## 3. Canonical Order ID Derivation

To prevent the cross-language hash mismatch identified in prior implementations (where Solidity used `keccak256(abi.encode(...))` while Rust used SHA-256 over ASCII strings), the v1 canonical order ID derivation is defined strictly as a binary SHA-256 digest over fixed-width, little-endian binary fields.

### 3.1 Preimage Definition
The hashing preimage is exactly **151 bytes**:

$$\text{Preimage} = \text{Domain} \parallel \text{Maker} \parallel \text{TokenMint} \parallel \text{Amount} \parallel \text{FiatHash} \parallel \text{Relayer} \parallel \text{Nonce}$$

| Field | Offset | Length (bytes) | Encoding / Type | Description |
|---|---|---|---|---|
| Domain Separator | `0..15` | 15 | ASCII `b"BAMBOO_ORDER_V1"` | Domain isolation tag |
| `maker` | `15..47` | 32 | Raw 32 bytes (Pubkey) | Maker's Solana public key |
| `token_mint` | `47..79` | 32 | Raw 32 bytes (Pubkey) | SPL Token mint (USDC) |
| `amount` | `79..87` | 8 | `u64` little-endian | Token atomic units (6 decimals) |
| `fiat_recipient_hash` | `87..119` | 32 | Raw 32 bytes | Salted SHA-256 hash of recipient phone |
| `assigned_relayer` | `119..151` | 32 | Raw 32 bytes (Pubkey) | Relayer's Solana public key |
| `nonce` | `151..159` | 8 | `u64` little-endian | Maker sequential order counter |

$$\text{order\_id} = \text{SHA256}(\text{Preimage})$$

### 3.2 Canonical Implementation (`bamboo-core`)
```rust
use sha2::{Digest, Sha256};

pub fn compute_canonical_order_id(
    maker: &[u8; 32],
    token_mint: &[u8; 32],
    amount: u64,
    fiat_recipient_hash: &[u8; 32],
    assigned_relayer: &[u8; 32],
    nonce: u64,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"BAMBOO_ORDER_V1");
    hasher.update(maker);
    hasher.update(token_mint);
    hasher.update(&amount.to_le_bytes());
    hasher.update(fiat_recipient_hash);
    hasher.update(assigned_relayer);
    hasher.update(&nonce.to_le_bytes());
    let mut result = [0u8; 32];
    result.copy_from_slice(&hasher.finalize());
    result
}
```

### 3.3 Test Vectors (for Cross-Platform Verification)

```
Test Vector 1:
  maker:               [0x01; 32]
  token_mint:          [0x02; 32]
  amount:              100_000_000 (100.000000 USDC)
  fiat_recipient_hash: [0x03; 32]
  assigned_relayer:    [0x04; 32]
  nonce:               1
  Expected order_id:   "e5beec6ecdf9f0868eb2a6886e6d1b747353f47e30dce44c35b81a70fe6f6cf0"

Test Vector 2:
  maker:               [0xAA; 32]
  token_mint:          [0xBB; 32]
  amount:              50_000_000 (50.000000 USDC)
  fiat_recipient_hash: [0xCC; 32]
  assigned_relayer:    [0xDD; 32]
  nonce:               42
  Expected order_id:   "18334812a76f254b79b69b92ea1295cbda7be4f74d08bcf13327d5301824ef6e"
```

---

## 4. Solana Client Implementation (`bamboo-solana`)

### 4.1 Order PDA & Vault PDA Derivation
```rust
use curve25519_dalek::edwards::CompressedEdwardsY;
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BambooSolanaError {
    #[error("No off-curve PDA found for bump iterations")]
    PdaDerivationFailed,
    #[error("Account data too short: expected at least {0} bytes, got {1}")]
    BufferTooShort(usize, usize),
    #[error("Anchor account discriminator mismatch")]
    DiscriminatorMismatch,
    #[error("Invalid status byte: {0}")]
    InvalidStatus(u8),
}

pub struct SolanaEscrowClient {
    pub program_id: [u8; 32],
}

impl SolanaEscrowClient {
    pub fn new(program_id: [u8; 32]) -> Self {
        Self { program_id }
    }

    /// Derive Order PDA: seeds = [b"order", maker, nonce.to_le_bytes()]
    pub fn derive_order_pda(
        &self,
        maker: &[u8; 32],
        nonce: u64,
    ) -> Result<([u8; 32], u8), BambooSolanaError> {
        let nonce_bytes = nonce.to_le_bytes();
        for bump in (0..=255u8).rev() {
            let mut hasher = Sha256::new();
            hasher.update(b"order");
            hasher.update(maker);
            hasher.update(&nonce_bytes);
            hasher.update([bump]);
            hasher.update(self.program_id);
            hasher.update(b"ProgramDerivedAddress");
            let hash: [u8; 32] = hasher.finalize().into();
            if CompressedEdwardsY(hash).decompress().is_none() {
                return Ok((hash, bump));
            }
        }
        Err(BambooSolanaError::PdaDerivationFailed)
    }

    /// Derive Vault PDA: seeds = [b"vault", order_pda]
    pub fn derive_vault_pda(
        &self,
        order_pda: &[u8; 32],
    ) -> Result<([u8; 32], u8), BambooSolanaError> {
        for bump in (0..=255u8).rev() {
            let mut hasher = Sha256::new();
            hasher.update(b"vault");
            hasher.update(order_pda);
            hasher.update([bump]);
            hasher.update(self.program_id);
            hasher.update(b"ProgramDerivedAddress");
            let hash: [u8; 32] = hasher.finalize().into();
            if CompressedEdwardsY(hash).decompress().is_none() {
                return Ok((hash, bump));
            }
        }
        Err(BambooSolanaError::PdaDerivationFailed)
    }
}
```

### 4.2 Anchor Discriminator Validation & Deserialization
Anchor prepends an 8-byte discriminator to all account state buffers:
$$\text{Discriminator} = \text{SHA256}(\text{"account:OrderAccount"})[0..8]$$

The client checks this discriminator before decoding to ensure foreign or uninitialized accounts cannot be deserialized as orders:

```rust
pub const ORDER_ACCOUNT_DISCRIMINATOR: [u8; 8] = [134, 173, 223, 185, 33, 82, 159, 133];

impl SolanaEscrowClient {
    pub fn decode_order_account(&self, data: &[u8]) -> Result<OrderAccountData, BambooSolanaError> {
        if data.len() < 8 + OrderAccountData::MIN_LEN {
            return Err(BambooSolanaError::BufferTooShort(8 + OrderAccountData::MIN_LEN, data.len()));
        }
        if data[0..8] != ORDER_ACCOUNT_DISCRIMINATOR {
            return Err(BambooSolanaError::DiscriminatorMismatch);
        }
        OrderAccountData::unpack(&data[8..])
    }
}
```

---

## 5. Isolation Testing

1. **Preimage Stability Test:** Test vectors checked against Anchor program Rust tests to verify cross-crate identity.
2. **PDA Off-Curve Invariant:** Run 100 randomized derivations verifying all resulting addresses fail Edwards decompresion.
3. **Fuzz Parsing:** Verify that random byte inputs to `decode_order_account` safely return `Err` without panicking.

<!-- OPENWIKI:END -->
