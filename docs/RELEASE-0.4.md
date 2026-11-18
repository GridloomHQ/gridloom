# Gridloom 0.4 preview: Accounting working papers

## Try the workflow

1. Open **Monthly close** and import a month of trial-balance CSV. The original CSV and its hash stay in the workbook. This example is synthetic:

   ```csv
   account,name,debit,credit
   1000,Cash,10000,0
   4000,Sales,0,12000
   5000,Processing fees,2000,0
   ```

2. Under **Entity, currency & report hierarchy**, name the entity, a three-letter currency code and the units (for example, `units` or `thousands`). This labels existing amounts without converting or scaling them. Keep the default P&L, or create IDs, labels, parent lines and debit or credit normal balances. Map accounts to leaf lines, then save mappings. Parent totals roll up their children's signed balances, expressed in the parent's normal balance.
3. In **Line review queue**, open a report line to explain it. Use **Inputs** and **Outputs** to follow or highlight direct worksheet dependencies. **Circle up a question** works even when a value is blank, erroneous or incomplete. **Open questions** lists open threads on every worksheet.
4. Return to **Monthly close → Adjustments & supporting evidence**. Post a balanced journal with a preparer name, rationale and at least one retained support file. For example, debit 5000 and credit 1000 by 75 to record fees omitted from the import. The original import remains intact. Account balances use **imported net + posted adjustment net = reported net**, debit positive. The default P&L now shows fees of 2,075 and profit of 9,925. The posting receipt lists report lines whose amounts or review status changed and the previously current sign-offs invalidated across the workbook.
5. Inspect the changed figures or compare a recorded review against the current capture. Resolve the open question with a dated explanation; reopen it if necessary. Resolving does not sign off the calculation. Record a new review only after checks pass.
6. Save the `.gridloom` file. **Export review pack** retains the source CSV, workbook calculation state, report setup and mappings, journals, supporting bytes and hashes, questions and their event captures, and recorded reviews with exact inputs. The passive HTML and CSV are readable summaries; retain the JSON alongside them.

## Boundaries

- A working paper has one entity, one month and one currency. There is no consolidation, FX or units conversion. IFRS taxonomy, iXBRL validation and filing are out of scope. Both default and custom reports are neutral workbook layouts, not regulatory statements.
- Journals affect the working paper only. They do not post to a ledger. Posted adjustments and supporting files are retained; correct an entry with another balanced journal. Direct edits to controlled cells cannot become valid posted adjustments. Undo/redo covers spreadsheet edits, not journals or close mutations.
- Rollforward imports a fresh trial balance and carries the entity, currency, units, report hierarchy and matching mappings; it carries no adjustments or approvals. A 0.3 period retains its original worksheet layout unless explicitly configured or upgraded through posting.
- Calculation dependencies are not proof of contribution. Formula inspection is conservative, includes all branches, and stops at SQL/external-model boundaries. Calculations can require re-review even when their numeric total is unchanged.
- Workbook identities and retained evidence are local, not independently verified or authenticated. Hashes identify bytes, not their trustworthiness. This is a reproducible working paper, not audit assurance or a locked financial close.
