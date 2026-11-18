# 0.5.9

- Cell drafts retain their original ownership and cannot appear under or submit to a newly navigated cell.
- Show formulas is a per-sheet saved view for inspecting calculation logic directly in the grid.
- Calculated values remain available in tooltips; summaries, copying, searches and exports retain their existing behavior.

# 0.5.8

- Exact mantissa-based ROUND, ROUNDUP, ROUNDDOWN and TRUNC avoid extreme-precision overflow, wrapping and underflow.
- Unrepresentable rounded results return `#NUM!` rather than crashing or silently losing a nonzero result.
- Existing half-even ROUND behavior is retained and explicitly documented.

# 0.5.7

- Formula-bar autocomplete uses the engine’s authoritative catalogue of supported functions and aliases.
- Complete dotted or nested function names at the caret without replacing surrounding formula text.
- Explicit acceptance changes only the draft; quoted text, sheet/table qualifiers and cell references are excluded.

# 0.5.6

- Correct XLOOKUP exact-first nearest matching, directional ties and row/column result alignment.
- Reject unsupported lookup modes and mismatched shapes instead of silently substituting results.
- Preserve lookup errors rather than treating them as missing matches; reject out-of-range VLOOKUP output columns.

# 0.5.5

- F4 cycles relative, absolute and mixed references in formula drafts, in both cell and formula-bar editors.
- Oversized range operations and delimiter-heavy clipboard input are bounded before large allocations.

# 0.5.4

- Save reuses the current path; Save As and Ctrl/Command+S provide familiar explicit saving.
- Save/Discard/Cancel replacement prompts and local-draft guards reduce accidental workbook loss.
- Unique same-directory temporary saves avoid clobbering unrelated temporary files.

# 0.5.3

- Read-only selection Sum/Average/Numbers with exact strings and explicit approximate, rounded or unavailable results.
- Cached staged and spill values included; stale-response and10,000-cell guards.

# 0.5.2

- Per-sheet frozen first row and first column, independently toggled and persisted.
- Bounded pane reads under one server lock preserve virtualization and consistent staged values.

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
