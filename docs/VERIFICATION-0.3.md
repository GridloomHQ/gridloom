# 0.3 verification record

## Candidate validation

- Rust core/server tests: 93 passed (including 25 spreadsheet, 14 review, 10 monthly-close and 6 API integration tests), zero failures.
- Frontend: 17 unit tests passed; TypeScript/Vite production build passed. Vite locked to 6.4.6; `npm audit` reported zero vulnerabilities at verification time.
- Benchmark smoke: all four correctness and resource gates passed. These are local smoke budgets, not production-scale guarantees.
- Real Brave/Chromium UI acceptance on an isolated Linux desktop: formula editing, range fill/copy, formatting, undo/redo, cross-sheet calculation, trial-balance import, mapping, reconciliation, dated support with a materiality note, same-total stale review, exact comparison, review-pack download, rollforward, save and reopen. Browser test uses synthetic data only.
- Full locked native workspace build and native window-open/process smoke passed on Linux with GTK3/WebKitGTK4.1. Screenshots were visually inspected for layout/clipping.
- Portable Linux server archive passed checksum verification and relocation: launched from `/tmp`, with its bundled DuckDB and frontend assets, without relying on an external library search path.

## What this does not establish

The local cloud sandbox cannot launch Chromium (Unix sockets prohibited) or compile WebKit (development libraries absent). The connected desktop supplied those checks. Native window-open smoke is not interactive native UI automation; Wry/WebKit-specific editing and interruption flows still require a manual release check. CI runs the real browser and native window-open tests for the published commit. Candidate `3ca768a` passed the full real-browser acceptance but failed headless native startup. After adding an isolated display/session bus and headless software rendering, published commit `ade2e7c` passed the full CI run, including browser CSV/HTML exports and native window-open smoke: [verified run](https://github.com/TayrivaHQ/tayriva/actions/runs/44089636776). Native-host snapshot checks alone must not be treated as final-commit proof.

There is no trusted identity provider, authenticated reviewer authorization, signed approval, locked-period ledger posting, encrypted workbook, automatic external-data monitoring, or inferred transaction lineage for aggregates/joins. Retained calculations are observed snapshots, not arbitrary SQL/database backups or audit assurance.

Linux tarballs depend on compatible system C/C++ libraries; the native bundle additionally needs GTK/WebKit. There are no signed macOS/Windows installers or AppImages in this release.

See [RELEASE-0.3.md](RELEASE-0.3.md) for the complete workflow, explicit data limits and trust boundaries.


## Exact-commit Linux packaging pipeline

The packaging job runs only for main pushes after the Rust/browser/native and frontend checks succeed. It builds release-mode server and desktop archives on Ubuntu 24.04 x86_64, records source commit/tree/dirty state and toolchain details in `docs/build-info.json`, and verifies archive and installed-file checksums. Cargo JSON chooses the actual executable rather than a guessed target path; ELF inspection rejects unsupported/cross-architecture output, and Git metadata failures or commit/tree changes abort packaging. The locked standalone DuckDB shared-library notice is included.

The install smoke extracts each archive into a fresh temporary directory, clears external DuckDB/asset/library-path overrides, launches it from a different working directory, requires relocated dependency resolution to the bundled DuckDB, and checks the packaged health endpoint and exact frontend asset bytes. Desktop packages additionally must emit the native window-open event under the isolated headless display. This is relocation/window-open verification, not an interactive native test or compatibility guarantee for all distributions.

Only archives passing those checks are uploaded as `gridloom-linux-x86_64-<commit>` CI artifacts, retained for 14 days. The adjacent `.sha256` files name archive basenames so they can be checked from the download directory. Content hashes and recorded source identity are not signatures or publisher attestations. No release tag, installer signing or deployment is performed.
