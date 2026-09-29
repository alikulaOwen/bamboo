#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use thiserror::Error;
use tracing::{error, info, instrument, warn};

#[derive(Error, Debug, PartialEq)]
pub enum ComplianceError {
    #[error("Jurisdiction {0} is restricted or subject to comprehensive international sanctions")]
    SanctionedTerritory(String),

    #[error("Wallet {address} flagged by pre-lock KYT screening: risk_score={score:.2}, reason={reason}")]
    SanctionedWallet {
        address: String,
        score: f32,
        reason: String,
    },

    #[error("Fiat currency {currency} is not supported for jurisdiction {jurisdiction:?}")]
    UnsupportedCurrency {
        currency: String,
        jurisdiction: SupportedJurisdiction,
    },

    #[error("Transaction amount {requested:.2} exceeds velocity cap {limit:.2} for tier {tier:?}")]
    VelocityLimitExceeded {
        requested: f64,
        limit: f64,
        tier: VerificationTier,
    },

    #[error("Invalid destination endpoint format: {0}")]
    InvalidEndpointFormat(String),

    #[error("KYC or screening oracle provider error: {0}")]
    ProviderFailure(String),
}

/// Supported sovereign jurisdictions for primary acquiring and settlement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupportedJurisdiction {
    Kenya,          // East Africa: CBK domestic mobile money & banking
    EuropeanUnion,  // Europe: MiCA CASP regime & SEPA Instant
    Brazil,         // Latin America: Central Bank Pix (SPI)
    UnitedKingdom,  // UK: FCA registered cryptoasset framework
    UnitedStates,   // US: FinCEN MSB / state money transmitter partner
    Restricted,     // OFAC / FATF high-risk territories
}

/// Dynamic Merchant of Record (MoR) contracting entities.
/// Prevents regulatory contagion by ring-fencing domestic operations into local operating subsidiaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MerchantOfRecord {
    /// Bamboo Kenya Ltd: Holds Safaricom Daraja B2C/C2B shortcodes, domestic banking, and ODPC filing.
    BambooKenyaLtd,
    /// Bamboo Europe CASP: MiCA-registered entity for Euro clearing and SEPA Instant execution.
    BambooEuropeCasp,
    /// Bamboo Pagamentos do Brasil Ltda: Direct integration into Central Bank SPI / Pix network.
    BambooBrazilLtda,
    /// Licensed US Sponsor / Partner: Operates under state MTL licenses and FinCEN registration.
    RegulatedPartnerUs,
}

/// Verification matrix matching Banxa, MoonPay, and Topper risk thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VerificationTier {
    /// Tier 0 (< $150 / KES 20,000): Frictionless checkout.
    /// Verifies MSISDN SIM registration and SMS one-time passcode (OTP).
    Tier0PhoneOnly,

    /// Tier 1 ($150 – $1,000 / KES 150,000): Standard KYC.
    /// Automated National ID / Passport OCR with biometric liveness (Smile ID / Sumsub).
    Tier1DocumentKyc,

    /// Tier 2 (> $1,000): Enhanced Due Diligence (EDD).
    /// Proof of address, source of wealth attestation, and continuous PEP/sanctions monitoring.
    Tier2EnhancedEdd,
}

/// Client telemetry and transaction payload evaluated before quote generation or smart contract locks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationContext {
    pub client_ip: String,
    pub country_iso2: String,
    pub wallet_address: String,
    /// Phone number (+254...), Pix key, or IBAN
    pub endpoint_identifier: String,
    pub fiat_currency: String,
    pub fiat_amount: f64,
    pub is_on_ramp: bool,
}

/// Comprehensive routing instruction produced by the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub jurisdiction: SupportedJurisdiction,
    pub mor_entity: MerchantOfRecord,
    pub required_tier: VerificationTier,
    pub allowed_rails: Vec<String>,
    pub max_ticket_usd: f64,
    pub kyt_risk_score: f32,
    pub operational_entity_name: String,
}

/// Forensic screening output for pre-lock wallet inspection.
#[derive(Debug, Clone)]
pub struct KytReport {
    pub address: String,
    pub risk_score: f32, // 0.00 (clean) to 1.00 (critical risk)
    pub is_sanctioned: bool,
    pub risk_category: Option<String>,
}

/// Pluggable interface for on-chain wallet intelligence (TRM Labs, Chainalysis, Elliptic).
#[async_trait::async_trait]
pub trait KytOracle: Send + Sync {
    async fn evaluate_wallet(&self, address: &str) -> Result<KytReport, ComplianceError>;
}

/// Built-in heuristic and mock oracle for development, local testing, and known test vectors.
pub struct EmbeddedKytOracle {
    sanctioned_wallets: HashSet<String>,
}

impl Default for EmbeddedKytOracle {
    fn default() -> Self {
        let mut sanctioned = HashSet::new();
        // OFAC SDN designated addresses and test vectors (e.g. Tornado Cash, Lazarus Group)
        sanctioned.insert("0xd90e2f925da726b50c4ed8d0fb90ad053324f31b".to_lowercase()); // Tornado Cash Router
        sanctioned.insert("0x8576acc5c05d6ce0714819ff22d045eeac673cfc".to_lowercase()); // High risk exploit wallet
        sanctioned.insert("1BoatSLRHtKNngkdXEeobR76b53LETtpyT".to_lowercase()); // Known BTC OFAC address
        Self {
            sanctioned_wallets: sanctioned,
        }
    }
}

#[async_trait::async_trait]
impl KytOracle for EmbeddedKytOracle {
    async fn evaluate_wallet(&self, address: &str) -> Result<KytReport, ComplianceError> {
        let normalized = address.trim().to_lowercase();
        if self.sanctioned_wallets.contains(&normalized) {
            return Ok(KytReport {
                address: address.to_string(),
                risk_score: 0.99,
                is_sanctioned: true,
                risk_category: Some("OFAC_SDN_SANCTIONED".to_string()),
            });
        }

        // Clean retail wallet baseline
        Ok(KytReport {
            address: address.to_string(),
            risk_score: 0.05,
            is_sanctioned: false,
            risk_category: None,
        })
    }
}

pub struct JurisdictionEngine {
    sanctioned_territories: HashSet<String>,
    kyt_oracle: Arc<dyn KytOracle>,
    max_kyt_risk_ceiling: f32,
}

impl JurisdictionEngine {
    pub fn new(kyt_oracle: Arc<dyn KytOracle>) -> Self {
        let mut sanctioned = HashSet::new();
        // FATF Blacklist and comprehensive OFAC jurisdictions
        for code in &["CU", "IR", "KP", "SY", "RU", "BY", "MM", "VE"] {
            sanctioned.insert(code.to_string());
        }

        Self {
            sanctioned_territories: sanctioned,
            kyt_oracle,
            max_kyt_risk_ceiling: 0.70, // 0.70 / 1.0 triggers immediate fail-closed rejection
        }
    }

    /// Evaluates incoming order context and executes the complete compliance pipeline.
    #[instrument(skip(self, ctx), fields(wallet = %ctx.wallet_address, country = %ctx.country_iso2))]
    pub async fn evaluate_order(
        &self,
        ctx: &EvaluationContext,
    ) -> Result<RoutingDecision, ComplianceError> {
        let country = ctx.country_iso2.trim().to_uppercase();

        // 1. Geofencing & Comprehensive Sanctions Filter
        if self.sanctioned_territories.contains(&country) {
            warn!("Rejected request from sanctioned territory: {}", country);
            return Err(ComplianceError::SanctionedTerritory(country));
        }

        // 2. Pre-Lock Wallet Forensics (KYT)
        let kyt_report = self.kyt_oracle.evaluate_wallet(&ctx.wallet_address).await?;
        if kyt_report.is_sanctioned || kyt_report.risk_score >= self.max_kyt_risk_ceiling {
            error!(
                "Wallet {} rejected by KYT screening (Score: {:.2}, Category: {:?})",
                ctx.wallet_address, kyt_report.risk_score, kyt_report.risk_category
            );
            return Err(ComplianceError::SanctionedWallet {
                address: ctx.wallet_address.clone(),
                score: kyt_report.risk_score,
                reason: kyt_report
                    .risk_category
                    .unwrap_or_else(|| "HIGH_RISK_CHAIN_EXPOSURE".to_string()),
            });
        }

        // 3. Resolve Jurisdiction, Merchant of Record (MoR), and Tier Matrix
        match country.as_str() {
            // ================================================================
            // Kenya & East Africa (KES / M-Pesa / PesaLink)
            // ================================================================
            "KE" => {
                if ctx.fiat_currency != "KES" {
                    return Err(ComplianceError::UnsupportedCurrency {
                        currency: ctx.fiat_currency.clone(),
                        jurisdiction: SupportedJurisdiction::Kenya,
                    });
                }

                // Tiered Velocity Limits for Kenya (KES 130 ≈ $1.00 USD)
                let (tier, max_ticket_usd) = if ctx.fiat_amount <= 20_000.0 {
                    (VerificationTier::Tier0PhoneOnly, 150.0)
                } else if ctx.fiat_amount <= 150_000.0 {
                    (VerificationTier::Tier1DocumentKyc, 1_150.0)
                } else if ctx.fiat_amount <= 300_000.0 {
                    (VerificationTier::Tier2EnhancedEdd, 2_500.0)
                } else {
                    return Err(ComplianceError::VelocityLimitExceeded {
                        requested: ctx.fiat_amount,
                        limit: 300_000.0,
                        tier: VerificationTier::Tier2EnhancedEdd,
                    });
                };

                // Validate Kenyan MSISDN format (2547... or 2541...)
                let sanitized_phone = ctx.endpoint_identifier.trim_start_matches('+').replace(' ', "");
                if !(sanitized_phone.starts_with("254") && sanitized_phone.len() == 12) {
                    return Err(ComplianceError::InvalidEndpointFormat(format!(
                        "Invalid Kenyan phone number: '{}'. Expected format '254XXXXXXXXX'",
                        ctx.endpoint_identifier
                    )));
                }

                info!(
                    "Routing order to Bamboo Kenya Ltd (Tier: {:?}, Amount: KES {:.2})",
                    tier, ctx.fiat_amount
                );

                Ok(RoutingDecision {
                    jurisdiction: SupportedJurisdiction::Kenya,
                    mor_entity: MerchantOfRecord::BambooKenyaLtd,
                    required_tier: tier,
                    allowed_rails: vec!["MPESA_DARAJA".into(), "PESALINK".into()],
                    max_ticket_usd,
                    kyt_risk_score: kyt_report.risk_score,
                    operational_entity_name: "Bamboo Kenya Limited (CR12 / Safaricom B2C)".into(),
                })
            }

            // ================================================================
            // Brazil & Latin America (BRL / Pix SPI)
            // ================================================================
            "BR" => {
                if ctx.fiat_currency != "BRL" {
                    return Err(ComplianceError::UnsupportedCurrency {
                        currency: ctx.fiat_currency.clone(),
                        jurisdiction: SupportedJurisdiction::Brazil,
                    });
                }

                let (tier, max_ticket_usd) = if ctx.fiat_amount <= 800.0 {
                    (VerificationTier::Tier0PhoneOnly, 150.0)
                } else if ctx.fiat_amount <= 5_000.0 {
                    (VerificationTier::Tier1DocumentKyc, 1_000.0)
                } else if ctx.fiat_amount <= 25_000.0 {
                    (VerificationTier::Tier2EnhancedEdd, 5_000.0)
                } else {
                    return Err(ComplianceError::VelocityLimitExceeded {
                        requested: ctx.fiat_amount,
                        limit: 25_000.0,
                        tier: VerificationTier::Tier2EnhancedEdd,
                    });
                };

                Ok(RoutingDecision {
                    jurisdiction: SupportedJurisdiction::Brazil,
                    mor_entity: MerchantOfRecord::BambooBrazilLtda,
                    required_tier: tier,
                    allowed_rails: vec!["PIX_SPI".into()],
                    max_ticket_usd,
                    kyt_risk_score: kyt_report.risk_score,
                    operational_entity_name: "Bamboo Pagamentos do Brasil Ltda (Pix Direct)".into(),
                })
            }

            // ================================================================
            // European Union (EUR / SEPA Instant / 3DS Card)
            // ================================================================
            "DE" | "FR" | "ES" | "IT" | "NL" | "BE" | "PT" | "IE" | "AT" | "FI" => {
                if ctx.fiat_currency != "EUR" {
                    return Err(ComplianceError::UnsupportedCurrency {
                        currency: ctx.fiat_currency.clone(),
                        jurisdiction: SupportedJurisdiction::EuropeanUnion,
                    });
                }

                let (tier, max_ticket_usd) = if ctx.fiat_amount <= 150.0 {
                    (VerificationTier::Tier0PhoneOnly, 150.0)
                } else if ctx.fiat_amount <= 1_000.0 {
                    (VerificationTier::Tier1DocumentKyc, 1_000.0)
                } else if ctx.fiat_amount <= 10_000.0 {
                    (VerificationTier::Tier2EnhancedEdd, 10_000.0)
                } else {
                    return Err(ComplianceError::VelocityLimitExceeded {
                        requested: ctx.fiat_amount,
                        limit: 10_000.0,
                        tier: VerificationTier::Tier2EnhancedEdd,
                    });
                };

                Ok(RoutingDecision {
                    jurisdiction: SupportedJurisdiction::EuropeanUnion,
                    mor_entity: MerchantOfRecord::BambooEuropeCasp,
                    required_tier: tier,
                    allowed_rails: vec!["SEPA_INSTANT".into(), "CARD_3DS2".into()],
                    max_ticket_usd,
                    kyt_risk_score: kyt_report.risk_score,
                    operational_entity_name: "Bamboo Europe UAB (MiCA Registered CASP)".into(),
                })
            }

            // ================================================================
            // United States (USD / ACH / FedNow Partner)
            // ================================================================
            "US" => {
                if ctx.fiat_currency != "USD" {
                    return Err(ComplianceError::UnsupportedCurrency {
                        currency: ctx.fiat_currency.clone(),
                        jurisdiction: SupportedJurisdiction::UnitedStates,
                    });
                }

                // In the US, all transactions require at least Tier 1 (CIP/BSA requirement)
                let tier = if ctx.fiat_amount <= 1_000.0 {
                    VerificationTier::Tier1DocumentKyc
                } else {
                    VerificationTier::Tier2EnhancedEdd
                };

                Ok(RoutingDecision {
                    jurisdiction: SupportedJurisdiction::UnitedStates,
                    mor_entity: MerchantOfRecord::RegulatedPartnerUs,
                    required_tier: tier,
                    allowed_rails: vec!["FEDNOW_A2A".into(), "ACH_PUSH".into()],
                    max_ticket_usd: 5_000.0,
                    kyt_risk_score: kyt_report.risk_score,
                    operational_entity_name: "Bamboo US FinCEN Sponsor & Partner".into(),
                })
            }

            // Unsupported or unlisted territory
            other => Err(ComplianceError::SanctionedTerritory(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_kenya_routing_and_tiers() {
        let oracle = Arc::new(EmbeddedKytOracle::default());
        let engine = JurisdictionEngine::new(oracle);

        // 1. Test Tier 0 Frictionless KES Flow
        let ctx_tier0 = EvaluationContext {
            client_ip: "105.160.45.12".into(),
            country_iso2: "KE".into(),
            wallet_address: "0x742d35Cc6634C0532925a3b844Bc454e4438f44e".into(),
            endpoint_identifier: "+254712345678".into(),
            fiat_currency: "KES".into(),
            fiat_amount: 5_000.0,
            is_on_ramp: false,
        };

        let decision = engine.evaluate_order(&ctx_tier0).await.unwrap();
        assert_eq!(decision.jurisdiction, SupportedJurisdiction::Kenya);
        assert_eq!(decision.mor_entity, MerchantOfRecord::BambooKenyaLtd);
        assert_eq!(decision.required_tier, VerificationTier::Tier0PhoneOnly);
        assert!(decision.allowed_rails.contains(&"MPESA_DARAJA".to_string()));

        // 2. Test Tier 1 KYC Escalation (> KES 20,000)
        let mut ctx_tier1 = ctx_tier0.clone();
        ctx_tier1.fiat_amount = 50_000.0;
        let decision_tier1 = engine.evaluate_order(&ctx_tier1).await.unwrap();
        assert_eq!(decision_tier1.required_tier, VerificationTier::Tier1DocumentKyc);

        // 3. Test Invalid MSISDN Phone Format Rejection
        let mut ctx_invalid_phone = ctx_tier0.clone();
        ctx_invalid_phone.endpoint_identifier = "0712345678".into(); // Missing 254
        let err = engine.evaluate_order(&ctx_invalid_phone).await.unwrap_err();
        assert!(matches!(err, ComplianceError::InvalidEndpointFormat(_)));
    }

    #[tokio::test]
    async fn test_kyt_wallet_rejection() {
        let oracle = Arc::new(EmbeddedKytOracle::default());
        let engine = JurisdictionEngine::new(oracle);

        // Tornado Cash Router address
        let tainted_ctx = EvaluationContext {
            client_ip: "197.232.1.20".into(),
            country_iso2: "KE".into(),
            wallet_address: "0xd90e2f925da726b50c4ed8d0fb90ad053324f31b".into(),
            endpoint_identifier: "254712345678".into(),
            fiat_currency: "KES".into(),
            fiat_amount: 5_000.0,
            is_on_ramp: false,
        };

        let err = engine.evaluate_order(&tainted_ctx).await.unwrap_err();
        assert!(matches!(err, ComplianceError::SanctionedWallet { .. }));
    }

    #[tokio::test]
    async fn test_sanctioned_geofence() {
        let oracle = Arc::new(EmbeddedKytOracle::default());
        let engine = JurisdictionEngine::new(oracle);

        let sanctioned_ctx = EvaluationContext {
            client_ip: "175.45.176.1".into(),
            country_iso2: "KP".into(), // North Korea
            wallet_address: "0x742d35Cc6634C0532925a3b844Bc454e4438f44e".into(),
            endpoint_identifier: "12345".into(),
            fiat_currency: "EUR".into(),
            fiat_amount: 100.0,
            is_on_ramp: true,
        };

        let err = engine.evaluate_order(&sanctioned_ctx).await.unwrap_err();
        assert!(matches!(err, ComplianceError::SanctionedTerritory(_)));
    }
}
