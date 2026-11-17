# Changelog

## 0.2.0 (preview)

- Explain a selected number through transitive worksheet references, spill anchors, structured tables and cached SQL query/result boundaries.
- Record local reviews, circled-up questions, notes and evidence references inside the workbook, with retained history.
- Bind reviews to versioned content snapshots. Reject stale submissions and invalidate affected reviews after observed calculation or dependency changes. Invalidation is sticky across reverting inputs and saving/reopening.
- Block review of errors, blank cells, incomplete traces, pending database patches and known-stale source caches after writeback.
- Include a small illustrative cash-reserve model in the sample workbook.
- Roll back rejected cyclic cell edits so they cannot leave a half-updated calculation behind an apparently valid review.

This preview does not add authenticated reviewers, document ingestion/verification, row-level aggregate provenance, live external-source monitoring or production accounting/audit assurance. Native desktop packaging and interactive browser smoke testing should be verified on a fully provisioned host before release publication.
