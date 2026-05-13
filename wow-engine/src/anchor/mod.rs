pub mod sep24;
pub mod sep38;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorInfo {
    pub name: String,
    pub domain: String,
    pub supported_assets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sep24Transaction {
    pub id: String,
    pub status: String,
    pub url: String, // Interactive web flow URL
    pub eta: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sep38Quote {
    pub id: String,
    pub buy_asset: String,
    pub sell_asset: String,
    pub buy_amount: f64,
    pub sell_amount: f64,
    pub expires_at: String,
    pub price: f64,
}
