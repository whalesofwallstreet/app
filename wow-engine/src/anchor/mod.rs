pub mod sep24;
pub mod sep38;
pub mod tracker;

use serde::{Deserialize, Serialize};

/// Generates a unique standard UUID v4.
/// Used across SEP-24 and SEP-38 flows to assign transaction and quote IDs.
pub(super) fn generate_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorInfo {
    pub name: String,
    pub domain: String,
    pub supported_assets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sep24InteractiveResponse {
    pub r#type: String, // e.g. "interactive_customer_info_needed"
    pub url: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sep38Quote {
    pub id: String,
    pub expires_at: String, // ISO-8601 string
    pub price: String, // decimal string
    pub sell_asset: String,
    pub sell_amount: String, // decimal string
    pub buy_asset: String,
    pub buy_amount: String, // decimal string
}
