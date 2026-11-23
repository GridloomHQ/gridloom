# 0.4 verification

- Core and server: 115 Rust tests passed locally. The HTTP working-paper scenario covers an incomplete report question, dependency navigation, required mappings, a supported balanced adjustment, affected lines and invalidated reviews, resolution, and a review pack preserving exact values and original evidence.
- Frontend: 20 tests passed, and the locked TypeScript/Vite build passed.
- Benchmark: the existing four local workloads passed their correctness and resource gates.
- Packaging: 28 package and workflow contract tests passed. The package job builds and tests relocatable archives for the exact main-branch commit only after the browser/native and frontend jobs pass.

The added browser acceptance uses synthetic data and exercises custom reporting, grid dependency highlights, the workbook-wide question queue, journal impact, resolution and export. Local cloud Chromium could not start because the process cannot open its required sockets; native WebKit development libraries are also absent there. Check the [GitHub Actions results for the exact commit](https://github.com/GridloomHQ/tayriva/actions/workflows/ci.yml) for browser, native and Linux packaging verification.
