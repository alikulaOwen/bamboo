---
okf_version: "0.2"
title: "Spoke 3: BNB Chain Native EVM Settlement"
description: "Technical specification for BambooBscSettlement.sol on BNB Smart Chain: native BNB and BEP-20 escrow, high-velocity retail off-ramping, and PancakeSwap routing."
type: "concept"
tags:
  - spoke
  - bnb-chain
  - bsc
  - bep-20
  - retail
  - non-custodial
categories:
  - Smart Contracts
resource: "contracts/bnb-escrow/src/BambooBscSettlement.sol"
generated:
  by: "openwiki/0.2"
  at: "2026-09-08T07:58:39Z"
---

<!-- OPENWIKI:START -->
# Spoke 3: BNB Chain Native EVM (`contracts/bnb-escrow/`)

Deployed as a post-stabilization expansion, Spoke 3 brings non-custodial settlement directly to **BNB Smart Chain (BSC)** via [`contracts/bnb-escrow/src/BambooBscSettlement.sol`](../../contracts/bnb-escrow/src/BambooBscSettlement.sol).

Unlike the Base spoke (which hooks into Uniswap v4's pool lifecycle), the BNB Chain spoke operates as a **high-throughput standalone non-custodial escrow** optimized for fast, low-cost retail peer-to-peer markets in Latin America, Southeast Asia, and Africa.

```mermaid
graph LR
    subgraph BSC["BNB Smart Chain (EVM)"]
        Order["BambooBscSettlement.sol"]
        BNBVault["Native BNB (address 0)"]
        BEP20Vault["BEP-20 Vault (USDC/USDT)"]
    end

    User["Maker (P2P Retail User)"] -->|lockOrder: Native BNB| BNBVault
    User -->|lockOrder: BEP-20 Tokens| BEP20Vault
    Relayer["bamboo-relayer"] -->|settlePayout: Fiat Confirmed| Order
    Order -->|Disburse Payout| Recipient["Local Fiat Provider / Recipient"]
    Order -->|Disburse Host Fee (<= 25 bps)| Relayer
    User -.->|claimRefund after expiry| Order
```

---

## Architectural Invariants

### 1. Dual Native and BEP-20 Handling
The escrow natively accepts both BNB (`address(0)`) and any standard BEP-20 token (such as USDT, USDC, FDUSD) in a single unified entry point:
```solidity
address public constant NATIVE_BNB = address(0);

function lockOrder(
    bytes32 orderId,
    address token,
    uint256 amount,
    bytes32 fiatRecipientHash,
    address assignedRelayer,
    uint256 duration,
    uint16 hostFeeBps
) external payable {
    if (hostFeeBps > MAX_HOST_FEE_BPS) revert HostFeeCeilingExceeded();
    if (orders[orderId].status != OrderStatus.None) revert OrderAlreadyExists();

    if (token == NATIVE_BNB) {
        if (msg.value != amount) revert IncorrectNativeDeposit();
    } else {
        IERC20(token).safeTransferFrom(msg.sender, address(this), amount);
    }
    // ...
}
```

### 2. Elimination of Cross-Chain Bridge Overhead
By deploying directly on BSC mainnet, retail users do not need to wrap assets or bridge across chains via LayerZero, Wormhole, or centralized bridges. Local merchants and P2P off-rampers settle instantly with low BSC gas fees (~$0.02).

### 3. Canonical Schema Parity
Spoke 3 reuses the canonical 32-byte order ID and fiat payload hashing schemes defined in `bamboo-core`. Both Base and BSC events emit standard 32-byte signatures parsed by `bamboo-evm`:
```solidity
event OrderLocked(
    bytes32 indexed orderId,
    address indexed maker,
    address token,
    uint256 amount,
    bytes32 fiatRecipientHash,
    uint256 expiry
);
```

### 4. Protocol Fee Ceiling
`MAX_HOST_FEE_BPS = 25` (0.25%) is strictly enforced at the contract level. Relayers cannot settle or lock with higher fees.
<!-- OPENWIKI:END -->
