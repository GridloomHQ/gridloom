#!/usr/bin/env python3
"""Generate synthetic journal entries for Gridloom's read-only SQLite ledger."""
import argparse
from contextlib import closing
from pathlib import Path
import json
import sqlite3


DEFAULT_ROWS = 4_000_000
MAX_ROWS = 10_000_000
DAY_ROWS = 100_000

# Dates are deliberately non-unique. Accounts and amounts vary within each date.
SCHEMA = """
CREATE TABLE entries (
    id INTEGER PRIMARY KEY,
    posted TEXT NOT NULL,
    account TEXT NOT NULL,
    amount_cents INTEGER NOT NULL
)
"""
INSERT = """
WITH RECURSIVE ids(id) AS (
    VALUES(?)
    UNION ALL SELECT id + 1 FROM ids WHERE id < ?
)
INSERT INTO entries
SELECT id,
       date('2026-01-01', printf('+%d days', (id - 1) / ?)),
       printf('%d', 1100 + ((id - 1) % 10) * 100),
       CASE WHEN (id - 1) % 3 = 0 THEN -1 ELSE 1 END *
           (10000 + (id - 1) % 3701)
FROM ids
"""


def create_demo(path: Path, rows: int) -> dict:
    if not 1 <= rows <= MAX_ROWS:
        raise ValueError(f"rows must be between 1 and {MAX_ROWS:,}")
    # Exclusive creation also protects existing files and symlinks.
    path.touch(exist_ok=False)
    try:
        with closing(sqlite3.connect(path)) as db, db:
            db.execute(SCHEMA)
            for start in range(1, rows + 1, DAY_ROWS):
                db.execute(INSERT, (start, min(start + DAY_ROWS - 1, rows), DAY_ROWS))
            db.execute("CREATE INDEX entries_posted ON entries(posted)")
            db.execute("CREATE INDEX entries_account_posted ON entries(account, posted)")
            sample = {"account": "1100", "from": "2026-01-01", "through": "2026-01-07"}
            count, cents = db.execute(
                "SELECT COUNT(*), COALESCE(SUM(amount_cents), 0) FROM entries "
                "WHERE account = ? AND posted BETWEEN ? AND ?",
                tuple(sample.values()),
            ).fetchone()
    except BaseException:
        path.unlink()
        raise
    return {"rows": rows, "sample": {**sample, "rows": count, "total_cents": str(cents)}}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("file", type=Path, help="new SQLite file (existing files are not overwritten)")
    parser.add_argument("--rows", type=int, default=DEFAULT_ROWS)
    args = parser.parse_args()
    try:
        result = create_demo(args.file, args.rows)
    except (OSError, sqlite3.Error, ValueError) as error:
        parser.exit(1, f"{parser.prog}: {error}\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
