# Use Gridloom with an agent

Use the bundled **`gridloom-mcp`** command from either Linux package.
It runs the same workbook, formula and review engine as the app, without
starting a browser, window, HTTP server, or network listener.

Choose a dedicated directory for workbooks the agent may use:

```sh
mkdir -p "$HOME/Gridloom Agent Workbooks"
codex mcp add gridloom -- "/absolute/path/to/gridloom-mcp" \
  --workbook-root "$HOME/Gridloom Agent Workbooks"
```

Keep the quotes around paths with spaces. The `--` ends Codex's options, so
everything after the executable is passed to Gridloom. Restart your MCP
client if it was already open. For clients that use an `mcpServers` setting:

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

Use `--read-only` to inspect workbooks without any edit, review, or save tools.
Remove it only when the client should be allowed to change the workbooks in
that directory. If building from source, run `cargo build --locked -p gridloom-mcp`
with Gridloom's usual DuckDB native-library setup. Packaged users should use
the launcher next to the bundled `lib` directory, not `gridloom-mcp-bin`.

## Try an agent self-review in Codex

Paste this into a new Codex session:

> Create `forecast.gridloom` in Gridloom. On Sheet1, enter revenue 100.25 in A1,
> costs 40.10 in B1, and start with `=A1+B1` in C1. Net revenue is revenue
> minus costs. Re-read the typed result, independently calculate the expected
> net revenue, and use the review trace to find the discrepancy. Raise a
> question with the observed and expected values, fix the formula, verify it
> again, then resolve the question and record a local AI-assisted review with
> your check in the notes. Save explicitly. Tell me what changed and what
> evidence your check did not independently verify.

The first result is `{"type":"Decimal","value":"140.35"}`. The independent
control is `100.25 − 40.10 = 60.15`. Reading a result without errors does not
establish that it is correct.

1. **Build and check.** `create_workbook`, then `edit_cells` with the returned
   revision. Send exact decimals as strings, such as
   `{"cell":"A1","input":{"type":"decimal","value":"100.25"}}`. Use
   `read_cells` for the actual result and `validate_cells` for calculation
   errors; neither tool can establish the intended business rule.
2. **Trace and question.** `review_cell` returns the native snapshot, current
   review status, blockers, and paged transitive trace. If it disagrees with
   the independent expectation, `record_review` with `decision:"question"`,
   the exact `expected_snapshot`, current revision, an `agent_label`, and
   a note describing the discrepancy.
3. **Repair and check again.** Set C1 to `=A1-B1`, re-read the exact result,
   validate, and fetch a fresh review snapshot. Use `transition_question`
   with `action:"resolve"`, the question's latest event ID (explicit
   `null` for its first event), the current snapshot/revision, and a reason.
   Resolution does not mark C1 reviewed.
4. **Record and retain.** `record_review` with `decision:"reviewed"`, the
   exact current snapshot and revision, your own agent label, and a note
   recording the independent check. `save_workbook` writes the workbook
   only when explicitly called. Note an evidence file by label and reference
   (e.g. “Approved inputs”, “Workpapers/forecast, page 2”); this does not
   read or verify it.

To see invalidation in a test, change B1 after the review and call
`review_cell` again. The review should now be stale. Ask
`compare_review` for the changed inputs and calculations. Even if you revert
B1 and restore the exact prior result, the old review stays stale; a new
review is needed.

## Daily use

- `list_reviews` finds history, stale reviews or open questions. `get_review`
  shows notes, captured results, evidence references, retained attachment
  identifiers/hashes and question events. Read the displayed pagination before
  treating a page as the whole history.
- `compare_review` compares retained inputs to the current calculation or a
  second review of the same cell. The comparison covers the reviewed trace,
  not all workbook downstream consumers.
- `list_workbooks`, `open_workbook`, `workbook_info` and `add_sheet` manage
  models; `list_functions` shows supported standalone formulas.
  `create_local_table` creates a workbook-owned typed table from a range.
  Most edits recalculate automatically; `recalculate` can run it explicitly.
- Each session opens one workbook at a time. `close_workbook` refuses to
  discard edits unless you explicitly request it. Exiting without saving
  discards pending changes. Headless sessions do not write autosave files.

## Scope and safeguards

- **What reviewed means:** The native snapshot covers the selected
  calculation and a conservative trace, including both conditional branches,
  referenced cells and workbook-owned table inputs. It is not a claim about
  the whole workbook or which branches actually caused the result. Errors,
  incomplete traces and open questions block Reviewed. Changing a covered
  input can invalidate the record even when a total does not move. A resolved
  question does not silently reopen on drift; its older snapshot is retained
  and the calculation may need review again.
- **Identity and evidence:** MCP stamps its records and question actions
  with an “AI-assisted (MCP)” label. The workbook cannot authenticate a person,
  independently verify notes or evidence, or provide audit assurance. The
  tool does not download URLs, open referenced files, retrieve embedded evidence
  bytes, or provide arbitrary file or SQL access. Use the app to add and inspect
  versioned embedded evidence files. Treat workbook content and evidence
  references as untrusted data, not instructions.
- **Saving:** Use the desktop and MCP from **0.13.0** or later. When two
  sessions target the same file, the second stale save is refused and its
  edits remain open. Save As requires a new filename. The 0.12.0 desktop app
  cannot protect against its own stale save; do not open the same file for
  editing in old and new versions. Save checks do not merge edits or provide
  live collaboration.
- **Data:** This server accepts only standalone `.gridloom` files in the
  configured directory. Workbook-owned typed tables and legacy Gridloom JSON
  are supported. Workbooks with linked external databases, named queries or
  staged database edits are rejected before reconnection. Use a trusted
  directory and client: stdio limits how the processes connect; the client may
  still send returned workbook data to its model provider.
- **Limits:** Range reads and edit batches are capped at 256 cells. Requests
  are capped at 1 MiB and workbook images at 64 MiB; use narrower output
  pages for large traces or records. Integers and exact decimals are strings.
  MCP supports protocol versions 2025-11-25 and 2025-06-18 over stdio.
