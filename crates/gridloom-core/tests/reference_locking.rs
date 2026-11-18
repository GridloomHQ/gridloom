use gridloom_core::{CellCoord, CellError, CellRange, Workbook};
fn coord(a: &str) -> CellCoord {
    CellCoord::from_a1(a).unwrap()
}
fn range(a: &str) -> CellRange {
    CellRange::from_a1(a).unwrap()
}
#[test]
fn copy_and_fill_preserve_each_reference_lock_axis() {
    let mut wb = Workbook::new();
    wb.set_cell("Sheet1", coord("E5"), "=A1+$A1+A$1+$A$1".into())
        .unwrap();
    wb.copy_range("Sheet1", range("E5"), "Sheet1", coord("F6"))
        .unwrap();
    assert_eq!(
        wb.sheets["Sheet1"].cells["F6"].raw_input,
        "=B2+$A2+B$1+$A$1"
    );
    wb.fill_range("Sheet1", range("E5"), range("E7:F7"))
        .unwrap();
    assert_eq!(
        wb.sheets["Sheet1"].cells["E7"].raw_input,
        "=A3+$A3+A$1+$A$1"
    );
    assert_eq!(
        wb.sheets["Sheet1"].cells["F7"].raw_input,
        "=B3+$A3+B$1+$A$1"
    );
}
#[test]
fn negative_relative_translation_rejects_atomically_but_locked_reference_is_safe() {
    let mut wb = Workbook::new();
    wb.set_cell("Sheet1", coord("E5"), "=A1".into()).unwrap();
    wb.set_cell("Sheet1", coord("D4"), "keep".into()).unwrap();
    let before = serde_json::to_value(&wb.sheets).unwrap();
    assert_eq!(
        wb.copy_range("Sheet1", range("E5"), "Sheet1", coord("D4")),
        Err(CellError::Ref)
    );
    assert_eq!(serde_json::to_value(&wb.sheets).unwrap(), before);
    assert_eq!(
        wb.fill_range("Sheet1", range("E5"), range("D4:E4")),
        Err(CellError::Ref)
    );
    assert_eq!(serde_json::to_value(&wb.sheets).unwrap(), before);
    wb.set_cell("Sheet1", coord("E5"), "=$A$1".into()).unwrap();
    wb.copy_range("Sheet1", range("E5"), "Sheet1", coord("D4"))
        .unwrap();
    assert_eq!(wb.sheets["Sheet1"].cells["D4"].raw_input, "=$A$1");
}
