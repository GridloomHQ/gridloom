# Gridloom

**Work with millions of rows. Show where the numbers come from.**

Gridloom is a local-first spreadsheet for financial models and accounting work. Browse a live database table without importing every row. Then audit a calculation in the same workbook, from the result back to its inputs.

[**Download Gridloom for Windows or Linux**](https://github.com/GridloomHQ/gridloom/releases/latest) · [Quick start](https://github.com/GridloomHQ/gridloom/blob/main/docs/INSTALL.md)

## 10 million rows. No million-row wall.

Gridloom's live SQLite tables don't stop at [Excel's 1,048,576-row worksheet limit](https://support.microsoft.com/en-us/excel/excel-specifications-and-limits). Gridloom fetches the rows around your view instead of loading the full table. Filter and sort in SQL, then jump to the rows you need.

**First invoice to invoice 10,000,000 in 141 ms**, including rendering, in one local test on a synthetic SQLite dataset. [See how it was measured.](https://github.com/GridloomHQ/gridloom/blob/main/docs/WORKBOOKS.md#live-table-performance-and-limits)

![Gridloom showing a live SQLite invoice register with invoice 10,000,000 selected and the column headings frozen at the top.](https://raw.githubusercontent.com/GridloomHQ/gridloom/main/docs/images/live-sql-10m.webp)

## Audit the number, not just the formula

Pick a result and open **Review → Explain & Review**. Follow its inputs or see which formulas use it. Record what you checked, attach supporting evidence, or raise a question without losing the calculation it came from. If the calculation or a traced input changes, Gridloom marks the review stale, even if the displayed total stays the same.

![Gridloom's Explain and Review panel traces gross profit of 51,200 to June sales and direct costs, with the sales input highlighted in the worksheet.](https://raw.githubusercontent.com/GridloomHQ/gridloom/main/docs/images/explain-and-review.webp)

## Give your agent a way to check its own work

The bundled headless MCP server lets an agent write a formula, compare the exact result with an independent calculation, trace and fix a mismatch, and record an AI-assisted review you can open in Gridloom. [Try the agent self-review walkthrough.](https://github.com/GridloomHQ/gridloom/blob/main/docs/MCP.md)

## Try it with a real workbook

Start with [**The Total Still Ties**](https://github.com/GridloomHQ/gridloom-demos/blob/main/workbooks/the-total-still-ties.gridloom). The control total is **$100.25**, but rent and software are swapped. Trace the amounts with **Explain & Review**, fix the two mappings, then record a review on the total. Move $1 from rent to software in the ledger: the total still ties, but the review goes stale because its inputs changed.

- [**Alphabet DCF**](https://github.com/GridloomHQ/gridloom-demos/blob/main/workbooks/alphabet-fcff.gridloom): Trace an illustrative per-share value back through cash flows, the cost of capital, and SEC-filed figures.
- [**U.S. Treasury curve**](https://github.com/GridloomHQ/gridloom-demos/blob/main/derivatives/treasury-snapshot.gridloom): Turn a dated set of public Treasury yields into modeled forward rates and a trade value.
- [**OIS swap**](https://github.com/GridloomHQ/gridloom-demos/blob/main/derivatives/ois-swap.gridloom): Trace synthetic swap quotes through the discount curve to the price and its sensitivity to a rate change.

[Download the workbooks and follow the walkthroughs](https://github.com/GridloomHQ/gridloom-demos). Open any of them in Gridloom using **File → Open**.

[Workbook guide](https://github.com/GridloomHQ/gridloom/blob/main/docs/WORKBOOKS.md) · [Build from source](https://github.com/GridloomHQ/gridloom/blob/main/docs/DEVELOPMENT.md) · [License](LICENSE)
