# Tayriva 0.15.0

Run exact-decimal worksheet formulas on each row of a linked SQLite table and write the results in one transaction. Input rows are untouched. The dedicated results table is replaced; an invalid input or missing rule aborts the batch without changing the prior results.

- Open a configured template and choose **Data → Run formula batch**. Template rules can use arithmetic, comparisons, IF, ROUND and exact VLOOKUP against another sheet.
- Read the [setup guide](https://github.com/TayrivaHQ/tayriva/blob/v0.15.0/docs/worksheet-formula-batches.md), or run the [four-case Receita demo](https://github.com/TayrivaHQ/tayriva-demos/blob/main/receita/mixed/README.md).
- The [648× result](https://github.com/TayrivaHQ/tayriva-demos/blob/main/receita/benchmark/matched-10m.md) was measured on a development build with the same formula batch implementation. It compares four synthetic CBS/IBS scenarios with Receita Federal's reference calculator, not general performance across all tax rules or every platform.

Extract the full archive. Launch **tayriva.exe** on Windows or **./tayriva** on Linux. See [installation](https://github.com/TayrivaHQ/tayriva/blob/v0.15.0/docs/INSTALL.md) and [agent setup](MCP.md).
