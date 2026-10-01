---
okf_version: "0.2"
title: "Protocol Sustainability & Unit Economics Design"
description: "Economic architecture for Project Bamboo v1: 100 bps on-chain fee ceiling, per-ticket margin model, reserve seeding, SQLite ledger invariants, and operational state controller."
type: "plan"
tags:
  - plan
  - economics
  - treasury
  - ledger
  - compliance
categories:
  - Plan
resource: "crates/bamboo-relayer/src/ledger.rs"
generated:
  by: "openwiki/0.2"
  at: "2026-10-01T23:15:00Z"
---

<!-- OPENWIKI:START -->
# Specification: Protocol Sustainability & Unit Economics (`bamboo-sustainability`)

## 1. Executive Summary & Core Economic Posture

Project Bamboo v1 provides deterministic, sovereign crypto-to-fiat routing between Solana USDC and Safaricom M-Pesa. To ensure long-term solvency while preserving radical consumer affordability over traditional card rails (4–8%) and P2P merchants (2–4%), Bamboo adheres to the following economic foundations:

1. **100 bps On-Chain Fee Ceiling:** The protocol parameter `MAX_HOST_FEE_BPS` is fixed at **100 bps ($1.00\%$)** in smart contracts. This is an immutable consumer protection ceiling: the relayer's published rate card sits at or below this ceiling, and no hidden FX spread is charged. Any future increase requires an on-chain program upgrade.
2. **Transparent Rate Card:** Users pay one predictable, auditable all-in price ($0.75\%\text{ to }1.00\%$) expressed entirely on-chain through `host_fee_bps`.
3. **Neutralizing Small-Ticket Drag via Minimum Order Size:** Rather than imposing an aggressive fee floor that penalizes micro-transactions, Bamboo enforces a minimum order threshold of **$20.00\text{ to }\$25.00\text{ USDC}$**. This guarantees that fixed domestic disbursements (Safaricom B2C minimum KES 5.00 $\approx \$0.038$) never consume more than 20 bps of the margin.
4. **V1 Fraud Defense Without Staking Traps:** Dispute bonds are deferred for v1 (resolving **D-7** as *No Bond in v1*). In v1, chargeback and extortion defense relies on per-user velocity caps, integrator-attested KYC tiers, and a dedicated **Dispute & Fraud Loss Reserve**.
5. **Separation of Spread vs. Floor Protections:** The fee floor/minimum order size neutralizes the Safaricom fixed bracket. The relayer's automated health controller transitions to **Amber** when wholesale OTC refill spreads exceed 50 bps, preventing spread spikes from eroding capital.
6. **Double-Entry SQLite Accounting:** System of record is local SQLite with Write-Ahead Logging (WAL) per **DEC-02**. All monetary values are recorded as `INTEGER` atomic units (lamports, micro-USDC, KES cents) with unique idempotency constraints per order lifecycle event.

---

## 2. Unit Cost Structure ($100 Ticket Benchmark)

### 2.1 The Per-Order Margin Equation

$$\text{Net Margin} = \text{Fee Revenue} - [\text{Safaricom B2C Fee} + \text{Wholesale Refill Spread} + \text{Excise Duty} + \text{Fraud Reserve} + \text{Float Cost}]$$

### 2.2 Verified Cost Parameters
1. **Safaricom B2C Tariff (Bulk Payment Form, Registered Users):**
   - KES 101 – 1,500: **KES 5.00**
   - KES 1,501 – 5,000: **KES 9.00**
   - KES 5,001 – 20,000: **KES 11.00**
   - KES 20,001 – 250,000: **KES 13.00**
   *(Customer pays 0; relayer absorbs business charge. Cap: KES 250,000).*
2. **Wholesale OTC Refill Spread (USDC $\to$ KES):**
   - Baseline target: **25 bps to 50 bps** *(D-5: Marked as pending signed counterparty quotes)*.
3. **Kenyan Virtual Asset Excise Duty:**
   - **10% statutory excise tax** on service fee revenue (Kenya Finance Act 2025, effective 1 July 2025). On a 100 bps fee, excise accrues as 10 bps.
4. **Dispute & Fraud Loss Provision:**
   - **5 bps** provision accrued into the Loss Reserve.
5. **Float Capital Holding Cost:**
   - Benchmark: **1.6 bps** on 12-hour weekday turnover; scales up to **~10 bps** across 72-hour weekend banking settlement cutoffs.

---

## 3. Per-Ticket Margin Breakdown

Below is the verified contribution margin table across order tickets under a **100 bps published fee card** (Exchange rate: USD/KES = 130.50):

| Order Ticket | Fee Rev (100 bps) | Safaricom B2C Fee | Refill Spread (25 bps) | Refill Spread (50 bps) | Excise Duty (10%) | Fraud Provision (5 bps) | Float Cost (12h) | Net Margin (at 25 bps OTC) | Net Margin (at 50 bps OTC) |
|---|---|---|---|---|---|---|---|---|---|
| **$10.00** (KES 1,305) | $0.100 | KES 5.00 ($0.038) | $0.025 | $0.050 | $0.010 | $0.005 | $0.002 | **+$0.020 (+20 bps)** | **-$0.005 (-5 bps)** |
| **$25.00** (KES 3,262) | $0.250 | KES 9.00 ($0.069) | $0.063 | $0.125 | $0.025 | $0.013 | $0.004 | **+$0.076 (+30 bps)** | **+$0.014 (+6 bps)** |
| **$100.00** (KES 13,050) | $1.000 | KES 11.00 ($0.084) | $0.250 | $0.500 | $0.100 | $0.050 | $0.016 | **+$0.500 (+50 bps)** | **+$0.250 (+25 bps)** |
| **$500.00** (KES 65,250) | $5.000 | KES 13.00 ($0.100) | $1.250 | $2.500 | $0.500 | $0.250 | $0.080 | **+$2.820 (+56 bps)** | **+$1.570 (+31 bps)** |

### 3.1 Observations & Invariants
- At **$10.00**, if wholesale refill costs 50 bps, net margin is slightly negative (-5 bps) due to the fixed KES 5.00 Safaricom fee.
- Raising the minimum order to **$20.00–$25.00** guarantees positive unit economics across all operational scenarios without requiring complex fee floor logic.
- If a fee floor is configured for small orders, it must satisfy $\frac{\text{floor}}{\text{min\_order}} \le 100\text{ bps}$ so it remains strictly expressible within the on-chain protocol ceiling. A $0.15 floor requires a minimum order of $\ge \$15.00$.

---

## 4. Float Sizing & The Settlement Calendar Multiplier

Working float must absorb operational latency between B2C disbursements and OTC bank refills.

### 4.1 Settlement Calendar Factor
Domestic banking rails (RTGS / EFT) do not operate round-the-clock over weekends. A refill initiated Friday afternoon may not clear until Monday morning (a **72-hour window**).

$$\text{Float Cost Factor} = \text{Annual Cost of Capital} \times \left(\frac{\text{Refill Hours}}{8{,}760}\right)$$

At a 12% annual cost of capital:
- **Weekday Cycle (12 hours):** $\approx 1.64\text{ bps}$.
- **Weekend Cycle (72 hours):** $\approx 9.86\text{ bps}$.

### 4.2 Sizing Formula
$$F_{\text{required}} = (\text{Peak Hourly Volume}) \times (\text{Settlement Horizon Hours}) \times (\text{Safety Buffer Multiplier})$$

At **Tier 1 (T1)**: $10{,}000\text{ USDC / day}$ ($\approx \text{KES }1{,}305{,}000$):
- **Weekday Float (12h buffer, 1.5x multiplier):** $\mathbf{\$15{,}000\text{ USDC}}$ ($\approx \text{KES }1{,}957{,}500$).
- **Weekend Float (72h buffer, 1.25x multiplier):** $\mathbf{\$37{,}500\text{ USDC}}$ ($\approx \text{KES }4{,}893{,}750$).

---

## 5. Dispute & Fraud Loss Reserve

### 5.1 Reserve Seeding (Capital Contribution vs. Revenue)
Accruing a $5,000 loss reserve strictly from a 5–10 bps protocol fee would require **$5,000,000 to $10,000,000** in cumulative volume—unreachable during early pilot stages.

**Policy:**
1. **Initial Seeding:** The reserve is seeded via **founder equity or grant capital** ($2,000 at Tier 0; scaled to 20x maximum single ticket).
2. **Accounting Treatment:** Seeded capital is booked on the balance sheet as **Capital Contribution (Equity)**, NEVER as fee revenue. This prevents contamination of the Operational Coverage Ratio.
3. **Scaling Rule:**
   $$\text{Target Reserve Fund} = 20 \times (\text{Max Order Size for Active Tier})$$
   - Tier 0 ($100 max ticket): Target = **$2,000 USDC**.
   - Tier 1 ($500 max ticket): Target = **$10,000 USDC**.

---

## 6. Fixed Operating Budget & Break-Even Targets

> [!WARNING]
> The fixed operating figures below are explicitly flagged as **initial placeholders (D-4)** pending binding service contracts, banking compliance retainers, and Kenya VASP legal filings.

| Category | Monthly Cost (Est.) | Description |
|---|---|---|
| Cloud & Validator RPC | $450 | Redundant Solana RPC (Helius/Triton) + dedicated relayer VPS |
| Safaricom Daraja Dedicated Link | $200 | Secure IPsec tunnel & corporate Paybill maintenance fees |
| AML & KYT Sanctions Screening | $300 | Automated wallet forensic API calls (Chainalysis/Elliptic) |
| Legal & Regulatory Audit Retainer | $300 | Ongoing compliance filing & Kenyan tax reporting support |
| **Total Fixed Opex** | **$1,250 / month** | *Baseline placeholder operational overhead* |

### 6.1 Break-Even Volume
At an average ticket size of $100 and a net margin of **+45 bps** ($0.45 per $100):
$$\text{Monthly Break-Even Volume} = \frac{\$1{,}250}{0.0045} \approx \mathbf{\$277{,}777\text{ USDC / month}} \approx \mathbf{\$9{,}250\text{ USDC / day}}$$

*At Tier 1 volume cap ($10,000 USDC/day), the relayer generates positive cash flow after covering fixed overhead.*

---

## 7. SQLite Double-Entry Ledger Schema (DEC-02 Alignment)

The relayer system of record implements a local SQLite database in Write-Ahead Logging (WAL) mode. All monetary balances are stored as exact `INTEGER` atomic units. Floating-point numbers are strictly forbidden.

### 7.1 Schema Definition (`schema.sql`)

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- Order state mirror
CREATE TABLE IF NOT EXISTS orders (
    order_id BLOB PRIMARY KEY,              -- 32-byte canonical hash
    maker_pubkey TEXT NOT NULL,             -- Solana base58 address
    assigned_relayer TEXT NOT NULL,         -- Solana base58 address
    token_mint TEXT NOT NULL,               -- USDC mint address
    amount INTEGER NOT NULL,                -- Atomic micro-USDC (6 decimals)
    fiat_recipient_hash BLOB NOT NULL,      -- SHA256 of recipient phone
    host_fee_bps INTEGER NOT NULL,          -- Protocol fee in bps (<= 100)
    status TEXT NOT NULL,                   -- Locked, SettlementRequested, Disputed, Finalized, Refunded
    fiat_tx_id TEXT,                        -- Safaricom M-Pesa receipt
    created_at INTEGER NOT NULL,            -- Unix timestamp
    updated_at INTEGER NOT NULL
);

-- Chart of Accounts
CREATE TABLE IF NOT EXISTS accounts (
    account_id TEXT PRIMARY KEY,            -- e.g. 'asset:vault:usdc', 'liability:user:kes'
    account_type TEXT NOT NULL,             -- Asset, Liability, Equity, Revenue, Expense
    currency TEXT NOT NULL                  -- 'USDC' (atomic micro-units) or 'KES' (cents)
);

-- Double-Entry Journal Entries
CREATE TABLE IF NOT EXISTS journal_entries (
    entry_id INTEGER PRIMARY KEY AUTOINCREMENT,
    order_ref BLOB NOT NULL,                -- Foreign key to orders(order_id)
    entry_type TEXT NOT NULL,               -- 'LOCK', 'DISBURSE', 'FINALIZE', 'EXCISE', 'LOSS'
    timestamp INTEGER NOT NULL,
    FOREIGN KEY(order_ref) REFERENCES orders(order_id),
    -- Idempotency constraint: guarantees exactly-once finalization per event
    CONSTRAINT uq_order_event UNIQUE(order_ref, entry_type)
);

-- Journal Line Postings (Debits and Credits must balance)
CREATE TABLE IF NOT EXISTS journal_postings (
    posting_id INTEGER PRIMARY KEY AUTOINCREMENT,
    entry_id INTEGER NOT NULL,
    account_id TEXT NOT NULL,
    amount INTEGER NOT NULL,                -- Positive integer
    direction TEXT CHECK(direction IN ('DEBIT', 'CREDIT')) NOT NULL,
    FOREIGN KEY(entry_id) REFERENCES journal_entries(entry_id),
    FOREIGN KEY(account_id) REFERENCES accounts(account_id)
);
```

### 7.2 Double-Entry Finalization Journal Entry ($100 Ticket)
```text
Event: FINALIZE (Order #100 USDC, Host Fee = 100 bps = $1.00 USDC, Spot = 130.50)
1. Debit:   asset:relayer:usdc_hot_wallet         100,000,000  (+$100.00 USDC received)
2. Credit:  asset:escrow:solana_vault             100,000,000  (-$100.00 USDC released)

3. Debit:   expense:fiat:mpesa_disbursement        13,050,000  (KES 13,050 payout)
4. Debit:   expense:fiat:safaricom_tariff              11,000  (KES 11.00 B2C fee)
5. Credit:  asset:fiat:mpesa_b2c_float             13,061,000  (-KES 13,061 float deducted)

6. Debit:   expense:tax:kenya_excise_duty              10,000  ($0.10 USDC accrued)
7. Credit:  liability:tax:kenya_revenue_authority      10,000  (Tax payable)

8. Debit:   expense:reserve:fraud_provision             5,000  ($0.05 USDC accrued)
9. Credit:  equity:reserve:fraud_loss_reserve           5,000  (Capital provision)
```

---

## 8. Operational State Controller (Health Engine)

The relayer daemon continuously monitors treasury balances, refill spreads, and dispute rates. State transitions modify system velocity without engaging dynamic surge pricing:

| State | Triggers | Operational Restrictions | Pricing Behavior |
|---|---|---|---|
| **Green (Nominal)** | Float $\ge F_{\min}$, OTC spread $\le 50$ bps, Dispute rate $< 0.1\%$ | Unrestricted processing up to daily volume cap ($10,000 USDC/day). | Published card (100 bps). |
| **Amber (Constrained)** | Float $< F_{\min}$, OTC spread $> 50$ bps, or dispute rate $\ge 0.1\%$ | Minimum order raised to **$50 USDC**; maximum order capped at **$250 USDC**; queue throttling enabled. | Published card (100 bps). No surge pricing. |
| **Red (Halted / Defensive)** | Float $< \text{KES }100{,}000$, dispute rate $\ge 1.0\%$, or unhedged volatility | New quote generation suspended (`HTTP 503 Capacity Depleted`). Only pending settlements finalized. | Relayer stops quoting. No surge pricing. |

<!-- OPENWIKI:END -->
