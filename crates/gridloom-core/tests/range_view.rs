use gridloom_core::range_view::{FilterMode, RangeViewRequest, RowFilter, RowSort};
use gridloom_core::{CellCoord, CellRange, Value, Workbook};

fn put(w: &mut Workbook, a1: &str, text: &str) {
    w.set_cell("Sheet1", CellCoord::from_a1(a1).unwrap(), text.into())
        .unwrap();
}
fn request(range: &str) -> RangeViewRequest {
    RangeViewRequest {
        sheet_name: "Sheet1".into(),
        range: CellRange::from_a1(range).unwrap(),
        first_row_header: true,
        sort: Some(RowSort {
            col: 1,
            descending: false,
        }),
        filter: None,
    }
}
#[test]
fn exact_numeric_sort_is_stable_and_does_not_move_formula_inputs() {
    let mut w = Workbook::new();
    for (address, text) in [
        ("A1", "Account"),
        ("B1", "Amount"),
        ("A2", "Keep"),
        ("B2", "9007199254740993.01"),
        ("A3", "skip"),
        ("B3", "9007199254740993"),
        ("A4", "KEEP"),
        ("B4", "9007199254740993.010"),
        ("A5", "keep"),
        ("B5", "-2"),
        ("A6", "keep"),
        ("A7", "keep"),
        ("B7", "=1/0"),
        ("D1", "=B2-B3"),
    ] {
        put(&mut w, address, text);
    }
    let before = w
        .capture_range("Sheet1", CellRange::from_a1("A1:D7").unwrap())
        .unwrap();
    let mut req = request("A1:B7");
    let ascending = w.sort_filter_view(&req).unwrap();
    assert_eq!(ascending.headers, vec!["Account (A)", "Amount (B)"]);
    let ids = |view: &gridloom_core::range_view::RangeView| {
        view.rows.iter().map(|r| r.row).collect::<Vec<_>>()
    };
    assert_eq!(ids(&ascending), vec![4, 2, 1, 3, 6, 5]);
    req.sort.as_mut().unwrap().descending = true;
    req.filter = Some(RowFilter {
        col: 0,
        mode: FilterMode::Equals,
        text: "KeEp".into(),
    });
    let descending = w.sort_filter_view(&req).unwrap();
    assert_eq!(descending.total_rows, 6);
    assert_eq!(ids(&descending), vec![1, 3, 4, 6, 5]);
    req.filter = Some(RowFilter {
        col: 1,
        mode: FilterMode::AtLeast,
        text: "9007199254740993.0001".into(),
    });
    assert_eq!(ids(&w.sort_filter_view(&req).unwrap()), vec![1, 3]);
    req.filter.as_mut().unwrap().text = "invalid".into();
    assert!(w.sort_filter_view(&req).unwrap_err().contains("threshold"));

    let after = w
        .capture_range("Sheet1", CellRange::from_a1("A1:D7").unwrap())
        .unwrap();
    assert_eq!(before.snapshot, after.snapshot);
    assert_eq!(before.values, after.values);
    assert_eq!(
        w.sheets["Sheet1"]
            .get_cell(&CellCoord::from_a1("D1").unwrap())
            .unwrap()
            .evaluated_value
            .to_string_repr(),
        "0.01"
    );
}
#[test]
fn filter_empty_and_malformed_or_stale_views_fail_closed() {
    let mut w = Workbook::new();
    put(&mut w, "B2", "12");
    let mut req = request("A1:B4");
    req.filter = Some(RowFilter {
        col: 1,
        mode: FilterMode::Empty,
        text: String::new(),
    });
    assert_eq!(
        w.sort_filter_view(&req)
            .unwrap()
            .rows
            .iter()
            .map(|r| r.row)
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
    req.filter.as_mut().unwrap().mode = FilterMode::NonEmpty;
    assert_eq!(w.sort_filter_view(&req).unwrap().rows[0].row, 1);
    req.filter.as_mut().unwrap().col = 2;
    assert!(w.sort_filter_view(&req).is_err());
    assert!(w.sort_filter_view(&request("A1:G1000")).is_err());
    assert!(w.sort_filter_view(&request("A1:B1")).is_err());
    assert!(w.sort_filter_view(&request("A1:B4294967295")).is_err());
    w.stale_calculation_sheets.insert("Sheet1".into());
    assert!(w
        .sort_filter_view(&request("A1:B4"))
        .unwrap_err()
        .contains("Recalculate"));
}

#[test]
fn source_bound_rows_keep_their_database_identity_and_staged_overlay() {
    let mut w = Workbook::new();
    let file = tempfile::NamedTempFile::new().unwrap();
    let source = rusqlite::Connection::open(file.path()).unwrap();
    source.execute_batch("CREATE TABLE entries(id INTEGER PRIMARY KEY, amount INTEGER); INSERT INTO entries VALUES(1,9),(2,3);").unwrap();
    w.add_sqlite_connection("Books", file.path()).unwrap();
    w.create_named_source(
        "journal",
        "Books",
        "SELECT id, amount FROM entries ORDER BY id",
    )
    .unwrap();
    w.insert_table("Sheet1", "Journal", "journal", CellCoord::new(0, 0))
        .unwrap();
    put(&mut w, "D1", "=A2");
    let range = CellRange::from_a1("A1:B3").unwrap();
    let before = w.capture_range("Sheet1", range).unwrap();
    let view = w.sort_filter_view(&request("A1:B3")).unwrap();
    assert_eq!(
        view.rows
            .iter()
            .map(|r| (r.row, r.values[0].as_str()))
            .collect::<Vec<_>>(),
        vec![(2, "2"), (1, "1")]
    );
    assert_eq!(
        w.capture_range("Sheet1", range).unwrap().snapshot,
        before.snapshot
    );
    assert!(w.staged.patches.is_empty());
    w.staged.stage_edit(
        "Books".into(),
        "entries".into(),
        vec!["id".into()],
        vec![Value::Integer(2)],
        "amount".into(),
        Value::Integer(3),
        Value::Integer(12),
    );
    let staged = w.sort_filter_view(&request("A1:B3")).unwrap();
    assert_eq!(
        staged
            .rows
            .iter()
            .map(|r| (r.row, r.values[0].as_str(), r.values[1].as_str()))
            .collect::<Vec<_>>(),
        vec![(1, "1", "9"), (2, "2", "12")]
    );
    assert_eq!(w.staged.patches.len(), 1);
    assert_eq!(
        source
            .query_row("SELECT amount FROM entries WHERE id=2", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        3
    );

    assert_eq!(
        w.sheets["Sheet1"]
            .get_cell(&CellCoord::from_a1("D1").unwrap())
            .unwrap()
            .evaluated_value
            .to_string_repr(),
        "1"
    );
}
