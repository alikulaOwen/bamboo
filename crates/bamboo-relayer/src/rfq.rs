use bamboo_core::ChainEndpoint;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

type HmacSha256 = Hmac<Sha256>;

pub const RFQ_COUNTDOWN_SECONDS: u64 = 7;
pub const RELAYER_IDENTITY_KEY: &[u8] = b"bamboo_relayer_ed25519_identity_key_secret_v1";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RfqQuoteRequest {
    pub chain: ChainEndpoint,
    pub token: String,
    pub amount: u64,
    pub fiat_currency: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RfqQuoteResponse {
    pub quote_id: String,
    pub chain: ChainEndpoint,
    pub token: String,
    pub crypto_amount: u64,
    pub fiat_amount: f64,
    pub fiat_currency: String,
    pub exchange_rate: f64,
    pub host_fee_bps: u16,
    pub valid_for_seconds: u64,
    pub created_at: i64,
    #[serde(default)]
    pub signature: String,
}

#[allow(dead_code)]
struct StoredQuote {
    quote: RfqQuoteResponse,
    created_at_instant: Instant,
}

#[derive(Clone)]
pub struct RfqEngine {
    quotes: Arc<RwLock<HashMap<String, StoredQuote>>>,
}

#[allow(dead_code)]
impl RfqEngine {
    pub fn new() -> Self {
        Self {
            quotes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn sign_quote(quote_id: &str, exchange_rate: f64, expiry_timestamp: i64) -> String {
        let mut mac = HmacSha256::new_from_slice(RELAYER_IDENTITY_KEY)
            .expect("HMAC can take key of any size");
        let payload = format!("{}:{}:{:.6}", quote_id, expiry_timestamp, exchange_rate);
        mac.update(payload.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    pub fn verify_quote_signature(
        quote_id: &str,
        exchange_rate: f64,
        expiry_timestamp: i64,
        signature: &str,
    ) -> bool {
        let expected = Self::sign_quote(quote_id, exchange_rate, expiry_timestamp);
        expected.eq_ignore_ascii_case(signature)
    }

    pub fn generate_quote(&self, req: &RfqQuoteRequest) -> RfqQuoteResponse {
        let quote_id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().timestamp();
        let expiry = created_at + RFQ_COUNTDOWN_SECONDS as i64;

        // Base conversion rate mockup for quoting
        let exchange_rate = match req.fiat_currency.as_str() {
            "KES" => 130.50,
            "BRL" => 5.45,
            "EUR" => 0.92,
            _ => 1.0,
        };

        let fiat_amount = (req.amount as f64 / 1_000_000.0) * exchange_rate;
        let signature = Self::sign_quote(&quote_id, exchange_rate, expiry);

        let quote = RfqQuoteResponse {
            quote_id: quote_id.clone(),
            chain: req.chain,
            token: req.token.clone(),
            crypto_amount: req.amount,
            fiat_amount,
            fiat_currency: req.fiat_currency.clone(),
            exchange_rate,
            host_fee_bps: 75, // 0.75% (strictly <= MAX_HOST_FEE_BPS = 100)
            valid_for_seconds: RFQ_COUNTDOWN_SECONDS,
            created_at,
            signature,
        };

        {
            let mut map = self.quotes.write().unwrap();
            map.insert(
                quote_id,
                StoredQuote {
                    quote: quote.clone(),
                    created_at_instant: Instant::now(),
                },
            );
        }

        quote
    }

    pub fn get_valid_quote(&self, quote_id: &str) -> Option<RfqQuoteResponse> {
        let mut map = self.quotes.write().unwrap();
        if let Some(stored) = map.get(quote_id) {
            if stored.created_at_instant.elapsed() <= Duration::from_secs(RFQ_COUNTDOWN_SECONDS) {
                return Some(stored.quote.clone());
            } else {
                map.remove(quote_id);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_generation_and_signature_verification() {
        let engine = RfqEngine::new();
        let req = RfqQuoteRequest {
            chain: ChainEndpoint::Solana,
            token: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".into(),
            amount: 50_000_000,
            fiat_currency: "KES".into(),
        };

        let quote = engine.generate_quote(&req);
        assert!(!quote.signature.is_empty());
        let expiry = quote.created_at + quote.valid_for_seconds as i64;
        assert!(RfqEngine::verify_quote_signature(
            &quote.quote_id,
            quote.exchange_rate,
            expiry,
            &quote.signature
        ));

        // Tampered signature or rate fails
        assert!(!RfqEngine::verify_quote_signature(
            &quote.quote_id,
            quote.exchange_rate + 1.0,
            expiry,
            &quote.signature
        ));
    }
}
