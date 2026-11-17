# 0.3 verification record

## Candidate validation

- Rust core/server tests: 93 passed (including 25 spreadsheet, 14 review, 10 monthly-close and 6 API integration tests), zero failures.
- Frontend: 17 unit tests passed; TypeScript/Vite production build passed. Vite locked to 6.4.6; `npm audit` reported zero vulnerabilities at verification time.
- Benchmark smoke: all four correctness and resource gates passed. These are local smoke budgets, not production-scale guarantees.
- Real Brave/Chromium UI acceptance on an isolated Linux desktop: formula editing, range fill/copy, formatting, undo/redo, cross-sheet calculation, trial-balance import, mapping, reconciliation, dated support with a materiality note, same-total stale review, exact comparison, review-pack download, rollforward, save and reopen. Browser test uses synthetic data only.
- Full locked native workspace build and native window-open/process smoke passed on Linux with GTK3/WebKitGTK4.1. Screenshots were visually inspected for layout/clipping.
- Portable Linux server archive passed checksum verification and relocation: launched from `/tmp`, with its bundled DuckDB and frontend assets, without relying on an external library search path.

## What this does not establish

The local cloud sandbox cannot launch Chromium (Unix sockets prohibited) or compile WebKit (development libraries absent). The connected desktop supplied those checks. Native window-open smoke is not interactive native UI automation; Wry/WebKit-specific editing and interruption flows still require a manual release check. CI runs the real browser and native window-open tests for the published commit. A published candidate passed the full real-browser acceptance; its headless native window-open check failed during WebKit startup, so headless session setup and explicit startup-stage diagnostics are under exact-commit re-verification. Do not infer a full green CI result from earlier native-host snapshot checks.

There is no trusted identity provider, authenticated reviewer authorization, signed approval, locked-period ledger posting, encrypted workbook, automatic external-data monitoring, or inferred transaction lineage for aggregates/joins. Retained calculations are observed snapshots, not arbitrary SQL/database backups or audit assurance.

Linux tarballs depend on compatible system C/C++ libraries; the native bundle additionally needs GTK/WebKit. There are no signed macOS/Windows installers or AppImages in this release.

See [RELEASE-0.3.md](RELEASE-0.3.md) for the complete workflow, explicit data limits and trust boundaries.
