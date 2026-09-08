use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Protocol fee ceiling: strict host fee ceiling (0.25% protocol ceiling)
pub const MAX_HOST_FEE_BPS: u16 = 25;

/// Basis points divisor (100% = 10,000 bps)
pub const BPS_DIVISOR: u64 = 10_000;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum BambooCoreError {
    #[error("Host fee ceiling exceeded: fee {0} bps > MAX_HOST_FEE_BPS ({MAX_HOST_FEE_BPS} bps)")]
    HostFeeCeilingExceeded(u16),

    #[error("Invalid order parameters: {0}")]
    InvalidOrder(String),

    #[error("Order expired at {expiry}, current time is {current_time}")]
    OrderExpired { expiry: i64, current_time: i64 },

    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidStateTransition {
        from: OrderStatus,
        to: OrderStatus,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChainEndpoint {
    Base,
    Solana,
    Bsc, // BNB Smart Chain Spoke
}

impl std::fmt::Display for ChainEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChainEndpoint::Base => write!(f, "base"),
            ChainEndpoint::Solana => write!(f, "solana"),
            ChainEndpoint::Bsc => write!(f, "bsc"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrderStatus {
    Open,
    Locked,
    FiatProcessing,
    Settled,
    Refunded,
}

impl OrderStatus {
    pub fn can_transition_to(&self, next: OrderStatus) -> bool {
        match (self, next) {
            (OrderStatus::Open, OrderStatus::Locked) => true,
            (OrderStatus::Locked, OrderStatus::FiatProcessing) => true,
            (OrderStatus::Locked, OrderStatus::Settled) => true,
            (OrderStatus::Locked, OrderStatus::Refunded) => true,
            (OrderStatus::FiatProcessing, OrderStatus::Settled) => true,
            (OrderStatus::FiatProcessing, OrderStatus::Refunded) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct UnifiedEscrowOrder {
    pub order_id: [u8; 32],
    pub chain: ChainEndpoint,
    pub maker: String,
    pub token_mint_or_address: String,
    pub amount: u64,
    pub fiat_recipient_hash: [u8; 32],
    pub assigned_host: String,
    pub host_fee_bps: u16,
    pub expiry_timestamp: i64,
    pub status: OrderStatus,
}

impl UnifiedEscrowOrder {
    pub fn validate_fee(&self) -> Result<(), BambooCoreError> {
        if self.host_fee_bps > MAX_HOST_FEE_BPS {
            return Err(BambooCoreError::HostFeeCeilingExceeded(self.host_fee_bps));
        }
        Ok(())
    }

    pub fn calculate_host_fee(&self) -> u64 {
        (self.amount as u128 * self.host_fee_bps as u128 / BPS_DIVISOR as u128) as u64
    }

    pub fn calculate_payout_amount(&self) -> u64 {
        self.amount.saturating_sub(self.calculate_host_fee())
    }

    pub fn transition_to(&mut self, next: OrderStatus) -> Result<(), BambooCoreError> {
        if !self.status.can_transition_to(next) {
            return Err(BambooCoreError::InvalidStateTransition {
                from: self.status,
                to: next,
            });
        }
        self.status = next;
        Ok(())
    }
}

/// Compute canonical 32-byte order ID using standard cryptographic domain hashing
pub fn compute_canonical_order_id(
    chain: ChainEndpoint,
    maker: &str,
    token: &str,
    amount: u64,
    fiat_hash: &[u8; 32],
    nonce: u64,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"BAMBOO_V1_ORDER");
    hasher.update(chain.to_string().as_bytes());
    hasher.update(maker.as_bytes());
    hasher.update(token.as_bytes());
    hasher.update(&amount.to_be_bytes());
    hasher.update(fiat_hash);
    hasher.update(&nonce.to_be_bytes());
    let result = hasher.finalize();
    let mut order_id = [0u8; 32];
    order_id.copy_from_slice(&result);
    order_id
}

/// Compute canonical fiat recipient hash for off-chain destination
pub fn compute_fiat_recipient_hash(recipient_identifier: &str, salt: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"BAMBOO_FIAT_V1");
    hasher.update(recipient_identifier.as_bytes());
    hasher.update(salt);
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fee_ceiling_enforcement() {
        let mut order = UnifiedEscrowOrder {
            order_id: [1u8; 32],
            chain: ChainEndpoint::Bsc,
            maker: "0x123".into(),
            token_mint_or_address: "0x456".into(),
            amount: 1_000_000,
            fiat_recipient_hash: [2u8; 32],
            assigned_host: "0x789".into(),
            host_fee_bps: 25,
            expiry_timestamp: 1800000000,
            status: OrderStatus::Locked,
        };
        assert!(order.validate_fee().is_ok());
        assert_eq!(order.calculate_host_fee(), 2_500); // 0.25% of 1,000,000
        assert_eq!(order.calculate_payout_amount(), 997_500);

        order.host_fee_bps = 26;
        assert!(matches!(
            order.validate_fee(),
            Err(BambooCoreError::HostFeeCeilingExceeded(26))
        ));
    }

    #[test]
    fn test_order_id_determinism() {
        let fiat_hash = compute_fiat_recipient_hash("+254712345678", b"mpesa_salt");
        let id1 = compute_canonical_order_id(ChainEndpoint::Base, "maker_a", "usdc", 100, &fiat_hash, 1);
        let id2 = compute_canonical_order_id(ChainEndpoint::Base, "maker_a", "usdc", 100, &fiat_hash, 1);
        assert_eq!(id1, id2);
    }
}
