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
    let mut other = Workbook::new();
    assert!(other
        .add_sqlite_connection("Workbook", &path)
        .unwrap_err()
        .message()
        .contains("not as a linked source"));
    assert!(other.add_duckdb_connection("Workbook", &path).is_err());
    assert!(other.connection_configs.is_empty());
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

#[test]
fn local_table_keeps_schema_and_exact_values_without_external_connection() {
    use gridloom_core::{
        table::{LocalColumn, LocalColumnType::*},
        CellRange,
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tables.gridloom");
    let recovery = dir.path().join("recovery.gridloom");
    let mut book = Workbook::new();
    book.add_sheet("Budget").unwrap();
    let sheet = "Budget";
    book.edit_range(
        sheet,
        CellCoord::new(0, 0),
        vec![
            vec!["Code".into(), "Amount".into(), "Date".into()],
            vec![
                "0007".into(),
                "123456789012.123456789".into(),
                "2026-11-21".into(),
            ],
        ],
    )
    .unwrap();
    let range = CellRange::new(0, 0, 3, 2);
    let columns = [("Code", Text), ("Amount", Decimal), ("Date", Date)]
        .into_iter()
        .map(|(name, kind)| LocalColumn {
            name: name.into(),
            kind,
        })
        .collect();
    let table = book
        .create_local_table(sheet, "Expenses", range, columns)
        .unwrap();
    let id = table.local.unwrap().id;
    assert_eq!(
        book.sheets[sheet].cells["A2"].evaluated_value,
        Value::Text("0007".into())
    );
    assert!(matches!(
        book.sheets[sheet].cells["C2"].evaluated_value,
        Value::Date(_)
    ));
    assert!(book
        .edit_range(
            sheet,
            CellCoord::new(1, 0),
            vec![vec!["0099".into(), "not decimal".into()]]
        )
        .is_err());
    assert_eq!(
        book.sheets[sheet].cells["A2"].evaluated_value,
        Value::Text("0007".into())
    );
    assert!(book
        .set_cell(sheet, CellCoord::new(1, 1), "=1+2".into())
        .is_err());
    assert!(book
        .set_cell(sheet, CellCoord::new(0, 0), "Other".into())
        .is_err());
    book.set_cell(sheet, CellCoord::new(2, 1), "0.000000001".into())
        .unwrap();
    book.set_cell(sheet, CellCoord::new(5, 0), "=SUM(Expenses[Amount])".into())
        .unwrap();
    assert_eq!(
        book.sheets[sheet].cells["A6"]
            .evaluated_value
            .to_string_repr(),
        "123456789012.123456790"
    );
    assert_eq!(book.staged.patches.len(), 0);
    book.save_to_file(&path).unwrap();
    book.save_recovery_copy(&recovery).unwrap();
    let db = Connection::open(&path).unwrap();
    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM worksheet_table_cell WHERE table_id=?1",
            [id.to_string()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 4);
    let amount_records: i64 = db
        .query_row(
            "SELECT count(*) FROM worksheet_cell WHERE coordinate='B2'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(amount_records, 0);
    for saved in [&path, &recovery] {
        let mut loaded = Workbook::open_from_file(saved).unwrap();
        assert!(loaded.connection_configs.is_empty());
        assert_eq!(
            loaded.sheets[sheet].tables["Expenses"]
                .local
                .as_ref()
                .unwrap()
                .id,
            id
        );
        assert_eq!(
            loaded.sheets[sheet].cells["A2"].evaluated_value,
            Value::Text("0007".into())
        );
        loaded.rename_sheet(sheet, "Renamed").unwrap();
        loaded
            .set_cell("Renamed", CellCoord::new(2, 1), "0.000000011".into())
            .unwrap();
        assert_eq!(
            loaded.sheets["Renamed"].cells["A6"]
                .evaluated_value
                .to_string_repr(),
            "123456789012.123456800"
        );
    }
}

#[test]
fn local_table_creation_does_not_hide_data_or_partially_coerce_cells() {
    use gridloom_core::{
        table::{LocalColumn, LocalColumnType::*},
        CellRange,
    };
    let mut book = Workbook::new();
    let sheet = "Sheet1";
    book.edit_range(
        sheet,
        CellCoord::new(0, 0),
        vec![
            vec!["Postal".into(), "Qty".into()],
            vec!["0005".into(), "invalid".into()],
        ],
    )
    .unwrap();
    let columns = || {
        vec![
            LocalColumn {
                name: "Postal".into(),
                kind: Text,
            },
            LocalColumn {
                name: "Qty".into(),
                kind: Integer,
            },
        ]
    };
    let generation = book.dep_graph.generation;
    assert!(book
        .create_local_table(sheet, "Order", CellRange::new(0, 0, 2, 1), columns())
        .is_err());
    assert!(book.sheets[sheet].tables.is_empty());
    assert_eq!(book.dep_graph.generation, generation);
    assert_eq!(
        book.sheets[sheet].cells["A2"].evaluated_value,
        Value::Integer(5)
    );
    let mut renamed = columns();
    renamed[0].name = "Changed".into();
    assert!(book
        .create_local_table(sheet, "Order", CellRange::new(0, 0, 2, 1), renamed)
        .unwrap_err()
        .message()
        .contains("would hide"));
}

#[test]
fn workbook_copy_forks_two_local_tables_while_shared_source_is_unchanged() {
    use gridloom_core::{
        table::{LocalColumn, LocalColumnType::*},
        CellRange,
    };
    let dir = tempfile::tempdir().unwrap();
    let folder_a = dir.path().join("original");
    let folder_b = dir.path().join("copy");
    fs::create_dir(&folder_a).unwrap();
    fs::create_dir(&folder_b).unwrap();
    let a = folder_a.join("ops.gridloom");
    let b = folder_b.join("ops.gridloom");
    let auto = dir.path().join("autosave.gridloom");
    let recovered_path = folder_b.join("recovered.gridloom");
    let external = dir.path().join("external.sqlite");
    let db = Connection::open(&external).unwrap();
    db.execute_batch("CREATE TABLE remote_input(id INTEGER PRIMARY KEY, value TEXT); INSERT INTO remote_input VALUES(1, 'untouched');").unwrap();
    let remote_bytes = fs::read(&external).unwrap();

    let mut original = Workbook::new();
    let sheet = "Sheet1";
    let primary = original
        .create_local_table(
            sheet,
            "PrimaryLocal",
            CellRange::new(0, 0, 4, 1),
            vec![
                LocalColumn {
                    name: "Code".into(),
                    kind: Text,
                },
                LocalColumn {
                    name: "Amount".into(),
                    kind: Decimal,
                },
            ],
        )
        .unwrap()
        .local
        .unwrap()
        .id;
    let secondary = original
        .create_local_table(
            sheet,
            "SecondaryLocal",
            CellRange::new(0, 3, 2, 3),
            vec![LocalColumn {
                name: "Quantity".into(),
                kind: Integer,
            }],
        )
        .unwrap()
        .local
        .unwrap()
        .id;
    original.add_sqlite_connection("Remote", &external).unwrap();
    original
        .create_named_source("RemoteSource", "Remote", "SELECT * FROM remote_input")
        .unwrap();
    original
        .insert_table(sheet, "RemoteTable", "RemoteSource", CellCoord::new(0, 5))
        .unwrap();
    original
        .edit_range(
            sheet,
            CellCoord::new(1, 0),
            vec![vec!["000005".into(), "123456789012.123456789".into()]],
        )
        .unwrap();
    original
        .set_cell(sheet, CellCoord::new(2, 0), "\"\"".into())
        .unwrap();
    original
        .set_local_table_null(sheet, CellCoord::new(3, 0))
        .unwrap();
    original
        .set_cell(sheet, CellCoord::new(4, 0), "NULL".into())
        .unwrap();
    original
        .set_cell(sheet, CellCoord::new(1, 3), "9".into())
        .unwrap();
    assert!(original
        .set_local_table_null(sheet, CellCoord::new(1, 5))
        .is_err());
    original.save_to_file(&a).unwrap();
    let committed = fs::read(&a).unwrap();
    assert_eq!(
        original.sheets[sheet].cells["A3"].evaluated_value,
        Value::EmptyText
    );
    assert_eq!(
        original.sheets[sheet].cells["A4"].evaluated_value,
        Value::Null
    );
    assert_eq!(
        original.sheets[sheet].cells["A5"].evaluated_value,
        Value::Text("NULL".into())
    );

    let mut save_as = Workbook::open_from_file(&a).unwrap();
    save_as.save_to_file(&b).unwrap();
    assert_eq!(fs::read(&a).unwrap(), committed);
    assert_eq!(
        save_as.sheets[sheet].tables["PrimaryLocal"]
            .local
            .as_ref()
            .unwrap()
            .id,
        primary
    );
    assert_eq!(
        save_as.sheets[sheet].tables["SecondaryLocal"]
            .local
            .as_ref()
            .unwrap()
            .id,
        secondary
    );
    save_as
        .set_cell(sheet, CellCoord::new(1, 3), "22".into())
        .unwrap();
    save_as
        .set_cell(sheet, CellCoord::new(1, 0), "000022".into())
        .unwrap();
    save_as.save_to_file(&b).unwrap();
    original
        .set_cell(sheet, CellCoord::new(1, 3), "11".into())
        .unwrap();
    original
        .set_cell(sheet, CellCoord::new(1, 0), "000011".into())
        .unwrap();
    original.save_to_file(&a).unwrap();

    let mut original = Workbook::open_from_file(&a).unwrap();
    let copy = Workbook::open_from_file(&b).unwrap();
    assert_eq!(
        original.sheets[sheet].cells["D2"].evaluated_value,
        Value::Integer(11)
    );
    assert_eq!(
        copy.sheets[sheet].cells["D2"].evaluated_value,
        Value::Integer(22)
    );
    assert_eq!(
        original.sheets[sheet].cells["A2"].evaluated_value,
        Value::Text("000011".into())
    );
    assert_eq!(
        copy.sheets[sheet].cells["A2"].evaluated_value,
        Value::Text("000022".into())
    );
    assert_eq!(
        copy.sheets[sheet].cells["A3"].evaluated_value,
        Value::EmptyText
    );
    assert_eq!(copy.sheets[sheet].cells["A4"].evaluated_value, Value::Null);
    assert_eq!(
        copy.sheets[sheet].cells["A5"].evaluated_value,
        Value::Text("NULL".into())
    );

    let committed = fs::read(&a).unwrap();
    original
        .set_cell(sheet, CellCoord::new(1, 3), "33".into())
        .unwrap();
    original.save_recovery_copy(&auto).unwrap();
    let table = original
        .sheets
        .get_mut(sheet)
        .unwrap()
        .tables
        .get_mut("SecondaryLocal")
        .unwrap();
    table.local.as_mut().unwrap().id = primary;
    assert!(original.save_to_file(&a).is_err());
    assert_eq!(fs::read(&a).unwrap(), committed);
    let mut recovered = Workbook::open_from_file(&auto).unwrap();
    assert_eq!(
        recovered.sheets[sheet].cells["D2"].evaluated_value,
        Value::Integer(33)
    );
    recovered
        .set_cell(sheet, CellCoord::new(1, 3), "44".into())
        .unwrap();
    recovered.save_to_file(&recovered_path).unwrap();
    assert_eq!(
        Workbook::open_from_file(&recovered_path).unwrap().sheets[sheet].cells["D2"]
            .evaluated_value,
        Value::Integer(44)
    );
    assert_eq!(fs::read(&a).unwrap(), committed);
    assert!(recovered.staged.patches.is_empty());
    assert_eq!(fs::read(&external).unwrap(), remote_bytes);
    let value: String = db
        .query_row("SELECT value FROM remote_input", [], |r| r.get(0))
        .unwrap();
    assert_eq!(value, "untouched");
}

#[test]
fn version_one_container_reads_without_writing_and_upgrades_only_on_save() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v1.gridloom");
    let mut first = Workbook::new();
    first
        .set_cell("Sheet1", CellCoord::new(0, 0), "=1+2".into())
        .unwrap();
    first.save_to_file(&path).unwrap();
    Connection::open(&path).unwrap().execute_batch(
        "DROP TABLE worksheet_table_cell; ALTER TABLE worksheet_table RENAME TO worksheet_table_v2;
         CREATE TABLE worksheet_table(sheet_id TEXT NOT NULL REFERENCES worksheet(id), name TEXT NOT NULL, json TEXT NOT NULL, PRIMARY KEY (sheet_id,name)) STRICT, WITHOUT ROWID;
         INSERT INTO worksheet_table SELECT sheet_id,name,json FROM worksheet_table_v2;
         DROP TABLE worksheet_table_v2;
         PRAGMA user_version=1;",
    ).unwrap();
    let previous = fs::read(&path).unwrap();
    let mut reopened = Workbook::open_from_file(&path).unwrap();
    assert_eq!(
        reopened.sheets["Sheet1"].cells["A1"]
            .evaluated_value
            .to_string_repr(),
        "3"
    );
    assert_eq!(fs::read(&path).unwrap(), previous);
    reopened.save_to_file(&path).unwrap();
    let version: i64 = Connection::open(&path)
        .unwrap()
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 2);
}
