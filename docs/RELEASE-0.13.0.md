# Gridloom 0.13.0

**Review the numbers your agent writes.** The Linux downloads now include
`gridloom-mcp`, a local command your agent can use without starting the app.
Let it write formulas, check an exact result against an independent expectation,
and trace the cells behind a discrepancy. The agent can raise a question,
correct the model, and record a review with notes. The worksheet app can open
the same review history.

- **Review records are tied to the calculation.** Trace transitive worksheet
  inputs, including workbook-owned table formulas. A changed input or formula
  can make the record stale even if the displayed number stays the same.
  Questions have their own history; resolving one does not mark a cell reviewed.
- **See what changed.** Compare a retained review with the current calculation
  or another review of the same cell. MCP exposes evidence references and
  embedded attachment metadata, but it does not open links or evidence files.
- **Protect work from competing saves.** The app and MCP reject a stale save,
  leave the original untouched, and let the losing editor Save As to a new file.
  Use both components from this release. The 0.12.0 app cannot block its own
  stale save when used alongside MCP.
- **Local by design.** The MCP command starts no network listener and accepts
  only standalone workbooks in a chosen folder. Add `--read-only` for inspection.
  Workbooks with external database connections still belong in the desktop or
  web app. MCP reviews are labeled AI-assisted, not human approval or audit
  assurance.

[Set up the MCP server and try a self-review](MCP.md).
