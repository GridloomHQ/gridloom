# Gridloom 0.3: monthly close preview

This release implements a local monthly P&L working-paper workflow, not a hosted audit product.

## Try an entire close

1. Build the frontend and run `cargo run -p gridloom-server -- --no-samples`.
2. Open **Monthly close**. Import a `YYYY-MM` period and CSV with exact headers `account,name,debit,credit`.
   Amounts must be non-negative exact decimal numbers without currency symbols or grouping separators. Quoted CSV fields are supported; duplicate accounts are rejected. Limits: 1 MiB and 1,000 accounts.
3. Map every account to Revenue, Cost of sales, Operating expenses, Other income, Other expense, or Balance sheet. An immutable copy of the imported CSV and its hash remain in the workbook; mapping and cell edits are separately retained in worksheet/review state.
4. Check debit-credit reconciliation and open a P&L line from the review queue. The generated worksheet contains account inputs and formula-derived P&L totals.
5. Explain a line, attach support, enter a local reviewer label and record a review or question. Upstream input changes invalidate the review even if the displayed total is unchanged.
6. Compare an old review with current inputs or another saved review. Exact JSON-pointer changes preserve before and after values, including cached SQL results.
7. Save the `.gridloom` file. Reopen it to retain the imported data, mappings, review history, captured input snapshots and embedded evidence.
8. Roll forward by supplying a later month's NEW trial balance. Only valid account mappings carry forward, not balances or approvals. Prior period worksheets remain intact.
9. Export a JSON review pack containing source CSV, current working paper, reconciliation, queue, explanations, retained review snapshots and evidence bytes. Download a conventional P&L CSV or a print-friendly readable HTML summary for review. Those summaries carry current calculation snapshot IDs, export timestamps and import identity; keep the companion JSON for full captured inputs and evidence bytes. HTML support references are passive text, never opened or fetched. CSV preserves exact decimal characters, but spreadsheet importers may round numeric cells, so import as text when exact preservation is required. No currency is inferred.

## Spreadsheet tools

- Shift-click, drag, or Shift-arrow selects a rectangular range. Ctrl/Cmd+C and V copy/paste; TSV quoting preserves tabs/newlines. Internal ordinary-cell copies translate relative formulas and preserve style. The captured source hash must still match; changed sources are rejected, never silently substituted.
- Fill down/right uses the first row/column as the template. Formula translation respects mixed/absolute references and quoted text. Table/spill result cells remain protected.
- Number, currency, percent, bold, italic and cell fill formatting now affect rendering and persist.
- Undo/redo covers up to 100 worksheet cell operations and 100,000 retained cell versions, not database writes, review history, sheet creation/rename or close imports. History is session-only and cleared at external/source/close boundaries.
- Create/select/rename worksheets. Formulas support `=Sheet2!A1` and `='Other sheet'!A1:B5`; cross-sheet changes recalculate and invalidate affected reviews. Managed close sheets cannot be renamed.

## Evidence and trust boundaries

Support files are embedded bytes, limited to 2 MiB each and 8 MiB per review. Replacements form immutable version chains. SHA-256 identifies captured content, not authenticity, document correctness or tamper resistance. Downloads are treated as untrusted binary attachments.

Review snapshots retain complete observed cell/formula inputs and named-source cached results, subject to 8,192 nodes, depth 64 and a 16 MiB record limit. They are reproducible records of the observed calculation, not backups of an external database, a general historical SQL replay engine or proof the external data is current. Arbitrary SQL expressions may be nondeterministic; refresh observes external changes.

Cached source-row drilldown is available only for conservative single-table direct projections with complete unique declared primary keys. It shows cached source rows, not a live transaction ledger or inferred aggregate/join contribution. Aggregates, joins and ambiguous keys are refused.

Reviewer labels are **self-reported and unauthenticated**. There is no identity provider, multi-user role enforcement, locked-period posting, digital signature or audit assurance. A balanced trial balance is not proof of completeness, correct account mapping or proper accounting treatment. The review queue is a local checklist, not an authorization system.

The server binds loopback and rejects foreign browser origins and non-loopback Host headers. Do not expose it through a reverse proxy as a remote service. `.gridloom` files and exported review packs are unencrypted and can contain private support files and database paths. Only share them deliberately.

## Linux packaging and verification

`scripts/package-linux.sh server` builds a relocatable server/browser tarball with frontend assets, DuckDB library, launcher, dependency list and SHA-256 manifest. `scripts/package-linux.sh desktop` additionally requires GTK3/WebKitGTK4.1 development packages at build time and runtime libraries on the destination. Distribution/system C/C++ libraries are not bundled. This is not an AppImage, signed installer or universal cross-distribution binary.

The launcher works from any working directory; `GRIDLOOM_FRONTEND_DIR` locates static assets. Tests include Rust core/API suites, frontend utility tests and a real Chromium monthly-close smoke test. Native window-open smoke is separate and does not establish full interactive Wry/WebKit coverage. See the release verification notes for what actually ran.

Do not round-trip a 0.3 workbook through an older Gridloom version: older applications may ignore and then discard newer review, attachment or close fields.


### Verified CI package downloads

After a successful main-branch CI run, open **Actions → CI → Artifacts** and download `gridloom-linux-x86_64-<commit>`. It contains release-mode server/browser and native desktop tarballs, matching `.sha256` files and relocation smoke logs. Artifacts expire after 14 days; the exact commit remains recorded in the package's `docs/build-info.json`.

From the directory containing a tarball and checksum file:

```bash
sha256sum -c gridloom-0.3.0-linux-x86_64-desktop.tar.gz.sha256
tar -xzf gridloom-0.3.0-linux-x86_64-desktop.tar.gz
./gridloom-0.3.0-linux-x86_64-desktop/gridloom --no-samples
```

The server variant launches a local URL instead of a native window. The native bundle needs compatible system GTK3/WebKitGTK4.1 and C/C++ runtime libraries. Builds are made on Ubuntu 24.04 x86_64; this is not a universal Linux image, signed installer or proof of publisher authenticity. Keep extracted executable permissions. The launcher locates its bundled DuckDB and frontend assets regardless of the working directory.

For a local Git checkout, run `scripts/package-linux.sh server` or `scripts/package-linux.sh desktop`; `GRIDLOOM_PACKAGE_DIR` optionally chooses the output directory. Uncommitted/untracked source is recorded as dirty. CI refuses dirty or mismatched-commit packages rather than labeling them exact-commit builds.
