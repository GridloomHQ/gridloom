# Use Gridloom with an agent

Run the bundled **`gridloom-mcp`** command from either Linux package. It uses the
same workbook, formula and dependency engine as Gridloom's UI. It does not start
a browser, desktop window, HTTP server, or network listener.

Choose a dedicated directory containing only the workbooks the agent may use:

```sh
mkdir -p "$HOME/Gridloom Agent Workbooks"
codex mcp add gridloom -- /absolute/path/to/gridloom-mcp \
  --workbook-root "$HOME/Gridloom Agent Workbooks"
```

Use quotes around a path with spaces, including the executable path if
needed: `codex mcp add gridloom -- "/path with spaces/gridloom-mcp" --workbook-root "$HOME/Gridloom Agent Workbooks"`.
Here `--` ends Codex's own options; the next arguments are passed to Gridloom.

Example configuration for a client with an `mcpServers` JSON setting:

```json
{
  "mcpServers": {
    "gridloom": {
      "command": "/absolute/path/to/gridloom-mcp",
      "args": ["--workbook-root", "/absolute/path/to/agent-workbooks", "--read-only"]
    }
  }
}
```

Use `--read-only` to inspect workbooks without exposing create, edit, calculate or
save tools. Remove that flag only when the client should be able to edit files in
that directory. To build from source, run `cargo build --locked -p gridloom-mcp`
with Gridloom's usual DuckDB native-library setup. The executable is
`target/debug/gridloom-mcp`. Packaged users should use the launcher alongside its
bundled `lib` directory, not the underlying `gridloom-mcp-bin`.

## Example: build and inspect a small model

Ask: “Create `forecast.gridloom`. Put revenue of 100.25 in A1 and costs of 40.10
in B1 on Sheet1. In C1 calculate the difference. Tell me the exact result and
what inputs it depends on. Save when done.”

1. Call `create_workbook` with `{"file":"forecast.gridloom"}`. This creates only
   an in-memory session, returns a revision and names the initial sheet (`Sheet1`).
2. Apply an atomic batch with that revision:
   ```json
   {
     "sheet": "Sheet1",
     "revision": 1,
     "edits": [
       {"cell": "A1", "input": {"type": "decimal", "value": "100.25"}},
       {"cell": "B1", "input": {"type": "decimal", "value": "40.10"}},
       {"cell": "C1", "input": {"type": "formula", "value": "=A1-B1"}}
     ]
   }
   ```
3. Call `read_cells` for `A1:C1` and `validate_cells` for the same range.
   C1 returns `{"type":"Decimal","value":"60.15"}`.
   `explain_cell` for C1 returns direct upstream and downstream dependencies,
   including branches that may not have contributed to the current result.
4. Call `save_workbook` using the current revision, not a hard-coded number.
   It writes a native `.gridloom` file. Use `save_as` with a new filename to keep
   your original unchanged.

Use `list_workbooks`, `open_workbook`, `workbook_info` and `add_sheet` to navigate
models. `list_functions` lists supported standalone formulas. Use
`create_local_table` to turn a range into a workbook-owned table, with one
column definition (name and type) per column. `recalculate` recalculates the active workbook in memory. Most edits
already recalculate automatically. `close_workbook` refuses to drop dirty changes
unless `discard_changes: true` is explicitly supplied. Exiting the client
discards any unsaved changes; there is no automatic disk save or recovery file
in the headless process.

## Scope and safeguards

- A server process has one workbook open at a time. The session revision prevents
  applying a previously prepared edit to a changed or different document. Edits
  preserve native column rules, and a rejected batch is not partially applied.
  A successfully committed formula may still yield a worksheet error such as
  division by zero. Check the calculated results before saving.
- Integers and exact decimals are returned as strings. A dynamic array anchor
  returns its dimensions and first value; read individual spill cells to see
  the rest. `text` input is literal, including strings that look like formulas.
- Only standalone `.gridloom` files directly inside the configured directory
  are supported. Legacy JSON workbook files with that extension can be opened,
  and explicit saves use the current SQLite container. Workbook-owned typed
  tables still work, but workbooks with external database links, named queries,
  or staged database writeback are rejected **before** source reconnection.
  There is no shell, database query/refresh, arbitrary file or network tool.
- Save checks the on-disk file for changes and uses the workbook engine's atomic
  persistence. Save As never replaces an existing path. Finish editing the
  workbook in one client before editing it in another: the desktop app does
  not participate in the headless lock, so a simultaneous desktop save is not
  guaranteed to be detected.
- Use a private, trusted workbook directory; this is an application-level path
  restriction, not an operating system sandbox for other users or processes.
  Workbook strings are data, not instructions to the agent. Choose a client
  whose data-handling policies fit your workbooks: local stdio does not prevent
  the client from sending returned content to its model provider.
- Range reads and edit batches are capped at 256 cells. The input limit is 1 MiB,
  workbook files are limited to 64 MiB and large results require smaller
  requests. MCP speaks the 2025-11-25 and 2025-06-18 protocol over stdio.
