mod common;

use gridloom_core::review::{ReviewDecision, ReviewRequest};
use gridloom_core::{CellCoord, Value, Workbook};

#[test]
fn freeze_panes_are_independent_per_sheet_and_missing_fields_default_on_open() {
    let mut wb = Workbook::new();
    wb.add_sheet("Other").unwrap();
    wb.set_freeze_panes("Sheet1", true, false).unwrap();
    wb.set_freeze_panes("Other", false, true).unwrap();
    assert!(wb.sheets["Sheet1"].freeze_top_row);
    assert!(!wb.sheets["Sheet1"].freeze_first_column);
    assert!(!wb.sheets["Other"].freeze_top_row);
    assert!(wb.sheets["Other"].freeze_first_column);
    let before = serde_json::to_value(&wb.sheets).unwrap();
    assert!(wb.set_freeze_panes("missing", true, true).is_err());
    assert_eq!(serde_json::to_value(&wb.sheets).unwrap(), before);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("freeze.tay");
    wb.save_to_file(&file).unwrap();
    let loaded = Workbook::open_from_file(&file).unwrap();
    assert_eq!(serde_json::to_value(&loaded.sheets).unwrap(), before);
    common::edit_workbook_metadata(&file, |json| {
        for sheet in json["sheets"].as_object_mut().unwrap().values_mut() {
            sheet.as_object_mut().unwrap().remove("freeze_top_row");
            sheet.as_object_mut().unwrap().remove("freeze_first_column");
        }
    });
    let legacy = Workbook::open_from_file(&file).unwrap();
    assert!(legacy
        .sheets
        .values()
        .all(|s| !s.freeze_top_row && !s.freeze_first_column));
    wb.set_freeze_panes("Sheet1", false, false).unwrap();
    assert!(!wb.sheets["Sheet1"].freeze_top_row);
}

#[test]
fn freezing_keeps_calculations_reviews_staging_and_edit_history_unchanged() {
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
    wb.set_freeze_panes("Sheet1", true, true).unwrap();
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
    let file = dir.path().join("review.tay");
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
    wb.set_freeze_panes("Sheet1", false, false).unwrap();
    assert_eq!(
        wb.explain_cell("Sheet1", coord).unwrap().snapshot,
        before.snapshot
    );
    assert_eq!(serde_json::to_value(&wb.staged).unwrap(), staged);
}

#[test]
fn freezing_does_not_replace_or_clear_cell_undo_redo() {
    let mut wb = Workbook::new();
    wb.edit_range("Sheet1", CellCoord::new(0, 0), vec![vec!["7".into()]])
        .unwrap();
    wb.set_freeze_panes("Sheet1", true, false).unwrap();
    wb.undo_sheet_edit().unwrap();
    assert!(wb.sheets["Sheet1"].freeze_top_row);
    assert!(wb.sheets["Sheet1"]
        .get_cell(&CellCoord::new(0, 0))
        .is_none());
    wb.set_freeze_panes("Sheet1", false, true).unwrap();
    wb.redo_sheet_edit().unwrap();
    assert_eq!(
        wb.sheets["Sheet1"]
            .get_cell(&CellCoord::new(0, 0))
            .unwrap()
            .raw_input,
        "7"
    );
    assert!(wb.sheets["Sheet1"].freeze_first_column);
}

#[test]
fn arbitrary_freeze_roundtrips_and_rejects_invalid_axes_atomically() {
    let mut wb = Workbook::new();
    wb.add_sheet("Other").unwrap();
    wb.set_frozen_counts("Sheet1", 8, 4).unwrap();
    assert!(wb.set_frozen_counts("Sheet1", 0, 100).is_err());
    assert_eq!(wb.sheets["Sheet1"].frozen_row_count(), 8);
    assert_eq!(wb.sheets["Sheet1"].frozen_column_count(), 4);
    assert_eq!(wb.sheets["Other"].frozen_row_count(), 0);
    wb.set_frozen_counts("Sheet1", 8, 0).unwrap();
    assert_eq!(wb.sheets["Sheet1"].frozen_column_count(), 0);

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("freeze.tay");
    wb.save_to_file(&file).unwrap();
    let loaded = Workbook::open_from_file(&file).unwrap();
    assert_eq!(loaded.sheets["Sheet1"].frozen_row_count(), 8);
    assert_eq!(loaded.sheets["Sheet1"].frozen_column_count(), 0);

    // A legacy payload with only flags still yields the correct effective counts.
    let legacy = serde_json::json!({
        "freeze_top_row": true,
        "freeze_first_column": false
    });
    let mut sheet = serde_json::to_value(&loaded.sheets["Sheet1"]).unwrap();
    sheet.as_object_mut().unwrap().remove("frozen_rows");
    sheet
        .as_object_mut()
        .unwrap()
        .extend(legacy.as_object().unwrap().clone());
    let legacy: gridloom_core::workbook::Sheet = serde_json::from_value(sheet).unwrap();
    assert_eq!(legacy.frozen_row_count(), 1);
    assert_eq!(legacy.frozen_column_count(), 0);
}
