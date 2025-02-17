#!/usr/bin/env bash
set -e

user=benchkittest
db_name=benchcointests

if ! pg_isready; then
  echo "PostgreSQL is not running"
  exit 1
fi

# Drop database if it exists
if sudo -u postgres psql -tAc "SELECT 1 FROM pg_database WHERE datname='$db_name'" | grep -q 1; then
  echo "Dropping database $db_name"
  sudo -u postgres psql -c "DROP DATABASE \"$db_name\""
else
  echo "Database $db_name does not exist"
fi

# Drop user if it exists
if sudo -u postgres psql -tAc "SELECT 1 FROM pg_roles WHERE rolname='$user'" | grep -q 1; then
  echo "Dropping user $user"
  sudo -u postgres psql -c "DROP USER \"$user\""
else
  echo "User $user does not exist"
fi

echo "Cleanup completed successfully"
