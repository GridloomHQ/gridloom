# Workbook guide

### Work in a spreadsheet

- Use multiple sheets, cross-sheet formulas, named table references and dynamic array spills.
- Select, copy and paste ranges; fill formulas; format numbers and cells; undo and redo supported cell operations.
- Use the tabs in the command bar to switch tool groups without adding another row. Right-click a worksheet tab to rename or delete it. You cannot delete the only worksheet, a worksheet referenced by another sheet, or one with recorded reviews or managed close data.
- Use Ctrl+Arrow to jump to the end of a populated run, across a gap to the next populated cell, or to the worksheet edge if there is no more data. Hold Shift to extend the selection. Ctrl+Space selects the current column; Shift+Space selects the current row.
- Use **Paste Special** on a worksheet selection copied in Tayriva: paste typed values without formulas, or paste formulas with adjusted relative references. Both keep destination formatting; formulas-only paste skips source constants and blanks. See **Home → Keyboard shortcuts**.
- Find cached values or formula inputs, jump to a cell, resize columns, freeze the rows above or columns to the left of the current cell from **View**, and show formulas directly in the grid.
- Get selection totals, browse functions by category, syntax, arguments and return values with **fx**, and complete function names in either editor. In an F2 edit, use arrows where a formula expects a reference and hold Shift to extend the range. Referenced cells are outlined while you edit. F4 cycles relative and absolute references.
- Select a worksheet range and open **Sort & filter** for a read-only view. You can use the first row as headers, sort and filter on different columns, and jump back to a result’s original row. Formulas, database links and staging are unchanged. Each view supports up to 1,000 rows, 50 columns and 5,000 cells.
- Build pivots and charts by dragging fields into the layout wells, or use the field pickers by keyboard. Choose signed bar and column charts, lines, pies or scatterplots. Reopen a saved pivot and choose **Refresh from source** to rerun its read query without changing its layout, the workbook snapshot, or any staged database edits.
- Save and reopen `.tay` workbooks, including layouts, calculations and pending source edits. A saved workbook is one local SQLite file. Ordinary sheets need no database connection or setup.

The formula engine includes arithmetic, statistics, logic, text, date, lookup and financial functions, plus `SQLVALUE` and `SQLTABLE`. Examples include `SUMIFS`, `SUMPRODUCT`, `XLOOKUP`, `FILTER`, `PMT` and `NPV`. The formula bar's suggestions and the local `/api/functions` endpoint expose the engine's supported function names and aliases.

### Create a local worksheet table

Select a header row and at least one data row, then choose **Insert → Create local table**. Give the table a name and set each column to Any, Text, Integer, Decimal, Boolean or Date. Existing headings are kept. Any columns can contain worksheet formulas; the other types validate literal entries, preserving leading zeroes in Text, exact Decimal values, and ISO dates. In a Text column, type two double quotes for an explicit empty string; **Data → Set SQL NULL** stores NULL, and clearing the cell leaves a blank.

Local table edits are workbook edits, not staged external database writes. The table and its rows live inside the same saved workbook file. Use regular cell/range references or structured references such as `=SUM(Transactions[Amount])`. No separate database connection is required.

### Bring SQL data into the worksheet

Connect an existing **SQLite** or **DuckDB** file, run a read-only query, and insert its result as a named table. Keep ordinary cells and formulas alongside it:

```excel
=SUM(Invoices[Amount])
=AVERAGEIFS(Invoices[Amount], Invoices[Status], "Paid")
=SQLVALUE("CustomerDatabase", "SELECT SUM(amount) FROM invoices WHERE customer_name = ?", A2)
```

Apply worksheet presentation formatting to source-backed cells without staging data edits. The database stays in its original format. Eligible single-table projections with direct column mappings and declared primary keys can support staged edits. Joined, aggregated or ambiguous results remain read-only.

When you edit an eligible source-backed cell, Tayriva checks the declared SQL column type before staging. For example, an integer rejects fractions, a decimal rejects excess scale rather than rounding, and text keeps leading zeroes. Use **Set SQL NULL** for a nullable cell; it is distinct from empty text. Unsupported types are read-only. The database still checks its remaining constraints when you apply edits.

Editing a source-backed cell does **not** immediately change the database. **Review Edits** shows original and proposed values before an explicit apply. Applying a changeset checks for conflicting external changes and uses a database transaction; a conflict or error rolls back that changeset. Saving a workbook is separate from applying its source edits.

### Browse a large SQLite ledger

Use **SQLite ledger** to browse a connected SQLite file without importing the rows into a worksheet. Choose a physical table with a single integer primary key, a posting-date column stored as `YYYY-MM-DD` text, and an amount stored as integer cents. Optional dates filter in SQLite. The count and total cover the full matching ledger, not just loaded rows. Totals outside SQLite’s signed 64-bit integer range produce an error.

To check where a balance comes from, select a cell and choose **Trace balance**. Choose the ledger, the account column and code, and both dates, then load the results. Tayriva compares the cell’s underlying value in currency units (an exact integer or decimal, with no fractions of a cent) with the matching journal total and shows the difference. **Save link to cell** keeps the query in the workbook after you save it; it does not store the journal rows or change SQLite. A saved link runs against live data when opened again. If a source changes during browsing, reload before relying on the comparison.

An index on the posting date (binary collation) and the primary key speeds ordered browsing. Scroll within a block, use the next/previous block controls, or jump directly to a row. Forward scrolling can seek from the current page; a distant jump still skips earlier rows in SQLite. If the source changes, reload the view so rows and totals agree. This read-only view does not write to the database or expose imported worksheet cells to formulas.

### Work with a live worksheet table

In **Data → SQL Query**, choose **Live table**, select a worksheet, and enter the top-left cell. A live table keeps the rows in SQLite and loads a small window around the visible cells. Use **Data → Refresh** after external changes to update the table's bounds. Ctrl+Arrow moves to the destination without loading the cells in between.

Live tables support `SELECT *` from one SQLite table with a single integer primary key, optional simple `WHERE` comparisons, and `ORDER BY` columns. SQL sorting and filtering apply to all matching rows. You can stage supported edits by the source row's primary key and apply them separately. Use a materialized snapshot for joins, computed projections, or worksheet formulas and review tools that need values from a table range.

### Live table performance and limits

The [10-million-row browser test](../frontend/tests/live-table-10m-smoke.mjs) generates a synthetic SQLite invoice table, opens it as a live worksheet table, refreshes its bounds, and presses Ctrl+Down from the first invoice. It waits for the last invoice to be selected, rendered and visible, and checks that the browser requests a small window near the destination without fetching the intervening rows. It also tests same-size data changes and reopen after restart. The test runs in [CI](../.github/workflows/ci.yml).

The project screenshot uses Gridloom 0.13.1 with the test dataset and normal worksheet formatting and frozen headers. On one local Linux/Chromium run with an Intel Core i7-8700K and 32 GB RAM, refresh took **322 ms** to the visible first invoice and the Ctrl+Down jump took **141 ms** to the visible final invoice. These times include rendering, not database creation. Query shape, indexes, hardware and starting position affect timing. This is a navigation measurement, not a head-to-head speed comparison with Excel.

Live tables are not subject to Excel's fixed grid size; they use SQLite query results and Tayriva's finite row coordinates and bounded viewport requests. This does not mean unlimited data in memory. Ordinary cells, worksheet-owned tables and materialized query results are stored as workbook data, while clipboard, worksheet sort/filter and other bulk operations have separate bounds. For millions of rows, keep the source in SQLite and use a live table instead of pasting or importing the full result into ordinary worksheet cells.

### Insert worksheet rows

Select a row or a range and choose **Insert rows above**. Tayriva adds one blank row per selected row, moves the existing rows and follows direct A1 references across the workbook, including absolute references. Ranges that cross the insertion point expand. A whole table can move, but an insertion cannot split a table.

If a formula or workbook structure cannot be updated safely, the insertion is rejected without changing the workbook. This includes active spills, computed references, affected historical review anchors, and Monthly close sheets. Row insertion clears the existing Undo/Redo history; use **Save As** first if you want a separate copy.

### Explain and review a calculation

Use **Explain & Review** to inspect a cell's formula, captured inputs and available source-row provenance. Follow upstream inputs and downstream consumers, compare observed states, and attach supporting files with retained versions.

Record a review or circle up an open question. Questions can be resolved or reopened, and changed calculation snapshots invalidate previous reviews. Source-row provenance is available only where the query's lineage can be established; it is not inferred for arbitrary SQL.

### Prepare accounting working papers

Enable **Monthly close** in **Add-ons** for a focused trial-balance workflow. It is off by default; switching it off leaves existing workbooks and close records intact.

1. Import a trial-balance CSV and map accounts to a report.
2. Define entity, currency, units and report hierarchy.
3. Generate a P&L and inspect debit/credit reconciliation and review items.
4. Add balanced adjustment journals with rationale and supporting evidence without replacing the imported trial balance.
5. See imported, adjusted and reported balances, roll mappings into a new period, and export a JSON review pack, P&L CSV or readable HTML summary.

### Save your work

Save the workbook before closing the application. Save refuses to overwrite another session’s changes; use **Save As** with a new filename to keep your edits as a separate copy. Only native Tayriva `.tay` files open. Earlier Gridloom SQLite and JSON files are not supported, even if renamed.

### Recover unsaved edits

Tayriva periodically saves committed workbook changes (approximately every two seconds) to local recovery files. After reopening the app or switching workbooks, choose **Recover autosave** to open a recovered copy, then **Save As** to keep it as a named workbook. Text still being typed or pending inside an unsubmitted dialog is not recoverable. Copies from previous editing sessions remain available, including after an explicit save; check their timestamps before restoring. A new workbook cannot silently overwrite an older recovery copy. Recovery copies never update the database; refreshing sources and applying staged database changes are separate steps.

Set `TAYRIVA_RECOVERY_DIR` to choose a private recovery directory. By default it is under the operating system's user-specific state/application-data directory. Recovery files can contain workbook data and should be protected like the original workbook.

## Workbook behavior and safety

- **Native workbook model.** Save the working model as `.tay`, which retains its source links, review notes and evidence. Use the reporting tools for CSV, HTML or JSON outputs where available. Tayriva does not import or export Excel workbooks.
- **Calculation and rounding.** Tayriva keeps exact integers and fixed-precision decimal values without rounding them to Excel’s display or file conventions. `ROUND` uses half-even rounding; some finance and statistical formulas use floating-point math internally. Results can differ from other spreadsheet tools. Check formulas and rounding policies before relying on a model for reporting.
- **Review packs.** Use packs to share your reconciliations, review notes and attached evidence. Reviewer names are entered by users, not verified signatures. Keep your source records and database backups separately.
- **Protect the files.** Workbooks and embedded evidence are unencrypted. Connection paths refer to the machine running the server; source databases are not embedded backups. Keep the local service private rather than exposing it as a multi-user server.
- **Save deliberately.** Use Save or Save As for your named workbook. Recovery may not include changes made just before a crash, and there is no universal native-window-close guard.
- **Know the scale.** The grid is virtualized. Snapshots materialize their results; live worksheet tables and the read-only SQLite ledger load bounded windows instead. Source refresh is explicit, not automatic monitoring. Find and bulk worksheet operations have bounded limits.
- **Extensions.** Monthly close is a built-in opt-in add-on. Third-party add-on installation and IFRS/XBRL taxonomy validation are not available.

## Practical examples

- [Monthly-close walkthrough and trial-balance CSVs](RELEASE-0.3.md)
- [Accounting working-paper example](RELEASE-0.4.md)
- [More worksheet and formula behavior](RELEASE-0.5.md)
