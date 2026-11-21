use gridloom_core::{CellCoord, Value, Workbook};
use rusqlite::Connection;
use std::fs;

const SQLITE: &[u8] = b"SQLite format 3\0";

#[test]
fn one_sqlite_file_persists_named_sheets_formulas_and_local_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("work.gridloom");
    let mut workbook = Workbook::new();
    assert!(workbook.connections.is_empty());
    assert!(workbook.sheets["Sheet1"].cells.is_empty());
    workbook.add_sheet("Working").unwrap();
    let initial_id = workbook.sheets["Working"].storage_id;
    workbook.rename_sheet("Working", "Totals").unwrap();
    workbook
        .set_cell("Sheet1", CellCoord::new(0, 0), "0.1".into())
        .unwrap();
    workbook
        .set_cell("Totals", CellCoord::new(0, 0), "=Sheet1!A1+0.2".into())
        .unwrap();
    workbook.set_column_width("Totals", 2, Some(132)).unwrap();
    workbook.select_sheet("Totals").unwrap();
    assert!(!path.exists());
    workbook.save_to_file(&path).unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(bytes.starts_with(SQLITE));
    let connection = Connection::open(&path).unwrap();
    let local_tables: i64 = connection
        .query_row("SELECT count(*) FROM worksheet", [], |r| r.get(0))
        .unwrap();
    assert_eq!(local_tables, 2);
    let raw_formula: String = connection.query_row(
        "SELECT json_extract(c.json, '$.raw_input') FROM worksheet_cell c JOIN worksheet s ON c.sheet_id = s.id WHERE s.name = 'Totals'",
        [],
        |r| r.get(0),
    ).unwrap();
    assert_eq!(raw_formula, "=Sheet1!A1+0.2");
    let reopened = Workbook::open_from_file(&path).unwrap();
    assert_eq!(reopened.active_sheet, "Totals");
    assert_eq!(reopened.sheets["Totals"].storage_id, initial_id);
    assert_eq!(reopened.sheets["Totals"].column_widths[&2], 132);
    assert_eq!(
        reopened.sheets["Totals"].cells["A1"]
            .evaluated_value
            .to_string_repr(),
        "0.3"
    );
    assert!(reopened.connection_configs.is_empty());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn old_workbook_opens_without_conversion_and_save_as_leaves_it_intact() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("previous.gridloom");
    let new = dir.path().join("new.gridloom");
    let mut source = Workbook::new();
    source
        .set_cell("Sheet1", CellCoord::new(0, 0), "Previous".into())
        .unwrap();
    source.save_recovery_copy(&old).unwrap();
    let mut before: serde_json::Value = serde_json::from_slice(&fs::read(&old).unwrap()).unwrap();
    before["sheets"]["Sheet1"]
        .as_object_mut()
        .unwrap()
        .remove("storage_id");
    let bytes = serde_json::to_vec(&before).unwrap();
    fs::write(&old, &bytes).unwrap();
    let mut reopened = Workbook::open_from_file(&old).unwrap();
    let id = reopened.sheets["Sheet1"].storage_id;
    assert_eq!(
        reopened.sheets["Sheet1"].cells["A1"].evaluated_value,
        Value::Text("Previous".into())
    );
    assert_eq!(fs::read(&old).unwrap(), bytes);
    reopened.save_to_file(&new).unwrap();
    assert_eq!(fs::read(&old).unwrap(), bytes);
    assert!(fs::read(&new).unwrap().starts_with(SQLITE));
    assert_eq!(
        Workbook::open_from_file(&new).unwrap().sheets["Sheet1"].storage_id,
        id
    );
}

#[test]
fn autosave_is_separate_and_save_as_is_independent() {
    let dir = tempfile::tempdir().unwrap();
    let saved = dir.path().join("saved.gridloom");
    let recovery = dir.path().join("recovery.gridloom");
    let copy = dir.path().join("copy.gridloom");
    let mut book = Workbook::new();
    book.save_to_file(&saved).unwrap();
    let untouched = fs::read(&saved).unwrap();
    book.set_cell("Sheet1", CellCoord::new(0, 0), "recoverable".into())
        .unwrap();
    book.save_recovery_copy(&recovery).unwrap();
    assert_eq!(book.file_path.as_deref(), Some(saved.as_path()));
    assert_eq!(fs::read(&saved).unwrap(), untouched);
    assert!(fs::read(&recovery).unwrap().starts_with(b"{"));
    let mut recovered = Workbook::open_from_file(&recovery).unwrap();
    recovered.save_to_file(&copy).unwrap();
    assert_eq!(fs::read(&saved).unwrap(), untouched);
    assert_eq!(
        Workbook::open_from_file(&copy).unwrap().sheets["Sheet1"].cells["A1"].raw_input,
        "recoverable"
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 3);
}

#[test]
fn failed_transaction_preserves_prior_save_and_foreign_db_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let saved = dir.path().join("saved.gridloom");
    let external = dir.path().join("external.sqlite");
    let mut book = Workbook::new();
    book.save_to_file(&saved).unwrap();
    let original = fs::read(&saved).unwrap();
    book.add_sheet("Duplicate").unwrap();
    book.sheets.get_mut("Duplicate").unwrap().storage_id = book.sheets["Sheet1"].storage_id;
    assert!(book
        .save_to_file(&saved)
        .unwrap_err()
        .message()
        .contains("UNIQUE"));
    assert_eq!(fs::read(&saved).unwrap(), original);
    assert_eq!(
        fs::read_dir(dir.path()).unwrap().count(),
        1,
        "failed write must not leak a journal or temporary database"
    );
    let external_db = Connection::open(&external).unwrap();
    external_db
        .execute_batch("CREATE TABLE source(amount INTEGER); INSERT INTO source VALUES (17);")
        .unwrap();
    let before = fs::read(&external).unwrap();
    assert!(book
        .save_to_file(&external)
        .unwrap_err()
        .message()
        .contains("not a supported Gridloom workbook"));
    assert!(Workbook::open_from_file(&external)
        .err()
        .unwrap()
        .message()
        .contains("not a Gridloom workbook"));
    book.add_sqlite_connection("Live", &external).unwrap();
    assert!(book
        .save_to_file(&external)
        .unwrap_err()
        .message()
        .contains("attached database"));
    assert_eq!(fs::read(&external).unwrap(), before);
}

#[test]
fn unknown_container_version_is_not_opened_or_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("newer.gridloom");
    let mut book = Workbook::new();
    book.save_to_file(&path).unwrap();
    Connection::open(&path)
        .unwrap()
        .pragma_update(None, "user_version", 999)
        .unwrap();
    let original = fs::read(&path).unwrap();
    let error = Workbook::open_from_file(&path).err().unwrap();
    assert!(error.message().contains("unsupported format version 999"));
    assert!(book.save_to_file(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
}
