use crate::anchor::Sep38Quote;
use reqwest::Client;

pub struct Sep38Client {
    #[allow(dead_code)]
    client: Client,
}

impl Sep38Client {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    pub async fn get_quote(
        &self,
        _anchor_domain: &str,
        sell_asset: &str,
        buy_asset: &str,
        sell_amount: f64,
    ) -> Result<Sep38Quote, anyhow::Error> {
        let quote_id = format!("q_sep38_{}", uuid_fast());
        
        // Simulates query to anchor's `https://<domain>/sep38/quote`
        // Model real exchange rates: e.g. converting 1 USDC to NGN or EUR
        let (price, buy_amount) = match buy_asset {
            b if b.contains("NGN") => (1450.0, sell_amount * 1450.0),
            b if b.contains("EUR") => (0.92, sell_amount * 0.92),
            _ => (1.0, sell_amount),
        };

        use std::time::{SystemTime, UNIX_EPOCH};
        let expires_secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + 600;
        let expires_at = format!("{} seconds from now", expires_secs);

        Ok(Sep38Quote {
            id: quote_id,
            buy_asset: buy_asset.to_string(),
            sell_asset: sell_asset.to_string(),
            buy_amount,
            sell_amount,
            expires_at,
            price,
        })
    }
}

fn uuid_fast() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let start = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("{:x}", start & 0xFFFFFFFFFFFFFFF)
}
