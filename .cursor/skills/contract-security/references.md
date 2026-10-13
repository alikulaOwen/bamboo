# Upstream skills

Fetch the raw file and follow it. URLs point at `main`; if a path 404s, use the tree in the source repo.

## Uniswap AI

Source: [Uniswap/uniswap-ai](https://github.com/Uniswap/uniswap-ai) (MIT). Overview: https://developers.uniswap.org/docs/uniswap-ai/overview

Install the set with `npx skills add Uniswap/uniswap-ai` when the user wants the official skills on disk. This router does not require that install.

| Skill | Use when | Raw SKILL.md |
| --- | --- | --- |
| `v4-security-foundations` | Any v4 hook edit, permission, callback, or audit | https://raw.githubusercontent.com/Uniswap/uniswap-ai/main/packages/plugins/uniswap-hooks/skills/v4-security-foundations/SKILL.md |
| `v4-hook-generator` | Scaffolding a new hook | https://raw.githubusercontent.com/Uniswap/uniswap-ai/main/packages/plugins/uniswap-hooks/skills/v4-hook-generator/SKILL.md |
| `swap-integration` | Uniswap API, Universal Router, or direct swap calls | https://raw.githubusercontent.com/Uniswap/uniswap-ai/main/packages/plugins/uniswap-trading/skills/swap-integration/SKILL.md |
| `v4-sdk-integration` | v4 SDK calls against a deployed hook | https://raw.githubusercontent.com/Uniswap/uniswap-ai/main/packages/plugins/uniswap-trading/skills/v4-sdk-integration/SKILL.md |
| `viem-integration` | viem or wagmi clients | https://raw.githubusercontent.com/Uniswap/uniswap-ai/main/packages/plugins/uniswap-viem/skills/viem-integration/SKILL.md |

Other published Uniswap skills (`configurator`, `deployer`, `swap-planner`, `liquidity-planner`, `dca-bot`, `index-bot`, `copy-trade`, `pay-with-any-token`) are out of scope for the settlement hook unless the user asks for that product.

After generating a hook, run `v4-security-foundations` before any network deploy.

## Trail of Bits

Source: [trailofbits/skills](https://github.com/trailofbits/skills) (CC BY-SA 4.0).

| Skill | Use when | Raw SKILL.md |
| --- | --- | --- |
| `entry-point-analyzer` | First pass of an audit. State-changing functions only | https://raw.githubusercontent.com/trailofbits/skills/main/plugins/entry-point-analyzer/skills/entry-point-analyzer/SKILL.md |
| `secure-workflow-guide` | End-to-end check: Slither, special features, properties, manual review | https://raw.githubusercontent.com/trailofbits/skills/main/plugins/building-secure-contracts/skills/secure-workflow-guide/SKILL.md |
| `token-integration-analyzer` | Any external ERC-20 | https://raw.githubusercontent.com/trailofbits/skills/main/plugins/building-secure-contracts/skills/token-integration-analyzer/SKILL.md |
| `solana-vulnerability-scanner` | `contracts/solana-escrow/` | https://raw.githubusercontent.com/trailofbits/skills/main/plugins/building-secure-contracts/skills/solana-vulnerability-scanner/SKILL.md |
| `audit-context-building` | Understand one function at a time before hunting bugs | https://raw.githubusercontent.com/trailofbits/skills/main/plugins/audit-context-building/skills/audit-context-building/SKILL.md |
| `differential-review` | Security review of a branch diff | https://raw.githubusercontent.com/trailofbits/skills/main/plugins/differential-review/skills/differential-review/SKILL.md |
| `spec-to-code-compliance` | Spoke docs versus contract | https://raw.githubusercontent.com/trailofbits/skills/main/plugins/spec-to-code-compliance/skills/spec-to-code-compliance/SKILL.md |
| `property-based-testing` | Foundry invariant tests, Echidna, or Medusa | https://raw.githubusercontent.com/trailofbits/skills/main/plugins/property-based-testing/skills/property-based-testing/SKILL.md |
| `insecure-defaults` | Fail-open defaults | https://github.com/trailofbits/skills/tree/main/plugins/insecure-defaults |
| `variant-analysis` | Same bug pattern elsewhere in the repo | https://raw.githubusercontent.com/trailofbits/skills/main/plugins/variant-analysis/skills/variant-analysis/SKILL.md |
| `guidelines-advisor` | Implementation quality during feature work | https://raw.githubusercontent.com/trailofbits/skills/main/plugins/building-secure-contracts/skills/guidelines-advisor/SKILL.md |
| `code-maturity-assessor` | Maturity scorecard before an external audit | https://raw.githubusercontent.com/trailofbits/skills/main/plugins/building-secure-contracts/skills/code-maturity-assessor/SKILL.md |

Solidity vulnerability detection in this set is the secure-workflow guide plus Slither, not a separate Solidity scanner. The Building Secure Contracts scanners cover Algorand, Cairo, Cosmos, Solana, Substrate, and TON.

## Bamboo files

- Hook: `contracts/uniswap-hook/src/BambooSettlementHook.sol`
- Spoke spec: `docs/spokes/base-uniswap-v4.md`
- Solana program: `contracts/solana-escrow/`
