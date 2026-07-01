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

    pub async fn get_indicative_quote(
        &self,
        _anchor_domain: &str,
        sell_asset: &str,
        buy_asset: &str,
        sell_amount: f64,
    ) -> Result<Sep38Quote, anyhow::Error> {
        self.generate_quote(_anchor_domain, sell_asset, buy_asset, sell_amount, 15)
    }

    pub async fn get_firm_quote(
        &self,
        _anchor_domain: &str,
        sell_asset: &str,
        buy_asset: &str,
        sell_amount: f64,
    ) -> Result<Sep38Quote, anyhow::Error> {
        self.generate_quote(_anchor_domain, sell_asset, buy_asset, sell_amount, 5) // Firm quotes expire faster
    }

    fn generate_quote(
        &self,
        _anchor_domain: &str,
        sell_asset: &str,
        buy_asset: &str,
        sell_amount: f64,
        expiration_minutes: i64,
    ) -> Result<Sep38Quote, anyhow::Error> {
        let quote_id = format!("q_sep38_{}", super::generate_uuid());
        
        let (price, buy_amount) = match buy_asset {
            b if b.contains("NGN") => (1450.0, sell_amount * 1450.0),
            b if b.contains("EUR") => (0.92, sell_amount * 0.92),
            _ => (1.0, sell_amount),
        };

        let expires_at = (chrono::Utc::now() + chrono::Duration::minutes(expiration_minutes))
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


