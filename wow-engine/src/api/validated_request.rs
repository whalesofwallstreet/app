use axum::{body::Body, extract::FromRequest, http::Request, Json};
use serde::Deserialize;
use uuid::Uuid;

use crate::bridge::Chain;
use crate::error::AppError;

use super::validation::{validate_asset_code, validate_stellar_address};

pub struct ValidatedQuoteRequest {
    pub source_chain: Chain,
    pub dest_chain: Chain,
    pub source_asset: String,
    pub dest_asset: String,
    pub amount_in: u64,
}

pub struct ValidatedDepositRequest {
    pub anchor_domain: String,
    pub asset_code: String,
    pub account: String,
}

pub struct ValidatedWithdrawRequest {
    pub anchor_domain: String,
    pub asset_code: String,
    pub account: String,
}

pub struct ValidatedAnchorQuoteRequest {
    pub anchor_domain: String,
    pub sell_asset: String,
    pub buy_asset: String,
    pub sell_amount: f64,
}

pub struct ValidatedExecuteRouteRequest {
    pub user_id: Uuid,
    pub source_chain: Chain,
    pub dest_chain: Chain,
    pub source_asset: String,
    pub dest_asset: String,
    pub amount_in: u64,
    pub amount_out: u64,
    pub provider: String,
    pub path: String,
    pub estimated_fee_usd: f64,
    pub anchor_domain: Option<String>,
    pub anchor_transaction_id: Option<String>,
}

#[derive(Deserialize, Debug)]
struct RawQuoteRequest {
    source_chain: Chain,
    dest_chain: Chain,
    source_asset: String,
    dest_asset: String,
    amount_in: u64,
}

#[derive(Deserialize, Debug)]
struct RawDepositRequest {
    anchor_domain: String,
    asset_code: String,
    account: String,
}

#[derive(Deserialize, Debug)]
struct RawWithdrawRequest {
    anchor_domain: String,
    asset_code: String,
    account: String,
}

#[derive(Deserialize, Debug)]
struct RawAnchorQuoteRequest {
    anchor_domain: String,
    sell_asset: String,
    buy_asset: String,
    sell_amount: f64,
}

#[derive(Deserialize, Debug)]
struct RawExecuteRouteRequest {
    user_id: Uuid,
    source_chain: String,
    dest_chain: String,
    source_asset: String,
    dest_asset: String,
    amount_in: u64,
    amount_out: u64,
    provider: String,
    path: String,
    estimated_fee_usd: f64,
    anchor_domain: Option<String>,
    anchor_transaction_id: Option<String>,
}

fn validate_quote_fields(
    source_chain: Chain,
    dest_chain: Chain,
    source_asset: &str,
    dest_asset: &str,
    amount_in: u64,
) -> Result<(), AppError> {
    if source_asset.trim().is_empty() {
        return Err(AppError::BadRequest("Source asset cannot be empty".into()));
    }
    if dest_asset.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Destination asset cannot be empty".into(),
        ));
    }
    if amount_in == 0 {
        return Err(AppError::BadRequest(
            "Amount in must be greater than zero".into(),
        ));
    }
    validate_chain_asset_compat(source_chain, source_asset, "source")?;
    validate_chain_asset_compat(dest_chain, dest_asset, "destination")?;
    Ok(())
}

fn validate_chain_asset_compat(chain: Chain, asset: &str, label: &str) -> Result<(), AppError> {
    match chain {
        Chain::Stellar => {
            if let Err(e) = validate_asset_code(asset) {
                return Err(AppError::BadRequest(format!(
                    "Invalid {} asset for Stellar chain: {}",
                    label, e
                )));
            }
        }
        Chain::Ethereum | Chain::Arbitrum | Chain::Solana => {
            if asset.is_empty() {
                return Err(AppError::BadRequest(format!(
                    "{} asset cannot be empty for {:?}",
                    label, chain
                )));
            }
            if asset.len() > 20 {
                return Err(AppError::BadRequest(format!(
                    "{} asset code too long for {:?} (max 20 chars)",
                    label, chain
                )));
            }
            for c in asset.chars() {
                if !c.is_ascii_alphanumeric() && c != '_' && c != '-' && c != '.' {
                    return Err(AppError::BadRequest(format!(
                        "Invalid character '{}' in {} asset for {:?}",
                        c, label, chain
                    )));
                }
            }
        }
    }
    Ok(())
}

fn validate_domain(domain: &str) -> Result<(), AppError> {
    if domain.trim().is_empty() {
        return Err(AppError::BadRequest("Anchor domain cannot be empty".into()));
    }
    if domain.contains(' ') {
        return Err(AppError::BadRequest(
            "Anchor domain cannot contain spaces".into(),
        ));
    }
    Ok(())
}

fn parse_chain(s: &str) -> Result<Chain, AppError> {
    match s {
        "Ethereum" => Ok(Chain::Ethereum),
        "Solana" => Ok(Chain::Solana),
        "Arbitrum" => Ok(Chain::Arbitrum),
        "Stellar" => Ok(Chain::Stellar),
        _ => Err(AppError::BadRequest(format!(
            "Invalid chain: '{}'. Must be one of: Ethereum, Solana, Arbitrum, Stellar",
            s
        ))),
    }
}

impl FromRequest for ValidatedQuoteRequest {
    type Error = AppError;
    type Future =
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self, Self::Error>> + Send>>;

    fn from_request(req: &Request<Body>) -> Self::Future {
        let fut = async {
            let Json(raw) = Json::<RawQuoteRequest>::from_request(req).await?;
            validate_quote_fields(
                raw.source_chain,
                raw.dest_chain,
                &raw.source_asset,
                &raw.dest_asset,
                raw.amount_in,
            )?;
            Ok(ValidatedQuoteRequest {
                source_chain: raw.source_chain,
                dest_chain: raw.dest_chain,
                source_asset: raw.source_asset,
                dest_asset: raw.dest_asset,
                amount_in: raw.amount_in,
            })
        };
        Box::pin(fut)
    }
}

impl FromRequest for ValidatedDepositRequest {
    type Error = AppError;
    type Future =
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self, Self::Error>> + Send>>;

    fn from_request(req: &Request<Body>) -> Self::Future {
        let fut = async {
            let Json(raw) = Json::<RawDepositRequest>::from_request(req).await?;
            validate_domain(&raw.anchor_domain)?;
            if let Err(e) = validate_stellar_address(&raw.account) {
                return Err(AppError::BadRequest(format!(
                    "Invalid account address: {}",
                    e
                )));
            }
            if let Err(e) = validate_asset_code(&raw.asset_code) {
                return Err(AppError::BadRequest(format!("Invalid asset code: {}", e)));
            }
            Ok(ValidatedDepositRequest {
                anchor_domain: raw.anchor_domain,
                asset_code: raw.asset_code,
                account: raw.account,
            })
        };
        Box::pin(fut)
    }
}

impl FromRequest for ValidatedWithdrawRequest {
    type Error = AppError;
    type Future =
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self, Self::Error>> + Send>>;

    fn from_request(req: &Request<Body>) -> Self::Future {
        let fut = async {
            let Json(raw) = Json::<RawWithdrawRequest>::from_request(req).await?;
            validate_domain(&raw.anchor_domain)?;
            if let Err(e) = validate_stellar_address(&raw.account) {
                return Err(AppError::BadRequest(format!(
                    "Invalid account address: {}",
                    e
                )));
            }
            if let Err(e) = validate_asset_code(&raw.asset_code) {
                return Err(AppError::BadRequest(format!("Invalid asset code: {}", e)));
            }
            Ok(ValidatedWithdrawRequest {
                anchor_domain: raw.anchor_domain,
                asset_code: raw.asset_code,
                account: raw.account,
            })
        };
        Box::pin(fut)
    }
}

impl FromRequest for ValidatedAnchorQuoteRequest {
    type Error = AppError;
    type Future =
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self, Self::Error>> + Send>>;

    fn from_request(req: &Request<Body>) -> Self::Future {
        let fut = async {
            let Json(raw) = Json::<RawAnchorQuoteRequest>::from_request(req).await?;
            validate_domain(&raw.anchor_domain)?;
            if let Err(e) = validate_asset_code(&raw.sell_asset) {
                return Err(AppError::BadRequest(format!("Invalid sell asset: {}", e)));
            }
            if let Err(e) = validate_asset_code(&raw.buy_asset) {
                return Err(AppError::BadRequest(format!("Invalid buy asset: {}", e)));
            }
            if raw.sell_amount <= 0.0 {
                return Err(AppError::BadRequest(
                    "Sell amount must be greater than zero".into(),
                ));
            }
            Ok(ValidatedAnchorQuoteRequest {
                anchor_domain: raw.anchor_domain,
                sell_asset: raw.sell_asset,
                buy_asset: raw.buy_asset,
                sell_amount: raw.sell_amount,
            })
        };
        Box::pin(fut)
    }
}

impl FromRequest for ValidatedExecuteRouteRequest {
    type Error = AppError;
    type Future =
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self, Self::Error>> + Send>>;

    fn from_request(req: &Request<Body>) -> Self::Future {
        let fut = async {
            let Json(raw) = Json::<RawExecuteRouteRequest>::from_request(req).await?;

            let source_chain = parse_chain(&raw.source_chain)?;
            let dest_chain = parse_chain(&raw.dest_chain)?;

            if raw.amount_in == 0 {
                return Err(AppError::BadRequest(
                    "Amount in must be greater than zero".into(),
                ));
            }
            if raw.amount_out == 0 {
                return Err(AppError::BadRequest(
                    "Amount out must be greater than zero".into(),
                ));
            }
            if raw.estimated_fee_usd < 0.0 {
                return Err(AppError::BadRequest(
                    "Estimated fee cannot be negative".into(),
                ));
            }
            if raw.source_asset.trim().is_empty() {
                return Err(AppError::BadRequest("Source asset cannot be empty".into()));
            }
            if raw.dest_asset.trim().is_empty() {
                return Err(AppError::BadRequest(
                    "Destination asset cannot be empty".into(),
                ));
            }
            if raw.provider.trim().is_empty() {
                return Err(AppError::BadRequest("Provider cannot be empty".into()));
            }
            if raw.path.trim().is_empty() {
                return Err(AppError::BadRequest("Path cannot be empty".into()));
            }

            validate_chain_asset_compat(source_chain, &raw.source_asset, "source")?;
            validate_chain_asset_compat(dest_chain, &raw.dest_asset, "destination")?;

            Ok(ValidatedExecuteRouteRequest {
                user_id: raw.user_id,
                source_chain,
                dest_chain,
                source_asset: raw.source_asset,
                dest_asset: raw.dest_asset,
                amount_in: raw.amount_in,
                amount_out: raw.amount_out,
                provider: raw.provider,
                path: raw.path,
                estimated_fee_usd: raw.estimated_fee_usd,
                anchor_domain: raw.anchor_domain,
                anchor_transaction_id: raw.anchor_transaction_id,
            })
        };
        Box::pin(fut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_chain_asset_compat_stellar_valid() {
        assert!(validate_chain_asset_compat(Chain::Stellar, "USDC", "source").is_ok());
        assert!(validate_chain_asset_compat(Chain::Stellar, "XLM", "source").is_ok());
        assert!(validate_chain_asset_compat(
            Chain::Stellar,
            "stellar:USDC:GA5Z3IX5VQ3N6FB77T342A27RWRN7CKEZ63M3W7S5VJB3D77J6F2JAFK",
            "dest"
        )
        .is_ok());
    }

    #[test]
    fn test_validate_chain_asset_compat_stellar_invalid() {
        assert!(validate_chain_asset_compat(Chain::Stellar, "", "source").is_err());
        assert!(
            validate_chain_asset_compat(Chain::Stellar, "VERYLONGASSETCODE12345", "dest").is_err()
        );
    }

    #[test]
    fn test_validate_chain_asset_compat_evm() {
        assert!(validate_chain_asset_compat(Chain::Ethereum, "ETH", "source").is_ok());
        assert!(validate_chain_asset_compat(Chain::Ethereum, "0xABC", "dest").is_ok());
        assert!(validate_chain_asset_compat(Chain::Arbitrum, "USDC", "source").is_ok());
    }

    #[test]
    fn test_validate_chain_asset_compat_evm_rejects_long() {
        assert!(validate_chain_asset_compat(
            Chain::Ethereum,
            "ASSETCODETHATISTOOLONGFORTHECHAIN",
            "source"
        )
        .is_err());
    }

    #[test]
    fn test_validate_chain_asset_compat_solana() {
        assert!(validate_chain_asset_compat(Chain::Solana, "SOL", "source").is_ok());
        assert!(validate_chain_asset_compat(
            Chain::Solana,
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
            "dest"
        )
        .is_ok());
    }

    #[test]
    fn test_validate_domain() {
        assert!(validate_domain("example.com").is_ok());
        assert!(validate_domain(" anchor.io ").is_ok());
        assert!(validate_domain("").is_err());
        assert!(validate_domain("   ").is_err());
        assert!(validate_domain("has space.com").is_err());
    }

    #[test]
    fn test_parse_chain() {
        assert!(parse_chain("Ethereum").is_ok());
        assert!(parse_chain("Solana").is_ok());
        assert!(parse_chain("Arbitrum").is_ok());
        assert!(parse_chain("Stellar").is_ok());
        assert!(parse_chain("Bitcoin").is_err());
        assert!(parse_chain("ethereum").is_err());
    }

    #[test]
    fn test_validate_quote_fields() {
        assert!(
            validate_quote_fields(Chain::Ethereum, Chain::Stellar, "ETH", "USDC", 1000,).is_ok()
        );

        assert!(validate_quote_fields(Chain::Ethereum, Chain::Stellar, "", "USDC", 1000,).is_err());

        assert!(validate_quote_fields(Chain::Ethereum, Chain::Stellar, "ETH", "", 1000,).is_err());

        assert!(validate_quote_fields(Chain::Ethereum, Chain::Stellar, "ETH", "USDC", 0,).is_err());

        // Stellar dest must have valid Stellar asset
        assert!(validate_quote_fields(
            Chain::Ethereum,
            Chain::Stellar,
            "ETH",
            "INVALIDASSET!@#",
            1000,
        )
        .is_err());
    }

    #[test]
    fn test_cross_chain_invalid_mapping() {
        // Empty asset for non-Stellar chain
        assert!(validate_chain_asset_compat(Chain::Ethereum, "", "source").is_err());

        // Invalid chars for EVM
        assert!(validate_chain_asset_compat(Chain::Arbitrum, "US DC", "dest").is_err());

        // Stellar rejects non-compliant asset
        assert!(validate_chain_asset_compat(Chain::Stellar, "not valid asset", "source").is_err());
    }
}
