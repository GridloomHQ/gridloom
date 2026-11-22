# Gridloom 0.13.1

This release adds a **Windows x64 desktop ZIP**. It includes the native app,
the headless MCP server, the worksheet UI and DuckDB. The same release also
includes Linux desktop and server packages.

- **Windows:** Extract the entire ZIP and open `gridloom.exe`. Keep the other
  files and folders alongside it. The desktop needs Microsoft Edge WebView2
  Evergreen; it is included with Windows 11 and available separately
  [from Microsoft](https://developer.microsoft.com/microsoft-edge/webview2/#download-section).
  If Windows reports a missing MSVC runtime DLL, install the
  [Microsoft Visual C++ x64 Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist).
  This is an unsigned portable ZIP, not an installer; it does not install runtimes.
- **Agent Review:** Use the bundled `gridloom-mcp.exe` on Windows or
  `gridloom-mcp` on Linux. An agent can recalculate and re-read exact values,
  trace dependencies, record an AI-assisted review or question, and verify
  that a later input change makes that review stale. This is a local workbook
  record, not an authenticated approval or audit opinion.
- **Safe saves:** Both components guard against overwriting a file that changed
  after opening. When a save conflicts, work remains open and can be saved as a copy.

Use the desktop and MCP from this release together. See [MCP setup](MCP.md) for
the Codex registration command and the self-review example. Verify the
downloads with the supplied SHA-256 checksums.
