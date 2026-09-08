---
okf_version: "0.2"
title: "Domestic Fiat Adapters & zkTLS Verification"
description: "Specification for bamboo-fiat adapters: Safaricom M-Pesa, Brazilian Pix, European SEPA Instant, and zkTLS zero-knowledge session attestations."
type: "concept"
tags:
  - fiat
  - m-pesa
  - pix
  - sepa
  - zktls
  - adapters
categories:
  - Fiat Integration
resource: "crates/bamboo-fiat/src/lib.rs"
generated:
  by: "openwiki/0.2"
  at: "2026-09-08T07:58:39Z"
---

<!-- OPENWIKI:START -->
# Domestic Fiat Adapters & zkTLS Verification

Project Bamboo bridges decentralized on-chain liquidity into sovereign domestic payment rails through modular adapters implemented in [`crates/bamboo-fiat/src/lib.rs`](../../crates/bamboo-fiat/src/lib.rs).

```mermaid
graph TD
    Order[UnifiedEscrowOrder] --> Split{Select Adapter}

    Split -->|East Africa / KES| Mpesa[Safaricom M-Pesa B2C]
    Split -->|Latin America / BRL| Pix[Banco Central do Brasil Pix]
    Split -->|Europe / EUR| Sepa[SEPA Instant SCT Inst]
    Split -->|Decentralized Web Proofs| ZKTLS[zkTLS TLS Notarization]

    Mpesa --> Verify[Hash Matching: SHA256(recipient + salt)]
    Pix --> Verify
    Sepa --> Verify
    ZKTLS --> Verify

    Verify --> Confirmation[FiatSettlementConfirmation]
    Confirmation --> Trigger[Trigger On-Chain settlePayout]
```

---

## Supported Fiat Payment Rails

### 1. Safaricom M-Pesa (East Africa)
* **Currency**: Kenyan Shilling (`KES`)
* **Mechanism**: Direct B2C (Business-to-Customer) Daraja API payout.
* **Verification**: Inbound webhook verification confirming mobile money transaction reference and MSISDN hash match.

### 2. Pix (Latin America)
* **Currency**: Brazilian Real (`BRL`)
* **Mechanism**: Banco Central do Brasil SPI (Sistema de Pagamentos Instantâneos) network.
* **Speed**: Sub-3 second settlement 24/7/365.
* **Verification**: End-to-End (`E2E_ID`) payment identifier verification against maker's registered CPF/CNPJ or email key hash.

### 3. SEPA Instant (Europe)
* **Currency**: Euro (`EUR`)
* **Mechanism**: SCT Inst (SEPA Instant Credit Transfer) with 10-second pan-European clearing.
* **Verification**: Bank statement reference or ISO 20022 camt.054 credit notification.

### 4. zkTLS Web Session Attestations
For peer-to-peer or closed-loop bank interfaces without open API access, Project Bamboo supports **zkTLS (zero-knowledge TLS session proofs)**:
* **Principle**: A user's browser session with their online banking portal is notarized using MPC-TLS / Proxy-TLS.
* **Privacy**: Sensitive bank account numbers and passwords remain hidden in zero knowledge; only the assertion (e.g. `Transfer Confirmed: €500.00 to IBAN ...`) is proven mathematically.
* **Validation**: The `ZkTlsVerifier` checks the cryptographic signature of the notarization authority and confirms matching transfer values before triggering smart contract escrow payout.
<!-- OPENWIKI:END -->
