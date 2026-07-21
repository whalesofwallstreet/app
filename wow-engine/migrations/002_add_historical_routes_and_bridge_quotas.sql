-- Migration: 002_add_historical_routes_and_bridge_quotas
-- Description: Adds historical_routes table for the GC job (Issue #16) and
--              bridge_quotas table for cross-chain bridge rate limiting.
-- Direction: UP

-- Historical routes: immutable archive of completed route executions.
-- The background GC job (Issue #16) periodically purges stale entries from this table.
CREATE TABLE IF NOT EXISTS historical_routes (
    id UUID PRIMARY KEY,
    original_route_execution_id UUID NOT NULL,
    user_id UUID NOT NULL,
    source_chain VARCHAR(255) NOT NULL,
    dest_chain VARCHAR(255) NOT NULL,
    source_asset VARCHAR(255) NOT NULL,
    dest_asset VARCHAR(255) NOT NULL,
    amount_in BIGINT NOT NULL,
    amount_out BIGINT NOT NULL,
    provider VARCHAR(255) NOT NULL,
    path TEXT NOT NULL,
    estimated_fee_usd NUMERIC(18, 8) NOT NULL,
    final_status VARCHAR(50) NOT NULL,
    archived_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    original_created_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT check_historical_amounts CHECK (amount_in > 0 AND amount_out >= 0)
);

CREATE INDEX IF NOT EXISTS idx_historical_routes_user_id ON historical_routes (user_id);
CREATE INDEX IF NOT EXISTS idx_historical_routes_archived_at ON historical_routes (archived_at);
CREATE INDEX IF NOT EXISTS idx_historical_routes_original_created_at ON historical_routes (original_created_at);
CREATE INDEX IF NOT EXISTS idx_historical_routes_final_status ON historical_routes (final_status);

-- Bridge quotas: tracks per-bridge rate and volume limits to prevent abuse.
-- Complements user_quotas by enforcing limits at the bridge/provider level.
CREATE TABLE IF NOT EXISTS bridge_quotas (
    id UUID PRIMARY KEY,
    bridge_provider VARCHAR(255) NOT NULL UNIQUE,
    daily_volume_limit_usd NUMERIC(18, 2) NOT NULL,
    used_volume_today_usd NUMERIC(18, 2) NOT NULL DEFAULT 0,
    max_tx_per_hour INT NOT NULL DEFAULT 1000,
    tx_count_this_hour INT NOT NULL DEFAULT 0,
    hour_window_start TIMESTAMPTZ NOT NULL,
    reset_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT check_bridge_quota_volumes
        CHECK (used_volume_today_usd >= 0 AND daily_volume_limit_usd > 0),
    CONSTRAINT check_bridge_quota_tx
        CHECK (tx_count_this_hour >= 0 AND max_tx_per_hour > 0)
);

CREATE INDEX IF NOT EXISTS idx_bridge_quotas_reset_at ON bridge_quotas (reset_at);
CREATE INDEX IF NOT EXISTS idx_bridge_quotas_hour_window_start ON bridge_quotas (hour_window_start);
