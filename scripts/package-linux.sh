#!/usr/bin/env bash
# Build a relocatable Linux x86_64/arm64 tarball. Native UI remains an explicit option.
set -euo pipefail
cd "$(dirname "$0")/.."
mode="${1:-server}"
case "$mode" in server|desktop) ;; *) echo 'Usage: package-linux.sh [server|desktop]' >&2; exit 2;; esac
command -v cargo >/dev/null
command -v rustc >/dev/null
build_commit=$(git rev-parse --verify HEAD) || { echo "Run packaging from a Git checkout with a committed source identity." >&2; exit 1; }
build_tree=$(git rev-parse HEAD^{tree})
build_status=$(git status --porcelain --untracked-files=normal)
initial_dirty=false
[[ -z "$build_status" ]] || initial_dirty=true
(cd frontend && npm ci && npm run build)
artifact=$(cargo build --locked --release -p "gridloom-$mode" --message-format=json-render-diagnostics \
  | python3 scripts/package-artifact.py "gridloom-$mode")
binary=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["executable"])' <<< "$artifact")
architecture=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["architecture"])' <<< "$artifact")
mcp_artifact=$(cargo build --locked --release -p gridloom-mcp --message-format=json-render-diagnostics \
  | python3 scripts/package-artifact.py gridloom-mcp)
mcp_binary=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["executable"])' <<< "$mcp_artifact")
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "crates/gridloom-$mode/Cargo.toml" | head -1)
name="tayriva-$version-linux-$architecture-$mode"
output="${GRIDLOOM_PACKAGE_DIR:-target/package}"
mkdir -p "$output"
output=$(cd "$output" && pwd)
work=$(mktemp -d "$output/.build-$name.XXXXXX")
trap 'rm -rf "$work"' EXIT
stage="$work/$name"
mkdir -p "$stage/lib" "$stage/frontend" "$stage/docs"
cp "$binary" "$stage/tayriva-$mode"
cp "$mcp_binary" "$stage/tayriva-mcp-bin"
cat > "$stage/tayriva-mcp" <<'EOFMCP'
#!/usr/bin/env bash
set -euo pipefail
base=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
export LD_LIBRARY_PATH="$base/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec "$base/tayriva-mcp-bin" "$@"
EOFMCP
chmod +x "$stage/tayriva-mcp"
cp -R frontend/dist "$stage/frontend/"
cp README.md LICENSE CHANGELOG.md "$stage/"
cp docs/RELEASE-*.md docs/VERIFICATION-*.md docs/MCP.md "$stage/docs/"
duckdb_version=$(python3 - <<'PYDUCK'
import tomllib
from pathlib import Path
packages = tomllib.loads(Path("Cargo.lock").read_text())["package"]
version = next(p["version"] for p in packages if p["name"] == "libduckdb-sys")
encoded = int(version.split(".")[1])
print(f"{encoded // 10000}.{encoded // 100 % 100}.{encoded % 100}")
PYDUCK
)
# Keep the copyright/license notice alongside the explicitly bundled shared library.
# A locked DuckDB upgrade must add its matching notice rather than silently reuse one.
cp "docs/third-party/duckdb-$duckdb_version.LICENSE" "$stage/docs/DuckDB-LICENSE"
# DuckDB is the only bundled shared library. GTK/WebKit and the system runtime
# remain distribution dependencies, with exact missing dependencies checked below.
lib=$(ldd "$binary" | python3 -c '
import re, sys
matches = re.findall(r"^\s*libduckdb\.so\s+=>\s+(.+?)\s+\(0x[0-9a-fA-F]+\)\s*$", sys.stdin.read(), re.MULTILINE)
if len(matches) != 1:
    raise SystemExit("Expected exactly one resolved libduckdb.so dependency.")
print(matches[0])')
if [[ -z "$lib" || ! -s "$lib" ]]; then echo 'DuckDB shared library not resolved; set DUCKDB_LIB_DIR and LD_LIBRARY_PATH.' >&2; exit 1; fi
cp -L "$lib" "$stage/lib/"
cat > "$stage/tayriva" <<EOF
#!/usr/bin/env bash
set -euo pipefail
base=\$(cd "\$(dirname "\${BASH_SOURCE[0]}")" && pwd)
export LD_LIBRARY_PATH="\$base/lib\${LD_LIBRARY_PATH:+:\$LD_LIBRARY_PATH}"
export GRIDLOOM_FRONTEND_DIR="\$base/frontend/dist"
exec "\$base/tayriva-$mode" "\$@"
EOF
chmod +x "$stage/tayriva"
python3 - "$stage/docs/build-info.json" "$mode" "$version" "$architecture" "$build_commit" "$build_tree" "$initial_dirty" "$duckdb_version" <<'PYINFO'
import datetime, json, platform, subprocess, sys
from pathlib import Path

def command(*args, required=False):
    try:
        result = subprocess.run(args, capture_output=True, text=True)
    except OSError as error:
        if required:
            raise SystemExit(f"Required metadata command failed: {args[0]}: {error}") from error
        return None
    if result.returncode != 0:
        if required:
            raise SystemExit(f"Required metadata command failed: {' '.join(args)}: {result.stderr.strip()}")
        return None
    return result.stdout.strip()

commit = command("git", "rev-parse", "HEAD", required=True)
tree = command("git", "rev-parse", "HEAD^{tree}", required=True)
status = command("git", "status", "--porcelain", "--untracked-files=normal", required=True)
if commit != sys.argv[5] or tree != sys.argv[6]:
    raise SystemExit("Source commit/tree changed during build; no package will be labeled with a different revision.")
info = {
    "format": "tayriva-linux-package", "format_version": 1, "mcp_stdio": True, "guarded_save": True, "mcp_review": True,
    "version": sys.argv[3], "mode": sys.argv[2], "build_profile": "release",
    "duckdb_version": sys.argv[8],
    "source_commit": commit, "source_tree": tree,
    "source_dirty": sys.argv[7] == "true" or bool(status),
    "built_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "architecture": sys.argv[4], "system": platform.system(),
    "rustc": command("rustc", "--version"), "cargo": command("cargo", "--version"),
    "node": command("node", "--version"),
    "trust_boundary": "Content checksums and recorded source identity are not a signature or publisher attestation.",
}
try:
    info["distribution"] = platform.freedesktop_os_release().get("PRETTY_NAME")
except OSError:
    info["distribution"] = None
Path(sys.argv[1]).write_text(json.dumps(info, indent=2) + "\n")
PYINFO
LD_LIBRARY_PATH="$stage/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" ldd "$stage/tayriva-$mode" "$stage/tayriva-mcp-bin" | tee "$stage/docs/shared-libraries.txt"
if grep -q 'not found' "$stage/docs/shared-libraries.txt"; then echo 'Package has unresolved runtime dependencies' >&2; exit 1; fi
(cd "$stage" && find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS)
tar -C "$work" -czf "$output/$name.tar.gz" "$name"
(cd "$output" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
echo "Built $output/$name.tar.gz"
