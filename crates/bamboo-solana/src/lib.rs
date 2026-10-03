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

/// Solana Instruction Account Metadata
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountMeta {
    pub pubkey: [u8; 32],
    pub is_signer: bool,
    pub is_writable: bool,
}

/// Compiled Solana Instruction
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledInstruction {
    pub program_id: [u8; 32],
    pub accounts: Vec<AccountMeta>,
    pub data: Vec<u8>,
}

/// Compute Anchor 8-byte instruction discriminator: sha256("global:<name>")[..8]
pub fn compute_instruction_discriminator(name: &str) -> [u8; 8] {
    let mut hasher = Sha256::new();
    hasher.update(b"global:");
    hasher.update(name.as_bytes());
    let result = hasher.finalize();
    let mut disc = [0u8; 8];
    disc.copy_from_slice(&result[..8]);
    disc
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

/// Solana Bamboo Escrow Protocol Configuration Account Representation
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ProtocolConfigAccount {
    pub admin: [u8; 32],
    pub mediator: [u8; 32],
    pub fee_treasury: [u8; 32],
    pub allowed_mint: [u8; 32],
    pub max_host_fee_bps: u16,
    pub paused: bool,
    pub min_order_amount: u64,
    pub max_order_amount: u64,
    pub default_capacity: u64,
    pub default_refill_rate_per_sec: u64,
    pub bump: u8,
}

impl ProtocolConfigAccount {
    pub const LEN: usize = 32 + 32 + 32 + 32 + 2 + 1 + 8 + 8 + 8 + 8 + 1; // 164 bytes

    pub fn from_account_bytes(data: &[u8]) -> Result<Self, BambooSolanaError> {
        let payload = if data.len() >= 8 + Self::LEN {
            &data[8..8 + Self::LEN]
        } else if data.len() >= Self::LEN {
            &data[..Self::LEN]
        } else {
            return Err(BambooSolanaError::Serialization(format!(
                "invalid protocol config byte length: expected {}, got {}",
                Self::LEN,
                data.len()
            )));
        };

        let mut offset = 0;
        let mut admin = [0u8; 32];
        admin.copy_from_slice(&payload[offset..offset + 32]);
        offset += 32;

        let mut mediator = [0u8; 32];
        mediator.copy_from_slice(&payload[offset..offset + 32]);
        offset += 32;

        let mut fee_treasury = [0u8; 32];
        fee_treasury.copy_from_slice(&payload[offset..offset + 32]);
        offset += 32;

        let mut allowed_mint = [0u8; 32];
        allowed_mint.copy_from_slice(&payload[offset..offset + 32]);
        offset += 32;

        let max_host_fee_bps = u16::from_le_bytes(payload[offset..offset + 2].try_into().unwrap());
        offset += 2;

        let paused = payload[offset] != 0;
        offset += 1;

        let min_order_amount = u64::from_le_bytes(payload[offset..offset + 8].try_into().unwrap());
        offset += 8;

        let max_order_amount = u64::from_le_bytes(payload[offset..offset + 8].try_into().unwrap());
        offset += 8;

        let default_capacity = u64::from_le_bytes(payload[offset..offset + 8].try_into().unwrap());
        offset += 8;

        let default_refill_rate_per_sec = u64::from_le_bytes(payload[offset..offset + 8].try_into().unwrap());
        offset += 8;

        let bump = payload[offset];

        Ok(Self {
            admin,
            mediator,
            fee_treasury,
            allowed_mint,
            max_host_fee_bps,
            paused,
            min_order_amount,
            max_order_amount,
            default_capacity,
            default_refill_rate_per_sec,
            bump,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::LEN);
        bytes.extend_from_slice(&self.admin);
        bytes.extend_from_slice(&self.mediator);
        bytes.extend_from_slice(&self.fee_treasury);
        bytes.extend_from_slice(&self.allowed_mint);
        bytes.extend_from_slice(&self.max_host_fee_bps.to_le_bytes());
        bytes.push(if self.paused { 1 } else { 0 });
        bytes.extend_from_slice(&self.min_order_amount.to_le_bytes());
        bytes.extend_from_slice(&self.max_order_amount.to_le_bytes());
        bytes.extend_from_slice(&self.default_capacity.to_le_bytes());
        bytes.extend_from_slice(&self.default_refill_rate_per_sec.to_le_bytes());
        bytes.push(self.bump);
        bytes
    }
}

/// Solana Bamboo Escrow Maker Rate Limiter PDA Account Representation
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct MakerRateLimiterAccount {
    pub maker: [u8; 32],
    pub tokens_remaining: u64,
    pub last_refill_timestamp: i64,
    pub total_orders_locked: u64,
    pub bump: u8,
}

impl MakerRateLimiterAccount {
    pub const LEN: usize = 32 + 8 + 8 + 8 + 1; // 57 bytes

    pub fn from_account_bytes(data: &[u8]) -> Result<Self, BambooSolanaError> {
        let payload = if data.len() >= 8 + Self::LEN {
            &data[8..8 + Self::LEN]
        } else if data.len() >= Self::LEN {
            &data[..Self::LEN]
        } else {
            return Err(BambooSolanaError::Serialization(format!(
                "invalid maker rate limiter byte length: expected {}, got {}",
                Self::LEN,
                data.len()
            )));
        };

        let mut offset = 0;
        let mut maker = [0u8; 32];
        maker.copy_from_slice(&payload[offset..offset + 32]);
        offset += 32;

        let tokens_remaining = u64::from_le_bytes(payload[offset..offset + 8].try_into().unwrap());
        offset += 8;

        let last_refill_timestamp = i64::from_le_bytes(payload[offset..offset + 8].try_into().unwrap());
        offset += 8;

        let total_orders_locked = u64::from_le_bytes(payload[offset..offset + 8].try_into().unwrap());
        offset += 8;

        let bump = payload[offset];

        Ok(Self {
            maker,
            tokens_remaining,
            last_refill_timestamp,
            total_orders_locked,
            bump,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::LEN);
        bytes.extend_from_slice(&self.maker);
        bytes.extend_from_slice(&self.tokens_remaining.to_le_bytes());
        bytes.extend_from_slice(&self.last_refill_timestamp.to_le_bytes());
        bytes.extend_from_slice(&self.total_orders_locked.to_le_bytes());
        bytes.push(self.bump);
        bytes
    }
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

    /// Derive the maker rate limiter PDA:
    /// `sha256(b"rate_limiter" || maker || [bump] || program_id || "ProgramDerivedAddress")`
    pub fn derive_rate_limiter_pda(
        &self,
        maker: &[u8; 32],
    ) -> Result<([u8; 32], u8), BambooSolanaError> {
        for bump in (0..=255u8).rev() {
            let mut hasher = Sha256::new();
            hasher.update(b"rate_limiter");
            hasher.update(maker);
            hasher.update([bump]);
            hasher.update(self.program_id);
            hasher.update(b"ProgramDerivedAddress");
            let hash: [u8; 32] = hasher.finalize().into();
            if CompressedEdwardsY(hash).decompress().is_none() {
                return Ok((hash, bump));
            }
        }
        Err(BambooSolanaError::PdaDerivationError(
            "no off-curve address found for any bump".into(),
        ))
    }

    /// Derive the global protocol config PDA:
    /// `sha256(b"config" || [bump] || program_id || "ProgramDerivedAddress")`
    pub fn derive_config_pda(&self) -> Result<([u8; 32], u8), BambooSolanaError> {
        for bump in (0..=255u8).rev() {
            let mut hasher = Sha256::new();
            hasher.update(b"config");
            hasher.update([bump]);
            hasher.update(self.program_id);
            hasher.update(b"ProgramDerivedAddress");
            let hash: [u8; 32] = hasher.finalize().into();
            if CompressedEdwardsY(hash).decompress().is_none() {
                return Ok((hash, bump));
            }
        }
        Err(BambooSolanaError::PdaDerivationError(
            "no off-curve address found for any bump".into(),
        ))
    }

    /// Derive the maker-seeded order PDA:
    /// `sha256(b"order" || maker || nonce.to_le_bytes() || [bump] || program_id || "ProgramDerivedAddress")`
    pub fn derive_maker_order_pda(
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
        Err(BambooSolanaError::PdaDerivationError(
            "no off-curve address found for maker order PDA".into(),
        ))
    }

    /// Derive the token vault PDA for an order:
    /// `sha256(b"vault" || order_pda || [bump] || program_id || "ProgramDerivedAddress")`
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
        Err(BambooSolanaError::PdaDerivationError(
            "no off-curve address found for vault PDA".into(),
        ))
    }

    /// Build instruction: initialize_maker_rate_limiter
    pub fn build_initialize_maker_rate_limiter_ix(
        &self,
        maker: &[u8; 32],
        system_program: &[u8; 32],
    ) -> Result<CompiledInstruction, BambooSolanaError> {
        let (config_pda, _) = self.derive_config_pda()?;
        let (limiter_pda, _) = self.derive_rate_limiter_pda(maker)?;
        let disc = compute_instruction_discriminator("initialize_maker_rate_limiter");

        Ok(CompiledInstruction {
            program_id: self.program_id,
            accounts: vec![
                AccountMeta { pubkey: *maker, is_signer: true, is_writable: true },
                AccountMeta { pubkey: config_pda, is_signer: false, is_writable: false },
                AccountMeta { pubkey: limiter_pda, is_signer: false, is_writable: true },
                AccountMeta { pubkey: *system_program, is_signer: false, is_writable: false },
            ],
            data: disc.to_vec(),
        })
    }

    /// Build instruction: lock_order
    pub fn build_lock_order_ix(
        &self,
        maker: &[u8; 32],
        assigned_relayer: &[u8; 32],
        token_mint: &[u8; 32],
        maker_token_account: &[u8; 32],
        token_program: &[u8; 32],
        system_program: &[u8; 32],
        nonce: u64,
        amount: u64,
        fiat_recipient_hash: &[u8; 32],
        duration_seconds: i64,
        host_fee_bps: u16,
    ) -> Result<CompiledInstruction, BambooSolanaError> {
        let (config_pda, _) = self.derive_config_pda()?;
        let (limiter_pda, _) = self.derive_rate_limiter_pda(maker)?;
        let (order_pda, _) = self.derive_maker_order_pda(maker, nonce)?;
        let (vault_pda, _) = self.derive_vault_pda(&order_pda)?;

        let disc = compute_instruction_discriminator("lock_order");
        let mut data = Vec::with_capacity(8 + 8 + 8 + 32 + 8 + 2);
        data.extend_from_slice(&disc);
        data.extend_from_slice(&nonce.to_le_bytes());
        data.extend_from_slice(&amount.to_le_bytes());
        data.extend_from_slice(fiat_recipient_hash);
        data.extend_from_slice(&duration_seconds.to_le_bytes());
        data.extend_from_slice(&host_fee_bps.to_le_bytes());

        Ok(CompiledInstruction {
            program_id: self.program_id,
            accounts: vec![
                AccountMeta { pubkey: *maker, is_signer: true, is_writable: true },
                AccountMeta { pubkey: *assigned_relayer, is_signer: false, is_writable: false },
                AccountMeta { pubkey: config_pda, is_signer: false, is_writable: false },
                AccountMeta { pubkey: limiter_pda, is_signer: false, is_writable: true },
                AccountMeta { pubkey: *token_mint, is_signer: false, is_writable: false },
                AccountMeta { pubkey: order_pda, is_signer: false, is_writable: true },
                AccountMeta { pubkey: vault_pda, is_signer: false, is_writable: true },
                AccountMeta { pubkey: *maker_token_account, is_signer: false, is_writable: true },
                AccountMeta { pubkey: *token_program, is_signer: false, is_writable: false },
                AccountMeta { pubkey: *system_program, is_signer: false, is_writable: false },
            ],
            data,
        })
    }

    /// Build instruction: request_settlement
    pub fn build_request_settlement_ix(
        &self,
        assigned_relayer: &[u8; 32],
        order_pda: &[u8; 32],
        mpesa_receipt: &[u8; 32],
    ) -> CompiledInstruction {
        let disc = compute_instruction_discriminator("request_settlement");
        let mut data = Vec::with_capacity(8 + 32);
        data.extend_from_slice(&disc);
        data.extend_from_slice(mpesa_receipt);

        CompiledInstruction {
            program_id: self.program_id,
            accounts: vec![
                AccountMeta { pubkey: *assigned_relayer, is_signer: true, is_writable: false },
                AccountMeta { pubkey: *order_pda, is_signer: false, is_writable: true },
            ],
            data,
        }
    }

    /// Build instruction: finalize (permissionless)
    pub fn build_finalize_ix(
        &self,
        caller: &[u8; 32],
        maker: &[u8; 32],
        order_pda: &[u8; 32],
        vault_pda: &[u8; 32],
        relayer_token_account: &[u8; 32],
        fee_treasury_token_account: &[u8; 32],
        token_program: &[u8; 32],
    ) -> Result<CompiledInstruction, BambooSolanaError> {
        let (config_pda, _) = self.derive_config_pda()?;
        let disc = compute_instruction_discriminator("finalize");

        Ok(CompiledInstruction {
            program_id: self.program_id,
            accounts: vec![
                AccountMeta { pubkey: *caller, is_signer: true, is_writable: false },
                AccountMeta { pubkey: *maker, is_signer: false, is_writable: true },
                AccountMeta { pubkey: config_pda, is_signer: false, is_writable: false },
                AccountMeta { pubkey: *order_pda, is_signer: false, is_writable: true },
                AccountMeta { pubkey: *vault_pda, is_signer: false, is_writable: true },
                AccountMeta { pubkey: *relayer_token_account, is_signer: false, is_writable: true },
                AccountMeta { pubkey: *fee_treasury_token_account, is_signer: false, is_writable: true },
                AccountMeta { pubkey: *token_program, is_signer: false, is_writable: false },
            ],
            data: disc.to_vec(),
        })
    }

    /// Build instruction: claim_refund (permissionless)
    pub fn build_claim_refund_ix(
        &self,
        caller: &[u8; 32],
        maker: &[u8; 32],
        order_pda: &[u8; 32],
        vault_pda: &[u8; 32],
        maker_token_account: &[u8; 32],
        token_program: &[u8; 32],
    ) -> CompiledInstruction {
        let disc = compute_instruction_discriminator("claim_refund");

        CompiledInstruction {
            program_id: self.program_id,
            accounts: vec![
                AccountMeta { pubkey: *caller, is_signer: true, is_writable: false },
                AccountMeta { pubkey: *maker, is_signer: false, is_writable: true },
                AccountMeta { pubkey: *order_pda, is_signer: false, is_writable: true },
                AccountMeta { pubkey: *vault_pda, is_signer: false, is_writable: true },
                AccountMeta { pubkey: *maker_token_account, is_signer: false, is_writable: true },
                AccountMeta { pubkey: *token_program, is_signer: false, is_writable: false },
            ],
            data: disc.to_vec(),
        }
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
        a.host_fee_bps = 101;
        assert!(client.to_unified_order(&a).is_err());
        a.host_fee_bps = 100;
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

    #[test]
    fn protocol_config_account_roundtrip() {
        let cfg = ProtocolConfigAccount {
            admin: [1u8; 32],
            mediator: [2u8; 32],
            fee_treasury: [3u8; 32],
            allowed_mint: [4u8; 32],
            max_host_fee_bps: 75,
            paused: false,
            min_order_amount: 1_000_000,
            max_order_amount: 50_000_000_000,
            default_capacity: 5,
            default_refill_rate_per_sec: 1,
            bump: 253,
        };
        let bytes = cfg.to_bytes();
        assert_eq!(bytes.len(), ProtocolConfigAccount::LEN);
        assert_eq!(ProtocolConfigAccount::LEN, 164);

        let parsed = ProtocolConfigAccount::from_account_bytes(&bytes).unwrap();
        assert_eq!(parsed, cfg);
    }

    #[test]
    fn maker_rate_limiter_account_roundtrip() {
        let limiter = MakerRateLimiterAccount {
            maker: [11u8; 32],
            tokens_remaining: 4,
            last_refill_timestamp: 1710000000,
            total_orders_locked: 3,
            bump: 251,
        };
        let bytes = limiter.to_bytes();
        assert_eq!(bytes.len(), MakerRateLimiterAccount::LEN);
        assert_eq!(MakerRateLimiterAccount::LEN, 57);

        let parsed = MakerRateLimiterAccount::from_account_bytes(&bytes).unwrap();
        assert_eq!(parsed, limiter);
    }

    #[test]
    fn derive_rate_limiter_pda_is_off_curve() {
        let client = SolanaEscrowClient::new([9u8; 32]);
        let (pda, _bump) = client.derive_rate_limiter_pda(&[42u8; 32]).unwrap();
        assert!(CompressedEdwardsY(pda).decompress().is_none());
    }

    #[test]
    fn test_instruction_builders() {
        let client = SolanaEscrowClient::new([9u8; 32]);
        let maker = [1u8; 32];
        let relayer = [2u8; 32];
        let mint = [3u8; 32];
        let maker_token_acct = [4u8; 32];
        let token_program = [5u8; 32];
        let system_program = [6u8; 32];
        let fiat_hash = [7u8; 32];

        // 1. Initialize maker rate limiter instruction
        let init_limiter_ix = client.build_initialize_maker_rate_limiter_ix(&maker, &system_program).unwrap();
        assert_eq!(init_limiter_ix.program_id, [9u8; 32]);
        assert_eq!(init_limiter_ix.accounts.len(), 4);
        assert_eq!(init_limiter_ix.data[..8], compute_instruction_discriminator("initialize_maker_rate_limiter"));

        // 2. Lock order instruction
        let lock_ix = client.build_lock_order_ix(
            &maker,
            &relayer,
            &mint,
            &maker_token_acct,
            &token_program,
            &system_program,
            1,
            50_000_000,
            &fiat_hash,
            3600,
            50,
        ).unwrap();
        assert_eq!(lock_ix.accounts.len(), 10);
        assert_eq!(lock_ix.data[..8], compute_instruction_discriminator("lock_order"));

        // 3. Request settlement instruction
        let req_ix = client.build_request_settlement_ix(&relayer, &lock_ix.accounts[5].pubkey, &[8u8; 32]);
        assert_eq!(req_ix.accounts.len(), 2);
        assert_eq!(req_ix.data[..8], compute_instruction_discriminator("request_settlement"));

        // 4. Finalize instruction
        let fin_ix = client.build_finalize_ix(
            &relayer,
            &maker,
            &lock_ix.accounts[5].pubkey,
            &lock_ix.accounts[6].pubkey,
            &relayer,
            &[10u8; 32],
            &token_program,
        ).unwrap();
        assert_eq!(fin_ix.accounts.len(), 8);
        assert_eq!(fin_ix.data[..8], compute_instruction_discriminator("finalize"));

        // 5. Claim refund instruction
        let refund_ix = client.build_claim_refund_ix(
            &maker,
            &maker,
            &lock_ix.accounts[5].pubkey,
            &lock_ix.accounts[6].pubkey,
            &maker_token_acct,
            &token_program,
        );
        assert_eq!(refund_ix.accounts.len(), 6);
        assert_eq!(refund_ix.data[..8], compute_instruction_discriminator("claim_refund"));
    }
}
