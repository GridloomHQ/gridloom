# Gridloom

[![CI](https://github.com/lhemerly/gridloom/actions/workflows/ci.yml/badge.svg)](https://github.com/lhemerly/gridloom/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**A local-first spreadsheet for following the numbers, not just calculating them.**

Gridloom combines a familiar worksheet with SQL-connected data and accounting working papers. Build a calculation, trace its inputs, ask a question about a number, and retain the evidence behind a review. It runs on your computer, either in a browser backed by a local server or in a native desktop window.

It is aimed at accounting and financial models where understanding **where a number came from** matters as much as the result. Gridloom is an early-stage application, not a drop-in replacement for Excel or an audited accounting system.

## What you can do

### Work in a spreadsheet

- Use multiple sheets, cross-sheet formulas, named table references and dynamic array spills.
- Select, copy and paste ranges; fill formulas; format numbers and cells; undo and redo supported cell operations.
- Find cached values or formula inputs, jump to a cell, resize columns, freeze the first row or column, and show formulas directly in the grid.
- Get selection totals, complete supported function names in the formula bar, and cycle relative/absolute references with F4.
- Build pivot tables and column, bar, line, pie or scatter charts.
- Save and reopen `.gridloom` workbooks, including layouts, calculations and pending source edits.

The formula engine includes arithmetic, statistics, logic, text, date, lookup and financial functions, plus `SQLVALUE` and `SQLTABLE`. Examples include `SUMIFS`, `SUMPRODUCT`, `XLOOKUP`, `FILTER`, `PMT` and `NPV`. The formula bar's suggestions and the local `/api/functions` endpoint expose the engine's supported function names and aliases.

### Bring SQL data into the worksheet

Connect an existing **SQLite** or **DuckDB** file, run a read-only query, and insert its result as a named table. Keep ordinary cells and formulas alongside it:

```excel
=SUM(Invoices[Amount])
=AVERAGEIFS(Invoices[Amount], Invoices[Status], "Paid")
=SQLVALUE("SalesDB", "SELECT SUM(amount) FROM invoices WHERE customer_name = ?", A2)
```

The database stays in its original format. Eligible single-table projections with direct column mappings and declared primary keys can support staged edits. Joined, aggregated or ambiguous results remain read-only.

Editing a source-backed cell does **not** immediately change the database. **Review Edits** shows original and proposed values before an explicit apply. Applying a changeset checks for conflicting external changes and uses a database transaction; a conflict or error rolls back that changeset. Saving a workbook is separate from applying its source edits.

### Explain and review a calculation

Use **Explain & Review** to inspect a cell's formula, captured inputs and available source-row provenance. Follow upstream inputs and downstream consumers, compare observed states, and attach supporting files with retained versions.

Record a review or circle up an open question. Questions can be resolved or reopened, and changed calculation snapshots invalidate previous reviews. Source-row provenance is available only where the query's lineage can be established; it is not inferred for arbitrary SQL.

### Prepare accounting working papers

**Monthly close** provides a focused trial-balance workflow:

1. Import a trial-balance CSV and map accounts to a report.
2. Define entity, currency, units and report hierarchy.
3. Generate a P&L and inspect debit/credit reconciliation and review items.
4. Add balanced adjustment journals with rationale and supporting evidence without replacing the imported trial balance.
5. See imported, adjusted and reported balances, roll mappings into a new period, and export a JSON review pack, P&L CSV or readable HTML summary.

## Get started

### Use a Linux package

Open a **successful main-branch run** in [Actions → CI](https://github.com/lhemerly/gridloom/actions/workflows/ci.yml) and download its `gridloom-linux-x86_64-<commit>` artifact. Extract the downloaded ZIP to find the server/browser and desktop tarballs, their SHA-256 files and verification logs. Artifacts expire after 14 days.

Choose the server variant for a browser interface, or desktop for a native window. From the directory containing the downloaded tarballs, substitute the actual filename below:

```bash
archive='gridloom-<version>-linux-x86_64-server.tar.gz'
sha256sum -c "$archive.sha256"
tar -xzf "$archive"
"./${archive%.tar.gz}/gridloom" --no-samples
```

For the server variant, open the local URL printed in the terminal. Omit `--no-samples` to start with sample data.

Packages include frontend assets and DuckDB, but not every system library. They are built and checked on Ubuntu 24.04 x86_64; the desktop variant needs compatible GTK3/WebKitGTK 4.1 runtime libraries. They are **not signed installers or universal Linux images**. Inspect the extracted `docs/shared-libraries.txt` and `docs/build-info.json` for dependencies and source identity. Checksums detect altered bytes, not publisher authenticity.

### Build from source on Linux

Use a current stable Rust toolchain, an up-to-date Node.js 22 or newer, npm, Python 3.11+, a C/C++ build toolchain, `curl` and `unzip`. CI runs on Ubuntu; other operating systems are not covered by the packaged verification path.

```bash
git clone https://github.com/lhemerly/gridloom.git
cd gridloom

# Build tools on Ubuntu/Debian.
sudo apt-get update
sudo apt-get install -y build-essential pkg-config curl unzip

# Optional: native desktop dependencies (omit for server/browser-only builds).
sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev

cd frontend
npm ci
npm run build
cd ..
```

The Rust DuckDB binding also needs a matching native DuckDB library. The following uses the version encoded in the checked-in lockfile and the official Linux x86_64 release, as CI does:

```bash
duckdb_version=$(python3 - <<'PY'
import tomllib
from pathlib import Path
packages = tomllib.loads(Path("Cargo.lock").read_text())["package"]
version = next(p["version"] for p in packages if p["name"] == "libduckdb-sys")
encoded = int(version.split(".")[1])
print(f"{encoded // 10000}.{encoded // 100 % 100}.{encoded % 100}")
PY
)
export DUCKDB_LIB_DIR="$HOME/.cache/gridloom/duckdb/$duckdb_version"
export DUCKDB_INCLUDE_DIR="$DUCKDB_LIB_DIR"
mkdir -p "$DUCKDB_LIB_DIR"
curl --fail --location --retry 3 \
  "https://github.com/duckdb/duckdb/releases/download/v$duckdb_version/libduckdb-linux-amd64.zip" \
  --output "$DUCKDB_LIB_DIR/libduckdb.zip"
unzip -q -o "$DUCKDB_LIB_DIR/libduckdb.zip" -d "$DUCKDB_LIB_DIR"
export LD_LIBRARY_PATH="$DUCKDB_LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

cargo run --locked -p gridloom-server
```

Open **http://127.0.0.1:3001**. For a separate native-window session, stop the server and run `cargo run --locked -p gridloom-desktop` instead. Keep the DuckDB environment variables set in the shell used to build or launch either application.

For frontend development, run the server in one terminal and `cd frontend && npm run dev` in another; Vite proxies API requests to port 3001. For local packaging, use `scripts/package-linux.sh server` or `scripts/package-linux.sh desktop`.

### Try a first workbook

Start with the sample databases to explore SQL tables, or use **Monthly close** for a trial balance. Add a formula outside any occupied table or spill range, select its result, and open **Explain & Review**. Save the workbook before closing the application. Use **Save As** to keep a separate copy.

## Important boundaries

- **Not Excel compatibility.** There is no general `.xlsx` import/export or complete Excel function/coercion compatibility. For example, `ROUND` uses half-even rounding; some functions and aliases intentionally retain Gridloom-specific coercions and behavior.
- **Fixed precision, not arbitrary precision.** Decimal values support accounting-oriented calculations, but some functions use floating-point operations or fixed-precision rounding. Numeric hardening is ongoing; this is not a guarantee that every calculation is exact.
- **Local working papers, not assurance.** Reviewer names are self-reported, not authenticated signatures. A review pack does not prove ledger completeness, provide audit assurance or replace an external database backup.
- **Protect the files.** Workbooks and embedded evidence are unencrypted. Connection paths refer to the machine running the server; source databases are not embedded backups. Keep the local service private rather than exposing it as a multi-user server.
- **Save deliberately.** There is no autosave or universal native-window-close protection. Save/Discard/Cancel guards cover supported in-app replacement flows, not every way a process can exit.
- **Know the scale.** The grid is virtualized, but SQL results are fully materialized in memory. Source refresh is explicit, not automatic monitoring. Find and bulk worksheet operations have bounded limits.
- **Standards are not implemented.** IFRS/XBRL taxonomy validation and an add-on system are not current capabilities.

## Development and verification

The repository separates `gridloom-core` (Rust calculation, database and working-paper logic), `gridloom-server` (Axum API and local app hosting), `gridloom-desktop` (Wry/Tao shell), and `frontend` (React/TypeScript interface).

With the source-build environment above configured, including the optional desktop dependencies for the full workspace tests:

```bash
cargo test --locked --workspace
python3 -m unittest discover -s scripts/tests -v
(cd frontend && npm test && npm run build)
cargo build --locked -p gridloom-server
(cd frontend && npx playwright install --with-deps chromium && npm run test:browser)
cargo run --locked -p gridloom-server -- --bench
```

`test:browser` exercises a synthetic monthly close. [CI](.github/workflows/ci.yml) also runs dedicated worksheet, save-safety and formula regression suites, a native window-startup smoke test, and exact-commit package integrity/relocation checks. Native startup coverage is not comprehensive interactive desktop testing.

The benchmark command checks four bounded workloads and resource budgets. Linux memory measurement uses the process peak RSS; these checks are smoke tests, not a production-scale performance guarantee. On hosts without the required memory measurement, the resource gate is unavailable and the command exits unsuccessfully.

## Further reading

- [Monthly-close walkthrough, CSV examples and review boundaries](docs/RELEASE-0.3.md)
- [Accounting working-paper example](docs/RELEASE-0.4.md)
- [Detailed worksheet and formula behavior](docs/RELEASE-0.5.md)
- [Change history](CHANGELOG.md)

## License

See the included [MIT license](LICENSE). The Rust package manifests additionally declare `MIT OR Apache-2.0`.
