# Wow Engine

Wow Engine is a high-performance, modular Rust-based bridging and routing service. It is designed to route multi-chain tokens into the Stellar network and facilitate instant fiat on-ramping and off-ramping via Stellar anchors.

This service acts as the shared transaction backend for the Whales of Wallstreet (WOW) ecosystem including the Web App and Native App

## How It Works

The Wow Engine coordinates cross-chain liquidity transfers and fiat gateway aggregation:

1. **Cross-Chain Routing**: Integrates with bridging protocols like Circle CCTP (Cross-Chain Transfer Protocol) and deBridge DLN to route assets from external networks such as Ethereum, Solana or Arbitrum into the Stellar network.
2. **Optimal Pathfinding**: The internal pathfinding router queries available bridge providers, evaluates estimated gas fees and completion times, and ranks the execution paths by highest output yield and lowest cost.
3. **Stellar Anchor On-Ramping and Off-Ramping**: The engine uses Stellar Ecosystem Proposals (SEP-24 for interactive deposits/withdrawals and SEP-38 for quotes) to bridge and exchange tokens between on-chain assets and local fiat currencies globally.

## Developer Integration

The engine is structured as an integratable on-ramp and off-ramp service, exposing a clean REST API so client applications do not need to implement complex cryptographic or cross-chain coordination logic locally. 

### API Endpoints

The server listens on port 8080 and provides the following endpoints:

- `GET /api/v1/health`: Checks the service status and version.
- `POST /api/v1/quote`: Evaluates and returns sorted, executable routes for cross-chain transfers.
- `POST /api/v1/anchor/deposit`: Sets up deposit transactions (on-ramp) using the SEP-24 interactive flow.
- `POST /api/v1/anchor/withdraw`: Sets up withdrawal transactions (off-ramp) using the SEP-24 interactive flow.
- `POST /api/v1/anchor/quote`: Returns price and rate quotes following the SEP-38 standard.

## Technical Details

- **Language**: Rust (ensures memory safety and predictable execution times)
- **Runtime**: Tokio (handles concurrent async operations)
- **Web Framework**: Axum (manages HTTP routing and request parsing)
- **HTTP Client**: Reqwest (manages outbound calls to anchors and bridge builders)

## Running Locally

1. Verify that the Rust toolchain is installed.
2. Change directory to the engine path.
3. Start the application:
   ```bash
   cargo run
   ```
4. The service will be active at `http://127.0.0.1:8080`.

## API Usage Examples

### Health Check

```bash
curl http://localhost:8080/api/v1/health
```

```json
{
  "status": "ok",
  "service": "wow-engine",
  "version": "0.1.0",
  "timestamp": "2026-06-19T08:14:00Z"
}
```

---

### Cross-Chain Quote

Returns ranked bridge routes from a source chain into Stellar, sorted by best output amount.

```bash
curl -X POST http://localhost:8080/api/v1/quote \
  -H "Content-Type: application/json" \
  -d '{
    "from_chain": "Ethereum",
    "to_chain": "Stellar",
    "asset": "USDC",
    "amount": 100.0
  }'
```

```json
{
  "routes": [
    {
      "provider": "CCTP",
      "from_chain": "Ethereum",
      "to_chain": "Stellar",
      "asset": "USDC",
      "amount_in": 100.0,
      "amount_out": 99.98,
      "fee": 0.02,
      "estimated_time_seconds": 20
    },
    {
      "provider": "deBridge",
      "from_chain": "Ethereum",
      "to_chain": "Stellar",
      "asset": "USDC",
      "amount_in": 100.0,
      "amount_out": 99.90,
      "fee": 0.10,
      "estimated_time_seconds": 15
    }
  ]
}
```

---

### Anchor Deposit (SEP-24)

Initiates an interactive on-ramp deposit flow via a registered Stellar anchor.

```bash
curl -X POST http://localhost:8080/api/v1/anchor/deposit \
  -H "Content-Type: application/json" \
  -d '{
    "anchor_domain": "testanchor.stellar.org",
    "asset_code": "USDC",
    "account": "GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
  }'
```

```json
{
  "transaction_id": "tx_sep24_1a2b3c4d5e6f",
  "url": "https://testanchor.stellar.org/sep24/interactive/deposit?asset_code=USDC&account=GXXX...",
  "status": "pending_user_transfer_start"
}
```

---

### Anchor Withdraw (SEP-24)

Initiates an interactive off-ramp withdrawal to local fiat via a Stellar anchor.

```bash
curl -X POST http://localhost:8080/api/v1/anchor/withdraw \
  -H "Content-Type: application/json" \
  -d '{
    "anchor_domain": "testanchor.stellar.org",
    "asset_code": "USDC",
    "account": "GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
  }'
```

```json
{
  "transaction_id": "tx_sep24_7f8e9d0c1b2a",
  "url": "https://testanchor.stellar.org/sep24/interactive/withdraw?asset_code=USDC&account=GXXX...",
  "status": "pending_user_transfer_start"
}
```

---

### Anchor Quote (SEP-38)

Fetches an exchange rate quote between a Stellar asset and a fiat currency.

```bash
curl -X POST http://localhost:8080/api/v1/anchor/quote \
  -H "Content-Type: application/json" \
  -d '{
    "sell_asset": "USDC",
    "buy_asset": "NGN",
    "sell_amount": 100.0
  }'
```

```json
{
  "sell_asset": "USDC",
  "buy_asset": "NGN",
  "sell_amount": 100.0,
  "buy_amount": 145000.0,
  "price": 1450.0,
  "expires_at": "2026-06-19T09:14:00Z"
}
```

---

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `8080` | TCP port the server listens on |
| `REQUEST_TIMEOUT_SECS` | `30` | Upper bound on how long any single HTTP request may run before the server aborts it and returns `408 Request Timeout` |

```bash
PORT=9090 cargo run
```

## Resilience & Chaos Testing

The engine is built to degrade gracefully when its downstream dependencies
(bridge quote APIs, gas oracles, the Postgres pool) slow down or fail:

- **Per-request timeout** — every request is wrapped in a `TimeoutLayer`
  (`REQUEST_TIMEOUT_SECS`) so a single stalled dependency can never pin a
  request, and its resources, open indefinitely. A timed-out request returns
  `408`.
- **Circuit breaker** — `resilience::CircuitBreaker` wraps calls to flaky
  dependencies with a hard call timeout, trips open after a configurable number
  of consecutive failures, fails fast while open, and self-heals via a half-open
  probe once a cooldown elapses.
- **Connection-pool starvation** — when the Postgres pool is exhausted the
  affected endpoints return `503 Service Unavailable` instead of hanging.

These behaviours are guarded by a deterministic chaos suite in
[`tests/chaos_tests.rs`](tests/chaos_tests.rs). It uses `tokio::time::pause` and
`tokio::time::advance` to simulate multi-second network hangs and complete
partitions *instantly*, with no real waiting and no external services, so it
runs fast and non-flaky in CI:

```bash
cargo test --test chaos_tests
```
