use gridloom_core::find::{FindLookIn, FindRequest};
use gridloom_core::{CellCoord, Workbook};
fn req(query: &str) -> FindRequest {
    FindRequest {
        query: query.into(),
        sheet_name: None,
        look_in: FindLookIn::Values,
        case_sensitive: false,
        entire_cell: false,
        offset: 0,
        limit: 100,
    }
}
#[test]
fn literal_find_is_ordered_paginated_and_read_only() {
    let mut wb = Workbook::new();
    wb.set_cell("Sheet1", CellCoord::new(9, 0), "Revenue".into())
        .unwrap();
    wb.set_cell("Sheet1", CellCoord::new(0, 2), "REVENUE total".into())
        .unwrap();
    wb.add_sheet("Accounts").unwrap();
    wb.set_cell("Accounts", CellCoord::new(0, 0), "revenue".into())
        .unwrap();
    let before = serde_json::to_value(&wb.sheets).unwrap();
    let mut r = req("revenue");
    r.limit = 1;
    let first = wb.find_cells(&r).unwrap();
    assert_eq!(first.total_matches, 3);
    assert_eq!(first.matches[0].sheet_name, "Accounts");
    assert_eq!(first.next_offset, Some(1));
    r.offset = 1;
    let second = wb.find_cells(&r).unwrap();
    assert_eq!(second.matches[0].address, "C1");
    r.offset = 2;
    assert_eq!(wb.find_cells(&r).unwrap().matches[0].address, "A10");
    r.offset = 0;
    r.entire_cell = true;
    r.case_sensitive = true;
    assert_eq!(wb.find_cells(&r).unwrap().total_matches, 1);
    r.sheet_name = Some("Sheet1".into());
    assert_eq!(wb.find_cells(&r).unwrap().total_matches, 0);
    assert_eq!(serde_json::to_value(&wb.sheets).unwrap(), before);
}
#[test]
fn formulas_values_spills_and_long_previews() {
    let mut wb = Workbook::new();
    wb.set_cell("Sheet1", CellCoord::new(0, 0), "=SUM(1,2)".into())
        .unwrap();
    wb.set_cell("Sheet1", CellCoord::new(1, 0), "=SEQUENCE(2,2,40)".into())
        .unwrap();
    assert_eq!(wb.find_cells(&req("SUM")).unwrap().total_matches, 0);
    let mut r = req("SUM");
    r.look_in = FindLookIn::Formulas;
    assert_eq!(wb.find_cells(&r).unwrap().matches[0].address, "A1");
    r = req("43");
    let found = wb.find_cells(&r).unwrap();
    assert_eq!(found.matches[0].address, "B3");
    assert!(found.matches[0].is_spill);
    let text = "a".repeat(5000) + "needle";
    wb.set_cell("Sheet1", CellCoord::new(10, 0), text).unwrap();
    let found = wb.find_cells(&req("needle")).unwrap();
    assert_eq!(found.total_matches, 1);
    assert_eq!(found.matches[0].display_text.len(), 1000);
    assert!(found.warnings.iter().any(|w| w.contains("previews")));
}
#[test]
fn rejects_unbounded_or_empty_requests() {
    let wb = Workbook::new();
    for r in [
        req(""),
        req(&"x".repeat(513)),
        FindRequest {
            limit: 201,
            ..req("a")
        },
        FindRequest {
            offset: 50_001,
            ..req("a")
        },
        FindRequest {
            sheet_name: Some("missing".into()),
            ..req("a")
        },
    ] {
        assert!(wb.find_cells(&r).is_err());
    }
}

#[test]
fn cached_table_headers_and_staged_values_match_the_display() {
    use gridloom_core::db::sqlite::{ColumnMeta, QueryResult};
    use gridloom_core::lineage::LineageReport;
    use gridloom_core::table::{NamedSource, WorksheetTable};
    use gridloom_core::Value;
    let mut wb = Workbook::new();
    let column = |name: &str| ColumnMeta {
        name: name.into(),
        data_type: "TEXT".into(),
        is_primary_key: name == "id",
        is_nullable: false,
    };
    wb.named_sources.insert(
        "Source".into(),
        NamedSource {
            name: "Source".into(),
            connection_name: "DB".into(),
            sql: "SELECT id,label FROM items".into(),
            parameters: vec![],
            last_result: Some(QueryResult {
                columns: vec![column("id"), column("label")],
                rows: vec![vec![Value::Integer(1), Value::Text("Original".into())]],
                total_rows: 1,
            }),
            lineage: Some(LineageReport {
                is_editable: true,
                base_table: Some("items".into()),
                primary_keys: vec!["id".into()],
                editable_columns: vec!["label".into()],
                read_only_columns: vec![],
                read_only_reasons: vec![],
            }),
        },
    );
    wb.sheets.get_mut("Sheet1").unwrap().tables.insert(
        "Items".into(),
        WorksheetTable::new(
            "Items".into(),
            "Source".into(),
            CellCoord::new(4, 2),
            vec!["id".into(), "label".into()],
            1,
        ),
    );
    assert_eq!(
        wb.find_cells(&req("label")).unwrap().matches[0].address,
        "D5"
    );
    wb.staged.stage_edit(
        "DB".into(),
        "items".into(),
        vec!["id".into()],
        vec![Value::Integer(1)],
        "label".into(),
        Value::Text("Original".into()),
        Value::Text("Proposed".into()),
    );
    assert_eq!(wb.find_cells(&req("Original")).unwrap().total_matches, 0);
    let found = wb.find_cells(&req("proposed")).unwrap();
    assert!(found.matches[0].is_staged);
    assert_eq!(found.matches[0].address, "D6");
    let mut r = req("proposed");
    r.look_in = FindLookIn::Formulas;
    assert_eq!(wb.find_cells(&r).unwrap().total_matches, 0);
    assert_eq!(wb.staged.patches.len(), 1);
}

#[test]
fn bounds_sparse_search_and_ignores_empty_grid_extents() {
    use gridloom_core::workbook::CellData;
    let mut wb = Workbook::new();
    let sheet = wb.sheets.get_mut("Sheet1").unwrap();
    sheet.max_rows = 1_000_000;
    sheet.max_cols = 100;
    for row in 0..50_001 {
        let coord = CellCoord::new(row, 0);
        sheet
            .cells
            .insert(coord.to_a1(), CellData::new(coord, "needle".into()));
    }
    let r = wb.find_cells(&req("needle")).unwrap();
    assert!(r.truncated);
    assert_eq!(r.scanned_cells, 50_000);
    assert_eq!(r.total_matches, 50_000);
    assert_eq!(r.matches.len(), 100);
}

#[test]
fn search_uses_formatted_values_and_literal_not_regex_or_wildcards() {
    let mut wb = Workbook::new();
    wb.set_cell("Sheet1", CellCoord::new(0, 0), "0.125".into())
        .unwrap();
    wb.sheets
        .get_mut("Sheet1")
        .unwrap()
        .cells
        .get_mut("A1")
        .unwrap()
        .number_format = Some("0.00%".into());
    let mut r = req("12.50%");
    r.entire_cell = true;
    assert_eq!(wb.find_cells(&r).unwrap().total_matches, 1);
    r.look_in = FindLookIn::Formulas;
    assert_eq!(wb.find_cells(&r).unwrap().total_matches, 0);
    wb.set_cell("Sheet1", CellCoord::new(0, 1), "*.[abc]".into())
        .unwrap();
    assert_eq!(wb.find_cells(&req("*.[")).unwrap().matches[0].address, "B1");
}
