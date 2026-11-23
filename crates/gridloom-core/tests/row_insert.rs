use gridloom_core::functions::EvaluationContext;
use gridloom_core::workbook::{CellData, LedgerLink, LedgerSpec, WorkbookEvalContext};
use gridloom_core::{CellCoord, CellRange, Value, Workbook};

fn coord(text: &str) -> CellCoord {
    CellCoord::from_a1(text).unwrap()
}
fn set(w: &mut Workbook, sheet: &str, cell: &str, value: &str) {
    w.set_cell(sheet, coord(cell), value.into()).unwrap();
}
fn raw<'a>(w: &'a Workbook, sheet: &str, cell: &str) -> &'a str {
    &w.sheets[sheet].get_cell(&coord(cell)).unwrap().raw_input
}
fn value(w: &Workbook, sheet: &str, cell: &str) -> String {
    WorkbookEvalContext {
        workbook: w,
        sheet_name: sheet,
        current_coord: coord(cell),
    }
    .get_cell_value(coord(cell))
    .unwrap()
    .to_string_repr()
}

#[test]
fn moves_sparse_cells_styles_links_and_cross_sheet_references_then_recalculates() {
    let mut w = Workbook::new();
    let name = "D'Água";
    w.add_sheet(name).unwrap();
    set(&mut w, name, "A1", "5");
    set(&mut w, name, "A2", "3");
    set(&mut w, name, "A3", "4");
    set(&mut w, name, "B4", "=SUM(A1 : $A$3) + A$2 + $A3");
    set(&mut w, "Sheet1", "A2", "11");
    set(
        &mut w,
        "Sheet1",
        "B2",
        "= 'D''Água'!($A$2) + SUM('D''Água'!A2:A3) + A2",
    );
    set(&mut w, "Sheet1", "C2", "=\"A2 : A3 – café\"");
    set(&mut w, "Sheet1", "D2", "=SUM('D''Água'!a3:a1)");
    set(&mut w, "Sheet1", "E2", "= 'D''Água'!A1 + Sheet1!A2");
    let target = w.sheets.get_mut(name).unwrap();
    target.get_cell_mut(&coord("A2")).unwrap().style.bold = true;
    target.ledger_links.insert(
        "A3".into(),
        LedgerLink {
            spec: LedgerSpec {
                connection: "local".into(),
                table: "ledger".into(),
                date: "date".into(),
                cents: "cents".into(),
                from: None,
                through: None,
                account: None,
            },
            source_path: "ledger.sqlite".into(),
        },
    );

    w.insert_rows(name, 1, 2).unwrap();
    assert_eq!(w.sheets[name].max_rows, 1002);
    assert!(w.sheets[name].get_cell(&coord("A2")).is_none());
    assert_eq!(raw(&w, name, "A4"), "3");
    assert!(w.sheets[name].get_cell(&coord("A4")).unwrap().style.bold);
    assert!(w.sheets[name].ledger_links.contains_key("A5"));
    assert_eq!(raw(&w, name, "B6"), "=SUM(A1 : $A$5) + A$4 + $A5");
    assert_eq!(
        raw(&w, "Sheet1", "B2"),
        "= 'D''Água'!($A$4) + SUM('D''Água'!A4:A5) + A2"
    );
    assert_eq!(raw(&w, "Sheet1", "C2"), "=\"A2 : A3 – café\"");
    assert_eq!(raw(&w, "Sheet1", "D2"), "=SUM('D''Água'!a5:a1)");
    assert_eq!(raw(&w, "Sheet1", "E2"), "= 'D''Água'!A1 + Sheet1!A2");
    assert_eq!(value(&w, name, "B6"), "19");
    assert_eq!(value(&w, "Sheet1", "B2"), "21");
    assert!(w.undo_sheet_edit().is_err());

    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("structure.tay");
    w.save_to_file(&path).unwrap();
    let loaded = Workbook::open_from_file(path).unwrap();
    assert_eq!(raw(&loaded, name, "A4"), "3");
    assert_eq!(value(&loaded, "Sheet1", "B2"), "21");
    assert!(loaded.sheets[name].ledger_links.contains_key("A5"));
}

#[test]
fn insertion_can_move_a_whole_named_table_but_never_split_it() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("source.sqlite");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE journal(amount INT); INSERT INTO journal VALUES(3),(4);")
        .unwrap();
    let mut w = Workbook::new();
    w.add_sqlite_connection("Test", path).unwrap();
    w.create_named_source("JournalQuery", "Test", "SELECT amount FROM journal")
        .unwrap();
    w.insert_table("Sheet1", "Journal", "JournalQuery", coord("B2"))
        .unwrap();
    set(&mut w, "Sheet1", "C1", "=SUM(Journal[amount]) + B3");
    let generation = w.dep_graph.generation;
    let rejected = w.insert_rows("Sheet1", 2, 1).unwrap_err();
    assert!(rejected.0.contains("split worksheet table"));
    assert_eq!(w.dep_graph.generation, generation);
    assert_eq!(
        w.sheets["Sheet1"].tables["Journal"].range,
        CellRange::from_a1("B2:B4").unwrap()
    );
    w.insert_rows("Sheet1", 1, 1).unwrap();
    assert_eq!(
        w.sheets["Sheet1"].tables["Journal"].range,
        CellRange::from_a1("B3:B5").unwrap()
    );
    assert_eq!(w.sheets["Sheet1"].tables["Journal"].anchor, coord("B3"));
    assert_eq!(raw(&w, "Sheet1", "C1"), "=SUM(Journal[amount]) + B4");
    assert_eq!(value(&w, "Sheet1", "C1"), "10");
}

#[test]
fn failures_are_atomic_and_keep_existing_edit_history() {
    let mut w = Workbook::new();
    set(&mut w, "Sheet1", "A2", "100");
    set(&mut w, "Sheet1", "B2", "=INDIRECT(\"A2\")");
    let unchanged = w.dep_graph.generation;
    assert!(w
        .insert_rows("Sheet1", 1, 1)
        .unwrap_err()
        .0
        .contains("Computed references"));
    assert_eq!(w.dep_graph.generation, unchanged);
    assert_eq!(w.sheets["Sheet1"].max_rows, 1000);
    assert_eq!(raw(&w, "Sheet1", "A2"), "100");
    w.undo_sheet_edit().unwrap();
    assert!(w.sheets["Sheet1"].get_cell(&coord("B2")).is_none());
    // A legacy/corrupt workbook can contain a cycle that current edits reject.
    w.sheets
        .get_mut("Sheet1")
        .unwrap()
        .cells
        .insert("B2".into(), CellData::new(coord("B2"), "=B2".into()));
    assert!(w
        .insert_rows("Sheet1", 1, 1)
        .unwrap_err()
        .0
        .contains("Recalculation failed"));
    assert_eq!(raw(&w, "Sheet1", "B2"), "=B2");
    assert!(!w.sheets["Sheet1"].cells.contains_key("B3"));
    assert_eq!(w.sheets["Sheet1"].max_rows, 1000);
    assert!(matches!(
        w.sheets["Sheet1"]
            .get_cell(&coord("B2"))
            .unwrap()
            .evaluated_value,
        Value::Blank
    ));
}

#[test]
fn spill_and_invalid_insert_bounds_fail_closed() {
    let mut w = Workbook::new();
    set(&mut w, "Sheet1", "A1", "={1;2}");
    assert!(w
        .insert_rows("Sheet1", 1, 1)
        .unwrap_err()
        .0
        .contains("spill"));
    assert!(w.insert_rows("Sheet1", 1000, 1).is_err());
    assert!(w.insert_rows("Sheet1", 0, 0).is_err());
    assert!(w.insert_rows("Sheet1", 0, 1001).is_err());
}
