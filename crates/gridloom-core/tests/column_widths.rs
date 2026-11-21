use gridloom_core::review::{ReviewDecision, ReviewRequest};
use gridloom_core::{CellCoord, Value, Workbook};

#[test]
fn column_widths_are_sparse_bounded_and_per_sheet() {
    let mut wb = Workbook::new();
    wb.add_sheet("Other").unwrap();
    wb.set_column_width("Sheet1", 0, Some(240)).unwrap();
    wb.set_column_width("Sheet1", 99, Some(48)).unwrap();
    wb.set_column_width("Other", 0, Some(600)).unwrap();
    let before = serde_json::to_value(&wb.sheets).unwrap();
    for (name, column, width) in [
        ("missing", 0, Some(200)),
        ("Sheet1", 100, Some(200)),
        ("Sheet1", 0, Some(47)),
        ("Sheet1", 0, Some(601)),
        ("Sheet1", 100, None),
    ] {
        assert!(wb.set_column_width(name, column, width).is_err());
        assert_eq!(serde_json::to_value(&wb.sheets).unwrap(), before);
    }
    wb.set_column_width("Sheet1", 0, Some(110)).unwrap();
    wb.set_column_width("Sheet1", 99, None).unwrap();
    assert!(wb.sheets["Sheet1"].column_widths.is_empty());
    assert_eq!(wb.sheets["Other"].column_widths[&0], 600);
}

#[test]
fn saved_widths_roundtrip_and_legacy_files_use_defaults() {
    let mut wb = Workbook::new();
    wb.set_column_width("Sheet1", 4, Some(321)).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("widths.gridloom");
    wb.save_to_file(&file).unwrap();
    assert_eq!(
        Workbook::open_from_file(&file).unwrap().sheets["Sheet1"].column_widths[&4],
        321
    );
    wb.save_recovery_copy(&file).unwrap();
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    json["sheets"]["Sheet1"]
        .as_object_mut()
        .unwrap()
        .remove("column_widths");
    std::fs::write(&file, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(Workbook::open_from_file(&file).unwrap().sheets["Sheet1"]
        .column_widths
        .is_empty());
}

#[test]
fn presentation_changes_leave_calculations_reviews_staging_and_history_alone() {
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
    let staged = serde_json::to_value(&wb.staged).unwrap();
    let reviews = serde_json::to_value(&wb.reviews).unwrap();
    let generation = wb.dep_graph.generation;
    wb.set_column_width("Sheet1", 0, Some(260)).unwrap();
    assert_eq!(
        wb.explain_cell("Sheet1", coord).unwrap().snapshot,
        before.snapshot
    );
    assert_eq!(wb.dep_graph.generation, generation);
    assert_eq!(
        serde_json::to_value(&wb.sheets["Sheet1"].cells).unwrap(),
        cells
    );
    assert_eq!(serde_json::to_value(&wb.staged).unwrap(), staged);
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
    assert_eq!(serde_json::to_value(&loaded.staged).unwrap(), staged);
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
    let current = wb.explain_cell("Sheet1", coord).unwrap();
    let reviews = serde_json::to_value(&wb.reviews).unwrap();
    wb.set_column_width("Sheet1", 1, Some(340)).unwrap();
    assert_eq!(
        wb.explain_cell("Sheet1", coord).unwrap().snapshot,
        current.snapshot
    );
    assert_eq!(serde_json::to_value(&wb.staged).unwrap(), staged);
    assert_eq!(serde_json::to_value(&wb.reviews).unwrap(), reviews);
}
