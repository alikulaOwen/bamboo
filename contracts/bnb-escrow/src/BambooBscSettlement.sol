// SPDX-License-Identifier: MIT
pragma solidity ^0.8.26;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

contract BambooBscSettlement {
    using SafeERC20 for IERC20;

    uint16 public constant MAX_HOST_FEE_BPS = 25; // 0.25% ceiling
    address public constant NATIVE_BNB = address(0);

    enum OrderStatus { None, Locked, Settled, Refunded }

    struct Order {
        address maker;
        address assignedRelayer;
        address token; // address(0) for native BNB
        uint256 amount;
        bytes32 fiatRecipientHash;
        uint256 expiry;
        uint16 hostFeeBps;
        OrderStatus status;
    }

    mapping(bytes32 => Order) public orders;

    event OrderLocked(
        bytes32 indexed orderId,
        address indexed maker,
        address token,
        uint256 amount,
        bytes32 fiatRecipientHash,
        uint256 expiry
    );
    event OrderSettled(bytes32 indexed orderId, address relayer, uint256 hostFee);
    event OrderRefunded(bytes32 indexed orderId, address recipient, uint256 amount);

    error HostFeeCeilingExceeded();
    error OrderAlreadyExists();
    error OrderNotActive();
    error UnauthorizedRelayer();
    error TimelockStillActive();
    error TransferFailed();
    error IncorrectNativeDeposit();

    /// Locks native BNB or BEP-20 into escrow
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

        orders[orderId] = Order({
            maker: msg.sender,
            assignedRelayer: assignedRelayer,
            token: token,
            amount: amount,
            fiatRecipientHash: fiatRecipientHash,
            expiry: block.timestamp + duration,
            hostFeeBps: hostFeeBps,
            status: OrderStatus.Locked
        });

        emit OrderLocked(orderId, msg.sender, token, amount, fiatRecipientHash, block.timestamp + duration);
    }

    /// Committed relayer triggers release upon fiat confirmation
    function settlePayout(bytes32 orderId, address recipient) external {
        Order storage order = orders[orderId];
        if (order.status != OrderStatus.Locked) revert OrderNotActive();
        if (msg.sender != order.assignedRelayer) revert UnauthorizedRelayer();

        order.status = OrderStatus.Settled;

        uint256 hostFee = (order.amount * order.hostFeeBps) / 10000;
        uint256 payoutAmount = order.amount - hostFee;

        if (order.token == NATIVE_BNB) {
            (bool s1, ) = recipient.call{value: payoutAmount}("");
            if (!s1) revert TransferFailed();
            if (hostFee > 0) {
                (bool s2, ) = order.assignedRelayer.call{value: hostFee}("");
                if (!s2) revert TransferFailed();
            }
        } else {
            IERC20(order.token).safeTransfer(recipient, payoutAmount);
            if (hostFee > 0) {
                IERC20(order.token).safeTransfer(order.assignedRelayer, hostFee);
            }
        }

        emit OrderSettled(orderId, msg.sender, hostFee);
    }

    /// Permissionless refund after expiration
    function claimRefund(bytes32 orderId) external {
        Order storage order = orders[orderId];
        if (order.status != OrderStatus.Locked) revert OrderNotActive();
        if (block.timestamp <= order.expiry) revert TimelockStillActive();

        order.status = OrderStatus.Refunded;

        if (order.token == NATIVE_BNB) {
            (bool success, ) = order.maker.call{value: order.amount}("");
            if (!success) revert TransferFailed();
        } else {
            IERC20(order.token).safeTransfer(order.maker, order.amount);
        }

        emit OrderRefunded(orderId, order.maker, order.amount);
    }
}
