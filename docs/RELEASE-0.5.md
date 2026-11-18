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

## Keyboard editing

- Enter / Shift+Enter: move down / up.
- Tab / Shift+Tab: move right / left.
- F2, double-click or typing: edit the selected cell.
- While editing, Enter or Tab commits and moves only after success. A failed submission keeps the draft available; Escape cancels a draft.
- Home: first column on this row. Ctrl/Command+Home: A1.
- Page Up / Page Down: approximately one visible page. Shift extends selection for arrow, Home and page navigation.

Existing copy/paste, formula fill, undo/redo and evidence workflows remain available. This release does not claim Excel file compatibility, full keyboard parity, frozen panes, arbitrary row insertion, sorting, autosave or authenticated review identities.

## Product research and next priorities

Official Excel documentation establishes [Find scope and values/formulas choices](https://support.microsoft.com/en-gb/excel/get-started/find-or-replace-text-and-numbers-on-a-worksheet), [Enter/Tab editing conventions](https://support.microsoft.com/en-us/excel/edit-cell-contents), and [frozen headings](https://support.microsoft.com/en-us/excel/get-started/freeze-panes-to-lock-rows-and-columns). These are useful behavioral references, not a claim of implementation parity.

The next candidates are freeze panes, then safe range sort/filter and explicit data-validation rules. Structural edits, sorting and replace must preserve formula references and invalidate captured reviews correctly. Recovery/autosave needs a clear distinction between a draft, a saved working paper and staged external writeback. XBRL/IFRS remains an add-on concern.
