// SPDX-License-Identifier: MIT
pragma solidity ^0.8.26;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

/// Uniswap v4 Hooks interface
interface IHooks {
    function beforeSwap(
        address sender,
        bytes32 poolKeyId,
        bytes calldata hookData
    ) external returns (bytes4);

    function afterSwap(
        address sender,
        bytes32 poolKeyId,
        int128 amount0,
        int128 amount1,
        bytes calldata hookData
    ) external returns (bytes4, int128);
}

/// Minimal PoolManager interface for v4 flash accounting
interface IPoolManager {
    function unlock(bytes calldata data) external returns (bytes memory);
    function sync(address currency) external;
    function take(address currency, address to, uint256 amount) external;
    function settle() external payable returns (uint256);
}

contract BambooSettlementHook is IHooks {
    using SafeERC20 for IERC20;

    uint16 public constant MAX_HOST_FEE_BPS = 25; // 0.25% protocol ceiling
    bytes32 private constant TRANSIENT_DELTA_SLOT = 0x23eb6f5aa47c03b35312cd3c44cf3be7f28a7245053499f82ba1188a015ba116;

    enum EscrowStatus { None, Locked, Settled, Refunded }

    struct OfframpOrder {
        address maker;
        address assignedRelayer;
        address tokenOut;
        uint256 amount;
        bytes32 fiatRecipientHash;
        uint256 expiry;
        uint16 hostFeeBps;
        EscrowStatus status;
    }

    IPoolManager public immutable poolManager;
    mapping(bytes32 => OfframpOrder) public orders;

    event OfframpEscrowCreated(
        bytes32 indexed orderId,
        address indexed maker,
        address tokenOut,
        uint256 amount,
        bytes32 fiatRecipientHash,
        uint256 expiry
    );
    event OrderSettled(bytes32 indexed orderId, address relayer, uint256 hostFee);
    event OrderRefunded(bytes32 indexed orderId, address recipient, uint256 amount);

    error UnauthorizedPoolManager();
    error HostFeeCeilingExceeded();
    error OrderAlreadyExists();
    error OrderNotActive();
    error UnauthorizedRelayer();
    error TimelockStillActive();
    error TransferFailed();

    modifier onlyPoolManager() {
        if (msg.sender != address(poolManager)) revert UnauthorizedPoolManager();
        _;
    }

    constructor(address _poolManager) {
        poolManager = IPoolManager(_poolManager);
    }

    /// Transient storage helper: store delta during swap lifecycle
    function _setTransientDelta(int128 delta) internal {
        assembly {
            tstore(TRANSIENT_DELTA_SLOT, delta)
        }
    }

    /// Transient storage helper: load delta during flash accounting
    function _getTransientDelta() internal view returns (int128 delta) {
        assembly {
            delta := tload(TRANSIENT_DELTA_SLOT)
        }
    }

    function beforeSwap(
        address,
        bytes32,
        bytes calldata
    ) external override onlyPoolManager returns (bytes4) {
        // Clear transient delta prior to swap execution
        _setTransientDelta(0);
        return IHooks.beforeSwap.selector;
    }

    /// Captures output deltas via afterSwap using transient storage and flash accounting
    function afterSwap(
        address,
        bytes32,
        int128,
        int128 amount1,
        bytes calldata
    ) external override onlyPoolManager returns (bytes4, int128) {
        // Record output delta into transient storage
        _setTransientDelta(amount1);
        return (IHooks.afterSwap.selector, 0);
    }

    /// Creates and locks an escrow order from swap output or peripheral vault deposit
    function lockOfframpEscrow(
        bytes32 orderId,
        address tokenOut,
        uint256 amount,
        bytes32 fiatRecipientHash,
        address assignedRelayer,
        uint256 duration,
        uint16 hostFeeBps
    ) external {
        if (hostFeeBps > MAX_HOST_FEE_BPS) revert HostFeeCeilingExceeded();
        if (orders[orderId].status != EscrowStatus.None) revert OrderAlreadyExists();

        IERC20(tokenOut).safeTransferFrom(msg.sender, address(this), amount);

        orders[orderId] = OfframpOrder({
            maker: msg.sender,
            assignedRelayer: assignedRelayer,
            tokenOut: tokenOut,
            amount: amount,
            fiatRecipientHash: fiatRecipientHash,
            expiry: block.timestamp + duration,
            hostFeeBps: hostFeeBps,
            status: EscrowStatus.Locked
        });

        emit OfframpEscrowCreated(
            orderId,
            msg.sender,
            tokenOut,
            amount,
            fiatRecipientHash,
            block.timestamp + duration
        );
    }

    /// Relayer triggers settlement release upon fiat payment verification
    function settlePayout(bytes32 orderId, address recipient) external {
        OfframpOrder storage order = orders[orderId];
        if (order.status != EscrowStatus.Locked) revert OrderNotActive();
        if (msg.sender != order.assignedRelayer) revert UnauthorizedRelayer();

        order.status = EscrowStatus.Settled;

        uint256 hostFee = (order.amount * order.hostFeeBps) / 10000;
        uint256 payoutAmount = order.amount - hostFee;

        IERC20(order.tokenOut).safeTransfer(recipient, payoutAmount);
        if (hostFee > 0) {
            IERC20(order.tokenOut).safeTransfer(order.assignedRelayer, hostFee);
        }

        emit OrderSettled(orderId, msg.sender, hostFee);
    }

    /// Permissionless refund after expiration timelock
    function claimRefund(bytes32 orderId) external {
        OfframpOrder storage order = orders[orderId];
        if (order.status != EscrowStatus.Locked) revert OrderNotActive();
        if (block.timestamp <= order.expiry) revert TimelockStillActive();

        order.status = EscrowStatus.Refunded;
        IERC20(order.tokenOut).safeTransfer(order.maker, order.amount);

        emit OrderRefunded(orderId, order.maker, order.amount);
    }
}
