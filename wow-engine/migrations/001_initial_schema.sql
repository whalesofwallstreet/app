-- Initial schema for cross-chain routing and transaction tracking
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
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL,
    CONSTRAINT check_amounts CHECK (amount_in > 0 AND amount_out >= 0),
    INDEX idx_user_id (user_id),
    INDEX idx_status (status),
    INDEX idx_created_at (created_at)
);

-- User activity history: tracks user actions related to routes
CREATE TABLE IF NOT EXISTS user_history (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL,
    route_execution_id UUID NOT NULL,
    action VARCHAR(255) NOT NULL,
    details TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    FOREIGN KEY (route_execution_id) REFERENCES route_executions(id) ON DELETE CASCADE,
    INDEX idx_user_id (user_id),
    INDEX idx_route_execution_id (route_execution_id),
    INDEX idx_created_at (created_at)
);

-- User quotas: tracks daily usage limits
CREATE TABLE IF NOT EXISTS user_quotas (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL UNIQUE,
    daily_limit_usd NUMERIC(18, 2) NOT NULL,
    used_today_usd NUMERIC(18, 2) NOT NULL DEFAULT 0,
    reset_at TIMESTAMP WITH TIME ZONE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL,
    CONSTRAINT check_quota_amounts CHECK (used_today_usd >= 0 AND daily_limit_usd > 0),
    INDEX idx_reset_at (reset_at)
);

-- Anchor transactions: tracks SEP-24 and SEP-38 transactions
CREATE TABLE IF NOT EXISTS anchor_transactions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL,
    route_execution_id UUID NOT NULL,
    anchor_domain VARCHAR(255) NOT NULL,
    transaction_id VARCHAR(255) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    url TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL,
    FOREIGN KEY (route_execution_id) REFERENCES route_executions(id) ON DELETE CASCADE,
    INDEX idx_user_id (user_id),
    INDEX idx_route_execution_id (route_execution_id),
    INDEX idx_status (status)
);
