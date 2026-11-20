# 0.8.0

- Group worksheet commands and simplify Explain & Review without changing the underlying trace or review rules.
- Add the **fx** function picker, inline completion, keyboard range picking in F2 edits, and live formula-reference highlighting.
- Format source-backed worksheet tables without editing the source or staging writeback.
- Freeze a row and column boundary at the current selection, or set each axis independently.
- Render signed bar and column charts and configure chart and pivot fields by drag-and-drop or keyboard.
- Generate and browse a ten-million-row SQLite sample on demand from **Help → About → Sample**. No large sample ships with the app.
- Make Monthly close an opt-in add-on and preserve existing close workpapers.
- Open the native Linux desktop window on Wayland as well as X11.

# 0.7.0

- Sort and filter bounded worksheet ranges without moving the underlying cells or changing formula references.
- Insert worksheet rows and update supported references, including cross-sheet references. Gridloom blocks changes it cannot perform safely.
- Paste typed values or adjusted formulas without changing destination formatting, and find common actions in the keyboard shortcut guide.
- Refresh saved pivot layouts from their read-only source query without changing workbook snapshots or staged database edits.
- Validate editable database cells against the declared SQLite or DuckDB column type before staging, without silent rounding.
- Regression-test exact decimal reconciliations, cross-sheet recalculation and saved workbooks. No calculation precision was changed.

# 0.6.0

- Browse multi-million-row SQLite journals without importing them into a worksheet. Counts and totals apply to all matching entries, and a changed source requires a reload.
- Trace a worksheet balance to an account and date range, reconcile the exact difference, and save the trace link with the workbook.
- Recover committed edits from private local autosave files without changing the named workbook or writing back to a database.
- Generate a synthetic four-million-entry demo ledger with an exact reference total for a sample reconciliation.

# 0.5.12

- CEILING.MATH and FLOOR.MATH round exact remainders rather than a potentially rounded or overflowing quotient.
- Tiny nonzero inputs retain the correct rounding direction; unrepresentable final multiples return `#NUM!`.

# 0.5.11

- SUMPRODUCT validates matching two-dimensional shapes rather than only flattened cell counts.
- Calculation errors and unconvertible numeric inputs are no longer silently replaced with zero.
- Product and accumulation overflow return `#NUM!` instead of panicking.

# 0.5.10

- PRODUCT reports numeric overflow instead of panicking, and rejects unconvertible numeric inputs rather than silently omitting them.
- MOD computes a remainder without a rounded quotient, retaining the divisor-sign convention and rejecting an unrepresentable exact result.

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
