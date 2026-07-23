# Multi-Path Routing - Testing Guide

## Quick Start

### 1. Build and Test

```bash
# Navigate to the engine directory
cd wow-engine

# Run all tests
cargo test

# Run only multi-path routing tests
cargo test multi_path

# Run with output visible
cargo test multi_path -- --nocapture

# Run specific test
cargo test test_large_order_multi_path_splitting -- --nocapture
```

### 2. Start the Server

```bash
# In the wow-engine directory
cargo run

# Server will start on http://127.0.0.1:8080
```

### 3. Test the API

#### Test Case 1: Small Order (No Splitting)

```bash
curl -X POST http://localhost:8080/api/v1/quote \
  -H "Content-Type: application/json" \
  -d '{
    "source_chain": "Ethereum",
    "dest_chain": "Stellar",
    "source_asset": "USDC",
    "dest_asset": "USDC",
    "amount_in": 10000
  }'
```

**Expected**: Single-path route, `is_split_route: false`

---

#### Test Case 2: Large Order ($100k - At Threshold)

```bash
curl -X POST http://localhost:8080/api/v1/quote \
  -H "Content-Type: application/json" \
  -d '{
    "source_chain": "Ethereum",
    "dest_chain": "Stellar",
    "source_asset": "USDC",
    "dest_asset": "USDC",
    "amount_in": 100000000000
  }'
```

**Expected**: May trigger multi-path optimization

---

#### Test Case 3: Whale Order ($1M - Definitely Splits)

```bash
curl -X POST http://localhost:8080/api/v1/quote \
  -H "Content-Type: application/json" \
  -d '{
    "source_chain": "Ethereum",
    "dest_chain": "Stellar",
    "source_asset": "USDC",
    "dest_asset": "USDC",
    "amount_in": 1000000000000
  }'
```

**Expected**: Multi-path route with `parallel_paths` array

**Sample Response**:
```json
{
  "routes": [
    {
      "provider": "Circle CCTP (50.0%) + deBridge DLN (50.0%)",
      "path": "Multi-path routing: Circle CCTP + deBridge DLN",
      "amount_in": 1000000000000,
      "amount_out": 124978751,
      "estimated_fee_usd": 15.0,
      "duration_seconds": 900,
      "execution_payload": null,
      "parallel_paths": [
        {
          "provider": "Circle CCTP",
          "split_percentage": 50.0,
          "amount_in": 500000000000,
          "amount_out": 99980003,
          "estimated_fee_usd": 7.5,
          "duration_seconds": 900,
          "slippage_percentage": 99.980,
          "execution_payload": "{\"action\": \"depositForBurn\", \"amount\": 500000000000, ...}"
        },
        {
          "provider": "deBridge DLN",
          "split_percentage": 50.0,
          "amount_in": 500000000000,
          "amount_out": 24998748,
          "estimated_fee_usd": 7.5,
          "duration_seconds": 150,
          "slippage_percentage": 99.995,
          "execution_payload": "{\"targetContract\": \"0x543A8e3...\", ...}"
        }
      ],
      "is_split_route": true,
      "slippage_percentage": 99.988
    }
  ]
}
```

---

## Running Specific Tests

### Slippage Calculation Tests

```bash
cargo test --lib router::slippage::tests -- --nocapture
```

**Tests**:
- ✓ Small trade has minimal slippage
- ✓ Large trade has significant slippage
- ✓ Amount out calculation with fees
- ✓ Deep liquidity reduces slippage

---

### Flow Optimization Tests

```bash
cargo test --lib router::flow_optimizer::tests -- --nocapture
```

**Tests**:
- ✓ Optimal split favors deeper liquidity
- ✓ Helper functions (argmax, argmin)

---

### Integration Tests

```bash
cargo test --lib router::tests -- --nocapture
```

**Tests**:
- ✓ USDC direct transfer
- ✓ Multi-hop ETH to XLM
- ✓ Large order multi-path splitting
- ✓ Small order no splitting

---

### Full Multi-Path Test Suite

```bash
cargo test --test multi_path_routing_tests -- --nocapture
```

**Tests**:
- ✓ Large order triggers optimization
- ✓ Small order uses single-path
- ✓ Multi-path reduces slippage
- ✓ Parallel paths have valid payloads
- ✓ Duration calculation correct
- ✓ Edge case at exact threshold
- ✓ Cross-chain asset conversion

---

## Verification Checklist

### ✅ Acceptance Criteria

- [ ] **Criterion 1**: $1M route successfully split across multiple bridges
  - Test: `test_large_order_multi_path_splitting`
  - Verify: `is_split_route == true` and `parallel_paths.len() >= 2`

- [ ] **Criterion 2**: Split route slippage lower than single-path
  - Test: `test_multi_path_reduces_slippage_vs_single_path`
  - Verify: Multi-path net value ≥ best single-path net value

- [ ] **Criterion 3**: JSON schema supports parallel execution
  - Test: All API responses
  - Verify: `parallel_paths` array present with all required fields

- [ ] **Criterion 4**: Safe fallback when gas > slippage savings
  - Test: `test_small_order_no_splitting`
  - Verify: Small orders don't unnecessarily split

---

## Expected Test Output

### Successful Test Run

```
running 16 tests

✓ Small order routing test passed

=== $1M Order Routing Result ===
Provider: Circle CCTP (50.0%) + deBridge DLN (50.0%)
Is Split: true
Amount Out: $124978751
Fee: $15.00
Slippage: 99.988%

Parallel Paths:
  Circle CCTP - 50.0% ($500000000000 in, $99980003 out, 99.980% slippage)
  deBridge DLN - 50.0% ($500000000000 in, $24998748 out, 99.995% slippage)

✓ Large order routing test passed

test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

---

## Debugging Tips

### Issue: Tests Fail to Compile

```bash
# Clean and rebuild
cargo clean
cargo build

# Check for syntax errors
cargo check
```

### Issue: Tests Timeout

```bash
# Increase timeout (if needed)
cargo test -- --test-threads=1 --nocapture
```

### Issue: Server Won't Start

```bash
# Check port availability
lsof -i :8080

# Use different port
PORT=9090 cargo run
```

### Issue: API Returns Errors

Check request format:
- `source_chain` and `dest_chain` must be valid enum values: "Ethereum", "Arbitrum", "Solana", "Stellar"
- `amount_in` must be > 0
- Assets must be non-empty strings

---

## Performance Testing

### Load Testing Script

```bash
# Install apache bench
sudo apt-get install apache2-utils

# Test small orders (should be fast)
ab -n 100 -c 10 -T application/json -p small_order.json \
  http://localhost:8080/api/v1/quote

# Test large orders (may be slower due to optimization)
ab -n 50 -c 5 -T application/json -p large_order.json \
  http://localhost:8080/api/v1/quote
```

**small_order.json**:
```json
{
  "source_chain": "Ethereum",
  "dest_chain": "Stellar",
  "source_asset": "USDC",
  "dest_asset": "USDC",
  "amount_in": 10000
}
```

**large_order.json**:
```json
{
  "source_chain": "Ethereum",
  "dest_chain": "Stellar",
  "source_asset": "USDC",
  "dest_asset": "USDC",
  "amount_in": 1000000000000
}
```

---

## Continuous Integration

### GitHub Actions Workflow

```yaml
name: Test Multi-Path Routing

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Run Tests
        run: |
          cd wow-engine
          cargo test --verbose
      - name: Run Multi-Path Tests
        run: |
          cd wow-engine
          cargo test multi_path -- --nocapture
```

---

## Test Data Reference

### Amount Scales

| Amount | Description | Expected Behavior |
|--------|-------------|-------------------|
| 10,000 | $10k | Single-path |
| 100,000 | $100k | Threshold check |
| 1,000,000 | $1M | Multi-path split |
| 10,000,000 | $10M | Heavy split |

**Note**: Amounts in smallest units (e.g., USDC with 6 decimals: 10,000 USDC = 10,000,000,000 smallest units)

### Supported Chains

- `Ethereum` - EVM mainnet
- `Arbitrum` - L2 rollup
- `Solana` - Fast chain
- `Stellar` - Destination chain

### Supported Assets

- `USDC` - Stablecoin (works with CCTP + DeBridge)
- `ETH` - Native Ethereum (works with DeBridge)
- `SOL` - Native Solana
- `XLM` - Native Stellar

---

## Troubleshooting

### Common Issues

**Issue**: "No available bridges for routing"
- **Solution**: Check asset compatibility. CCTP only works with USDC.

**Issue**: Slippage seems too high
- **Solution**: This is expected for simulated liquidity. Production will use real on-chain data.

**Issue**: Split percentages don't sum to 100%
- **Solution**: Check for rounding errors. Code enforces ±0.1% tolerance.

**Issue**: Test compilation errors
- **Solution**: Ensure you're on the feature branch:
  ```bash
  git checkout feature/multi-path-routing-order-splitting
  ```

---

## Next Steps

After successful testing:

1. ✅ Create Pull Request on GitHub
2. ✅ Request code review from team
3. ✅ Run integration tests on staging
4. ✅ Performance benchmark on production-like load
5. ✅ Gradual rollout with feature flag

---

## Resources

- **Feature Documentation**: `FEATURE_MULTI_PATH_ROUTING.md`
- **Implementation Summary**: `IMPLEMENTATION_SUMMARY.md`
- **Source Code**: `wow-engine/src/router/`
- **Tests**: `wow-engine/tests/multi_path_routing_tests.rs`

---

*Happy Testing! 🚀*
