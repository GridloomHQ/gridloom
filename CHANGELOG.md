# Changelog

## 0.3.0 (preview)

- Monthly trial-balance CSV import, account mappings, generated P&L, live reconciliation/review queue, period rollforward and JSON review packs.
- Readable passive HTML close summaries and exact-decimal P&L CSV, with export time/current snapshot identifiers and injection-safe text handling.
- Full retained review inputs and precise before/after comparison; embedded hashed support with immutable replacement versions.
- Conservative cached source-row drilldown for direct keyed SQL projections.
- Atomic range edits/copy/paste/fill, persisted formatting, bounded worksheet undo/redo, multi-sheet/cross-sheet formulas.
- Real Chromium workflow acceptance, Linux server/desktop packaging scripts and native window-open smoke tooling.
- Loopback-host/origin boundary; no claim of trusted reviewer authentication, locked ledger close or audit assurance.


## 0.2.0 (preview)

- Explain a selected number through transitive worksheet references, spill anchors, structured tables and cached SQL query/result boundaries.
- Record local reviews, circled-up questions, notes and evidence references inside the workbook, with retained history.
- Bind reviews to versioned content snapshots. Reject stale submissions and invalidate affected reviews after observed calculation or dependency changes. Invalidation is sticky across reverting inputs and saving/reopening.
- Block review of errors, blank cells, incomplete traces, pending database patches and known-stale source caches after writeback.
- Include a small illustrative cash-reserve model in the sample workbook.
- Roll back rejected cyclic cell edits so they cannot leave a half-updated calculation behind an apparently valid review.

This preview does not add authenticated reviewers, document ingestion/verification, row-level aggregate provenance, live external-source monitoring or production accounting/audit assurance. Native desktop packaging and interactive browser smoke testing should be verified on a fully provisioned host before release publication.
