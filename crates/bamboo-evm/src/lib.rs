use alloy::primitives::Address;
use alloy::sol;
use bamboo_core::{ChainEndpoint, OrderStatus, UnifiedEscrowOrder};

// Base Uniswap v4 Hook ABI
sol! {
    #[sol(rpc)]
    interface IBambooSettlementHook {
        event OfframpEscrowCreated(
            bytes32 indexed orderId,
            address indexed maker,
            address tokenOut,
            uint256 amount,
            bytes32 fiatRecipientHash,
            uint256 expiry
        );
        function settlePayout(bytes32 orderId, address recipient) external;
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
            address token,
            uint256 amount,
            bytes32 fiatRecipientHash,
            uint256 expiry
        );
        function settlePayout(bytes32 orderId, address recipient) external;
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

    pub fn parse_bsc_lock(log: &IBambooBscSettlement::OrderLocked) -> UnifiedEscrowOrder {
        UnifiedEscrowOrder {
            order_id: log.orderId.0,
            chain: ChainEndpoint::Bsc,
            maker: log.maker.to_string(),
            token_mint_or_address: log.token.to_string(),
            amount: log.amount.to_string().parse().unwrap_or(0),
            fiat_recipient_hash: log.fiatRecipientHash.0,
            assigned_host: String::new(),
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
            assigned_host: String::new(),
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
}
