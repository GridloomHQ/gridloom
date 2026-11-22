# Install Gridloom

Choose a Windows or Linux x86-64 package and download the accompanying `SHA256SUMS` from the [latest release](https://github.com/lhemerly/gridloom/releases/latest).

## Windows

To verify the Windows desktop ZIP, run `Get-FileHash <zip-file> -Algorithm SHA256` in PowerShell (replace `<zip-file>` with the actual name) and compare it with the ZIP entry in `SHA256SUMS`. Extract the entire
ZIP into a writable folder, then open **gridloom.exe**. Keep the executables,
`duckdb.dll` and `frontend` directory together.

The desktop uses the [Microsoft Edge WebView2 Evergreen Runtime](https://developer.microsoft.com/microsoft-edge/webview2/#download-section),
included with Windows 11. If your system is missing it, install it separately
from Microsoft. If Windows reports missing MSVC runtime DLLs, install the
[official Visual C++ x64 Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist).
The ZIP is portable and unsigned; it does not install or update either Microsoft runtime.
The bundled headless `gridloom-mcp.exe` does not need WebView2.
Windows ARM64 is not included. Use an NTFS-backed workbook folder for guarded saves.

## Linux

Choose the server variant for a browser interface, or desktop for a native window. From the directory containing the downloaded tarballs, substitute the actual filename below:

```bash
archive='gridloom-<version>-linux-x86_64-server.tar.gz'
grep -F "  $archive" SHA256SUMS | sha256sum -c -
tar -xzf "$archive"
"./${archive%.tar.gz}/gridloom"
```

For the server variant, open the local URL printed in the terminal. Both variants start with one blank worksheet. Open a saved workbook to load its data and connections.

Packages include frontend assets and DuckDB, but not every system library. They are built and checked on Ubuntu 24.04 x86_64; the desktop variant needs compatible GTK3/WebKitGTK 4.1 runtime libraries. They are **not signed installers or universal Linux images**. Inspect the extracted `docs/shared-libraries.txt` and `docs/build-info.json` for dependencies and source identity. Checksums detect altered bytes, not publisher authenticity.

## Next steps

- [Build your first model](../README.md#start-with-a-number)
- [Connect an agent over MCP](MCP.md)
- [Build from source](DEVELOPMENT.md)
