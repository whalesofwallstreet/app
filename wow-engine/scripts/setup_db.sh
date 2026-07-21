#!/bin/bash
# Database setup script for Wow Engine
# Sets up PostgreSQL database and runs migrations

set -e

# Configuration
DB_NAME="${DB_NAME:-wow_engine}"
DB_USER="${DB_USER:-postgres}"
DB_PASSWORD="${DB_PASSWORD:-postgres}"
DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-5432}"
DATABASE_URL="postgres://${DB_USER}:${DB_PASSWORD}@${DB_HOST}:${DB_PORT}/${DB_NAME}"

echo "Setting up Wow Engine database..."
echo "   Host: $DB_HOST:$DB_PORT"
echo "   Database: $DB_NAME"
echo "   User: $DB_USER"

# Check if psql is available
if ! command -v psql &> /dev/null; then
    echo "ERROR: psql not found. Please install PostgreSQL client tools."
    exit 1
fi

# Check if sqlx-cli is available
if ! command -v sqlx &> /dev/null; then
    echo "WARNING: sqlx-cli not found. Installing..."
    cargo install sqlx-cli --no-default-features --features postgres
fi

# Create database
echo "Creating database '$DB_NAME'..."
PGPASSWORD="$DB_PASSWORD" psql -h "$DB_HOST" -U "$DB_USER" -p "$DB_PORT" -tc "SELECT 1 FROM pg_database WHERE datname = '$DB_NAME'" | grep -q 1 || \
    PGPASSWORD="$DB_PASSWORD" psql -h "$DB_HOST" -U "$DB_USER" -p "$DB_PORT" -c "CREATE DATABASE $DB_NAME;"

echo "Database '$DB_NAME' ready."

# Run migrations
echo "Running migrations..."
DATABASE_URL="$DATABASE_URL" sqlx migrate run

echo "Migrations completed successfully!"
echo ""
echo "Next steps:"
echo "   1. Set DATABASE_URL environment variable:"
echo "      export DATABASE_URL='$DATABASE_URL'"
echo ""
echo "   2. Run the server:"
echo "      cargo run"
echo ""
echo "   3. Verify database connection:"
echo "      curl http://localhost:8080/api/v1/health"
echo ""
