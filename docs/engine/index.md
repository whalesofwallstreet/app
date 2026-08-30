# Wow Engine

Rust-based cross-chain routing and fiat orchestration engine.

---

## Responsibilities

- Cross-chain token routing
- Bridge aggregation (CCTP, deBridge)
- Optimal path selection
- Stellar anchor integration (SEP-24, SEP-38)

---

## API

| Method | Endpoint                        | Description                  |
|--------|---------------------------------|------------------------------|
| GET    | `/api/v1/health`                | Health check                 |
| POST   | `/api/v1/quote`                 | Get cross-chain quote        |
| POST   | `/api/v1/anchor/deposit`        | Initiate anchor deposit      |
| POST   | `/api/v1/anchor/withdraw`       | Initiate anchor withdrawal   |
| GET    | `/api/v1/anchor/transaction/:id`| Poll anchor transaction status |
| POST   | `/api/v1/anchor/quote`          | Get anchor quote             |

---

## Configuration

Full list of environment variables lives in `wow-engine/.env.example`. The
gas oracle's provider keys are called out separately here since running
without them is easy to miss in production:

| Variable            | Required | Description                                                                                                     |
|----------------------|----------|-------------------------------------------------------------------------------------------------------------------|
| `ETHERSCAN_API_KEY`  | Recommended | Authenticates outbound Etherscan gas-tracker calls. Without it, requests are sent unauthenticated and are subject to Etherscan's strict per-IP rate limits — under real load this pushes gas pricing onto static fallback values. Get a free key at https://etherscan.io/myapikey. |
| `ARBISCAN_API_KEY`   | Recommended | Same as above, for Arbiscan gas-tracker calls. Get a free key at https://arbiscan.io/myapikey. |

When a gas-fee lookup falls back to static pricing, the engine logs a
`reason` of `missing_api_key` (no key configured) or `provider_outage` (a
key is configured but the provider call still failed), and increments
`GasOracle::fallback_count()` — watch both for sustained degradation.

---

## Runtime

- **Language**: Rust
- **Async Runtime**: Tokio
- **Web Framework**: Axum
- **HTTP Client**: Reqwest

**Runs on:**  
`http://127.0.0.1:8080`