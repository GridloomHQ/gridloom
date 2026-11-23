# Build and test Tayriva

## Build from source on Linux

Use a current stable Rust toolchain, an up-to-date Node.js 22 or newer, npm, Python 3.11+, a C/C++ build toolchain, `curl` and `unzip`. Linux packages are built and verified on Ubuntu 24.04 x86-64.

```bash
git clone https://github.com/GridloomHQ/tayriva.git
cd tayriva

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

## Development and verification

The repository separates `gridloom-core` (Rust calculation, database and working-paper logic), `gridloom-server` (Axum API and local app hosting), `gridloom-desktop` (Wry/Tao shell), `gridloom-mcp` (stdio agent tools), and `frontend` (React/TypeScript interface).

With the source-build environment above configured, including the optional desktop dependencies for the full workspace tests:

```bash
cargo test --locked --workspace
python3 -m unittest discover -s scripts/tests -v
(cd frontend && npm test && npm run build)
cargo build --locked -p gridloom-server
(cd frontend && npx playwright install --with-deps chromium && npm run test:browser)
cargo run --locked -p gridloom-server -- --bench
```

`test:browser` exercises a synthetic monthly close. [Linux CI](../.github/workflows/ci.yml) also runs dedicated worksheet, save-safety and formula regression suites, a native window-startup smoke test, and exact-commit package integrity/relocation checks. Native startup coverage is not comprehensive interactive desktop testing.

To smoke-test the connected ledger in a browser, run `(cd frontend && npm run test:ledger)` after building the server and frontend. The ignored server test `ledger_benchmark` recreates the two-million-row synthetic dataset with `GRIDLOOM_LEDGER_BENCH_MODE=fixture`; rerun with `paged` and `eager` in separate processes using the same fresh `GRIDLOOM_LEDGER_BENCH_PATH` to compare timings and peak RSS.

The benchmark command checks four bounded workloads and resource budgets. Linux memory measurement uses the process peak RSS; these checks are smoke tests, not a production-scale performance guarantee. On hosts without the required memory measurement, the resource gate is unavailable and the command exits unsuccessfully.

## Build the Windows x64 package

Use the Rust MSVC x64 target, a Visual Studio C++ build toolchain, Node.js 22+, and Python 3.11+. Download the Windows x64 DuckDB library for the version encoded in `Cargo.lock`, and point `DUCKDB_LIB_DIR` and `DUCKDB_INCLUDE_DIR` to the directory containing `duckdb.dll`, `duckdb.lib` and `duckdb.h`. From a clean committed checkout, run `python scripts/package-windows.py`. The archive and checksum are written to `target/package` unless you set `GRIDLOOM_PACKAGE_DIR`. [Windows CI](../.github/workflows/windows.yml) pins the upstream archive digest and checks the relocated package, visible WebView2 window, agent review and conflicting saves.
