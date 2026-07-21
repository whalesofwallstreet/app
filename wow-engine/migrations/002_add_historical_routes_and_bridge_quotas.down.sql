-- Migration: 002_add_historical_routes_and_bridge_quotas
-- Direction: DOWN (rollback)
-- WARNING: This is destructive. All data in these tables will be permanently lost.

DROP TABLE IF EXISTS bridge_quotas;
DROP TABLE IF EXISTS historical_routes;
