---
okf_version: "0.2"
title: "ADR 0006 — Dynamic Merchant of Record (MoR) Routing, Tiered Compliance, and Bidirectional Fiat Rails"
description: "Architecture Decision Record: Ring-fencing domestic acquiring across sovereign subsidiaries, implementing a 3-tier KYT/KYC risk engine, and polymorphically supporting both on-ramp (ingress) and off-ramp (egress) fiat flows."
type: "decision"
tags:
  - adr
  - decision
  - jurisdiction
  - mor
  - compliance
  - fiat
  - on-ramp
  - off-ramp
categories:
  - Decisions
resource: "crates/bamboo-relayer/src/jurisdiction.rs"
generated:
  by: "openwiki/0.2"
  at: "2026-09-29T10:00:00Z"
---

<!-- OPENWIKI:START -->
# ADR 0006 — Dynamic Merchant of Record (MoR) Routing, Tiered Compliance, and Bidirectional Fiat Rails

**Status:** Accepted  
**Date:** 2026-09-29  
**Area:** Relayer Routing, Regulatory Isolation & Liquidity Rails (`crates/bamboo-relayer`, `crates/bamboo-fiat`)  
**Deciders:** Protocol Architecture & Legal Engineering Team  
**Related Docs:** [ADR 0005 — Domestic Fiat Adapters](./0005-domestic-fiat-adapters-and-zktls.md), [Domestic Fiat Rails](../fiat/fiat-adapters-and-zktls.md), [Unified Host Daemon](../relayer/unified-host-daemon.md)

---

## Context

Global fiat-to-crypto gateways frequently suffer from severe regulatory and operational bottlenecks:
1. **Regulatory Contagion:** Operating a monolithic global entity exposes the entire protocol to cross-jurisdictional enforcement actions and banking debanking if a single region's laws shift.
2. **One-Way Liquidity Imbalance:** Hardcoding fiat rails as unidirectional off-ramp sinks creates liquidity starvation, requiring costly manual treasury rebalancing and banking wire fees.
3. **Rigid Compliance Barriers:** Forcing full document KYC and biometric scans for small $20 P2P transfers degrades user conversion rates by 80%+, while offering zero anti-fraud protection against stolen credit or sanctioned wallet interactions.

---

## Decision

We introduce a **Dynamic Merchant of Record (MoR) Engine** and **Polymorphic Bidirectional Rail Architecture**:

### 1. Dynamic Merchant of Record (MoR) Ring-Fencing
Operating entities are strictly partitioned across regional subsidiaries to guarantee legal compliance and banking ring-fencing:
* **`BambooKenyaLtd`**: Holds local Safaricom Daraja B2C/C2B shortcodes, domestic CBK banking, and Kenya ODPC data privacy compliance.
* **`BambooEuropeCasp`**: Operates under the EU Markets in Crypto-Assets (MiCA) CASP regime for Euro clearing and SEPA Instant.
* **`BambooBrazilLtda`**: Direct integration into the Banco Central do Brasil SPI / Pix payment network.
* **`RegulatedPartnerUs`**: Sponsored domestic partner operating under state Money Transmitter Licenses (MTLs) and FinCEN registration.

### 2. Three-Tier Proportional Risk Matrix
Verification scales dynamically based on transaction velocity and value:
* **Tier 0 (< $150 / KES 20,000)**: Frictionless mobile checkout; verifies MSISDN SIM registration and SMS one-time passcode (OTP).
* **Tier 1 ($150 – $1,000 / KES 150,000)**: Standard automated National ID / Passport OCR with biometric liveness (Smile ID / Sumsub).
* **Tier 2 (> $1,000)**: Enhanced Due Diligence (EDD) requiring proof of address, source of wealth attestation, and continuous PEP/sanctions screening.

### 3. Pre-Lock On-Chain KYT & Sanctions Geofencing
Before order quotation or on-chain escrow locking:
* IP geolocation and sanctioned territory screening immediately reject high-risk/FATF-blacklisted regions.
* Real-time wallet KYT scoring evaluates counterparty exposure to illicit clusters (mixers, darknet markets, sanctioned addresses), rejecting transactions with `risk_score > 0.60`.

### 4. Bidirectional Polymorphic Rails (`bamboo-fiat`)
Payment adapters implement the `LiquidityRailAdapter` trait supporting both `RailDirection::Ingress` (Fiat In $\rightarrow$ Crypto Out / On-Ramp) and `RailDirection::Egress` (Crypto In $\rightarrow$ Fiat Out / Off-Ramp):
* **M-Pesa**: C2B STK Push for Ingress; B2C Disbursement for Egress.
* **Pix**: Dynamic QR code generation for Ingress; Key Transfer for Egress.
* **SEPA Instant**: camt.054 credit notifications for Ingress; SCT Inst pain.001 transfers for Egress.
* **YellowCard OTC**: Institutional liquidity and on/off-ramp settlement across 20+ African nations.

---

## Consequences

### Positive
* **Zero Regulatory Single-Point-of-Failure:** Legal challenges in one jurisdiction do not compromise clearing in other regions.
* **Self-Rebalancing Float:** Bidirectional flows naturally balance local fiat accounts against crypto inventory.
* **Optimized Conversion & Safety:** Frictionless low-value onboarding paired with strict high-value compliance and automated KYT protection.

### Negative / Tradeoffs
* **Corporate Entity Maintenance:** Requires maintaining multiple localized legal entities and banking relationships.
* **Dual Integration Surface:** Each rail adapter must support both inbound payment webhooks and outbound disbursement APIs.
<!-- OPENWIKI:END -->
