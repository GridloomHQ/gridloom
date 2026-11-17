use gridloom_core::close::{ImportTrialBalanceRequest, MapAccountsRequest, RollforwardRequest};
use gridloom_core::{CellCoord, Value, Workbook};
use std::collections::BTreeMap;
fn csv() -> String {
    "account,name,debit,credit\n1000,Cash,150,0\n4000,Sales,0,200\n5000,Cost,50,0\n".into()
}
fn mappings() -> BTreeMap<String, String> {
    [
        ("1000", "Balance sheet"),
        ("4000", "Revenue"),
        ("5000", "Cost of sales"),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect()
}
#[test]
fn import_map_reconcile_rollforward_and_pack() {
    let mut wb = Workbook::new();
    let p = wb
        .import_trial_balance(ImportTrialBalanceRequest {
            period: "2026-10".into(),
            csv: csv(),
            mappings: BTreeMap::new(),
        })
        .unwrap();
    assert_eq!(wb.close_status(p.id).unwrap().unmapped_accounts.len(), 3);
    wb.map_close_accounts(MapAccountsRequest {
        period_id: p.id,
        mappings: mappings(),
    })
    .unwrap();
    let status = wb.close_status(p.id).unwrap();
    assert!(status.balanced);
    assert!(!status.ready_for_close);
    assert_eq!(status.queue[5].value.to_string(), "150");
    let pack = wb.export_review_pack(p.id).unwrap();
    assert_eq!(pack["status"]["period"]["source_csv"], csv());
    let next = wb
        .rollforward_close(RollforwardRequest {
            source_period_id: p.id,
            period: "2026-11".into(),
            csv: csv().replace("150,0", "160,0"),
        })
        .unwrap();
    assert_eq!(next.prior_period_id, Some(p.id));
    assert_ne!(p.sheet_name, next.sheet_name);
    assert!(!wb.close_status(next.id).unwrap().balanced);
    assert!(wb.close_status(p.id).unwrap().balanced);
    assert_eq!(next.accounts[1].mapping, "Revenue");
    let copy = wb
        .import_trial_balance(ImportTrialBalanceRequest {
            period: "2026-10".into(),
            csv: csv(),
            mappings: mappings(),
        })
        .unwrap();
    assert_eq!(copy.version, 2);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("close.gridloom");
    wb.save_to_file(&path).unwrap();
    let reopened = Workbook::open_from_file(&path).unwrap();
    assert_eq!(reopened.close_periods.len(), 3);
    assert_eq!(
        reopened.export_review_pack(p.id).unwrap()["status"]["period"]["source_sha256"],
        pack["status"]["period"]["source_sha256"]
    );
}
#[test]
fn csv_quoting_and_formula_injection_are_safe() {
    let mut wb = Workbook::new();
    let csv="account,name,debit,credit\r\n001,\"=SUM(1,2)\",0,12.34\r\n002,\"Name \"\"quoted\"\"\nnew line\",12.34,0\r\n";
    let p = wb
        .import_trial_balance(ImportTrialBalanceRequest {
            period: "2026-01".into(),
            csv: csv.into(),
            mappings: BTreeMap::new(),
        })
        .unwrap();
    let sheet = &wb.sheets[&p.sheet_name];
    assert!(sheet
        .get_cell(&CellCoord::new(1, 1))
        .unwrap()
        .formula
        .is_none());
    assert!(
        matches!(&sheet.get_cell(&CellCoord::new(1,0)).unwrap().evaluated_value, Value::Text(t) if t=="001")
    );
    assert!(wb.close_status(p.id).unwrap().balanced);
}
#[test]
fn invalid_imports_are_atomic() {
    for csv in [
        "account,name,debit,credit\na,A,0,1\na,B,1,0",
        "account,name,debit,credit\na,A,-1,0",
        "account,name,debit,credit\na,\"unterminated,1,0",
        "account,name,debit,credit\na,A,NaN,0",
        "account,name,debit,credit\na,A,1e3,0",
    ] {
        let mut wb = Workbook::new();
        assert!(wb
            .import_trial_balance(ImportTrialBalanceRequest {
                period: "2026-01".into(),
                csv: csv.into(),
                mappings: BTreeMap::new()
            })
            .is_err());
        assert_eq!(wb.sheets.len(), 1);
        assert!(wb.close_periods.is_empty());
    }
    let mut wb = Workbook::new();
    for period in ["2026-13", "2026-00", "2026-1", "abc💥"] {
        assert!(wb
            .import_trial_balance(ImportTrialBalanceRequest {
                period: period.into(),
                csv: csv(),
                mappings: BTreeMap::new()
            })
            .is_err());
    }
}
#[test]
fn manual_formula_changes_block_close() {
    let mut wb = Workbook::new();
    let p = wb
        .import_trial_balance(ImportTrialBalanceRequest {
            period: "2026-01".into(),
            csv: csv(),
            mappings: mappings(),
        })
        .unwrap();
    wb.set_cell(&p.sheet_name, CellCoord::new(6, 8), "123".into())
        .unwrap();
    let status = wb.close_status(p.id).unwrap();
    assert!(!status.ready_for_close);
    assert!(status
        .warnings
        .iter()
        .any(|w| w.contains("P&L formula changed")));
    let original = wb.sheets[&p.sheet_name].clone();
    assert!(wb
        .map_close_accounts(MapAccountsRequest {
            period_id: p.id,
            mappings: [("5000".into(), "Bogus".into())].into_iter().collect()
        })
        .is_err());
    assert_eq!(
        serde_json::to_string(&original).unwrap().len(),
        serde_json::to_string(&wb.sheets[&p.sheet_name])
            .unwrap()
            .len()
    );
}

#[test]
fn close_queue_requires_current_reviews_even_when_total_unchanged() {
    use gridloom_core::review::{ReviewDecision, ReviewRequest};
    let mut wb = Workbook::new();
    let p = wb
        .import_trial_balance(ImportTrialBalanceRequest {
            period: "2026-01".into(),
            csv: csv(),
            mappings: mappings(),
        })
        .unwrap();
    for row in 1..=6 {
        let explanation = wb
            .explain_cell(&p.sheet_name, CellCoord::new(row, 8))
            .unwrap();
        wb.record_review(ReviewRequest {
            sheet_name: p.sheet_name.clone(),
            row,
            col: 8,
            expected_snapshot: explanation.snapshot,
            decision: ReviewDecision::Reviewed,
            reviewer: "Local preparer".into(),
            note: "Checked source balances".into(),
            evidence: vec![],
        })
        .unwrap();
    }
    assert!(wb.close_status(p.id).unwrap().ready_for_close);
    let profit = wb.close_status(p.id).unwrap().queue[5].value.to_string();
    wb.map_close_accounts(MapAccountsRequest {
        period_id: p.id,
        mappings: [("4000".into(), "Other income".into())]
            .into_iter()
            .collect(),
    })
    .unwrap();
    let status = wb.close_status(p.id).unwrap();
    assert_eq!(status.queue[5].value.to_string(), profit);
    assert!(!status.ready_for_close);
    assert_eq!(status.queue[5].status, "Needs re-review");
    assert_eq!(wb.reviews.len(), 6);
    let pack = wb.export_review_pack(p.id).unwrap();
    assert_eq!(pack["reviews"].as_array().unwrap().len(), 6);
    assert!(pack["reviews"][0]["input_snapshot"].is_object());
}

#[test]
fn thousand_account_trial_balance_is_reviewable() {
    let mut csv = "account,name,debit,credit\n".to_string();
    let mut mappings = BTreeMap::new();
    for i in 0..1000 {
        let (debit, credit, mapping) = if i % 2 == 0 {
            (1, 0, "Operating expenses")
        } else {
            (0, 1, "Revenue")
        };
        csv.push_str(&format!("{i},Account {i},{debit},{credit}\n"));
        mappings.insert(i.to_string(), mapping.to_string());
    }
    let mut wb = Workbook::new();
    let p = wb
        .import_trial_balance(ImportTrialBalanceRequest {
            period: "2026-01".into(),
            csv,
            mappings,
        })
        .unwrap();
    let explanation = wb
        .explain_cell(&p.sheet_name, CellCoord::new(6, 8))
        .unwrap();
    assert!(explanation.can_review, "{:?}", explanation.warnings);
    assert_eq!(explanation.value.to_string(), "0");
    assert!(explanation.nodes.len() > 4000);
}

#[test]
fn live_account_status_tracks_grid_edits_without_rewriting_import_baseline() {
    let mut wb = Workbook::new();
    let p = wb
        .import_trial_balance(ImportTrialBalanceRequest {
            period: "2026-10".into(),
            csv: csv(),
            mappings: mappings(),
        })
        .unwrap();
    wb.set_cell(&p.sheet_name, CellCoord::new(2, 2), "Other income".into())
        .unwrap();
    wb.set_cell(&p.sheet_name, CellCoord::new(2, 4), "220.50".into())
        .unwrap();
    wb.set_cell(
        &p.sheet_name,
        CellCoord::new(2, 1),
        "Updated sales label".into(),
    )
    .unwrap();
    let status = wb.close_status(p.id).unwrap();
    assert_eq!(status.live_accounts[1].mapping, "Other income");
    assert_eq!(status.live_accounts[1].name, "Updated sales label");
    assert_eq!(
        status.live_accounts[1].credit.unwrap().to_string(),
        "220.50"
    );
    assert_eq!(status.live_accounts[1].row, 2);
    assert_eq!(status.live_accounts[1].imported_account, "4000");
    assert!(status.live_accounts[1].identity_matches_import);
    assert_eq!(status.period.accounts[1].mapping, "Revenue");
    assert_eq!(status.period.accounts[1].credit.to_string(), "200");
    assert_eq!(status.period.source_csv, csv());
    assert_eq!(status.credits.to_string(), "220.50");
    wb.map_close_accounts(MapAccountsRequest {
        period_id: p.id,
        mappings: [("4000".into(), "Revenue".into())].into_iter().collect(),
    })
    .unwrap();
    let status = wb.close_status(p.id).unwrap();
    assert_eq!(status.live_accounts[1].mapping, "Revenue");
    assert_eq!(
        status.live_accounts[1].credit.unwrap().to_string(),
        "220.50"
    );
    assert_eq!(wb.close_periods[0].source_csv, csv());
    let pack = wb.export_review_pack(p.id).unwrap();
    assert_eq!(pack["status"]["live_accounts"][1]["credit"], "220.50");
    assert_eq!(pack["status"]["period"]["accounts"][1]["credit"], "200");
}

#[test]
fn invalid_live_amounts_never_display_retained_old_values() {
    let mut wb = Workbook::new();
    let p = wb
        .import_trial_balance(ImportTrialBalanceRequest {
            period: "2026-10".into(),
            csv: csv(),
            mappings: mappings(),
        })
        .unwrap();
    wb.set_cell(&p.sheet_name, CellCoord::new(1, 3), "invalid".into())
        .unwrap();
    let status = wb.close_status(p.id).unwrap();
    assert!(status.live_accounts[0].debit.is_none());
    assert!(!status.balanced);
    assert!(!status.ready_for_close);
    assert!(status
        .warnings
        .iter()
        .any(|w| w.contains("Invalid debit/credit at D2")));
    assert_eq!(status.period.accounts[0].debit.to_string(), "150");
    wb.set_cell(
        &p.sheet_name,
        CellCoord::new(1, 0),
        "renamed-account".into(),
    )
    .unwrap();
    let status = wb.close_status(p.id).unwrap();
    assert_eq!(status.live_accounts[0].account, "renamed-account");
    assert_eq!(status.live_accounts[0].imported_account, "1000");
    assert!(!status.live_accounts[0].identity_matches_import);
    assert!(wb
        .map_close_accounts(MapAccountsRequest {
            period_id: p.id,
            mappings: mappings()
        })
        .is_err());
}

#[test]
fn mapping_api_preserves_import_mapping_and_export_derives_review_validity() {
    use gridloom_core::review::{ReviewDecision, ReviewRequest};
    let mut wb = Workbook::new();
    let p = wb
        .import_trial_balance(ImportTrialBalanceRequest {
            period: "2026-10".into(),
            csv: csv(),
            mappings: BTreeMap::new(),
        })
        .unwrap();
    wb.map_close_accounts(MapAccountsRequest {
        period_id: p.id,
        mappings: mappings(),
    })
    .unwrap();
    assert_eq!(wb.close_periods[0].accounts[1].mapping, "");
    assert_eq!(
        wb.close_status(p.id).unwrap().live_accounts[1].mapping,
        "Revenue"
    );
    let coord = CellCoord::new(6, 8);
    let snapshot = wb.explain_cell(&p.sheet_name, coord).unwrap().snapshot;
    wb.record_review(ReviewRequest {
        sheet_name: p.sheet_name.clone(),
        row: coord.row,
        col: coord.col,
        expected_snapshot: snapshot,
        decision: ReviewDecision::Reviewed,
        reviewer: "Controller".into(),
        note: "Checked".into(),
        evidence: vec![],
    })
    .unwrap();
    wb.set_cell(&p.sheet_name, CellCoord::new(2, 2), "Other income".into())
        .unwrap();
    // Simulate a persisted record whose cached invalidation flag was not updated by an older writer.
    wb.reviews[0].invalidated = false;
    wb.reviews[0].invalidation_reason = None;
    let pack = wb.export_review_pack(p.id).unwrap();
    assert_eq!(pack["reviews"][0]["invalidated"], true);
    assert_eq!(pack["status"]["queue"][5]["status"], "Needs re-review");
}

#[test]
fn source_overlay_uses_displayed_values_and_blocks_close() {
    let mut wb = Workbook::new();
    let p = wb
        .import_trial_balance(ImportTrialBalanceRequest {
            period: "2026-10".into(),
            csv: csv(),
            mappings: mappings(),
        })
        .unwrap();
    let dbfile = tempfile::NamedTempFile::new().unwrap();
    let db = rusqlite::Connection::open(dbfile.path()).unwrap();
    db.execute_batch(
        "CREATE TABLE amounts(amount INTEGER); INSERT INTO amounts VALUES(200),(0),(50)",
    )
    .unwrap();
    wb.add_sqlite_connection("db", dbfile.path()).unwrap();
    wb.create_named_source("overlay", "db", "SELECT amount FROM amounts")
        .unwrap();
    // Model a loaded workbook/source refresh whose table footprint overlays existing cells.
    wb.sheets.get_mut(&p.sheet_name).unwrap().tables.insert(
        "Overlay".into(),
        gridloom_core::table::WorksheetTable::new(
            "Overlay".into(),
            "overlay".into(),
            CellCoord::new(0, 3),
            vec!["amount".into()],
            3,
        ),
    );
    wb.recalculate(&p.sheet_name).unwrap();
    let status = wb.close_status(p.id).unwrap();
    assert_eq!(status.live_accounts[0].debit.unwrap().to_string(), "200");
    assert_eq!(status.debits.to_string(), "250");
    assert_eq!(status.period.accounts[0].debit.to_string(), "150");
    assert!(!status.ready_for_close);
    assert!(status
        .warnings
        .iter()
        .any(|w| w.contains("source table overlaps")));
}
