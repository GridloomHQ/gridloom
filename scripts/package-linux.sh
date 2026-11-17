#!/usr/bin/env bash
# Build a relocatable Linux x86_64/arm64 tarball. Native UI remains an explicit option.
set -euo pipefail
cd "$(dirname "$0")/.."
mode="${1:-server}"
case "$mode" in server|desktop) ;; *) echo 'Usage: package-linux.sh [server|desktop]' >&2; exit 2;; esac
command -v cargo >/dev/null
(cd frontend && npm ci && npm run build)
cargo build --locked --release -p "gridloom-$mode"
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "crates/gridloom-$mode/Cargo.toml" | head -1)
name="gridloom-$version-linux-$(uname -m)-$mode"
mkdir -p target/package
work=$(mktemp -d "target/package/.build-$name.XXXXXX")
trap 'rm -rf "$work"' EXIT
stage="$work/$name"
mkdir -p "$stage/lib" "$stage/frontend" "$stage/docs"
cp "target/release/gridloom-$mode" "$stage/"
cp -R frontend/dist "$stage/frontend/"
cp README.md LICENSE CHANGELOG.md "$stage/"
cp docs/RELEASE-0.3.md docs/VERIFICATION-0.3.md "$stage/docs/"
# DuckDB is the only bundled shared library. GTK/WebKit and the system runtime
# remain distribution dependencies, with exact missing dependencies checked below.
lib=$(ldd "target/release/gridloom-$mode" | awk '/libduckdb/ && $3 != "not" {print $3; exit}')
if [[ -z "$lib" || ! -s "$lib" ]]; then echo 'DuckDB shared library not resolved; set DUCKDB_LIB_DIR and LD_LIBRARY_PATH.' >&2; exit 1; fi
cp -L "$lib" "$stage/lib/"
cat > "$stage/gridloom" <<EOF
#!/usr/bin/env bash
set -euo pipefail
base=\$(cd "\$(dirname "\${BASH_SOURCE[0]}")" && pwd)
export LD_LIBRARY_PATH="\$base/lib\${LD_LIBRARY_PATH:+:\$LD_LIBRARY_PATH}"
export GRIDLOOM_FRONTEND_DIR="\$base/frontend/dist"
exec "\$base/gridloom-$mode" "\$@"
EOF
chmod +x "$stage/gridloom"
LD_LIBRARY_PATH="$stage/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" ldd "$stage/gridloom-$mode" | tee "$stage/docs/shared-libraries.txt"
if grep -q 'not found' "$stage/docs/shared-libraries.txt"; then echo 'Package has unresolved runtime dependencies' >&2; exit 1; fi
(cd "$stage" && find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS)
tar -C "$work" -czf "target/package/$name.tar.gz" "$name"
sha256sum "target/package/$name.tar.gz" > "target/package/$name.tar.gz.sha256"
echo "Built target/package/$name.tar.gz"
