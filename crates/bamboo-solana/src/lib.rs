use bamboo_core::{ChainEndpoint, OrderStatus, UnifiedEscrowOrder, MAX_HOST_FEE_BPS};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const ANTI_GRIEFING_BOND_LAMPORTS: u64 = 10_000_000; // 0.01 SOL anti-griefing micro-bond

#[derive(Error, Debug)]
pub enum BambooSolanaError {
    #[error("Invalid PDA derivation: {0}")]
    PdaDerivationError(String),

    #[error("Anti-griefing bond insufficient: required {ANTI_GRIEFING_BOND_LAMPORTS}, got {0}")]
    InsufficientBond(u64),

    #[error("Host fee ceiling exceeded: {0} bps > {MAX_HOST_FEE_BPS} bps")]
    FeeCeilingExceeded(u16),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Solana Bamboo Escrow On-Chain Account Representation
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct SolanaEscrowAccount {
    pub order_id: [u8; 32],
    pub maker: [u8; 32],
    pub assigned_relayer: [u8; 32],
    pub token_mint: [u8; 32],
    pub token_vault: [u8; 32],
    pub amount: u64,
    pub fiat_recipient_hash: [u8; 32],
    pub anti_griefing_bond: u64,
    pub expiry_timestamp: i64,
    pub host_fee_bps: u16,
    pub status: u8, // 0 = None, 1 = Locked, 2 = Settled, 3 = Refunded
    pub bump: u8,
}

#[derive(Debug, Clone)]
pub struct SolanaEscrowClient {
    pub program_id: [u8; 32],
}

impl SolanaEscrowClient {
    pub fn new(program_id: [u8; 32]) -> Self {
        Self { program_id }
    }

    /// Derive order PDA: seeds = [b"order", order_id]
    pub fn derive_order_pda(&self, order_id: &[u8; 32]) -> ([u8; 32], u8) {
        let mut hasher = Sha256::new();
        hasher.update(b"order");
        hasher.update(order_id);
        hasher.update(&self.program_id);
        let hash = hasher.finalize();
        let mut pda = [0u8; 32];
        pda.copy_from_slice(&hash);
        let bump = pda[31];
        (pda, bump)
    }

    /// Convert on-chain account to canonical UnifiedEscrowOrder
    pub fn to_unified_order(
        &self,
        account: &SolanaEscrowAccount,
    ) -> Result<UnifiedEscrowOrder, BambooSolanaError> {
        let status = match account.status {
            1 => OrderStatus::Locked,
            2 => OrderStatus::Settled,
            3 => OrderStatus::Refunded,
            _ => OrderStatus::Open,
        };

        Ok(UnifiedEscrowOrder {
            order_id: account.order_id,
            chain: ChainEndpoint::Solana,
            maker: hex::encode(account.maker),
            token_mint_or_address: hex::encode(account.token_mint),
            amount: account.amount,
            fiat_recipient_hash: account.fiat_recipient_hash,
            assigned_host: hex::encode(account.assigned_relayer),
            host_fee_bps: account.host_fee_bps,
            expiry_timestamp: account.expiry_timestamp,
            status,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pda_derivation() {
        let client = SolanaEscrowClient::new([7u8; 32]);
        let order_id = [42u8; 32];
        let (pda, _bump) = client.derive_order_pda(&order_id);
        assert_ne!(pda, [0u8; 32]);
    }

    #[test]
    fn test_to_unified_order() {
        let client = SolanaEscrowClient::new([7u8; 32]);
        let acc = SolanaEscrowAccount {
            order_id: [1u8; 32],
            maker: [2u8; 32],
            assigned_relayer: [3u8; 32],
            token_mint: [4u8; 32],
            token_vault: [5u8; 32],
            amount: 50_000_000,
            fiat_recipient_hash: [6u8; 32],
            anti_griefing_bond: ANTI_GRIEFING_BOND_LAMPORTS,
            expiry_timestamp: 1780000000,
            host_fee_bps: 20,
            status: 1,
            bump: 254,
        };
        let unified = client.to_unified_order(&acc).unwrap();
        assert_eq!(unified.chain, ChainEndpoint::Solana);
        assert_eq!(unified.status, OrderStatus::Locked);
        assert_eq!(unified.host_fee_bps, 20);
    }
}
