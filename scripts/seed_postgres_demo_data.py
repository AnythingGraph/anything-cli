#!/usr/bin/env python3
"""
Seed Postgres CRM demo data for Anything CLI playbooks.

Uses the existing schema:
  users(user_id TEXT PK, full_name TEXT NOT NULL)
  accounts(account_name TEXT PK, industry TEXT, owner_user_id TEXT FK -> users)

Creates and fills:
  "transaction"(transaction_id, user_id, amount, currency, transaction_date, category)
  (quoted name — transaction is a reserved word in SQL)

Defaults: 5,000 users + 5,000 accounts (10,000 rows) and 5,000 transactions.

Usage:
  export AG_SQL_DSN="postgres://user:pass@localhost:5432/yourdb"
  pip install 'psycopg[binary]'   # one-time
  python scripts/seed_postgres_demo_data.py

Options:
  --users 5000 --accounts 5000 --transactions 5000
  --dry-run          # print counts only, no writes
  --reset            # TRUNCATE transactions, accounts, users (demo data only)
"""

from __future__ import annotations

import argparse
import os
import random
import string
import sys
from datetime import date, timedelta
from decimal import Decimal
from typing import Iterable, Sequence

try:
    import psycopg
    from psycopg.rows import tuple_row
except ImportError:
    print("Install psycopg: pip install 'psycopg[binary]'", file=sys.stderr)
    sys.exit(1)


FIRST_NAMES = (
    "Alex", "Jordan", "Taylor", "Morgan", "Casey", "Riley", "Quinn", "Avery",
    "Blake", "Cameron", "Dakota", "Emery", "Finley", "Harper", "Jamie", "Kai",
    "Logan", "Noah", "Olivia", "Parker", "Reese", "Sage", "Skyler", "Tatum",
)

LAST_NAMES = (
    "Anderson", "Brooks", "Chen", "Diaz", "Ellis", "Foster", "Garcia", "Hayes",
    "Ivanov", "Johnson", "Kim", "Lopez", "Martinez", "Nguyen", "Okafor", "Patel",
    "Quinn", "Reed", "Singh", "Torres", "Upton", "Vargas", "Walker", "Young",
)

INDUSTRIES = (
    "Retail", "Technology", "Manufacturing", "Healthcare", "Finance",
    "Education", "Energy", "Logistics", "Media", "Hospitality",
)

COMPANY_PREFIXES = (
    "Northwind", "Contoso", "Fabrikam", "Adventure", "Tailspin", "Wide World",
    "Blue Yonder", "Lucerne", "Alpine", "Summit", "Horizon", "Pioneer",
)

COMPANY_SUFFIXES = (
    "Traders", "Ltd", "Inc", "Group", "Partners", "Systems", "Solutions",
    "Holdings", "Industries", "Labs",
)

TRANSACTION_CATEGORIES = (
    "subscription", "license", "services", "hardware", "support", "training",
    "consulting", "renewal", "upgrade", "refund",
)

BATCH_SIZE = 500


# Build a Postgres connection string from AG_SQL_DSN.
def resolve_database_url() -> str:
    database_url = os.environ.get("AG_SQL_DSN", "").strip()
    if not database_url:
        print("Set AG_SQL_DSN, e.g. postgres://user:pass@localhost:5432/yourdb", file=sys.stderr)
        sys.exit(1)
    return database_url


# Generate one unique user row (user_id, full_name).
def build_user_row(index: int, random_generator: random.Random) -> tuple[str, str]:
    first_name = random_generator.choice(FIRST_NAMES)
    last_name = random_generator.choice(LAST_NAMES)
    user_id = f"demo.user.{index:05d}"
    full_name = f"{first_name} {last_name}"
    return user_id, full_name


# Generate one unique account row (account_name, industry, owner_user_id).
def build_account_row(
    index: int,
    owner_user_ids: Sequence[str],
    random_generator: random.Random,
) -> tuple[str, str | None, str]:
    prefix = random_generator.choice(COMPANY_PREFIXES)
    suffix = random_generator.choice(COMPANY_SUFFIXES)
    account_name = f"{prefix} {suffix} #{index:05d}"
    industry = random_generator.choice(INDUSTRIES)
    owner_user_id = random_generator.choice(owner_user_ids)
    return account_name, industry, owner_user_id


# Generate one transaction row linked to an existing user_id.
def build_transaction_row(
    index: int,
    user_ids: Sequence[str],
    random_generator: random.Random,
) -> tuple[str, str, Decimal, str, date, str]:
    transaction_id = f"txn_{index:06d}"
    user_id = random_generator.choice(user_ids)
    amount = Decimal(random_generator.randint(500, 250000)) / Decimal("100")
    currency = "USD"
    days_ago = random_generator.randint(0, 730)
    transaction_date = date.today() - timedelta(days=days_ago)
    category = random_generator.choice(TRANSACTION_CATEGORIES)
    return transaction_id, user_id, amount, currency, transaction_date, category


# Insert rows in batches with ON CONFLICT DO NOTHING for idempotent re-runs.
def insert_batches(
    connection: psycopg.Connection,
    insert_sql: str,
    rows: Iterable[Sequence[object]],
    label: str,
    dry_run: bool,
) -> int:
    row_list = list(rows)
    if dry_run:
        print(f"[dry-run] would insert {len(row_list)} {label}")
        return len(row_list)

    inserted_total = 0
    with connection.cursor() as cursor:
        for batch_start in range(0, len(row_list), BATCH_SIZE):
            batch = row_list[batch_start : batch_start + BATCH_SIZE]
            cursor.executemany(insert_sql, batch)
            inserted_total += cursor.rowcount
    connection.commit()
    print(f"Inserted {inserted_total} {label} (attempted {len(row_list)})")
    return inserted_total


# Create the transaction table if it does not exist.
def ensure_transaction_table(connection: psycopg.Connection, dry_run: bool) -> None:
    ddl = """
    CREATE TABLE IF NOT EXISTS "transaction" (
      transaction_id   TEXT PRIMARY KEY,
      user_id          TEXT NOT NULL REFERENCES users(user_id),
      amount           NUMERIC(12, 2) NOT NULL,
      currency         TEXT NOT NULL DEFAULT 'USD',
      transaction_date DATE NOT NULL,
      category         TEXT
    );

    CREATE INDEX IF NOT EXISTS transaction_user_id_idx ON "transaction"(user_id);
    CREATE INDEX IF NOT EXISTS transaction_date_idx ON "transaction"(transaction_date);
    """
    if dry_run:
        print('[dry-run] would create "transaction" table + indexes')
        return

    with connection.cursor() as cursor:
        cursor.execute(ddl)
    connection.commit()
    print('Ensured "transaction" table exists')


# Optionally clear demo tables (destructive).
def reset_tables(connection: psycopg.Connection, dry_run: bool) -> None:
    reset_sql = """
    TRUNCATE TABLE "transaction" RESTART IDENTITY CASCADE;
    TRUNCATE TABLE accounts RESTART IDENTITY CASCADE;
    TRUNCATE TABLE users RESTART IDENTITY CASCADE;
    """
    if dry_run:
        print('[dry-run] would TRUNCATE users, accounts, "transaction"')
        return

    with connection.cursor() as cursor:
        cursor.execute(reset_sql)
    connection.commit()
    print("Truncated users, accounts, transaction")


# Load all user_id values currently in the database.
def load_user_ids(connection: psycopg.Connection) -> list[str]:
    with connection.cursor(row_factory=tuple_row) as cursor:
        cursor.execute("SELECT user_id FROM users")
        return [row[0] for row in cursor.fetchall()]


# Parse CLI arguments.
def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Seed Postgres CRM demo data")
    parser.add_argument("--users", type=int, default=5000, help="Number of users to insert")
    parser.add_argument("--accounts", type=int, default=5000, help="Number of accounts to insert")
    parser.add_argument("--transactions", type=int, default=5000, help="Number of transactions to insert")
    parser.add_argument("--seed", type=int, default=42, help="Random seed for reproducibility")
    parser.add_argument("--dry-run", action="store_true", help="Print plan without writing")
    parser.add_argument(
        "--reset",
        action="store_true",
        help="Truncate users, accounts, transactions before seeding (destructive)",
    )
    return parser.parse_args()


# Main entry point.
def main() -> None:
    arguments = parse_arguments()
    random_generator = random.Random(arguments.seed)
    database_url = resolve_database_url()

    print(
        f"Plan: {arguments.users} users, {arguments.accounts} accounts, "
        f"{arguments.transactions} transactions (seed={arguments.seed})"
    )

    user_rows = [
        build_user_row(index, random_generator)
        for index in range(1, arguments.users + 1)
    ]

    with psycopg.connect(database_url) as connection:
        if arguments.reset:
            reset_tables(connection, arguments.dry_run)

        insert_batches(
            connection,
            """
            INSERT INTO users (user_id, full_name)
            VALUES (%s, %s)
            ON CONFLICT (user_id) DO NOTHING
            """,
            user_rows,
            "users",
            arguments.dry_run,
        )

        all_user_ids = load_user_ids(connection) if not arguments.dry_run else [row[0] for row in user_rows]
        if not all_user_ids:
            print("No user_id values available for accounts/transactions", file=sys.stderr)
            sys.exit(1)

        account_rows = [
            build_account_row(index, all_user_ids, random_generator)
            for index in range(1, arguments.accounts + 1)
        ]
        insert_batches(
            connection,
            """
            INSERT INTO accounts (account_name, industry, owner_user_id)
            VALUES (%s, %s, %s)
            ON CONFLICT (account_name) DO NOTHING
            """,
            account_rows,
            "accounts",
            arguments.dry_run,
        )

        ensure_transaction_table(connection, arguments.dry_run)

        if not arguments.dry_run:
            all_user_ids = load_user_ids(connection)

        transaction_rows = [
            build_transaction_row(index, all_user_ids, random_generator)
            for index in range(1, arguments.transactions + 1)
        ]
        insert_batches(
            connection,
            """
            INSERT INTO "transaction" (
              transaction_id, user_id, amount, currency, transaction_date, category
            )
            VALUES (%s, %s, %s, %s, %s, %s)
            ON CONFLICT (transaction_id) DO NOTHING
            """,
            transaction_rows,
            "transaction rows",
            arguments.dry_run,
        )

    print("Done.")


if __name__ == "__main__":
    main()
