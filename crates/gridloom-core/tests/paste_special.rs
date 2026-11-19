use gridloom_core::{spreadsheet::PasteMode, CellCoord, CellRange, Value, Workbook};

const SHEET: &str = "Sheet1";
fn point(s: &str) -> CellCoord {
    CellCoord::from_a1(s).unwrap()
}
fn range(s: &str) -> CellRange {
    CellRange::from_a1(s).unwrap()
}

#[test]
fn values_only_preserves_exact_and_literal_types_destination_format_and_no_formula() {
    let mut wb = Workbook::new();
    wb.edit_range(
        SHEET,
        point("A1"),
        vec![vec![
            "=9007199254740993.123456".into(),
            "=\"=1+1\"".into(),
            "=\"001\"".into(),
        ]],
    )
    .unwrap();
    wb.format_range_attribute(SHEET, range("A1:A1"), "number_format", "percent")
        .unwrap();
    wb.format_range_attribute(SHEET, range("A2:A2"), "number_format", "currency")
        .unwrap();
    let snapshot = wb.capture_range(SHEET, range("A1:C1")).unwrap().snapshot;
    wb.paste_special(
        SHEET,
        range("A1:C1"),
        SHEET,
        point("A2"),
        &snapshot,
        PasteMode::Values,
    )
    .unwrap();
    let sheet = &wb.sheets[SHEET];
    let value = sheet.get_cell(&point("A2")).unwrap();
    assert_eq!(
        value.evaluated_value,
        Value::Decimal("9007199254740993.123456".parse().unwrap())
    );
    assert_eq!(value.formula, None);
    assert_eq!(value.number_format.as_deref(), Some("currency"));
    let b = sheet.get_cell(&point("B2")).unwrap();
    assert_eq!(b.formula, None);
    assert_eq!(b.evaluated_value, Value::Text("=1+1".into()));
    assert_eq!(
        sheet.get_cell(&point("C2")).unwrap().evaluated_value,
        Value::Text("001".into())
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("paste.gridloom");
    wb.save_to_file(&path).unwrap();
    let wb = Workbook::open_from_file(&path).unwrap();
    let b = wb.sheets[SHEET].get_cell(&point("B2")).unwrap();
    assert_eq!(b.formula, None);
    assert_eq!(b.evaluated_value, Value::Text("=1+1".into()));
}

#[test]
fn formulas_only_shifts_references_skips_constants_and_is_undoable() {
    let mut wb = Workbook::new();
    wb.edit_range(
        SHEET,
        point("A1"),
        vec![vec!["1".into(), "=A1+$A$1".into(), "".into()]],
    )
    .unwrap();
    wb.edit_range(
        SHEET,
        point("A2"),
        vec![vec!["8".into(), "9".into(), "10".into()]],
    )
    .unwrap();
    let snapshot = wb.capture_range(SHEET, range("A1:C1")).unwrap().snapshot;
    wb.paste_special(
        SHEET,
        range("A1:C1"),
        SHEET,
        point("A2"),
        &snapshot,
        PasteMode::Formulas,
    )
    .unwrap();
    let sheet = &wb.sheets[SHEET];
    assert_eq!(
        sheet.get_cell(&point("B2")).unwrap().formula.as_deref(),
        Some("=A2+$A$1")
    );
    assert_eq!(sheet.get_cell(&point("A2")).unwrap().raw_input, "8");
    assert_eq!(sheet.get_cell(&point("C2")).unwrap().raw_input, "10");
    wb.undo_sheet_edit().unwrap();
    assert_eq!(
        wb.sheets[SHEET].get_cell(&point("B2")).unwrap().raw_input,
        "9"
    );
    wb.set_cell(SHEET, point("A1"), "17".into()).unwrap();
    assert!(wb
        .paste_special(
            SHEET,
            range("A1:C1"),
            SHEET,
            point("A2"),
            &snapshot,
            PasteMode::Values
        )
        .is_err());
    assert_eq!(
        wb.sheets[SHEET].get_cell(&point("B2")).unwrap().raw_input,
        "9"
    );
}

#[test]
fn values_only_materializes_all_spill_children_and_guards_destination_spills() {
    let mut wb = Workbook::new();
    wb.set_cell(SHEET, point("A1"), "=SEQUENCE(2,2)".into())
        .unwrap();
    let r = range("A1:B2");
    let capture = wb.capture_range(SHEET, r).unwrap();
    wb.paste_special(
        SHEET,
        r,
        SHEET,
        point("D1"),
        &capture.snapshot,
        PasteMode::Values,
    )
    .unwrap();
    let sheet = &wb.sheets[SHEET];
    assert!(sheet.get_cell(&point("D1")).unwrap().formula.is_none());
    assert_eq!(
        sheet
            .get_cell(&point("E2"))
            .unwrap()
            .evaluated_value
            .to_string_repr(),
        "4"
    );
    assert!(wb
        .paste_special(
            SHEET,
            r,
            SHEET,
            point("B2"),
            &capture.snapshot,
            PasteMode::Values
        )
        .is_err());
    assert!(wb.sheets[SHEET].get_cell(&point("C2")).is_none());
}
