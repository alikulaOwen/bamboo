use bamboo_core::{ChainEndpoint, OrderStatus, UnifiedEscrowOrder, MAX_HOST_FEE_BPS};
use curve25519_dalek::edwards::CompressedEdwardsY;
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

    #[error("Unknown or uninitialised escrow status byte: {0}")]
    InvalidStatus(u8),
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

    /// Derive the order PDA exactly as Solana does:
    /// `sha256(seeds || [bump] || program_id || "ProgramDerivedAddress")`,
    /// iterating bump from 255 down until the hash is OFF the ed25519 curve.
    /// Seeds mirror the Anchor program: `[b"order", order_id]`.
    pub fn derive_order_pda(
        &self,
        order_id: &[u8; 32],
    ) -> Result<([u8; 32], u8), BambooSolanaError> {
        for bump in (0..=255u8).rev() {
            let mut hasher = Sha256::new();
            hasher.update(b"order");
            hasher.update(order_id);
            hasher.update([bump]);
            hasher.update(self.program_id);
            hasher.update(b"ProgramDerivedAddress");
            let hash: [u8; 32] = hasher.finalize().into();
            // A valid PDA must NOT be a valid ed25519 point.
            if CompressedEdwardsY(hash).decompress().is_none() {
                return Ok((hash, bump));
            }
        }
        Err(BambooSolanaError::PdaDerivationError(
            "no off-curve address found for any bump".into(),
        ))
    }

    /// Verify an on-chain account's bump/PDA matches its order_id.
    pub fn verify_account_pda(
        &self,
        account_address: &[u8; 32],
        account: &SolanaEscrowAccount,
    ) -> Result<(), BambooSolanaError> {
        let (pda, bump) = self.derive_order_pda(&account.order_id)?;
        if &pda != account_address || bump != account.bump {
            return Err(BambooSolanaError::PdaDerivationError(
                "account address/bump does not match order_id".into(),
            ));
        }
        Ok(())
    }

    /// Convert on-chain account to canonical UnifiedEscrowOrder
    pub fn to_unified_order(
        &self,
        account: &SolanaEscrowAccount,
    ) -> Result<UnifiedEscrowOrder, BambooSolanaError> {
        // Fail closed: enforce protocol invariants at the conversion boundary.
        if account.host_fee_bps > MAX_HOST_FEE_BPS {
            return Err(BambooSolanaError::FeeCeilingExceeded(account.host_fee_bps));
        }
        if account.anti_griefing_bond < ANTI_GRIEFING_BOND_LAMPORTS {
            return Err(BambooSolanaError::InsufficientBond(account.anti_griefing_bond));
        }

        // 0 = None (empty/default account) must never surface as a live order.
        let status = match account.status {
            1 => OrderStatus::Locked,
            2 => OrderStatus::Settled,
            3 => OrderStatus::Refunded,
            other => return Err(BambooSolanaError::InvalidStatus(other)),
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

    fn hex32(s: &str) -> [u8; 32] {
        let mut out = [0u8; 32];
        hex::decode_to_slice(s, &mut out).unwrap();
        out
    }

    fn account() -> SolanaEscrowAccount {
        SolanaEscrowAccount {
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
        }
    }

    /// Vectors generated with `solders.Pubkey.find_program_address([b"order", order_id], program_id)`
    #[test]
    fn pda_matches_solana_known_vectors() {
        let cases = [
            (
                [7u8; 32],
                [42u8; 32],
                "4f176f543857b217ebf579be498d20b6406d46e6ce881de0ad925a7d9c979a9f",
                253u8,
            ),
            (
                [1u8; 32],
                [0u8; 32],
                "edea7a3fad9026c1db69743ca98ff930fa586bc116b7166b078f2f41146f00cc",
                254u8,
            ),
        ];
        for (pid, oid, expected, bump) in cases {
            let (pda, b) = SolanaEscrowClient::new(pid).derive_order_pda(&oid).unwrap();
            assert_eq!(pda, hex32(expected));
            assert_eq!(b, bump);
        }
        let mut oid = [0u8; 32];
        for (i, b) in oid.iter_mut().enumerate() {
            *b = i as u8;
        }
        let (pda, b) = SolanaEscrowClient::new([255u8; 32])
            .derive_order_pda(&oid)
            .unwrap();
        assert_eq!(
            pda,
            hex32("070aa377f091be1e878a2ae484ae2254f90925071c0abbb0d9b24bb60e548afb")
        );
        assert_eq!(b, 254);
    }

    #[test]
    fn pda_is_off_curve() {
        let c = SolanaEscrowClient::new([9u8; 32]);
        for i in 0..64u8 {
            let (pda, _) = c.derive_order_pda(&[i; 32]).unwrap();
            assert!(CompressedEdwardsY(pda).decompress().is_none());
        }
    }

    #[test]
    fn converts_valid_account() {
        let client = SolanaEscrowClient::new([7u8; 32]);
        let u = client.to_unified_order(&account()).unwrap();
        assert_eq!(u.chain, ChainEndpoint::Solana);
        assert_eq!(u.status, OrderStatus::Locked);
        assert_eq!(u.host_fee_bps, 20);
    }

    #[test]
    fn rejects_fee_above_ceiling() {
        let client = SolanaEscrowClient::new([7u8; 32]);
        let mut a = account();
        a.host_fee_bps = 65_535;
        assert!(matches!(
            client.to_unified_order(&a),
            Err(BambooSolanaError::FeeCeilingExceeded(65_535))
        ));
        a.host_fee_bps = 26;
        assert!(client.to_unified_order(&a).is_err());
        a.host_fee_bps = 25;
        assert!(client.to_unified_order(&a).is_ok());
    }

    #[test]
    fn rejects_insufficient_bond() {
        let client = SolanaEscrowClient::new([7u8; 32]);
        let mut a = account();
        a.anti_griefing_bond = ANTI_GRIEFING_BOND_LAMPORTS - 1;
        assert!(matches!(
            client.to_unified_order(&a),
            Err(BambooSolanaError::InsufficientBond(_))
        ));
    }

    #[test]
    fn status_zero_and_unknown_are_not_open() {
        let client = SolanaEscrowClient::new([7u8; 32]);
        for bad in [0u8, 4, 255] {
            let mut a = account();
            a.status = bad;
            assert!(matches!(
                client.to_unified_order(&a),
                Err(BambooSolanaError::InvalidStatus(_))
            ));
        }
    }

    #[test]
    fn verify_account_pda_checks_address_and_bump() {
        let client = SolanaEscrowClient::new([7u8; 32]);
        let mut a = account();
        let (pda, bump) = client.derive_order_pda(&a.order_id).unwrap();
        a.bump = bump;
        assert!(client.verify_account_pda(&pda, &a).is_ok());
        assert!(client.verify_account_pda(&[0u8; 32], &a).is_err());
        a.bump = bump.wrapping_sub(1);
        assert!(client.verify_account_pda(&pda, &a).is_err());
    }
}
