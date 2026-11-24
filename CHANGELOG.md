# 0.15.0

- Calculate linked SQLite rows using a worksheet template with **Data → Run formula batch**. Input rows are not modified; the destination result snapshot is replaced in one transaction.
- Use exact-decimal arithmetic, comparisons, IF and exact VLOOKUP against worksheet rule tables. An invalid row or rule aborts the batch and preserves prior results.
- See [worksheet formula batches](docs/worksheet-formula-batches.md) for setup, supported formulas and write-back behavior.

# 0.14.0

- Tayriva-branded Windows and Linux packages and launchers.
- New native `.tay` workbooks and autosaves; old Gridloom files are intentionally unsupported.
- MCP improvements for cell reads, review comparisons and local Codex setup.

# 0.13.1

- Add a portable Windows x64 desktop ZIP with the bundled headless MCP and the same workbook and Review tools as Linux.
- Launch the native Windows window without Linux display environment variables and preserve guarded workbook saves and workbook-folder restrictions on Windows.

# 0.13.0

- Bundle a local headless MCP server in Linux desktop and server packages. Agents can build, inspect, edit, validate, and explicitly save native standalone workbooks.
- Give agents the same snapshot-bound Review workflow as the app: trace transitive inputs, record a labeled AI-assisted review or open question, resolve and reopen questions, inspect notes and retained evidence metadata, and compare changes.
- Follow workbook-owned table cells and their formulas in review traces. Relevant input changes invalidate reviews even when the number stays the same.
- Share a guarded save between app and MCP. A stale save is rejected without losing edits; Save As writes a separate new file, and recovered copies cannot overwrite a named workbook.

# 0.12.0

- Start with one blank worksheet. Gridloom no longer creates or offers built-in demonstration data. Existing workbook and database files are unchanged.
- Create an editable table directly on a worksheet with **Insert → Create local table**. Choose column types, keep leading zeroes in text and calculate with exact decimals. No external database connection is needed.
- Save the worksheet and its local tables together in one SQLite-based `.gridloom` file. Older Gridloom workbooks still open; they switch formats only when saved. Use **Save As** to keep the original available for older Gridloom versions. Autosave recovery remains separate.

# 0.11.0

- Refresh the existing demo Invoices table in place after generating ten million rows, without loading all rows into the worksheet.
- Jump to the first or last invoice with Ctrl+Up or Ctrl+Down. Gridloom fetches the destination window and reuses it when possible.
- Place a supported SQLite query in the worksheet, then refresh or stage edits by stable row key. Refresh also picks up source changes when the row count stays the same.

# 0.10.0

- Preview read-only SQL query results in pages of up to 100 rows. The editor no longer loads the entire result into memory or sends it to the browser.
- Use SQL `WHERE` and `ORDER BY` to filter and sort the results in the database before paging. Specify a stable order to browse large results consistently.
- Previewing a query does not import its full result into an editable worksheet.

# 0.9.0

- Use a single-row command bar with tabs, consistent buttons, a dedicated Add-ons tab, and an unobstructed keyboard-shortcuts dialog.
- Jump across populated cells with Ctrl+Arrow, or select the whole column with Ctrl+Space and the whole row with Shift+Space.
- Right-click a worksheet tab to rename or delete it. Gridloom blocks deletion that would break references or discard recorded reviews.
- Open **fx** to browse functions by category, inspect arguments and return values, and insert the chosen function into the draft.
- Expand the existing demo invoice table to ten million rows on demand, without importing them into the workbook.

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
