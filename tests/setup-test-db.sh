#!/usr/bin/env bash
set -e

echo "Set up test databases"
user=benchkittest
password=benchkitpw
db_name=benchcointests

# Check if postgres is running
if ! pg_isready; then
  echo "PostgreSQL is not running"
  exit 1
fi

# Create user if it doesn't exist
if ! sudo -u postgres psql -tAc "SELECT 1 FROM pg_roles WHERE rolname='$user'" | grep -q 1; then
  sudo -u postgres psql -c "CREATE USER \"$user\" WITH PASSWORD '$password'"
fi

# Grant CREATEDB permission if user doesn't have it
if ! sudo -u postgres psql -tAc "SELECT 1 FROM pg_roles WHERE rolname='$user' AND rolcreatedb" | grep -q 1; then
    sudo -u postgres psql -c "ALTER USER \"$user\" CREATEDB"
    echo "Granted CREATEDB permission to $user"
else
    echo "User $user already has CREATEDB permission"
fi

# Create test database if it doesn't exist
if ! sudo -u postgres psql -tAc "SELECT 1 FROM pg_database WHERE datname='$db_name'" | grep -q 1; then
    sudo -u postgres psql -c "CREATE DATABASE \"$db_name\" WITH OWNER=\"$user\""
    sudo -u postgres psql -c "GRANT ALL PRIVILEGES ON DATABASE \"$db_name\" TO \"$user\""
    echo "Created new test database"
else
    echo "Test database already exists"
fi

echo "Test database setup completed successfully"
