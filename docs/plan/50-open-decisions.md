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

## 1. Open Decisions Matrix

| Decision ID | Area | Options | Recommendation | Impact / What It Blocks |
|---|---|---|---|---|
| **DEC-01** | **Float Refill / Liquidity Partner** | 1. Licensed OTC Desk (Yellow Card, Onafriq)<br>2. Global Exchange (Coinbase wire)<br>3. P2P Merchant Network | **Option 1 (Licensed OTC Partner):** Cleanest AML lineage, reliable RTGS settlement directly to Safaricom Paybill. | Blocks Phase P4 production deployment and treasury automation. |
| **DEC-02** | **Ledger Database Engine** | 1. File-backed SQLite in WAL mode<br>2. Standalone PostgreSQL server | **Option 1 (SQLite WAL):** Zero external operational dependencies, single-binary daemon, sub-millisecond ACID writes. | Blocks Phase P0 database implementation. |
| **DEC-03** | **Solana Ingestion Commitment Level** | 1. `confirmed` (~1–2s latency)<br>2. `finalized` (~12–15s latency) | **Option 2 (`finalized`):** M-Pesa fiat is irreversible. Paying fiat on an un-finalized block risks total capital loss during fork resets. | Blocks Phase P3 event listener implementation. |
| **DEC-04** | **Dispute Window ($W$) Duration** | 1. 10 minutes (600s)<br>2. 30 minutes (1800s)<br>3. 60 minutes (3600s) | **Option 2 (30 minutes):** Ample time for telecom SMS delivery and maker verification without excessive relayer float lockup. | Blocks Phase P1 Anchor contract constants. |
| **DEC-05** | **Dispute Freeze Timeout** | 1. 24 hours<br>2. 7 days<br>3. 14 days | **Option 2 (7 days):** Provides necessary window for operator support to contact the maker and reconcile Safaricom records. | Blocks Phase P1 Anchor dispute resolution logic. |
| **DEC-06** | **Supported Collateral Mints** | 1. USDC only<br>2. USDC + USDT | **Option 1 (USDC only):** Deepest liquidity on Solana, predictable Circle compliance, zero peg-divergence risk. | Blocks Phase P1 program constants. |
| **DEC-07** | **Relayer Operating Legal Entity** | 1. Foundation (offshore, e.g. BVI / Cayman)<br>2. Kenyan Operating Company (Ltd)<br>3. Hybrid (Offshore IP + Local Operating Subsidiary) | **Option 3 (Hybrid):** Offshore entity owns open-source protocol IP; local Kenyan entity holds Safaricom business contract and bank accounts. | Blocks Safaricom corporate onboarding and Daraja production credentials. |
| **DEC-08** | **Kenyan Regulatory & VASP Licensing** | 1. Apply for CBK PSP / VASP license<br>2. Operate under an existing licensed payment aggregator<br>3. Closed pilot under regulatory sandbox | **Option 2 (Licensed Partner):** Fastest path to compliance without a multi-year direct central bank licensing queue. | Blocks public retail launch in Kenya. |

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

### Flaw B: Protocol Fee Ceiling (25 bps) vs. Safaricom B2C Fixed Tariffs

#### The Vulnerability:
ADR 0001 and core protocol constants enforce a strict host fee ceiling of **25 bps ($0.25\%$)**:
```rust
pub const MAX_HOST_FEE_BPS: u16 = 25; // 0.25%
```
However, Safaricom Daraja B2C charges **fixed tiered tariffs** that are independent of percentage:
- An order of **100 USDC** yields a maximum host fee of **$0.25 USDC ($\approx \text{KES }32.60$)**.
- Safaricom's official B2C tariff to disburse $\text{KES }13{,}050$ is **$\text{KES }60.00$ ($0.46 USDC$)**.
- **Net Result:** The relayer loses $\text{KES }27.40$ on every 100 USDC transaction if relying solely on the on-chain host fee!

#### Recommended Economic Remediation:
1. **Differentiate Host Fee from Rail Tariff:** The on-chain `host_fee_bps` should be recognized as a **protocol network coordination fee**.
2. **Incorporate Operational Cost into Quoted Exchange Rate:** The reference relayer must set its RFQ conversion rate at wholesale spot minus $1.0\%\text{ to }1.5\%$. This spread absorbs Safaricom's B2C tariffs and banking rebalancing costs, ensuring positive unit economics across all order sizes above 10 USDC.

<!-- OPENWIKI:END -->
