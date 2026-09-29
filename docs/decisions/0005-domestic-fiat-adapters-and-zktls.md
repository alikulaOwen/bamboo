---
okf_version: "0.2"
title: "ADR 0005 — Domestic Fiat Adapters and zkTLS Attestation"
description: "Architecture Decision Record: Integrating direct domestic rails (M-Pesa, Pix, SEPA Instant) and zero-knowledge TLS (zkTLS) web session proofs for non-custodial fiat settlement."
type: "decision"
tags:
  - adr
  - decision
  - fiat
  - m-pesa
  - pix
  - sepa
  - zktls
categories:
  - Decisions
resource: "crates/bamboo-fiat/src/lib.rs"
generated:
  by: "openwiki/0.2"
  at: "2026-09-16T12:57:00Z"
---

<!-- OPENWIKI:START -->
# ADR 0005 — Domestic Fiat Adapters and zkTLS Attestation

**Status:** Accepted  
**Date:** 2026-09-08  
**Area:** Fiat Integration & Privacy (`crates/bamboo-fiat`)  
**Deciders:** Core Protocol Team  
**Related Docs:** [Domestic Fiat Adapters](../fiat/fiat-adapters-and-zktls.md)

---

> [!NOTE]
> **Status: v1 differs — see [docs/plan/](../plan/00-overview.md)**  
> Bamboo v1 exclusively supports M-Pesa KES via Safaricom Daraja B2C under a trusted reference relayer model with contract-enforced timeouts. zkTLS session proofs, Pix, and SEPA are future roadmap targets. See [12-mpesa-adapter.md](../plan/12-mpesa-adapter.md).

## Context

Most Web3 fiat on/off-ramps rely on centralized card network processors (Stripe, MoonPay, Ramp). While functional in North America, this approach fails catastrophically across emerging markets:
1. **High Card Processing Rejection Rates:** In Africa, Latin America, and parts of Europe, over 70% of crypto-related debit/credit card attempts are rejected by issuing banks.
2. **Exorbitant Fees (4% to 8%):** Card interchange fees and processor margins make micro-settlement impossible.
3. **Identity Leakage:** Centralized gateways demand invasive KYC uploads for even a $10 transfer.

Meanwhile, developing regions run on real-time, zero-fee sovereign instant payment rails:
*   **M-Pesa (Kenya / East Africa):** Sub-second mobile money used by 90%+ of the adult population.
*   **Pix (Brazil / LatAm):** Instant account-to-account settlement operated by the Central Bank of Brazil.
*   **SEPA Instant (Europe):** Sub-10 second euro clearing across the Single Euro Payments Area.

However, bridging sovereign bank rails to permissionless blockchains requires verifiable proof of payment without introducing custodial escrow middlemen.

---

## Decision

We implement direct, sovereign domestic payment adapters coupled with **zkTLS (zero-knowledge Transport Layer Security) notarization**:

1. **Modular Domestic Adapters (`crates/bamboo-fiat`):**
   * **M-Pesa Daraja B2C:** Direct API disbursement into Kenyan mobile phone numbers (`KES`).
   * **Pix Instant:** Direct API disbursement via Brazilian SPI network keys (`BRL`).
   * **SEPA Instant (SCT Inst):** Direct ISO 20022 account transfer across European IBANs (`EUR`).

2. **zkTLS Web Session Proofs (Notarized Payout Verification):**
   * Where direct API webhook callbacks are unavailable or when privacy is paramount, settlement is proven using **zkTLS**.
   * A user or relayer generates a zero-knowledge proof of the HTTPS session with their banking portal, proving that a wire of amount $A$ to recipient hash $H$ was executed successfully.
   * Private data (account balances, personal names, account numbers) are redacted cryptographically in zero knowledge.

3. **Privacy via Salted Recipient Hashing:**
   * Recipient identifiers (phone numbers, IBANs, Pix keys) are never stored in plaintext on public ledgers.
   * All on-chain escrows reference only:
   $$\text{fiat\_hash} = \text{SHA256}("BAMBOO\_FIAT\_V1" \parallel \text{recipient\_id} \parallel \text{salt})$$
   * Only the committed relayer and the user possess the salt required to verify the match.

---

## Consequences

### Positive
* **Instant Settlement for Real Users:** Settles into the actual payment systems that 100M+ people use daily.
* **Non-Custodial Cryptographic Proof:** zkTLS allows trustless peer-to-peer verification without a centralized intermediary declaring who paid whom.
* **On-Chain Privacy:** No personal financial identifiers are exposed on public block explorers.

### Negative / Tradeoffs
* **Regional Regulatory Variances:** Direct API integrations (e.g., Safaricom Daraja) require regional corporate registrations or partnerships with licensed local aggregators.
* **zkTLS Prover Compute:** Generating TLS session notarization proofs requires several seconds of client-side computation.
<!-- OPENWIKI:END -->
