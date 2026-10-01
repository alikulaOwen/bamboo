---
okf_version: "0.2"
title: "Open Decisions & Flaw Analysis"
description: "Stakeholder decision matrix, unresolved architectural questions, and vulnerability critique of baseline dispute mechanisms and unit economics."
type: "plan"
tags:
  - plan
  - decisions
  - flaws
  - economics
  - disputes
categories:
  - Plan
resource: "crates/bamboo-core/src/lib.rs"
generated:
  by: "openwiki/0.2"
  at: "2026-09-29T07:30:00Z"
---

<!-- OPENWIKI:START -->
# Open Decisions & Flaw Analysis: Project Bamboo v1

This document consolidates every architectural, legal, and economic decision that requires human stakeholder resolution, along with an explicit critique of flaws identified in the baseline design.

---

## 1. Decisions Matrix & Resolution Status

| Decision ID | Area | Options | Resolution Status | Notes / Rationale |
|---|---|---|---|---|
| **DEC-01 / D-5** | **Float Refill / Liquidity Partner** | 1. Licensed OTC Desk (Yellow Card, Onafriq)<br>2. Global Exchange (Coinbase wire)<br>3. P2P Merchant Network | **Pending Counterparty Quotes (Placeholder)** | Candidate: Licensed OTC Desk. Spreads (25–50 bps) and wire cutoffs marked as unverified placeholders pending signed rate cards. |
| **DEC-02** | **Ledger Database Engine** | 1. File-backed SQLite in WAL mode<br>2. Standalone PostgreSQL server | **RESOLVED: Option 1 (SQLite WAL)** | Zero external operational dependencies, single-binary daemon, sub-millisecond ACID writes, exact INTEGER atomic units. |
| **DEC-03** | **Solana Ingestion Commitment Level** | 1. `confirmed` (~1–2s latency)<br>2. `finalized` (~12–15s latency) | **RESOLVED: Option 2 (`finalized`)** | M-Pesa fiat is irreversible. Paying fiat on an un-finalized block risks total capital loss during fork resets. |
| **DEC-04** | **Dispute Window ($W$) Duration** | 1. 10 minutes (600s)<br>2. 30 minutes (1800s)<br>3. 60 minutes (3600s) | **RESOLVED: Option 2 (30 minutes = 1,800s)** | Sufficient for telecom SMS delivery; documented as a UX-dependent control requiring active push alerts. |
| **DEC-05** | **Dispute Freeze Timeout** | 1. 24 hours<br>2. 7 days<br>3. 14 days | **RESOLVED: Option 3 (14 days = 1,209,600s)** | Adequate window for offline reconciliation, operator arbitration, and bank record inspection. |
| **DEC-06** | **Supported Collateral Mints** | 1. USDC only<br>2. USDC + USDT | **RESOLVED: Option 1 (USDC only)** | Deepest liquidity on Solana, predictable Circle compliance, zero peg-divergence risk. |
| **DEC-07** | **Relayer Operating Legal Entity** | 1. Foundation (offshore, e.g. BVI / Cayman)<br>2. Kenyan Operating Company (Ltd)<br>3. Hybrid (Offshore IP + Local Operating Subsidiary) | **RESOLVED: Option 3 (Hybrid)** | Offshore entity owns open-source protocol IP; local Kenyan entity holds Safaricom business contract and bank accounts. |
| **DEC-08** | **Kenyan Regulatory & VASP Licensing** | 1. Apply for CBK PSP / VASP license<br>2. Operate under an existing licensed payment aggregator<br>3. Closed pilot under regulatory sandbox | **Option 2 (Licensed Partner)** | Fastest path to compliance without a multi-year direct central bank licensing queue. |
| **D-2** | **Protocol Fee Ceiling** | 1. 25 bps ceiling + hidden FX spread<br>2. 100 bps immutable on-chain ceiling | **RESOLVED: Option 2 (100 bps ceiling)** | `MAX_HOST_FEE_BPS = 100` in smart contract. Published card sits at or below ceiling; no hidden spreads. |
| **D-4** | **Operating Budget** | 1. Estimated $1,250/mo<br>2. Formally quoted vendor pricing | **Marked as Placeholder (Pending Quotes)** | Line items for RPC, tunneling, KYT, and legal retainers are provisional placeholders. |
| **D-7** | **Maker Dispute Bond** | 1. 2%–5% token dispute bond<br>2. No bond in v1; velocity caps + loss reserve | **RESOLVED: Option 2 (No Bond in v1)** | 2% bond risks extortion without deterring fraud; v1 uses per-user caps, KYC attestation, and seeded loss reserve. Revisit at T1. |

---

## 2. Flaw Analysis & Vulnerability Critique of Baseline Decisions

The baseline decisions in Section 2 of the project brief contain two fundamental economic and game-theoretic flaws. Per protocol guidelines, these decisions are **not altered in the implementation plan**, but their failure modes and recommended remediations are formally documented below.

### Flaw A: The "Dispute Returns Funds to Maker After Timeout" Exploit (Free-Riding / Extortion Attack)

#### The Vulnerability:
Decision 2.3 specifies:
> *"dispute $\to$ `Disputed`; funds are frozen and return to the maker after a timeout unless the maker withdraws the dispute."*

In a system without an arbiter, this mechanism creates an **asymmetric moral hazard**:
1. An attacker (maker) locks 100 USDC and submits their genuine Safaricom phone number.
2. The honest relayer observes the finalized lock and disburses $\text{KES }13{,}050$ via M-Pesa.
3. The attacker receives the M-Pesa SMS and the cash sits in their mobile wallet.
4. The attacker immediately invokes `dispute(order_id)` on Solana before window $W$ elapses.
5. The escrow transitions to `Disputed`. Funds are frozen for the timeout period (e.g. 7 days).
6. Because there is no arbiter and the team cannot move user funds, the attacker simply waits 7 days and calls `resolve_dispute_timeout()`.
7. **Attacker Outcome:** The attacker keeps the $\text{KES }13{,}050$ fiat AND reclaims $100\%$ of their 100 USDC deposit.
8. **Relayer Outcome:** The relayer loses both fiat cash and crypto collateral.

#### Why Timelocks Alone Cannot Solve Dispute Attribution:
A smart contract cannot know whether Safaricom credited the recipient's phone number. Therefore:
- If a dispute defaults to **releasing funds to the relayer**, a dishonest relayer can steal crypto without paying fiat.
- If a dispute defaults to **returning funds to the maker**, a dishonest maker can steal fiat by falsely claiming non-receipt.

#### Recommended Mitigation for v1 / v2:
1. **Off-Chain Legal Deterrent (v1):** Under the Kenyan Kenya Information and Communications (Registration of SIM-cards) Regulations, all M-Pesa phone numbers are bound to a verified National Identity Card or Passport. Because the maker registered their phone number with the relayer over TLS, the relayer holds cryptographic and banking proof of payment to a legally identifiable individual. Wire fraud and unjust enrichment are actionable under Kenyan penal code.
2. **Maker Dispute Bond (v1.1 Patch):** Require the maker to post a substantial dispute bond (e.g., $10\%\text{ of order size}$) to trigger `dispute()`. If Safaricom's official statement proves payment completed, the dispute bond is slashed.
3. **Optimistic Multi-Sig Arbitration (v2):** Introduce a designated dispute committee or decentralized oracle (e.g. Kleros / UMA) that is invoked ONLY when a dispute is lodged, examining Safaricom statement extracts.

---

### Flaw B: Protocol Fee Ceiling (25 bps) vs. Real Wholesale Unit Economics

#### The Reality:
ADR 0001 and core protocol constants enforce an on-chain host fee ceiling of **25 bps ($0.25\%$)**:
```rust
pub const MAX_HOST_FEE_BPS: u16 = 25; // 0.25%
```
Analysis of Safaricom's verified *M-PESA Bulk Payment Tariff Form* shows that Safaricom charges KES 11.00 for a KES 13,050 ($100) payout (~8.5 bps) and KES 13.00 for KES 65,250 ($500) payout (~2 bps). Thus, for tickets $\ge \$50$, the on-chain 25 bps host fee easily absorbs the Safaricom B2C fee.

However, a strict 25 bps fee model without an exchange spread is **still commercially unviable** due to three separate wholesale cost components:
1. **Wholesale OTC Refill Spread (25–75 bps):** Converting received USDC back into KES to replenish Paybill working float costs 25 to 75 bps across domestic OTC desks.
2. **Kenyan Virtual Asset Excise Duty (10%):** Enacted under the Kenya Finance Act 2025, virtual asset service providers are levied a 10% excise duty on transaction fees.
3. **Small-Ticket Disproportionality:** On a $10 ticket, the minimum KES 5.00 B2C charge equals ~39 bps, which exceeds 25 bps on its own.

#### Recommended Economic Remediation:
1. **Transparent All-In Pricing:** Package fee and quoted FX spread into a transparent all-in price (~0.75% to 1.00%), which provides sustainable gross margin while beating card off-ramps (4–8%) and P2P desks (2–4%).
2. **Wholesale Refill Optimization:** Secure competitive wholesale OTC execution ($\le 25\text{ bps}$) from multiple institutional liquidity providers.
3. **Minimum Order Size:** Set a minimum order floor of $20\text{ to }25\text{ USDC}$ to protect against small-ticket fixed fee erosion.
4. **Two-Way Flow Netting (v2):** Introducing KES $\to$ USDC on-ramp flow allows internal netting of opposing orders, drastically reducing the volume requiring wholesale OTC bank refills.

<!-- OPENWIKI:END -->
