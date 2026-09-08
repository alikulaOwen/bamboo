use bamboo_core::ChainEndpoint;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

pub const RFQ_COUNTDOWN_SECONDS: u64 = 7;

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

    pub fn generate_quote(&self, req: &RfqQuoteRequest) -> RfqQuoteResponse {
        let quote_id = uuid::Uuid::new_v4().to_string();
        // Base conversion rate mockup for quoting
        let exchange_rate = match req.fiat_currency.as_str() {
            "KES" => 130.50,
            "BRL" => 5.45,
            "EUR" => 0.92,
            _ => 1.0,
        };

        let fiat_amount = (req.amount as f64 / 1_000_000.0) * exchange_rate;

        let quote = RfqQuoteResponse {
            quote_id: quote_id.clone(),
            chain: req.chain,
            token: req.token.clone(),
            crypto_amount: req.amount,
            fiat_amount,
            fiat_currency: req.fiat_currency.clone(),
            exchange_rate,
            host_fee_bps: 20, // 0.20% (strictly <= MAX_HOST_FEE_BPS = 25)
            valid_for_seconds: RFQ_COUNTDOWN_SECONDS,
            created_at: chrono::Utc::now().timestamp(),
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
