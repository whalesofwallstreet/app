-- Migration: 001_initial_schema
-- Description: Initial schema for cross-chain routing and transaction tracking
-- Direction: UP
-- Isolation level: READ COMMITTED (default) for balancing consistency and concurrency

-- Route executions: tracks each routing operation
CREATE TABLE IF NOT EXISTS route_executions (
    id UUID PRIMARY KEY,
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
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT check_amounts CHECK (amount_in > 0 AND amount_out >= 0)
);

CREATE INDEX IF NOT EXISTS idx_route_executions_user_id ON route_executions (user_id);
CREATE INDEX IF NOT EXISTS idx_route_executions_status ON route_executions (status);
CREATE INDEX IF NOT EXISTS idx_route_executions_created_at ON route_executions (created_at);

-- User activity history: tracks user actions related to routes
CREATE TABLE IF NOT EXISTS user_history (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL,
    route_execution_id UUID NOT NULL,
    action VARCHAR(255) NOT NULL,
    details TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT fk_user_history_route_execution
        FOREIGN KEY (route_execution_id) REFERENCES route_executions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_user_history_user_id ON user_history (user_id);
CREATE INDEX IF NOT EXISTS idx_user_history_route_execution_id ON user_history (route_execution_id);
CREATE INDEX IF NOT EXISTS idx_user_history_created_at ON user_history (created_at);

-- User quotas: tracks daily usage limits per user
CREATE TABLE IF NOT EXISTS user_quotas (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL UNIQUE,
    daily_limit_usd NUMERIC(18, 2) NOT NULL,
    used_today_usd NUMERIC(18, 2) NOT NULL DEFAULT 0,
    reset_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT check_quota_amounts CHECK (used_today_usd >= 0 AND daily_limit_usd > 0)
);

CREATE INDEX IF NOT EXISTS idx_user_quotas_reset_at ON user_quotas (reset_at);

-- Anchor transactions: tracks SEP-24 and SEP-38 transactions
CREATE TABLE IF NOT EXISTS anchor_transactions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL,
    route_execution_id UUID NOT NULL,
    anchor_domain VARCHAR(255) NOT NULL,
    transaction_id VARCHAR(255) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    url TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT fk_anchor_transactions_route_execution
        FOREIGN KEY (route_execution_id) REFERENCES route_executions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_anchor_transactions_user_id ON anchor_transactions (user_id);
CREATE INDEX IF NOT EXISTS idx_anchor_transactions_route_execution_id ON anchor_transactions (route_execution_id);
CREATE INDEX IF NOT EXISTS idx_anchor_transactions_status ON anchor_transactions (status);
