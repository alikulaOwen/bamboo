use alloy::primitives::{Address, B256, U256};
use alloy::sol;
use bamboo_core::{ChainEndpoint, OrderStatus, UnifiedEscrowOrder};

// Base Uniswap v4 Hook ABI
sol! {
    #[sol(rpc)]
    interface IBambooSettlementHook {
        event OfframpEscrowCreated(
            bytes32 indexed orderId,
            address indexed maker,
            address indexed assignedRelayer,
            address tokenOut,
            uint256 amount,
            bytes32 fiatRecipientHash,
            uint256 expiry,
            uint64 nonce
        );
        event SettlementRequested(
            bytes32 indexed orderId,
            bytes32 mpesaReceipt,
            uint256 settlementRequestedAt
        );
        event OrderFinalized(
            bytes32 indexed orderId,
            uint256 payout,
            uint256 hostFee
        );
        event OrderDisputed(
            bytes32 indexed orderId,
            uint256 disputedAt,
            uint256 disputeBondAmount
        );
        event DisputeResolved(
            bytes32 indexed orderId,
            bool awardedToRelayer
        );
        event OrderRefunded(
            bytes32 indexed orderId,
            address indexed maker,
            uint256 refundAmount
        );

        function lockOfframpEscrow(
            uint64 nonce,
            address tokenOut,
            uint256 amount,
            bytes32 fiatRecipientHash,
            address assignedRelayer,
            address mediator,
            uint256 duration,
            uint16 hostFeeBps
        ) external payable returns (bytes32 orderId);

        function requestSettlement(bytes32 orderId, bytes32 mpesaReceipt) external;
        function finalize(bytes32 orderId) external;
        function raiseDispute(bytes32 orderId, uint256 disputeBondAmount) external;
        function resolveDisputeMultisig(
            bytes32 orderId,
            bool awardToRelayer,
            bytes calldata sig1,
            bytes calldata sig2
        ) external;
        function claimRefund(bytes32 orderId) external;
    }
}

// BNB Chain Standalone Escrow ABI
sol! {
    #[sol(rpc)]
    interface IBambooBscSettlement {
        event OrderLocked(
            bytes32 indexed orderId,
            address indexed maker,
            address indexed assignedRelayer,
            address token,
            uint256 amount,
            bytes32 fiatRecipientHash,
            uint256 expiry,
            uint64 nonce
        );
        event SettlementRequested(
            bytes32 indexed orderId,
            bytes32 mpesaReceipt,
            uint256 settlementRequestedAt
        );
        event OrderFinalized(
            bytes32 indexed orderId,
            uint256 payout,
            uint256 hostFee
        );
        event OrderDisputed(
            bytes32 indexed orderId,
            uint256 disputedAt,
            uint256 disputeBondAmount
        );
        event DisputeResolved(
            bytes32 indexed orderId,
            bool awardedToRelayer
        );
        event OrderRefunded(
            bytes32 indexed orderId,
            address indexed maker,
            uint256 refundAmount
        );

        function lockOrder(
            uint64 nonce,
            address token,
            uint256 amount,
            bytes32 fiatRecipientHash,
            address assignedRelayer,
            address mediator,
            uint256 duration,
            uint16 hostFeeBps
        ) external payable returns (bytes32 orderId);

        function requestSettlement(bytes32 orderId, bytes32 mpesaReceipt) external;
        function finalize(bytes32 orderId) external;
        function raiseDispute(bytes32 orderId, uint256 disputeBondAmount) external payable;
        function resolveDisputeMultisig(
            bytes32 orderId,
            bool awardToRelayer,
            bytes calldata sig1,
            bytes calldata sig2
        ) external;
        function claimRefund(bytes32 orderId) external;
    }
}

#[derive(Debug, Clone)]
pub struct MultiChainEvmClient {
    pub base_hook_address: Address,
    pub bsc_escrow_address: Address,
}

impl MultiChainEvmClient {
    pub fn new(base_hook: Address, bsc_escrow: Address) -> Self {
        Self {
            base_hook_address: base_hook,
            bsc_escrow_address: bsc_escrow,
        }
    }

    /// Computes deterministic order ID matching on-chain contracts:
    /// keccak256(abi.encodePacked("BAMBOO_ORDER_V1", maker, token, amount, fiatRecipientHash, assignedRelayer, nonce))
    pub fn compute_order_id(
        maker: Address,
        token: Address,
        amount: U256,
        fiat_recipient_hash: B256,
        assigned_relayer: Address,
        nonce: u64,
    ) -> [u8; 32] {
        use alloy::primitives::keccak256;
        let mut packed = Vec::with_capacity(168);
        packed.extend_from_slice(b"BAMBOO_ORDER_V1");
        packed.extend_from_slice(maker.as_slice());
        packed.extend_from_slice(token.as_slice());
        packed.extend_from_slice(&amount.to_be_bytes::<32>());
        packed.extend_from_slice(fiat_recipient_hash.as_slice());
        packed.extend_from_slice(assigned_relayer.as_slice());
        packed.extend_from_slice(&nonce.to_be_bytes());
        keccak256(packed).0
    }

    pub fn parse_bsc_lock(log: &IBambooBscSettlement::OrderLocked) -> UnifiedEscrowOrder {
        UnifiedEscrowOrder {
            order_id: log.orderId.0,
            chain: ChainEndpoint::Bsc,
            maker: log.maker.to_string(),
            token_mint_or_address: log.token.to_string(),
            amount: log.amount.to_string().parse().unwrap_or(0),
            fiat_recipient_hash: log.fiatRecipientHash.0,
            assigned_host: log.assignedRelayer.to_string(),
            host_fee_bps: 0,
            expiry_timestamp: log.expiry.to_string().parse().unwrap_or(0),
            status: OrderStatus::Locked,
        }
    }

    pub fn parse_base_hook_lock(
        log: &IBambooSettlementHook::OfframpEscrowCreated,
    ) -> UnifiedEscrowOrder {
        UnifiedEscrowOrder {
            order_id: log.orderId.0,
            chain: ChainEndpoint::Base,
            maker: log.maker.to_string(),
            token_mint_or_address: log.tokenOut.to_string(),
            amount: log.amount.to_string().parse().unwrap_or(0),
            fiat_recipient_hash: log.fiatRecipientHash.0,
            assigned_host: log.assignedRelayer.to_string(),
            host_fee_bps: 0,
            expiry_timestamp: log.expiry.to_string().parse().unwrap_or(0),
            status: OrderStatus::Locked,
        }
    }

    pub fn target_address_for_chain(&self, chain: ChainEndpoint) -> Option<Address> {
        match chain {
            ChainEndpoint::Base => Some(self.base_hook_address),
            ChainEndpoint::Bsc => Some(self.bsc_escrow_address),
            ChainEndpoint::Solana => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_init() {
        let base_addr = Address::ZERO;
        let bsc_addr = Address::repeat_byte(0xbb);
        let client = MultiChainEvmClient::new(base_addr, bsc_addr);
        assert_eq!(client.target_address_for_chain(ChainEndpoint::Base), Some(base_addr));
        assert_eq!(client.target_address_for_chain(ChainEndpoint::Bsc), Some(bsc_addr));
        assert_eq!(client.target_address_for_chain(ChainEndpoint::Solana), None);
    }

    #[test]
    fn test_compute_order_id_determinism() {
        let maker = Address::repeat_byte(0x11);
        let token = Address::repeat_byte(0x22);
        let amount = U256::from(1_000_000_000u64);
        let fiat_hash = B256::repeat_byte(0x33);
        let relayer = Address::repeat_byte(0x44);
        let nonce = 12345u64;

        let id1 = MultiChainEvmClient::compute_order_id(maker, token, amount, fiat_hash, relayer, nonce);
        let id2 = MultiChainEvmClient::compute_order_id(maker, token, amount, fiat_hash, relayer, nonce);
        assert_eq!(id1, id2);
        assert_ne!(id1, [0u8; 32]);
    }
}
