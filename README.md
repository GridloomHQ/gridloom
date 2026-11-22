# Gridloom

[![CI](https://github.com/lhemerly/gridloom/actions/workflows/ci.yml/badge.svg)](https://github.com/lhemerly/gridloom/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**A spreadsheet for building and reviewing models. Trace a number back to the inputs that drive it.**

Gridloom combines a familiar worksheet with SQL-backed tables and a built-in review workflow. Build financial models, follow the formulas behind a number, and keep questions, review notes, and supporting evidence in the workbook. Gridloom runs locally.

## Download

[**Get Gridloom for Windows or Linux**](https://github.com/lhemerly/gridloom/releases/latest)

- **Windows x64:** Extract the entire ZIP and open `gridloom.exe`. The desktop app uses [Microsoft Edge WebView2 Evergreen](https://developer.microsoft.com/microsoft-edge/webview2/#download-section), included with Windows 11.
- **Linux x64:** Extract the archive and run `./gridloom` from the extracted folder. The desktop build needs GTK3 and WebKitGTK 4.1; for the server build, open the local address printed in the terminal.

[Installation and checksum instructions](https://github.com/lhemerly/gridloom/blob/main/docs/INSTALL.md)

## Try it

1. Enter `100.25` in **A1**, `40.10` in **B1**, and `=A1-B1` in **C1**. The result is `60.15`.
2. Select **C1** and open **Review → Explain & Review** to inspect the formula and follow its inputs. Record a review with a note on what you checked, or circle up a question if something doesn't add up.
3. Change an input and return to a recorded review to see it marked stale. Save the workbook as a `.gridloom` file to keep the calculation and review history together.

## Work with your data

Create a typed table in the worksheet or connect a SQLite or DuckDB file and use query results in formulas, pivots, and charts. Supported source edits are staged for review before you apply them to the database. The [workbook guide](https://github.com/lhemerly/gridloom/blob/main/docs/WORKBOOKS.md) covers tables, live data, review, and the Monthly close add-on.

## Let an agent check its own work

The Windows and Linux downloads bundle a headless MCP server. An agent can write a formula, compare the exact result to an independently calculated expectation, trace a mismatch, fix it, and record the check in the same workbook you open. These are local AI-assisted review records, not human sign-off or audit assurance.

[Connect an agent and try the self-review walkthrough](https://github.com/lhemerly/gridloom/blob/main/docs/MCP.md)

[Build from source and run tests](https://github.com/lhemerly/gridloom/blob/main/docs/DEVELOPMENT.md) · [License](LICENSE)
