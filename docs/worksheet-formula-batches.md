# Worksheet formula batches (SQLite)

A formula batch uses worksheet cells as a per-row calculation template for a linked SQLite database. The app streams input rows, runs the template through the regular decimal formula evaluator and replaces the result set in one transaction. It never applies changes to the source table. Writes are explicit and not part of worksheet Undo.

## Template setup

1. Add a linked SQLite file. Prepare distinct source and destination tables in that database. Both use the same name for their one declared INTEGER primary key. The results table must be dedicated to this snapshot: it cannot have triggers or inbound foreign key references. The command replaces all rows in that table, including rows whose source key was removed.
2. Set worksheet A1 to `TAYRIVA FORMULA BATCH 1`. On row 2, enter the linked connection name, source table, results table and primary key in A2:D2.
3. On row 5, list contiguous `in:column` headers from column A for SQLite INTEGER inputs. Follow them with `calc:name` for intermediate formulas or `out:column` for mapped SQLite INTEGER outputs.
4. On row 6, use normal worksheet formula cells under the calculation and output headers. The batch substitutes each SQLite row under the input headers. For an interactive single-row preview, put parameterized SQLVALUE lookups in those input cells and point at an editable key elsewhere in the worksheet. Preview overrides do not change persisted input rows.
5. Open **Data → Run formula batch** on the template sheet. Complete or discard any staged edits to the linked connection first. Inspect the source and destination before confirming.

Templates are limited to 63 columns and scalar decimal arithmetic (`+`, `-`, `*`, `/`, unary signs/percent and `ROUND`), numeric literals and references to earlier columns on row 6. The batch rejects all other formulas instead of silently substituting alternate semantics. Output values must be exact signed 64-bit integers; store money in cents when decimal storage is required. Formula, schema, constraint, overflow, or write errors roll back prior results. No per-invoice result cache is used.
