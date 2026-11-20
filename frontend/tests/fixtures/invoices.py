#!/usr/bin/env python3
"""Build a synthetic SQLite invoice source for the large live-table browser smoke."""
from contextlib import closing
from pathlib import Path
import sqlite3
import sys

action, filename = sys.argv[1:]
path = Path(filename)
if action == "init":
    # Only create a fixture in the caller's private temporary test directory.
    path.touch(exist_ok=False)
elif action != "expand":
    raise ValueError(f"Unknown fixture action: {action}")
with closing(sqlite3.connect(path.as_uri() + "?mode=rw", uri=True, timeout=10)) as db, db:
    if action == "init":
        db.execute("PRAGMA application_id = 0x474C494E")
        db.execute("CREATE TABLE invoices (id INTEGER PRIMARY KEY, customer_name TEXT NOT NULL, amount_cents INTEGER NOT NULL)")
        start, end = 1, 10
    else:
        assert db.execute("PRAGMA application_id").fetchone()[0] == 0x474C494E
        assert db.execute("SELECT count(*), max(id) FROM invoices").fetchone() == (10, 10)
        start, end = 11, 10_000_000
    db.execute(
        """
        WITH RECURSIVE ids(id) AS (VALUES(?) UNION ALL SELECT id + 1 FROM ids WHERE id < ?)
        INSERT INTO invoices
        SELECT id, printf('Fixture customer %05d', id % 10000), 1000 + id % 250000
        FROM ids
        """,
        (start, end),
    )
