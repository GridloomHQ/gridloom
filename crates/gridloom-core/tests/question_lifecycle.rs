use gridloom_core::review::{
    QuestionAction, QuestionTransitionRequest, ReviewDecision, ReviewRecord, ReviewRequest,
};
use gridloom_core::{CellCoord, Workbook};

fn cell() -> CellCoord {
    CellCoord::new(0, 0)
}
fn request(wb: &Workbook, decision: ReviewDecision) -> ReviewRequest {
    ReviewRequest {
        sheet_name: "Sheet1".into(),
        row: 0,
        col: 0,
        expected_snapshot: wb.explain_cell("Sheet1", cell()).unwrap().snapshot,
        decision,
        reviewer: "Local preparer".into(),
        note: "Please verify this calculation".into(),
        evidence: vec![],
    }
}
fn transition(
    wb: &Workbook,
    q: &ReviewRecord,
    action: QuestionAction,
) -> QuestionTransitionRequest {
    QuestionTransitionRequest {
        action,
        actor: "Local reviewer".into(),
        note: "Documented disposition".into(),
        expected_snapshot: wb.explain_cell("Sheet1", cell()).unwrap().snapshot,
        expected_last_event_id: q.question_events.last().map(|e| e.id),
    }
}

#[test]
fn questions_on_errors_and_blanks_are_not_signoff() {
    for input in ["=1/0", ""] {
        let mut wb = Workbook::new();
        wb.set_cell("Sheet1", cell(), input.into()).unwrap();
        assert!(!wb.explain_cell("Sheet1", cell()).unwrap().can_review);
        assert!(wb
            .record_review(request(&wb, ReviewDecision::Reviewed))
            .is_err());
        let q = wb
            .record_review(request(&wb, ReviewDecision::Question))
            .unwrap();
        assert!(q.question_is_open());
        assert!(
            !wb.review_history()[0].invalidated,
            "an unchanged error question is not a failed signoff"
        );
        let req = transition(&wb, &q, QuestionAction::Resolved);
        let resolved = wb.transition_question(q.id, req).unwrap();
        assert!(!resolved.question_is_open());
        assert!(wb.open_questions().is_empty());
        assert!(
            wb.record_review(request(&wb, ReviewDecision::Reviewed))
                .is_err(),
            "resolving never approves errors"
        );
    }
}

#[test]
fn lifecycle_is_explicit_concurrency_checked_and_persisted() {
    let mut wb = Workbook::new();
    wb.set_cell("Sheet1", cell(), "10".into()).unwrap();
    let q = wb
        .record_review(request(&wb, ReviewDecision::Question))
        .unwrap();
    assert!(wb
        .record_review(request(&wb, ReviewDecision::Reviewed))
        .unwrap_err()
        .contains("open questions"));
    let stale = transition(&wb, &q, QuestionAction::Resolved);
    wb.set_cell("Sheet1", cell(), "20".into()).unwrap();
    assert!(wb
        .transition_question(q.id, stale)
        .unwrap_err()
        .contains("Calculation changed"));
    assert_eq!(wb.open_questions().len(), 1);
    let req = transition(&wb, &q, QuestionAction::Resolved);
    let resolved = wb.transition_question(q.id, req).unwrap();
    assert_eq!(resolved.note, q.note);
    assert_eq!(resolved.snapshot, q.snapshot);
    assert_ne!(resolved.question_events[0].snapshot, q.snapshot);
    assert!(wb.open_questions().is_empty());
    wb.record_review(request(&wb, ReviewDecision::Reviewed))
        .unwrap();
    let stale_history = transition(&wb, &q, QuestionAction::Reopened);
    assert!(wb
        .transition_question(q.id, stale_history)
        .unwrap_err()
        .contains("history changed"));
    let reopen = transition(&wb, &resolved, QuestionAction::Reopened);
    let reopened = wb.transition_question(q.id, reopen).unwrap();
    assert_eq!(reopened.question_events.len(), 2);
    assert_eq!(wb.open_questions().len(), 1);
    let duplicate = transition(&wb, &reopened, QuestionAction::Reopened);
    assert!(wb
        .transition_question(q.id, duplicate)
        .unwrap_err()
        .contains("already"));
    assert_eq!(wb.reviews[0].question_events.len(), 2);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("questions.gridloom");
    wb.save_to_file(&path).unwrap();
    let loaded = Workbook::open_from_file(&path).unwrap();
    assert_eq!(loaded.open_questions().len(), 1);
    assert_eq!(loaded.reviews[0].question_events.len(), 2);
    assert_eq!(loaded.reviews[0].question_events[0].actor, "Local reviewer");
}

#[test]
fn legacy_questions_are_open_and_cannot_transition_review_records() {
    let mut wb = Workbook::new();
    wb.set_cell("Sheet1", cell(), "10".into()).unwrap();
    let reviewed = wb
        .record_review(request(&wb, ReviewDecision::Reviewed))
        .unwrap();
    assert!(wb
        .transition_question(
            reviewed.id,
            transition(&wb, &reviewed, QuestionAction::Resolved)
        )
        .is_err());
    let q = wb
        .record_review(request(&wb, ReviewDecision::Question))
        .unwrap();
    let mut json = serde_json::to_value(&q).unwrap();
    json.as_object_mut().unwrap().remove("question_events");
    let old: ReviewRecord = serde_json::from_value(json).unwrap();
    assert!(old.question_is_open());
    assert!(old.question_events.is_empty());
}

#[test]
fn comparison_classifies_formula_and_input_changes_even_when_total_matches() {
    let mut wb = Workbook::new();
    wb.set_cell("Sheet1", cell(), "10".into()).unwrap();
    wb.set_cell("Sheet1", CellCoord::new(0, 1), "=A1*2".into())
        .unwrap();
    let mut req = request(&wb, ReviewDecision::Reviewed);
    req.col = 1;
    req.expected_snapshot = wb
        .explain_cell("Sheet1", CellCoord::new(0, 1))
        .unwrap()
        .snapshot;
    let review = wb.record_review(req).unwrap();
    wb.set_cell("Sheet1", CellCoord::new(0, 1), "=A1+A1".into())
        .unwrap();
    let diff = wb.compare_review(review.id).unwrap();
    assert!(diff
        .summaries
        .iter()
        .any(|s| s.category == "formula" && s.coord == Some(CellCoord::new(0, 1))));
    assert_eq!(diff.affected_cells.len(), 1);
    assert!(!diff.changes.is_empty());
    wb.set_cell("Sheet1", cell(), "15".into()).unwrap();
    let diff = wb.compare_review(review.id).unwrap();
    assert!(diff
        .summaries
        .iter()
        .any(|s| s.category == "input" && s.coord == Some(cell())));
    assert_eq!(diff.affected_cells.len(), 2);
}

#[test]
fn close_snapshot_binds_metadata_without_duplicating_evidence_and_detects_tampering() {
    use gridloom_core::close::{AdjustmentLine, ImportTrialBalanceRequest, PostAdjustmentRequest};
    use gridloom_core::review::EvidenceUpload;
    use rust_decimal::Decimal;
    let mut wb = Workbook::new();
    let period = wb
        .import_trial_balance(ImportTrialBalanceRequest {
            period: "2026-10".into(),
            csv: "account,name,debit,credit\n100,Cash,100,0\n400,Sales,0,100\n".into(),
            mappings: [
                ("100".into(), "Balance sheet".into()),
                ("400".into(), "Revenue".into()),
            ]
            .into_iter()
            .collect(),
        })
        .unwrap();
    let journal = wb
        .post_close_adjustment(PostAdjustmentRequest {
            period_id: period.id,
            author: "Preparer".into(),
            rationale: "Supported accrual".into(),
            lines: vec![
                AdjustmentLine {
                    account: "100".into(),
                    debit: Decimal::from(5),
                    credit: Decimal::ZERO,
                },
                AdjustmentLine {
                    account: "400".into(),
                    debit: Decimal::ZERO,
                    credit: Decimal::from(5),
                },
            ],
            evidence: vec![EvidenceUpload {
                filename: "invoice.txt".into(),
                media_type: "text/plain".into(),
                bytes: b"retained only once".to_vec(),
                replaces: None,
            }],
        })
        .unwrap();
    wb.set_cell("Sheet1", cell(), format!("='{}'!I2", period.sheet_name))
        .unwrap();
    let ex = wb.explain_cell("Sheet1", cell()).unwrap();
    assert!(ex.can_review, "{:?}", ex.warnings);
    let config = &ex.input_snapshot.inputs[&format!("close_working_paper:{}", period.id)];
    assert!(config.get("source_csv").is_none());
    assert!(config.get("configuration_history").is_none());
    assert_eq!(config["source_sha256"], period.source_sha256);
    let journals = &ex.input_snapshot.inputs[&format!("close_adjustments:{}", period.id)];
    assert!(journals[0]["evidence"][0].get("bytes").is_none());
    assert_eq!(
        journals[0]["evidence"][0]["sha256"],
        journal.evidence[0].sha256
    );
    let review = wb
        .record_review(request(&wb, ReviewDecision::Reviewed))
        .unwrap();
    wb.set_cell(&period.sheet_name, CellCoord::new(2, 4), "101".into())
        .unwrap();
    let after = wb.explain_cell("Sheet1", cell()).unwrap();
    assert!(!after.can_review);
    assert_eq!(
        ex.value.to_string(),
        after.value.to_string(),
        "unsupported import edits do not silently rewrite the baseline"
    );
    let diff = wb.compare_review(review.id).unwrap();
    assert!(diff
        .summaries
        .iter()
        .any(|s| s.category == "working_paper_input"));
    assert!(diff.summaries.iter().any(|s| s.category == "reviewability"));
    assert!(diff
        .changes
        .iter()
        .any(|c| c.path.contains("close_import_cells") && c.path.contains("E3")));
    assert!(wb
        .record_review(request(&wb, ReviewDecision::Reviewed))
        .is_err());
    assert!(wb
        .record_review(request(&wb, ReviewDecision::Question))
        .is_ok());
}

#[test]
fn reviewability_only_change_has_summary_and_exact_diff() {
    let mut wb = Workbook::new();
    wb.set_cell("Sheet1", cell(), "10".into()).unwrap();
    let review = wb
        .record_review(request(&wb, ReviewDecision::Reviewed))
        .unwrap();
    wb.stale_calculation_sheets.insert("Sheet1".into());
    let diff = wb.compare_review(review.id).unwrap();
    assert!(diff.summaries.iter().any(|s| s.category == "reviewability"));
    assert!(!diff.changes.is_empty());
}
