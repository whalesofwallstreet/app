//! Human-readable decoding of EVM revert reasons.
//!
//! An `eth_call` simulation failure surfaces only as an opaque hex blob
//! (e.g. `0x08c379a0...`), which is useless for debugging a failed bridge
//! transaction without manually decoding it by hand. This module turns that
//! blob into a [`RevertReason`]: the standard `Error(string)` / `Panic(uint256)`
//! reasons (or a raw UTF-8 Vyper-style revert), a match against a registry of
//! known custom bridge/router errors, or — if nothing matches — the raw bytes,
//! so callers always have *something* to log without risking a crash on
//! malformed or truncated data.

use alloy_sol_types::{sol, SolInterface};

sol! {
    /// Custom Solidity errors this engine can attribute to a specific
    /// on-chain failure mode, beyond the standard `Error(string)` /
    /// `Panic(uint256)` reverts `alloy_sol_types` already decodes for us.
    #[derive(Debug, PartialEq, Eq)]
    interface KnownBridgeErrors {
        error SlippageExceeded(uint256 expected, uint256 actual);
        error InsufficientLiquidity();
        error DeadlineExpired(uint256 deadline, uint256 blockTimestamp);
        error SenderNotAttester(address sender);
        error MessageAlreadyProcessed(bytes32 messageHash);
    }
}

/// A custom bridge/router error matched against [`KnownBridgeErrors`].
pub use KnownBridgeErrors::KnownBridgeErrorsErrors as KnownBridgeError;

/// A decoded EVM revert reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevertReason {
    /// A standard `Error(string)` or `Panic(uint256)` revert, or a raw
    /// UTF-8 (Vyper-style) revert string — decoded by `alloy_sol_types`.
    Standard(String),
    /// A custom Solidity error matched against the known bridge/router
    /// error registry, with its decoded parameters.
    Known(KnownBridgeError),
    /// The revert data didn't match any known shape. Carries the raw
    /// bytes so callers can still log/inspect them.
    Undecoded(Vec<u8>),
}

impl std::fmt::Display for RevertReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RevertReason::Standard(reason) => write!(f, "{reason}"),
            RevertReason::Known(err) => write!(f, "{err:?}"),
            RevertReason::Undecoded(bytes) => {
                write!(f, "undecoded revert data: 0x{}", hex::encode(bytes))
            }
        }
    }
}

/// Decodes raw EVM revert data into a human-readable [`RevertReason`].
///
/// Never fails: data that doesn't match any known shape falls back to
/// [`RevertReason::Undecoded`] rather than propagating an error, so callers
/// can always log a best-effort reason for a failed `eth_call` without
/// risking a crash on malformed or truncated data.
pub fn decode_revert_reason(data: &[u8]) -> RevertReason {
    if let Some(reason) = alloy_sol_types::decode_revert_reason(data) {
        return RevertReason::Standard(reason);
    }
    if let Ok(known) = KnownBridgeError::abi_decode(data) {
        return RevertReason::Known(known);
    }
    RevertReason::Undecoded(data.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{address, hex as ahex, U256};
    use alloy_sol_types::SolError;

    #[test]
    fn decodes_standard_error_string_revert() {
        // A real revert captured from Uniswap V2's `UniswapV2Pair` require()
        // check (solc 0.5.16 pads the string with a spurious trailing 0x80
        // byte, which real-world decoders must tolerate):
        // https://github.com/Uniswap/v2-core/blob/ee547b17853e71ed4e0101ccfd52e70d5acded58/contracts/UniswapV2Pair.sol#L178
        let data = ahex!(
            "08c379a000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000024556e697377617056323a20494e53554646494349454e545f494e5055545f414d4f554e5400000000000000000000000000000000000000000000000000000080"
        );

        let reason = decode_revert_reason(&data);

        assert_eq!(
            reason,
            RevertReason::Standard("revert: UniswapV2: INSUFFICIENT_INPUT_AMOUNT".to_string())
        );
    }

    #[test]
    fn decodes_standard_panic_revert() {
        // Synthesized via alloy_sol_types' own `Panic` encoder — byte-for-byte
        // what a Solidity >=0.8.4 compiler-inserted arithmetic overflow check
        // emits on revert.
        let panic = alloy_sol_types::Panic {
            code: U256::from(0x11u8),
        };
        let data = panic.abi_encode();

        let reason = decode_revert_reason(&data);

        match reason {
            RevertReason::Standard(text) => {
                assert!(text.contains("arithmetic"), "unexpected panic text: {text}");
            }
            other => panic!("expected a standard panic reason, got {other:?}"),
        }
    }

    #[test]
    fn decodes_known_custom_error_with_parameters() {
        let err = KnownBridgeErrors::SlippageExceeded {
            expected: U256::from(1_000_000u64),
            actual: U256::from(950_000u64),
        };
        let data = KnownBridgeError::SlippageExceeded(err.clone()).abi_encode();

        let reason = decode_revert_reason(&data);

        assert_eq!(
            reason,
            RevertReason::Known(KnownBridgeError::SlippageExceeded(err))
        );
    }

    #[test]
    fn decodes_known_custom_error_with_no_parameters() {
        let data =
            KnownBridgeError::InsufficientLiquidity(KnownBridgeErrors::InsufficientLiquidity {})
                .abi_encode();

        let reason = decode_revert_reason(&data);

        assert_eq!(
            reason,
            RevertReason::Known(KnownBridgeError::InsufficientLiquidity(
                KnownBridgeErrors::InsufficientLiquidity {}
            ))
        );
    }

    #[test]
    fn decodes_known_custom_error_with_address_parameter() {
        let sender = address!("0xa48388222c7ee7daefde5d0b9c99319995c4a990");
        let data =
            KnownBridgeError::SenderNotAttester(KnownBridgeErrors::SenderNotAttester { sender })
                .abi_encode();

        let reason = decode_revert_reason(&data);

        assert_eq!(
            reason,
            RevertReason::Known(KnownBridgeError::SenderNotAttester(
                KnownBridgeErrors::SenderNotAttester { sender }
            ))
        );
    }

    #[test]
    fn falls_back_to_raw_hex_for_unrecognized_data() {
        // A selector that matches neither a standard error/panic nor any
        // entry in the known bridge error registry.
        let data = vec![0xde, 0xad, 0xbe, 0xef, 0x01, 0x02, 0x03];

        let reason = decode_revert_reason(&data);

        assert_eq!(reason, RevertReason::Undecoded(data.clone()));
        assert_eq!(
            reason.to_string(),
            "undecoded revert data: 0xdeadbeef010203"
        );
    }

    #[test]
    fn empty_data_decodes_as_an_empty_standard_reason_not_undecoded() {
        // A bare `revert()`/`require(false)` with no message at all is, by
        // alloy_sol_types' own semantics, a valid (empty) UTF-8 "Vyper-style"
        // revert string rather than a decode failure — it still doesn't
        // panic or need the raw-hex fallback.
        let reason = decode_revert_reason(&[]);
        assert_eq!(reason, RevertReason::Standard(String::new()));
    }
}
