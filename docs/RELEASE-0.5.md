# 0.5: Find the number, stay in the worksheet

Gridloom's accounting features need ordinary spreadsheet ergonomics underneath them. This release adds discovery and keyboard navigation without changing formulas, ledger data, review evidence or journal semantics.

## Find

Use **Find (Ctrl+F)** or Ctrl/Command+F while the grid has focus. Search the current sheet or workbook, choose displayed values or formula-bar inputs, and optionally match case or the entire cell. Click a result to select its cell, then Explain it using the existing review/dependency tools.

This is literal text search: `*` and `?` are ordinary characters, not wildcards. Formulas / inputs includes literal input cells as well as formulas. Values searches formatted values (so `$ 2.00` differs from a raw `2`). Cached source tables, staged display values and spill results are included in values search. Source data is not refreshed or queried. Evidence files, notes and charts are not searched. There is no bulk Replace: replacements of reviewed accounting data need a separate, safe design.

Results are sorted by sheet name, row and column. Each request examines at most the first 50,000 occupied candidate cells; an explicit warning marks incomplete results. Narrowing to a sheet can help. Pages show 50 matches. Previews are limited to 1,000 characters, while matching uses the full text. Rerun after changes: pages are computed from current cached content, not a historical search snapshot.

## Go to cell

The address field beside the formula bar is now editable. Enter `D200` and press Enter to reveal and select that cell in the current worksheet. Invalid, range, cross-sheet and out-of-bounds addresses are rejected, rather than silently moving elsewhere. Escape restores the selected address.

## 0.5.1: Persistent column widths

Drag a column header boundary, or set the selected column’s width in pixels beside Find. Widths are per-sheet, saved with the workbook, and constrained to 48–600 px. Reset returns to 110 px. Escape cancels a drag; failed saves keep the previous width. This is presentation only: it does not recalculate cells, invalidate reviews, apply source writes or enter cell undo history. AutoFit, hidden columns and row heights are not implemented.

The design follows the familiar [explicit width and drag sizing pattern](https://support.microsoft.com/en-us/excel/change-column-width-or-row-height), but uses pixels rather than Excel character-width units.

## 0.5.2: Keep the first row and column visible

Toggle **Freeze top row** and **Freeze first column** independently. The first row and/or column stays pinned while the rest of the worksheet scrolls. Settings are per-sheet and saved in the workbook. Frozen cells remain selectable, editable and available to existing clipboard and dependency highlighting. Column resizing still applies.

This bounded first version does not freeze arbitrary numbers of rows or columns, split windows, hide data or change calculations. Frozen strips are read as separate bounded regions within one server read lock rather than one huge rectangle spanning the origin and a distant viewport. All panes observe the same workbook and staged-edit state.

## 0.5.3: Quick selection totals

Select a range to see Sum, Average and Numbers in the bottom summary strip. Totals use underlying cached numeric values, including staged edits, not the rounded strings on screen. Numeric-looking text, booleans, dates/times, NULL and blanks are not coerced into numbers. Hover for the detailed counts and scope.

Results remain strings end to end so large integers do not pass through JavaScript floating-point arithmetic. Float inputs are labelled approximate; repeating or unrepresentable decimal averages are marked rounded or unavailable. Errors, nonfinite values or arithmetic overflow make totals unavailable rather than showing an unexplained partial total. Empty numeric selections display a dash. Selections above 10,000 cells have an explicit limit message.

This is a read-only convenience, not a posted journal or recorded review. It does not refresh connected data or recalculate the workbook. Results clear when the selection changes; delayed responses cannot overwrite a newer selection or workbook. The familiar behavior is informed by [Excel's status-bar summaries](https://support.microsoft.com/en-us/excel/view-summary-data-on-the-status-bar).

## 0.5.4: Deliberate, safer saving

**Save** uses the current workbook path after a successful open or save. **Save As** chooses another path. Ctrl/Command+S uses the same explicit save flow. New and Open ask whether to save the current workbook, discard it, or cancel. A canceled filename or failed save must not continue to replace the workbook.

Saving writes committed workbook content, including staged source edits, to a `.gridloom` file on the machine running the server. It does **not** apply those edits to a source database. Unsubmitted cell or working-paper drafts are not silently submitted by Save: finish or cancel them and close open work panels first. The same conservative guard applies before New/Open, including before the discard choice. This is not autosave, a universal dirty/clean indicator, recovery after a crash, multi-client conflict protection, or a native-window-close guard. Save before closing the application.

Each save writes through a uniquely created temporary file beside the destination, flushes and synchronizes it, then replaces the destination. It does not reuse or truncate an unrelated fixed `.gridloom.tmp` file. Failures before replacement leave the previous saved file intact. Filesystem and operating-system behavior still determine durability; directory synchronization on Unix is best-effort after replacement, with any failure logged as a durability warning rather than falsely reporting that the save did not occur.

The explicit Save/Save As distinction follows [familiar workbook saving conventions](https://support.microsoft.com/en-us/excel/save-a-workbook). File synchronization and replacement use the documented [Rust file synchronization](https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all) and [rename](https://doc.rust-lang.org/std/fs/fn.rename.html) behavior.

## 0.5.5: Lock formula assumptions with F4

While editing a formula, place the caret in one cell reference or select that exact reference and press **F4**. The reference cycles `A1 → $A$1 → A$1 → $A1 → A1`. This makes it quicker to reuse fixed assumptions and mixed row/column references in financial models. It works in the cell editor and formula bar; only the draft changes until you commit. Escape cancels, and a failed commit retains the draft.

The shortcut targets one reference, not an entire formula or range. Quoted strings, SQL text, structured table references, function names and ambiguous selections are not rewritten. It is not a general formula parser or an Excel-parity claim. The behavior follows [relative, absolute and mixed reference conventions](https://support.microsoft.com/en-us/excel/switch-between-relative-absolute-and-mixed-references).

Large clear/format/fill actions are rejected before constructing or sending oversized changes. Clipboard parsing bounds the number of fields while reading, not only after building the whole matrix. The existing 10,000-cell operation limit remains; a small paste is not rejected merely because the current selection is large.

## 0.5.6: Trustworthy lookup thresholds

XLOOKUP now gives an exact match priority before choosing the nearest smaller (`-1`) or larger (`1`) value, including on unsorted lookup vectors. Forward and reverse linear search resolve duplicate matches in their respective directions. Horizontal lookup can return a matching column from a multi-row array; vertical lookup can return a row from multiple columns. Lookup and return dimensions must align rather than silently producing a blank.

This release explicitly supports match modes `0`, `-1`, `1` and search modes `1`, `-1`. Wildcard and binary-search modes are not implemented and now return `#VALUE!` rather than silently using different behavior. Modes must be numeric supported integers. Lookup errors are not a missing-match fallback; selected return errors remain errors. VLOOKUP also returns `#REF!` for a requested output column outside the table.

Integer/Decimal comparisons remain exact. Float-only comparisons use finite IEEE floating-point values. Mixed Float and Integer/Decimal comparisons are allowed only when the Float's exact binary value can be represented in the Decimal domain; otherwise `#NUM!` is returned instead of risking a false match. For example, binary Float `0.1` cannot be compared as if it were exact Decimal `0.1`. This conservative contract is not complete Excel coercion or function parity.

Previously saved calculation caches are rebuilt when a workbook opens. A corrected result can change dependent values and invalidate an earlier captured review. Inspect and re-review affected working papers rather than relying on an old displayed result.

The exact-first/nearest and directional-search design follows [Microsoft's XLOOKUP documentation](https://support.microsoft.com/en-us/excel/functions/xlookup-function). MATCH and VLOOKUP approximate searches still require their documented sorted input contracts; this release is not a general lookup-function rewrite.

## Keyboard editing

- Enter / Shift+Enter: move down / up.
- Tab / Shift+Tab: move right / left.
- F2, double-click or typing: edit the selected cell.
- While editing, Enter or Tab commits and moves only after success. A failed submission keeps the draft available; Escape cancels a draft.
- Home: first column on this row. Ctrl/Command+Home: A1.
- Page Up / Page Down: approximately one visible page. Shift extends selection for arrow, Home and page navigation.

Existing copy/paste, formula fill, undo/redo and evidence workflows remain available. This release does not claim Excel file compatibility, full keyboard parity, arbitrary multi-row/multi-column frozen panes, arbitrary row insertion, sorting, autosave or authenticated review identities.

## Product research and next priorities

Official Excel documentation establishes [Find scope and values/formulas choices](https://support.microsoft.com/en-gb/excel/get-started/find-or-replace-text-and-numbers-on-a-worksheet), [Enter/Tab editing conventions](https://support.microsoft.com/en-us/excel/edit-cell-contents), and [frozen headings](https://support.microsoft.com/en-us/excel/get-started/freeze-panes-to-lock-rows-and-columns). These are useful behavioral references, not a claim of implementation parity.

The next candidates are safe range sort/filter and explicit data-validation rules. Structural edits, sorting and replace must preserve formula references and invalidate captured reviews correctly. Recovery/autosave needs a clear distinction between a draft, a saved working paper and staged external writeback. XBRL/IFRS remains an add-on concern.


## 0.5.7: Discover the functions actually supported

The formula bar now suggests all 95 names and aliases accepted by this engine, from an authoritative read-only catalogue shared with function dispatch. Type a function prefix at the caret, including dotted names such as `STDEV.S` or a nested function. Up/Down chooses an item; Tab/Enter or a click accepts it into the draft. Acceptance does not submit the formula. Escape dismisses suggestions first; another Escape cancels the draft.

Completion preserves the rest of the formula and excludes quoted text, sheet/table qualifiers, structured references and cell references. Existing F4 reference locking and failed-commit draft retention remain available. If the catalogue cannot load, manual entry still works. This is formula-bar function-name completion, not in-cell completion, argument validation, named-range completion or a claim of Excel function compatibility.

The interaction follows the explicit, nested-formula completion pattern documented in [Microsoft’s Formula AutoComplete guidance](https://support.microsoft.com/en-us/excel/use-formula-autocomplete), while advertising only Gridloom’s own supported function names.


## 0.5.8: Rounding without silent underflow or crashes

`ROUND`, `ROUNDUP`, `ROUNDDOWN` and `TRUNC` now quantize decimal coefficients directly, without overflowing integer powers or dividing tiny nonzero values into zero before rounding. Extreme precision arguments do not wrap. Results outside the supported decimal range return `#NUM!`; requesting more decimal places than a value already has leaves that value unchanged.

`ROUND` retains Gridloom’s existing **half-even** tie rule: `ROUND(1.25,1)` is `1.2`, while `ROUND(1.35,1)` is `1.4`. This is not a claim of Excel rounding parity. `ROUNDUP` rounds away from zero, while `ROUNDDOWN` and `TRUNC` round toward zero. Negative precision rounds to positions left of the decimal point. Existing argument coercion is unchanged, including truncating numeric fractional precision arguments toward zero.

Saved workbooks are recalculated on open. Previously wrong cached formula results may therefore change, and existing review invalidation safeguards remain in force; old evidence is not silently treated as approval of a corrected number. These fixes are limited to the four named rounding functions, not a complete numerical audit.

The retained tie behavior follows [Rust Decimal’s nearest-even rounding](https://docs.rs/rust_decimal/latest/rust_decimal/enum.RoundingStrategy.html). Directional rounding follows the familiar [away-from-zero](https://support.microsoft.com/en-us/excel/functions/roundup-function) and [toward-zero](https://support.microsoft.com/en-us/excel/functions/rounddown-function) definitions.
