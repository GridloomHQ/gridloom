use gridloom_core::close::*;
use gridloom_core::review::{EvidenceUpload, ReviewDecision, ReviewRequest};
use gridloom_core::{CellCoord, Workbook};
use rust_decimal::Decimal;
use std::collections::BTreeMap;
fn import(wb: &mut Workbook) -> ClosePeriod {
    wb.import_trial_balance(ImportTrialBalanceRequest {
        period: "2026-10".into(),
        csv: "account,name,debit,credit\n100,Cash,100,0\n400,Sales,0,100\n".into(),
        mappings: [
            ("100".into(), "Balance sheet".into()),
            ("400".into(), "Revenue".into()),
        ]
        .into_iter()
        .collect(),
    })
    .unwrap()
}
fn journal(id: uuid::Uuid) -> PostAdjustmentRequest {
    PostAdjustmentRequest {
        period_id: id,
        author: "Preparer".into(),
        rationale: "Accrued supported revenue".into(),
        lines: vec![
            AdjustmentLine {
                account: "100".into(),
                debit: Decimal::from(25),
                credit: Decimal::ZERO,
            },
            AdjustmentLine {
                account: "400".into(),
                debit: Decimal::ZERO,
                credit: Decimal::from(25),
            },
        ],
        evidence: vec![EvidenceUpload {
            filename: "support.txt".into(),
            media_type: "text/plain".into(),
            bytes: b"Invoice A-123: 25".to_vec(),
            replaces: None,
        }],
    }
}
fn config(id: uuid::Uuid) -> ConfigureCloseRequest {
    ConfigureCloseRequest {
        period_id: id,
        entity: "Example Ltd".into(),
        currency: "BRL".into(),
        units: "units".into(),
        report_lines: vec![
            ReportLine {
                id: "net".into(),
                label: "Net position".into(),
                parent_id: None,
                normal_balance: NormalBalance::Debit,
            },
            ReportLine {
                id: "cash".into(),
                label: "Cash".into(),
                parent_id: Some("net".into()),
                normal_balance: NormalBalance::Debit,
            },
            ReportLine {
                id: "sales".into(),
                label: "Sales".into(),
                parent_id: Some("net".into()),
                normal_balance: NormalBalance::Credit,
            },
        ],
        mappings: [
            ("100".into(), "cash".into()),
            ("400".into(), "sales".into()),
        ]
        .into_iter()
        .collect(),
    }
}
#[test]
fn imported_plus_supported_adjustments_equals_reported_and_invalidates_review() {
    let mut wb = Workbook::new();
    let p = import(&mut wb);
    let ex = wb
        .explain_cell(&p.sheet_name, CellCoord::new(1, 8))
        .unwrap();
    wb.record_review(ReviewRequest {
        sheet_name: p.sheet_name.clone(),
        row: 1,
        col: 8,
        expected_snapshot: ex.snapshot,
        decision: ReviewDecision::Reviewed,
        reviewer: "Controller".into(),
        note: "Checked baseline".into(),
        evidence: vec![],
    })
    .unwrap();
    let j = wb.post_close_adjustment(journal(p.id)).unwrap();
    assert_eq!(j.evidence[0].bytes, b"Invoice A-123: 25");
    let status = wb.close_status(p.id).unwrap();
    assert_eq!(status.live_accounts[0].imported_balance, Decimal::from(100));
    assert_eq!(
        status.live_accounts[0].adjustment_balance,
        Decimal::from(25)
    );
    assert_eq!(
        status.live_accounts[0].reported_balance,
        Some(Decimal::from(125))
    );
    assert_eq!(status.queue[0].value.to_string(), "125");
    assert_eq!(status.queue[0].status, "Needs re-review");
    assert_eq!(status.warnings.len(), 1, "{:?}", status.warnings);
    assert_eq!(status.period.source_csv, p.source_csv);
    let pack = wb.export_review_pack(p.id).unwrap();
    assert_eq!(
        pack["status"]["period"]["adjustments"][0]["rationale"],
        "Accrued supported revenue"
    );
    let path = tempfile::tempdir().unwrap();
    let file = path.path().join("supported.gridloom");
    wb.save_to_file(&file).unwrap();
    let reopened = Workbook::open_from_file(file).unwrap();
    assert_eq!(
        reopened.close_status(p.id).unwrap().live_accounts[0].reported_balance,
        Some(Decimal::from(125))
    );
}
#[test]
fn adjustment_failures_are_atomic_and_generated_cell_edits_block_close() {
    let mut wb = Workbook::new();
    let p = import(&mut wb);
    let before = serde_json::to_string(&wb.sheets[&p.sheet_name]).unwrap();
    let mut request = journal(p.id);
    request.lines[0].debit = Decimal::from(24);
    assert!(wb.post_close_adjustment(request).is_err());
    let mut request = journal(p.id);
    request.evidence.clear();
    assert!(wb.post_close_adjustment(request).is_err());
    assert!(wb.close_periods[0].adjustments.is_empty());
    assert_eq!(
        serde_json::to_string(&wb.sheets[&p.sheet_name]).unwrap(),
        before
    );
    wb.set_cell(&p.sheet_name, CellCoord::new(1, 10), "999".into())
        .unwrap();
    let status = wb.close_status(p.id).unwrap();
    assert!(status
        .warnings
        .iter()
        .any(|w| w.contains("Reconciliation or journal cell changed")));
    assert!(!status.ready_for_close);
    assert!(wb.post_close_adjustment(journal(p.id)).is_err());
}
#[test]
fn user_hierarchy_dimensions_and_rollforward_preserve_mapping_not_adjustments() {
    let mut wb = Workbook::new();
    let p = import(&mut wb);
    wb.configure_close(config(p.id)).unwrap();
    wb.post_close_adjustment(journal(p.id)).unwrap();
    let status = wb.close_status(p.id).unwrap();
    assert_eq!(status.queue.len(), 3);
    assert_eq!(status.queue[0].value.to_string(), "0");
    assert_eq!(status.queue[1].value.to_string(), "125");
    assert_eq!(status.queue[2].value.to_string(), "125");
    let next = wb
        .rollforward_close(RollforwardRequest {
            source_period_id: p.id,
            period: "2026-11".into(),
            csv: p.source_csv.clone(),
        })
        .unwrap();
    assert_eq!(next.currency, "BRL");
    assert_eq!(next.entity, "Example Ltd");
    assert_eq!(next.report_lines.len(), 3);
    assert!(next.adjustments.is_empty());
    assert_eq!(
        wb.close_status(next.id).unwrap().live_accounts[0].mapping,
        "cash"
    );
    assert_eq!(
        wb.close_status(next.id).unwrap().queue[1].value.to_string(),
        "100"
    );
    assert_eq!(next.prior_period_id, Some(p.id));
    assert!(!next.configuration_history.is_empty());
    assert!(wb
        .map_close_accounts(MapAccountsRequest {
            period_id: p.id,
            mappings: BTreeMap::from([("100".into(), "net".into())])
        })
        .is_err());
}
#[test]
fn hierarchy_cycles_bad_currency_and_unknown_mapping_are_rejected_without_mutation() {
    let mut wb = Workbook::new();
    let p = import(&mut wb);
    let mut request = config(p.id);
    request.report_lines[0].parent_id = Some("cash".into());
    assert!(wb.configure_close(request).is_err());
    let mut request = config(p.id);
    request.currency = "brl".into();
    assert!(wb.configure_close(request).is_err());
    let mut request = config(p.id);
    request.mappings.insert("100".into(), "missing".into());
    assert!(wb.configure_close(request).is_err());
    assert!(wb.close_periods[0].report_lines.is_empty());
    assert!(wb.close_periods[0].configuration_history.is_empty());
}
#[test]
fn imported_facts_remain_baseline_even_after_working_grid_edit() {
    let mut wb = Workbook::new();
    let p = import(&mut wb);
    wb.set_cell(&p.sheet_name, CellCoord::new(1, 3), "200".into())
        .unwrap();
    let status = wb.close_status(p.id).unwrap();
    assert_eq!(
        status.live_accounts[0].reported_balance,
        Some(Decimal::from(100))
    );
    assert!(status
        .warnings
        .iter()
        .any(|w| w.contains("Imported debit/credit changed")));
    assert!(!status.ready_for_close);
    assert!(wb.post_close_adjustment(journal(p.id)).is_err());
}
#[test]
fn integrity_guard_blocks_signoff_but_allows_questions_about_incomplete_working_papers() {
    let mut wb = Workbook::new();
    let p = import(&mut wb);
    wb.set_cell(&p.sheet_name, CellCoord::new(1, 10), "999".into())
        .unwrap();
    let ex = wb
        .explain_cell(&p.sheet_name, CellCoord::new(1, 8))
        .unwrap();
    assert!(!ex.can_review);
    let request = |decision| ReviewRequest {
        sheet_name: p.sheet_name.clone(),
        row: 1,
        col: 8,
        expected_snapshot: ex.snapshot.clone(),
        decision,
        reviewer: "Controller".into(),
        note: "Why was the baseline changed?".into(),
        evidence: vec![],
    };
    assert!(wb.record_review(request(ReviewDecision::Reviewed)).is_err());
    assert!(wb.record_review(request(ReviewDecision::Question)).is_ok());
}
#[test]
fn decimal_aggregate_overflow_is_rejected_before_mutation() {
    let mut wb = Workbook::new();
    let max = Decimal::MAX.to_string();
    let p = wb
        .import_trial_balance(ImportTrialBalanceRequest {
            period: "2026-10".into(),
            csv: format!("account,name,debit,credit\n100,Cash,{max},0\n400,Sales,0,{max}\n"),
            mappings: [
                ("100".into(), "Balance sheet".into()),
                ("400".into(), "Revenue".into()),
            ]
            .into_iter()
            .collect(),
        })
        .unwrap();
    assert!(wb.post_close_adjustment(journal(p.id)).is_err());
    assert!(wb.close_periods[0].adjustments.is_empty());
}
#[test]
fn cumulative_overflow_does_not_post_or_create_a_false_reviewable_result() {
    let mut wb = Workbook::new();
    let near = (Decimal::MAX - Decimal::from(10)).to_string();
    let p = wb
        .import_trial_balance(ImportTrialBalanceRequest {
            period: "2026-10".into(),
            csv: format!("account,name,debit,credit\n100,Cash,{near},0\n400,Sales,0,{near}\n"),
            mappings: [
                ("100".into(), "Balance sheet".into()),
                ("400".into(), "Revenue".into()),
            ]
            .into_iter()
            .collect(),
        })
        .unwrap();
    let before = wb
        .explain_cell(&p.sheet_name, CellCoord::new(1, 8))
        .unwrap();
    assert!(wb.post_close_adjustment(journal(p.id)).is_err());
    let after = wb
        .explain_cell(&p.sheet_name, CellCoord::new(1, 8))
        .unwrap();
    assert_eq!(before.snapshot, after.snapshot);
    assert_eq!(before.value, after.value);
    assert!(wb.close_periods[0].adjustments.is_empty());
    // Bypassing the supported journal API through a grid edit must not enable signoff.
    wb.set_cell(&p.sheet_name, CellCoord::new(1, 11), "25".into())
        .unwrap();
    let ex = wb
        .explain_cell(&p.sheet_name, CellCoord::new(1, 8))
        .unwrap();
    assert!(!ex.can_review);
    assert!(wb
        .record_review(ReviewRequest {
            sheet_name: p.sheet_name.clone(),
            row: 1,
            col: 8,
            expected_snapshot: ex.snapshot,
            decision: ReviewDecision::Reviewed,
            reviewer: "Controller".into(),
            note: "Must not pass".into(),
            evidence: vec![]
        })
        .is_err());
}
#[test]
fn posted_journals_cannot_be_reinterpreted_as_a_different_currency() {
    let mut wb = Workbook::new();
    let p = import(&mut wb);
    wb.configure_close(config(p.id)).unwrap();
    wb.post_close_adjustment(journal(p.id)).unwrap();
    let mut request = config(p.id);
    request.currency = "USD".into();
    assert!(wb.configure_close(request).is_err());
    assert_eq!(wb.close_periods[0].currency, "BRL");
}
