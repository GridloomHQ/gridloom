use gridloom_core::{
    review::{EvidenceReference, ReviewDecision, ReviewRequest},
    workbook::Sheet,
    CellCoord, Value, Workbook,
};

fn coord(a1: &str) -> CellCoord {
    CellCoord::from_a1(a1).unwrap()
}
fn set(wb: &mut Workbook, cell: &str, input: &str) {
    wb.set_cell("Sheet1", coord(cell), input.into()).unwrap();
}
fn request(wb: &Workbook, sheet: &str, cell: &str) -> ReviewRequest {
    let c = coord(cell);
    ReviewRequest {
        sheet_name: sheet.into(),
        row: c.row,
        col: c.col,
        expected_snapshot: wb.explain_cell(sheet, c).unwrap().snapshot,
        decision: ReviewDecision::Reviewed,
        reviewer: "Controller".into(),
        note: "Checked model".into(),
        evidence: vec![EvidenceReference {
            label: "Budget approval".into(),
            reference: "board-pack.pdf, page 4".into(),
        }],
    }
}
fn model() -> Workbook {
    let mut wb = Workbook::new();
    set(&mut wb, "A1", "100");
    set(&mut wb, "B1", "=A1*2");
    set(&mut wb, "C1", "=B1+10");
    wb
}

#[test]
fn transitive_trace_and_sticky_invalidation_even_when_result_is_unchanged() {
    let mut wb = model();
    let e = wb.explain_cell("Sheet1", coord("C1")).unwrap();
    assert!(e.can_review);
    assert_eq!(
        e.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
        vec!["cell:A1", "cell:B1", "cell:C1"]
    );
    assert_eq!(
        e.nodes
            .iter()
            .find(|n| n.id == "cell:C1")
            .unwrap()
            .depends_on,
        vec!["cell:B1"]
    );
    wb.record_review(request(&wb, "Sheet1", "C1")).unwrap();
    set(&mut wb, "Z1", "unrelated");
    assert!(!wb.review_history()[0].invalidated);
    set(&mut wb, "B1", "=A1+A1");
    assert_eq!(
        wb.explain_cell("Sheet1", coord("C1"))
            .unwrap()
            .value
            .to_string_repr(),
        "210"
    );
    assert!(wb.review_history()[0].invalidated);
    set(&mut wb, "B1", "=A1*2");
    assert!(
        wb.review_history()[0].invalidated,
        "restoration must not resurrect review"
    );
    let first = wb.review_history()[0].id;
    wb.record_review(request(&wb, "Sheet1", "C1")).unwrap();
    assert!(!wb.review_history()[0].invalidated);
    assert_eq!(wb.review_history()[1].id, first);
    set(&mut wb, "A1", "100.0");
    assert!(
        wb.review_history()[0].invalidated,
        "type/raw-input changes matter even with equal numeric output"
    );
}

#[test]
fn stale_submission_and_invalid_question_are_rejected_without_writing_history() {
    let mut wb = model();
    let stale = request(&wb, "Sheet1", "C1");
    set(&mut wb, "A1", "101");
    assert!(wb.record_review(stale).unwrap_err().contains("changed"));
    let mut question = request(&wb, "Sheet1", "C1");
    question.decision = ReviewDecision::Question;
    question.note = " ".into();
    assert!(wb.record_review(question).unwrap_err().contains("question"));
    assert!(wb.reviews.is_empty());
}

#[test]
fn save_open_preserves_evidence_history_and_loads_legacy_without_reviews() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("model.gridloom");
    let mut wb = model();
    let mut question = request(&wb, "Sheet1", "C1");
    question.decision = ReviewDecision::Question;
    question.note = "Confirm the growth assumption".into();
    wb.record_review(question).unwrap();
    wb.record_review(request(&wb, "Sheet1", "C1")).unwrap();
    wb.save_to_file(&path).unwrap();
    let loaded = Workbook::open_from_file(&path).unwrap();
    assert_eq!(loaded.reviews.len(), 2);
    assert_eq!(loaded.reviews[0].decision, ReviewDecision::Question);
    assert_eq!(
        loaded.review_history()[0].evidence[0].reference,
        "board-pack.pdf, page 4"
    );
    assert!(!loaded.review_history()[0].invalidated);
    let mut legacy: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    legacy.as_object_mut().unwrap().remove("reviews");
    std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    assert!(Workbook::open_from_file(path).unwrap().reviews.is_empty());
}

#[test]
fn sheets_with_identical_coordinates_keep_independent_reviews() {
    let mut wb = model();
    wb.sheets.insert("Other".into(), Sheet::new("Other".into()));
    wb.set_cell("Other", coord("A1"), "100".into()).unwrap();
    wb.record_review(request(&wb, "Sheet1", "A1")).unwrap();
    wb.record_review(request(&wb, "Other", "A1")).unwrap();
    set(&mut wb, "A1", "200");
    assert!(
        !wb.explain_cell("Other", coord("A1"))
            .unwrap()
            .review
            .unwrap()
            .invalidated
    );
    assert!(
        wb.explain_cell("Sheet1", coord("A1"))
            .unwrap()
            .review
            .unwrap()
            .invalidated
    );
}

#[test]
fn range_spill_and_error_review_boundaries() {
    let mut wb = Workbook::new();
    set(&mut wb, "A1", "1");
    set(&mut wb, "A2", "2");
    set(&mut wb, "B1", "=SUM(A1:A2)");
    let e = wb.explain_cell("Sheet1", coord("B1")).unwrap();
    assert_eq!(
        e.nodes
            .iter()
            .find(|n| n.id == "cell:B1")
            .unwrap()
            .depends_on,
        vec!["cell:A1", "cell:A2"]
    );
    set(&mut wb, "D1", "=SEQUENCE(2,1)");
    let spill = wb.explain_cell("Sheet1", coord("D2")).unwrap();
    assert!(spill.can_review);
    assert_eq!(
        spill
            .nodes
            .iter()
            .find(|n| n.id == "cell:D2")
            .unwrap()
            .depends_on,
        vec!["cell:D1"]
    );
    wb.record_review(request(&wb, "Sheet1", "D2")).unwrap();
    set(&mut wb, "D1", "=SEQUENCE(2,1,2)");
    assert!(wb.review_history()[0].invalidated);
    set(&mut wb, "F1", "=1/0");
    assert!(!wb.explain_cell("Sheet1", coord("F1")).unwrap().can_review);
    assert!(wb.record_review(request(&wb, "Sheet1", "F1")).is_err());
    assert!(!wb.explain_cell("Sheet1", coord("G1")).unwrap().can_review);
    set(&mut wb, "H1", "=SUM(A1:A300)");
    let partial = wb.explain_cell("Sheet1", coord("H1")).unwrap();
    assert!(!partial.can_review);
    assert!(partial.nodes.len() <= 256);
    assert!(partial.warnings.iter().any(|w| w.contains("partial")));
}

#[test]
fn structured_source_and_sql_are_cached_boundaries_not_invented_row_provenance() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let db = rusqlite::Connection::open(file.path()).unwrap();
    db.execute_batch("CREATE TABLE amounts(id INTEGER PRIMARY KEY, amount INTEGER); INSERT INTO amounts VALUES(1,10),(2,20)").unwrap();
    let mut wb = Workbook::new();
    wb.add_sqlite_connection("db", file.path()).unwrap();
    wb.create_named_source("totals", "db", "SELECT SUM(amount) AS total FROM amounts")
        .unwrap();
    wb.insert_table("Sheet1", "Totals", "totals", coord("A1"))
        .unwrap();
    set(&mut wb, "C1", "=SUM(Totals[total])");
    let e = wb.explain_cell("Sheet1", coord("C1")).unwrap();
    assert!(e
        .nodes
        .iter()
        .any(|n| n.kind == "source" && n.detail.as_ref().unwrap().contains("SUM(amount)")));
    assert!(e
        .warnings
        .iter()
        .any(|w| w.contains("Contributing database rows are not inferred")));
    wb.record_review(request(&wb, "Sheet1", "C1")).unwrap();
    db.execute("UPDATE amounts SET amount=11 WHERE id=1", [])
        .unwrap();
    assert!(
        !wb.review_history()[0].invalidated,
        "external DB changes are unobserved until refresh"
    );
    wb.refresh_source("totals").unwrap();
    assert!(wb.review_history()[0].invalidated);
    set(
        &mut wb,
        "E1",
        "=SQLVALUE(\"db\",\"SELECT SUM(amount) FROM amounts\")",
    );
    let sql = wb.explain_cell("Sheet1", coord("E1")).unwrap();
    assert!(sql.nodes.iter().any(|n| n.kind == "sql"));
    assert!(sql
        .warnings
        .iter()
        .any(|w| w.contains("do not re-run queries")));
    wb.staged.stage_edit(
        "db".into(),
        "amounts".into(),
        vec!["id".into()],
        vec![Value::Integer(1)],
        "amount".into(),
        Value::Integer(11),
        Value::Integer(12),
    );
    wb.invalidate_changed_reviews();
    assert!(!wb.explain_cell("Sheet1", coord("E1")).unwrap().can_review);
    assert!(wb.record_review(request(&wb, "Sheet1", "E1")).is_err());
}

#[test]
fn rejected_cyclic_edit_preserves_review_and_calculation() {
    let mut wb = model();
    wb.record_review(request(&wb, "Sheet1", "C1")).unwrap();
    assert!(wb.set_cell("Sheet1", coord("A1"), "=C1".into()).is_err());
    let e = wb.explain_cell("Sheet1", coord("C1")).unwrap();
    assert_eq!(e.value.to_string_repr(), "210");
    assert!(!e.review.unwrap().invalidated);
}

#[test]
fn applied_writeback_blocks_stale_source_reviews_until_refresh_even_after_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("data.sqlite");
    let model_path = dir.path().join("model.gridloom");
    let db = rusqlite::Connection::open(&db_path).unwrap();
    db.execute_batch("CREATE TABLE amounts(id INTEGER PRIMARY KEY, amount INTEGER); INSERT INTO amounts VALUES(1,10),(2,20)").unwrap();
    let mut wb = Workbook::new();
    wb.add_sqlite_connection("db", &db_path).unwrap();
    wb.create_named_source("amounts", "db", "SELECT * FROM amounts ORDER BY id")
        .unwrap();
    wb.insert_table("Sheet1", "Amounts", "amounts", coord("A1"))
        .unwrap();
    set(&mut wb, "D1", "=SUM(Amounts[amount])");
    set(
        &mut wb,
        "F1",
        "=SQLVALUE(\"db\",\"SELECT SUM(amount) FROM amounts\")",
    );
    set(&mut wb, "H1", "42");
    wb.record_review(request(&wb, "Sheet1", "D1")).unwrap();
    wb.staged.stage_edit(
        "db".into(),
        "amounts".into(),
        vec!["id".into()],
        vec![Value::Integer(1)],
        "amount".into(),
        Value::Integer(10),
        Value::Integer(15),
    );
    wb.apply_writeback("db").unwrap();
    let stale = wb.explain_cell("Sheet1", coord("D1")).unwrap();
    assert_eq!(stale.value.to_string_repr(), "30");
    assert!(
        !stale.can_review,
        "known-stale source cannot receive a new sign-off"
    );
    assert!(stale.review.unwrap().invalidated);
    assert!(wb.record_review(request(&wb, "Sheet1", "D1")).is_err());
    assert!(
        wb.explain_cell("Sheet1", coord("H1")).unwrap().can_review,
        "unrelated literal model remains reviewable"
    );
    assert_eq!(
        wb.explain_cell("Sheet1", coord("F1"))
            .unwrap()
            .value
            .to_string_repr(),
        "35",
        "direct SQL formulas recalculate after writeback"
    );
    wb.save_to_file(&model_path).unwrap();
    let mut wb = Workbook::open_from_file(&model_path).unwrap();
    assert!(
        !wb.explain_cell("Sheet1", coord("D1")).unwrap().can_review,
        "save/reopen must not clear source staleness"
    );
    wb.refresh_source("amounts").unwrap();
    let fresh = wb.explain_cell("Sheet1", coord("D1")).unwrap();
    assert!(fresh.can_review);
    assert_eq!(fresh.value.to_string_repr(), "35");
    assert!(
        fresh.review.unwrap().invalidated,
        "refresh does not resurrect previous review"
    );
    wb.record_review(request(&wb, "Sheet1", "D1")).unwrap();
    assert!(!wb.review_history()[0].invalidated);
}

#[test]
fn pnl_review_retains_trial_balance_evidence_when_a_contributing_mapping_changes() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let db = rusqlite::Connection::open(file.path()).unwrap();
    db.execute_batch("CREATE TABLE trial_balance(account TEXT PRIMARY KEY, mapping TEXT, amount INTEGER); INSERT INTO trial_balance VALUES('Sales','P&L',1000),('Payroll','P&L',-300),('Cash','Balance sheet',700)").unwrap();
    let mut wb = Workbook::new();
    wb.add_sqlite_connection("Ledger", file.path()).unwrap();
    wb.create_named_source(
        "TrialBalanceSnapshot",
        "Ledger",
        "SELECT account, mapping, amount FROM trial_balance ORDER BY account",
    )
    .unwrap();
    wb.insert_table("Sheet1", "TB", "TrialBalanceSnapshot", coord("A1"))
        .unwrap();
    set(&mut wb, "F1", "=SUMIF(TB[mapping],\"P&L\",TB[amount])");
    let mut review = request(&wb, "Sheet1", "F1");
    review.note = "Agreed P&L total to the trial-balance snapshot".into();
    review.evidence = vec![EvidenceReference {
        label: "September trial balance".into(),
        reference: "TB-2026-09.pdf, page 2".into(),
    }];
    wb.record_review(review).unwrap();
    assert_eq!(
        wb.explain_cell("Sheet1", coord("F1"))
            .unwrap()
            .value
            .to_string_repr(),
        "700"
    );
    db.execute(
        "UPDATE trial_balance SET mapping='Balance sheet' WHERE account='Payroll'",
        [],
    )
    .unwrap();
    wb.refresh_source("TrialBalanceSnapshot").unwrap();
    let current = wb.explain_cell("Sheet1", coord("F1")).unwrap();
    assert_eq!(current.value.to_string_repr(), "1000");
    let prior = current.review.unwrap();
    assert!(prior.invalidated);
    assert_eq!(prior.evidence[0].reference, "TB-2026-09.pdf, page 2");
    assert_eq!(prior.note, "Agreed P&L total to the trial-balance snapshot");
}

#[test]
fn duplicate_source_cannot_relabel_existing_tables_or_bypass_refresh_guards() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let db = rusqlite::Connection::open(file.path()).unwrap();
    db.execute_batch("CREATE TABLE amounts(id INTEGER PRIMARY KEY, amount INTEGER); INSERT INTO amounts VALUES(1,10),(2,20)").unwrap();
    let mut wb = Workbook::new();
    wb.add_sqlite_connection("db", file.path()).unwrap();
    wb.create_named_source("amounts", "db", "SELECT id, amount FROM amounts")
        .unwrap();
    wb.insert_table("Sheet1", "Amounts", "amounts", coord("A1"))
        .unwrap();
    set(&mut wb, "E1", "=SUM(Amounts[amount])");
    wb.record_review(request(&wb, "Sheet1", "E1")).unwrap();
    let error = wb
        .create_named_source("amounts", "db", "SELECT amount, id FROM amounts")
        .unwrap_err();
    assert!(error.message().contains("already exists"));
    assert_eq!(
        wb.named_sources["amounts"].sql,
        "SELECT id, amount FROM amounts"
    );
    assert_eq!(
        wb.explain_cell("Sheet1", coord("E1"))
            .unwrap()
            .value
            .to_string_repr(),
        "30"
    );
    assert!(!wb.review_history()[0].invalidated);
    wb.stale_sources.insert("amounts".into());
    assert!(wb
        .create_named_source("amounts", "db", "SELECT amount, id FROM amounts")
        .is_err());
    assert!(!wb.explain_cell("Sheet1", coord("E1")).unwrap().can_review);
}
