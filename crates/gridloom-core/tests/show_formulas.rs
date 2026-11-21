use gridloom_core::review::{ReviewDecision, ReviewRequest};
use gridloom_core::{CellCoord, Value, Workbook};

#[test]
fn show_formulas_are_independent_per_sheet_and_legacy_defaults_roundtrip() {
    let mut wb = Workbook::new();
    wb.add_sheet("Other").unwrap();
    wb.set_show_formulas("Sheet1", true).unwrap();
    wb.set_show_formulas("Other", false).unwrap();
    assert!(wb.sheets["Sheet1"].show_formulas);
    assert!(!wb.sheets["Other"].show_formulas);
    let before = serde_json::to_value(&wb.sheets).unwrap();
    assert!(wb.set_show_formulas("missing", true).is_err());
    assert_eq!(serde_json::to_value(&wb.sheets).unwrap(), before);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("freeze.gridloom");
    wb.save_to_file(&file).unwrap();
    let loaded = Workbook::open_from_file(&file).unwrap();
    assert_eq!(serde_json::to_value(&loaded.sheets).unwrap(), before);
    wb.save_recovery_copy(&file).unwrap();
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    for sheet in json["sheets"].as_object_mut().unwrap().values_mut() {
        sheet.as_object_mut().unwrap().remove("show_formulas");
    }
    std::fs::write(&file, serde_json::to_vec(&json).unwrap()).unwrap();
    let legacy = Workbook::open_from_file(&file).unwrap();
    assert!(legacy.sheets.values().all(|s| !s.show_formulas));
    wb.set_show_formulas("Sheet1", false).unwrap();
    assert!(!wb.sheets["Sheet1"].show_formulas);
}

#[test]
fn showing_formulas_keeps_calculations_reviews_staging_and_edit_history_unchanged() {
    let mut wb = Workbook::new();
    let coord = CellCoord::new(0, 1);
    wb.set_cell("Sheet1", CellCoord::new(0, 0), "12".into())
        .unwrap();
    wb.set_cell("Sheet1", coord, "=A1*2".into()).unwrap();
    let before = wb.explain_cell("Sheet1", coord).unwrap();
    wb.record_review(ReviewRequest {
        sheet_name: "Sheet1".into(),
        row: 0,
        col: 1,
        expected_snapshot: before.snapshot.clone(),
        decision: ReviewDecision::Reviewed,
        reviewer: "Local reviewer".into(),
        note: "Verified".into(),
        evidence: vec![],
    })
    .unwrap();
    wb.clear_edit_history();
    let cells = serde_json::to_value(&wb.sheets["Sheet1"].cells).unwrap();
    let reviews = serde_json::to_value(&wb.reviews).unwrap();
    let generation = wb.dep_graph.generation;
    wb.set_show_formulas("Sheet1", true).unwrap();
    assert_eq!(
        wb.explain_cell("Sheet1", coord).unwrap().snapshot,
        before.snapshot
    );
    assert_eq!(wb.dep_graph.generation, generation);
    assert_eq!(
        serde_json::to_value(&wb.sheets["Sheet1"].cells).unwrap(),
        cells
    );
    assert_eq!(serde_json::to_value(&wb.reviews).unwrap(), reviews);
    assert!(wb.undo_sheet_edit().is_err());
    assert!(wb.redo_sheet_edit().is_err());
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("review.gridloom");
    wb.save_to_file(&file).unwrap();
    let loaded = Workbook::open_from_file(&file).unwrap();
    assert_eq!(
        loaded.explain_cell("Sheet1", coord).unwrap().snapshot,
        before.snapshot
    );
    assert!(!loaded.reviews[0].invalidated);
    wb.staged.stage_edit(
        "DB".into(),
        "table".into(),
        vec!["id".into()],
        vec![Value::Integer(1)],
        "amount".into(),
        Value::Integer(1),
        Value::Integer(2),
    );
    let staged = serde_json::to_value(&wb.staged).unwrap();
    let before = wb.explain_cell("Sheet1", coord).unwrap();
    wb.set_show_formulas("Sheet1", false).unwrap();
    assert_eq!(
        wb.explain_cell("Sheet1", coord).unwrap().snapshot,
        before.snapshot
    );
    assert_eq!(serde_json::to_value(&wb.staged).unwrap(), staged);
}

#[test]
fn showing_formulas_does_not_replace_or_clear_cell_undo_redo() {
    let mut wb = Workbook::new();
    wb.edit_range("Sheet1", CellCoord::new(0, 0), vec![vec!["7".into()]])
        .unwrap();
    wb.set_show_formulas("Sheet1", true).unwrap();
    wb.undo_sheet_edit().unwrap();
    assert!(wb.sheets["Sheet1"].show_formulas);
    assert!(wb.sheets["Sheet1"]
        .get_cell(&CellCoord::new(0, 0))
        .is_none());
    wb.set_show_formulas("Sheet1", false).unwrap();
    wb.redo_sheet_edit().unwrap();
    assert_eq!(
        wb.sheets["Sheet1"]
            .get_cell(&CellCoord::new(0, 0))
            .unwrap()
            .raw_input,
        "7"
    );
    assert!(!wb.sheets["Sheet1"].show_formulas);
}
