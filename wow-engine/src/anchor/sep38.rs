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

        // Formulate standard ISO-8601 UTC date string
        let expires_at = (chrono::Utc::now() + chrono::Duration::minutes(15))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

        Ok(Sep38Quote {
            id: quote_id,
            expires_at,
            price: format!("{:.7}", price),
            sell_asset: sell_asset.to_string(),
            sell_amount: format!("{:.7}", sell_amount),
            buy_asset: buy_asset.to_string(),
            buy_amount: format!("{:.7}", buy_amount),
        })
    }
}

fn uuid_fast() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let start = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("{:x}", start & 0xFFFFFFFFFFFFFFF)
}
