# Use Tayriva with an agent

Use **`tayriva-mcp`** from either Linux package or **`tayriva-mcp.exe`**
from the Windows ZIP. Keep it in the unpacked package next to its bundled
DuckDB library. It does not start a window, web server, or network listener.

Create a dedicated directory for workbooks the agent may access. Use absolute
paths for both the launcher and the workbook directory.

## Connect to local Codex

**For a trusted project:** In the project root, create or edit
`.codex/config.toml` and add this block. This controls discovery for sessions
started in that project, not for all your work. Only trust projects whose
configuration you have inspected. Use an untracked local config when paths
are specific to your computer.

```toml
[mcp_servers.tayriva]
command = "/absolute/path/to/tayriva-mcp"
args = ["--workbook-root", "/absolute/path/to/agent-workbooks", "--read-only"]
```

On Windows, use single-quoted TOML strings for paths to avoid escaping
backslashes, e.g. `command = 'C:\Apps\Tayriva\tayriva-mcp.exe'`.
Open a new **local** Codex session with the project as its working directory.
From that same directory, run `codex mcp list` to confirm the entry is loaded;
in an interactive CLI session, `/mcp` shows the connected server and tools.
If the entry is absent, check the active workspace and project trust. If the
entry exists but tools are absent, check the absolute paths, bundled library,
and that the workbook root already exists. A cloud session cannot launch a
binary that exists only on your computer.

**For just one CLI invocation:** Use the same settings as command-line
overrides instead of writing any config. For example, on Linux:

```sh
codex -c 'mcp_servers.tayriva.command="/absolute/path/to/tayriva-mcp"' \
  -c 'mcp_servers.tayriva.args=["--workbook-root","/absolute/path/to/agent-workbooks","--read-only"]'
```

The overrides apply to that invocation only. `codex mcp add` normally writes
to your user-wide Codex config, so it is not needed for either scoped method.
See [Codex MCP configuration](https://developers.openai.com/codex/mcp)
for the client configuration reference.

For other clients that use an `mcpServers` setting:

```json
{
  "mcpServers": {
    "tayriva": {
      "command": "/absolute/path/to/tayriva-mcp",
      "args": ["--workbook-root", "/absolute/path/to/agent-workbooks", "--read-only"]
    }
  }
}
```

Keep `--read-only` for inspection. Remove it and start a new session only
if the agent should be able to edit, record reviews, and save. If building
from source, run `cargo build --locked -p gridloom-mcp` with the
[DuckDB setup](DEVELOPMENT.md); the internal build target still uses that name.
On Linux, use the packaged launcher, not `tayriva-mcp-bin`.

## Try an agent self-review in Codex

Remove `--read-only` for this exercise, then paste this into a new session:

> Create `forecast.tay` in Tayriva. On Sheet1, enter revenue 100.25 in A1,
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
  second review of the same cell. Edited inputs, mappings and formulas appear
  before recalculated results. `category_counts` covers the whole comparison;
  `changes` is only the current page. Pass `page.next_offset` as `offset`
  and the first page's `revision` as `expected_revision` for later pages.
  If the revision changes, restart at offset zero. The comparison covers the
  reviewed trace, not all downstream consumers.
- `list_workbooks`, `open_workbook`, `workbook_info` and `add_sheet` manage
  models; `list_functions` shows supported standalone formulas.
  `create_local_table` creates a workbook-owned typed table from a range.
  Most edits recalculate automatically; `recalculate` can run it explicitly.
- `read_cells` includes one row per requested cell, including blanks. An
  unset cell is `{"type":"Blank"}` with null `raw_input`, `formula`, and
  `table`, and false `spill` and `error` flags. Intentionally empty text
  and null are distinct value types.
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
- **Saving:** When two sessions target the same file, the second stale save
  is refused and its edits remain open. Save As requires a new filename.
  `create_workbook` and `save_workbook`'s `save_as` append `.tay` to
  extensionless names. Save checks do not merge edits or provide live collaboration.
- **Data:** This server accepts only standalone Tayriva `.tay` workbooks
  in the configured directory. Older Gridloom files are not supported,
  even if renamed. Workbooks with linked external databases, named queries
  or staged database edits are rejected before reconnection. Use a trusted
  directory and client: stdio limits how the processes connect; the client
  may still send returned workbook data to its model provider.
- **Limits:** Range reads and edit batches are capped at 256 cells. Requests
  are capped at 1 MiB and workbook images at 64 MiB; use narrower output
  pages for large traces or records. Integers and exact decimals are strings.
  MCP supports protocol versions 2025-11-25 and 2025-06-18 over stdio.
