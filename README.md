# Gridloom

**Work with millions of rows. Show where the numbers come from.**

Gridloom is a local-first spreadsheet for financial models and accounting work. Browse a live database table without importing every row. Then audit a calculation in the same workbook, from the result back to its inputs.

[**Download Gridloom for Windows or Linux**](https://github.com/lhemerly/gridloom/releases/latest) · [Quick start](https://github.com/lhemerly/gridloom/blob/main/docs/INSTALL.md)

## 10 million rows. No million-row wall.

Gridloom's live SQLite tables don't stop at [Excel's 1,048,576-row worksheet limit](https://support.microsoft.com/en-us/excel/excel-specifications-and-limits). Gridloom fetches the rows around your view instead of loading the full table. Filter and sort in SQL, then jump to the rows you need.

**First invoice to invoice 10,000,000 in 141 ms**, including rendering, in one local test on a synthetic SQLite dataset. [See how it was measured.](https://github.com/lhemerly/gridloom/blob/main/docs/WORKBOOKS.md#live-table-performance-and-limits)

![Gridloom showing a live SQLite invoice register with invoice 10,000,000 selected and the column headings frozen at the top.](https://raw.githubusercontent.com/lhemerly/gridloom/main/docs/images/live-sql-10m.webp)

## Audit the number, not just the formula

Pick a result and open **Review → Explain & Review**. Follow its inputs or see which formulas use it. Record what you checked, attach supporting evidence, or raise a question without losing the calculation it came from. If the calculation or a traced input changes, Gridloom marks the review stale, even if the displayed total stays the same.

![Gridloom's Explain and Review panel traces gross profit of 51,200 to June sales and direct costs, with the sales input highlighted in the worksheet.](https://raw.githubusercontent.com/lhemerly/gridloom/main/docs/images/explain-and-review.webp)

These are workbook review records, not authenticated sign-offs or audit assurance.

## Give your agent a way to check its own work

The bundled headless MCP server lets an agent write a formula, compare the exact result with an independent calculation, trace and fix a mismatch, and record an AI-assisted review you can open in Gridloom. [Try the agent self-review walkthrough.](https://github.com/lhemerly/gridloom/blob/main/docs/MCP.md)

## Start with a number

Enter `100.25` in **A1**, `40.10` in **B1**, and `=A1-B1` in **C1**. Select the **60.15** result, open **Explain & Review**, and record a check. Change **A1** and see that review go stale. Save the calculation and review history together in one `.gridloom` file.

[Workbook guide](https://github.com/lhemerly/gridloom/blob/main/docs/WORKBOOKS.md) · [Build from source](https://github.com/lhemerly/gridloom/blob/main/docs/DEVELOPMENT.md) · [License](LICENSE)
