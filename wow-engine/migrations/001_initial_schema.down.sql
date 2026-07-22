-- Migration: 001_initial_schema
-- Direction: DOWN (rollback)
-- WARNING: This is destructive. All data in these tables will be permanently lost.

DROP TABLE IF EXISTS anchor_transactions;
DROP TABLE IF EXISTS user_quotas;
DROP TABLE IF EXISTS user_history;
DROP TABLE IF EXISTS route_executions;
