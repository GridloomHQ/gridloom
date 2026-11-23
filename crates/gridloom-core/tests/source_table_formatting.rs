use gridloom_core::spreadsheet::CellStyle;
use gridloom_core::virtual_view::{CellRenderData, ViewportRequest};
use gridloom_core::{CellCoord, CellError, CellRange, Value, Workbook};

const SHEET: &str = "Sheet1";

fn coord(a1: &str) -> CellCoord {
    CellCoord::from_a1(a1).unwrap()
}

fn range(a1: &str) -> CellRange {
    CellRange::from_a1(a1).unwrap()
}

fn cell(w: &Workbook, a1: &str) -> CellRenderData {
    let at = coord(a1);
    w.get_viewport(&ViewportRequest {
        sheet_name: SHEET.into(),
        start_row: at.row,
        end_row: at.row,
        start_col: at.col,
        end_col: at.col,
        overscan: 0,
    })
    .unwrap()
    .cells
    .remove(a1)
    .unwrap()
}

fn fixture() -> (Workbook, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ledger.sqlite");
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute_batch(
            "CREATE TABLE ledger(id INTEGER PRIMARY KEY, amount INTEGER);
             INSERT INTO ledger VALUES (1, 7), (2, 18);",
        )
        .unwrap();
    let mut w = Workbook::new();
    w.add_sqlite_connection("db", db.to_str().unwrap()).unwrap();
    w.create_named_source("ledger", "db", "SELECT id, amount FROM ledger ORDER BY id")
        .unwrap();
    w.insert_table(SHEET, "Data", "ledger", coord("A1"))
        .unwrap();
    (w, dir)
}

#[test]
fn presentation_formats_sql_and_staged_values_without_mutating_either() {
    let (mut w, dir) = fixture();
    w.set_cell(SHEET, coord("D1"), "=SUM(Data[amount])".into())
        .unwrap();
    w.format_range_attribute(SHEET, range("B2:B3"), "number_format", "currency")
        .unwrap();
    assert!(w.staged.patches.is_empty());
    assert_eq!(cell(&w, "D1").raw_value.to_string_repr(), "25");
    assert_eq!(cell(&w, "B2").display_text, "$ 7.00");

    w.staged.stage_edit(
        "db".into(),
        "ledger".into(),
        vec!["id".into()],
        vec![Value::Integer(1)],
        "amount".into(),
        Value::Integer(7),
        Value::Integer(11),
    );
    w.format_range_attribute(SHEET, range("B2"), "fill", "#f1a100")
        .unwrap();
    let cell = cell(&w, "B2");
    assert_eq!(cell.display_text, "$ 11.00");
    assert_eq!(cell.raw_value, Value::Integer(11));
    assert!(cell.is_staged);
    assert_eq!(cell.style.fill.as_deref(), Some("#f1a100"));
    assert_eq!(w.staged.patches.len(), 1);
    assert_eq!(w.staged.audit_log.len(), 0);
    let amount: i64 = rusqlite::Connection::open(dir.path().join("ledger.sqlite"))
        .unwrap()
        .query_row("SELECT amount FROM ledger WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(amount, 7);
    assert_eq!(
        w.set_cell(SHEET, coord("B2"), "500".into()),
        Err(CellError::Ref)
    );
}

#[test]
fn full_column_format_survives_growth_and_workbook_reopen() {
    let (mut w, dir) = fixture();
    w.format_range_attribute(SHEET, range("B2:B3"), "number_format", "currency")
        .unwrap();
    w.format_range_attribute(SHEET, range("B3"), "number_format", "general")
        .unwrap();
    w.format_range_attribute(SHEET, range("B1"), "bold", "false")
        .unwrap();

    rusqlite::Connection::open(dir.path().join("ledger.sqlite"))
        .unwrap()
        .execute("INSERT INTO ledger VALUES (3, 200)", [])
        .unwrap();
    w.refresh_source("ledger").unwrap();
    let path = dir.path().join("demo.tay");
    w.save_to_file(&path).unwrap();
    let w = Workbook::open_from_file(&path).unwrap();
    assert_eq!(cell(&w, "B1").display_text, "amount");
    assert!(!cell(&w, "B1").style.bold);
    assert_eq!(cell(&w, "B2").display_text, "$ 7.00");
    assert_eq!(cell(&w, "B3").display_text, "18");
    assert_eq!(cell(&w, "B4").display_text, "$ 200.00");
    assert_eq!(cell(&w, "B4").raw_value, Value::Integer(200));
    assert!(w.sheets[SHEET].cells.is_empty());
}

#[test]
fn formats_mixed_selections_atomically_and_replays_undo_redo() {
    let (mut w, _dir) = fixture();
    w.set_cell(SHEET, coord("C2"), "42".into()).unwrap();
    w.format_range_attribute(SHEET, range("B2:C2"), "italic", "true")
        .unwrap();
    assert!(cell(&w, "B2").style.italic);
    assert!(cell(&w, "C2").style.italic);
    w.undo_sheet_edit().unwrap();
    assert!(!cell(&w, "B2").style.italic);
    assert!(!cell(&w, "C2").style.italic);
    w.redo_sheet_edit().unwrap();
    assert!(cell(&w, "B2").style.italic);
    assert!(cell(&w, "C2").style.italic);

    w.set_cell(SHEET, coord("D1"), "=SEQUENCE(2)".into())
        .unwrap();
    assert_eq!(
        w.format_range_attribute(SHEET, range("B2:D2"), "bold", "true"),
        Err(CellError::Spill)
    );
    assert!(!cell(&w, "B2").style.bold);
    assert!(!cell(&w, "C2").style.bold);
}

#[test]
fn formatting_a_ten_million_row_table_uses_one_rectangle() {
    let (mut w, _dir) = fixture();
    // Synthetic table dimensions; no source materialization is needed to test this operation.
    let sheet = w.sheets.get_mut(SHEET).unwrap();
    sheet.tables.get_mut("Data").unwrap().range.end_row = 10_000_000;
    sheet.max_rows = 10_000_001;
    w.format_range(
        SHEET,
        range("B2:B10000001"),
        CellStyle {
            number_format: Some("0.00".into()),
            ..CellStyle::default()
        },
    )
    .unwrap();
    let sheet = &w.sheets[SHEET];
    assert!(sheet.cells.is_empty());
    let table = serde_json::to_value(&sheet.tables["Data"]).unwrap();
    assert_eq!(table["formats"].as_array().unwrap().len(), 1);
    assert_eq!(cell(&w, "B2").display_text, "7.00");
    w.undo_sheet_edit().unwrap();
    assert_eq!(cell(&w, "B2").display_text, "7");
    w.redo_sheet_edit().unwrap();
    assert_eq!(cell(&w, "B2").display_text, "7.00");
}

#[test]
fn read_only_sql_values_can_be_formatted_and_copied_as_typed_values() {
    let (mut w, _dir) = fixture();
    let source = w
        .create_named_source("totals", "db", "SELECT SUM(amount) AS total FROM ledger")
        .unwrap();
    assert!(!source.lineage.unwrap().is_editable);
    w.insert_table(SHEET, "Totals", "totals", coord("F1"))
        .unwrap();
    w.format_range_attribute(SHEET, range("F2"), "number_format", "currency")
        .unwrap();
    assert_eq!(w.read_range(SHEET, range("F2")).unwrap(), vec![vec!["25"]]);
    w.copy_range(SHEET, range("F2"), SHEET, coord("G2"))
        .unwrap();
    assert_eq!(cell(&w, "G2").display_text, "$ 25.00");
    assert_eq!(cell(&w, "G2").raw_value, Value::Integer(25));
    assert!(w.staged.patches.is_empty());
}
