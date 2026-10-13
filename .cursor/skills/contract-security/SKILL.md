---
name: contract-security
description: Routes Bamboo smart-contract work through Uniswap v4 hook security and Trail of Bits audit workflows. Use when editing or reviewing BambooSettlementHook, Uniswap v4 hooks, PoolManager, beforeSwap, afterSwap, the Base spoke, Solana escrow, token integration, Slither, Echidna, or any security review under contracts/.
---

# Contract security

Apply this when changing or reviewing Bamboo contracts. It routes work to the official Uniswap AI skills and Trail of Bits skills. Fetch the upstream `SKILL.md` linked in [references.md](references.md) before running a named workflow. If this file and the upstream file disagree, follow the upstream file.

Do not write exploit payloads, malicious hook implementations, or proof-of-concept attacks. Report impact and the fix.

## Route the task

| Task | Workflow |
| --- | --- |
| Edit or review `contracts/uniswap-hook/` | Uniswap v4 hook pass, then the five-step secure workflow |
| Start any contract audit | Entry points, then context, then the platform scanner |
| Review a diff | Differential review of the changed functions only, then re-check invariants those functions touch |
| Integrate an external token | Token-integration pass on every `transfer`, `transferFrom`, and balance read |
| Check code against a spoke doc | Spec-to-code pass; cite both the doc and the contract |
| Review `contracts/solana-escrow/` | Solana six-pattern scan with a full coverage table |
| Call the Uniswap Trading API | Swap-integration skill. Send the optional `X-Agent-Info` header so agent traffic is measured separately |

`llms.txt` (`https://developers.uniswap.org/llms.txt`) is documentation context. It does not replace these workflows.

## Uniswap v4 hook pass

Read `contracts/uniswap-hook/src/BambooSettlementHook.sol` and `docs/spokes/base-uniswap-v4.md` before editing.

Threats to check on every hook change:

1. **Caller.** `beforeSwap` and `afterSwap` revert unless `msg.sender` is the PoolManager. `msg.sender` inside a callback is the PoolManager, never the swapper. The `sender` argument is the router.
2. **Return deltas.** `beforeSwapReturnDelta` can make the PoolManager treat the swap as fully handled while the hook keeps the input. Leave it disabled. `afterSwap` must return a zero unspecified delta unless a documented hook fee takes value and settles it in the same unlock. Delta permissions are critical or high risk.
3. **Accounting.** For every transaction, PoolManager deltas sum to zero. Token movement with the PoolManager is `sync`, then transfer, then `settle`. `take` withdraws.
4. **Permissions.** Enable only callbacks the hook uses. Permissions are bits in the deployed address. Changing them requires a new address mined with `HookMiner`. This contract uses a local `IHooks` interface. Say so when it does not match `v4-core` `BaseHook` permissions.
5. **Transient storage.** `tstore` / `tload` data dies at the end of the transaction. A value written in `beforeSwap` or `afterSwap` must be read by the settlement path in that same transaction. Report a write with no reader.
6. **Tokens.** Measure the received balance. Fee-on-transfer, rebasing, ERC-777 callbacks, pausable, and blocklisted tokens break raw `amount` accounting.
7. **Gas.** `beforeSwap` should stay under 50,000 gas (hard ceiling 150,000). `afterSwap` should stay under 30,000 (hard ceiling 100,000). No unbounded loops and no cold `SSTORE` on the swap path.
8. **Identity.** Do not use `tx.origin` for authorization. Do not store secrets on-chain.

Permission risk, from the Uniswap v4 security foundations skill: `beforeSwapReturnDelta` is critical; `beforeRemoveLiquidity`, `beforeSwap`, `afterSwapReturnDelta`, and both liquidity return-delta flags are high; `beforeAddLiquidity` and `afterSwap` are medium; the rest are low.

Before deployment, confirm: PoolManager checks on every callback, reentrancy protection on external calls, deltas sum to zero, fee-on-transfer handling, no `beforeSwapReturnDelta` unless justified, Foundry unit tests, fuzz tests, and invariant tests. Run `forge test --gas-report` in `contracts/uniswap-hook/` when the hook compiles.

## Bamboo settlement invariants

Check these against the contract. Do not assume the spoke doc matches the code. On a mismatch, report both locations and stop for a decision before changing either side.

- Host fee cannot exceed `MAX_HOST_FEE_BPS`. The spoke doc states a 25 bps ceiling. Compare that sentence to the constant before treating either as correct.
- `lockOfframpEscrow` records the token balance delta, and `computeOrderId` uses that actual amount.
- Order status moves `None → Locked → SettlementRequested → Settled`, or `SettlementRequested → Disputed → Settled`, or `Locked → Refunded`, or `Disputed → Refunded` after `DISPUTE_TIMEOUT`. No path pays principal twice or drops the anti-griefing bond.
- `requestSettlement` is only the assigned relayer, only while `Locked`, and only before `expiry - SETTLEMENT_WINDOW`.
- `finalize` waits out `SETTLEMENT_WINDOW`, pays `amount - hostFee` to the relayer and the fee to `feeTreasury`, then refunds `ANTI_GRIEFING_BOND` to the maker.
- `raiseDispute` is only the maker, only inside the challenge window, and pulls a bond of at least 5% of `amount` measured by balance delta.
- `resolveDisputeMultisig` requires two distinct signers from `{maker, assignedRelayer, mediator}`. The signed message binds `orderId`, `awardToRelayer`, `address(this)`, and `block.chainid`.
- `claimRefund` is permissionless only after `expiry` while `Locked`, or after `DISPUTE_TIMEOUT` while `Disputed`, and returns principal plus any dispute bond and the ETH bond.
- `msg.value` on lock is exactly `ANTI_GRIEFING_BOND`. Duration is within `MIN_DURATION` and `MAX_DURATION`.

## Trail of Bits review loop

Run the steps that match the change. Skip a step only after a search shows it cannot apply, and write that reason down.

1. **Entry points.** List state-changing external functions. Classify each as public, role-restricted, or contract-only. Exclude `view` and `pure`. For Solidity, run `slither . --print entry-points` when Slither is installed. Hook callbacks are contract-only: the expected caller is the PoolManager.
2. **Known issues.** Run Slither when it is installed. Triage each finding. If Slither is missing, say so and continue with manual review. Do not claim a clean scan.
3. **Special features.** This hook is not upgradeable unless the code says it is. It integrates external ERC-20s, so run the token-integration checks. It does not need an ERC conformance check unless the change adds a token implementation.
4. **Properties.** Write the invariants above as Foundry invariant or fuzz tests before calling the review done. Echidna is the Trail of Bits property-based tool when a campaign is in scope.
5. **Manual areas.** Signature replay and missing domain fields, front-running of `finalize` and `claimRefund`, ETH refund failure locking the whole payout, and PoolManager flash-accounting assumptions.
6. **Platform scanner.** Solana programs: report all six patterns even when the verdict is clear or not applicable. The six are arbitrary CPI, improper PDA validation, missing ownership check, missing signer check, sysvar account check, and improper instruction introspection. A missing row is an incomplete scan.

## Findings

Use this shape:

```markdown
## [severity] Title

**Location**: `path:line`
**Evidence**: what the code does
**Impact**: funds, liveness, or spec drift
**Fix**: the change to make, without exploit steps
```

Severity is critical, high, medium, or low. End with a coverage note: which workflow steps ran, which tools were absent, and which invariants were not tested.
