# 0.5.1

- Per-sheet column widths, numeric sizing, drag resize with cancel, and reset.
- Widths persist without changing calculation or review evidence.

# 0.5.0

- Bounded, read-only Find across cached worksheet values and formula inputs.
- Go to cell by address and standard keyboard navigation.
- Failed cell commits retain the draft; successful Enter/Tab commits move only after acknowledgement.
- Dedicated real-browser usability regression checks alongside accounting acceptance.

# Changelog

## 0.4.0 (preview)

- Circle up questions on erroneous, blank or incomplete calculations. Resolve or reopen with a captured state and dated note, and see outstanding questions across the workbook. Resolution is not a sign-off.
- Walk direct upstream inputs and downstream formula consumers with range/table grouping, worksheet highlights and a navigation trail.
- Keep the trial-balance import unchanged; post balanced journals with rationale and retained support. See imported, adjusted and reported balances, affected report values and invalidated sign-offs.
- Define a working paper's entity, currency, units and report hierarchy. Roll forward a fresh import with matching mappings, not adjustments or sign-offs.
- Compare captured changes by accounting category while retaining exact before/after fields. Review packs retain the report structure, adjustment evidence and question history.

## 0.3.0 (preview)

- Monthly trial-balance CSV import, account mappings, generated P&L, live reconciliation/review queue, period rollforward and JSON review packs.
- Readable passive HTML close summaries and exact-decimal P&L CSV, with export time/current snapshot identifiers and injection-safe text handling.
- Full retained review inputs and precise before/after comparison; embedded hashed support with immutable replacement versions.
- Conservative cached source-row drilldown for direct keyed SQL projections.
- Atomic range edits/copy/paste/fill, persisted formatting, bounded worksheet undo/redo, multi-sheet/cross-sheet formulas.
- Real Chromium workflow acceptance, Linux server/desktop packaging scripts and native window-open smoke tooling.
- Exact-commit Ubuntu 24.04 x86_64 release packages in CI, with build identities, complete integrity manifests and relocated backend/native-window smoke before artifact upload.
- Loopback-host/origin boundary; no claim of trusted reviewer authentication, locked ledger close or audit assurance.


## 0.2.0 (preview)

- Explain a selected number through transitive worksheet references, spill anchors, structured tables and cached SQL query/result boundaries.
- Record local reviews, circled-up questions, notes and evidence references inside the workbook, with retained history.
- Bind reviews to versioned content snapshots. Reject stale submissions and invalidate affected reviews after observed calculation or dependency changes. Invalidation is sticky across reverting inputs and saving/reopening.
- Block review of errors, blank cells, incomplete traces, pending database patches and known-stale source caches after writeback.
- Include a small illustrative cash-reserve model in the sample workbook.
- Roll back rejected cyclic cell edits so they cannot leave a half-updated calculation behind an apparently valid review.

This preview does not add authenticated reviewers, document ingestion/verification, row-level aggregate provenance, live external-source monitoring or production accounting/audit assurance. Native desktop packaging and interactive browser smoke testing should be verified on a fully provisioned host before release publication.
