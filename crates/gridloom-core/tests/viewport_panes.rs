use gridloom_core::virtual_view::{ViewportPanesRequest, ViewportRequest};
use gridloom_core::{CellCoord, Value, Workbook};
fn req() -> ViewportPanesRequest {
    ViewportPanesRequest {
        viewport: ViewportRequest {
            sheet_name: "Sheet1".into(),
            start_row: 500,
            end_row: 505,
            start_col: 50,
            end_col: 55,
            overscan: 2,
        },
        freeze_top_row: true,
        freeze_first_column: true,
        visible_frozen_rows: None,
        visible_frozen_columns: None,
    }
}
#[test]
fn composite_bounds_stay_sparse_and_never_mutate() {
    let mut wb = Workbook::new();
    for (row, col) in [(0, 0), (0, 50), (500, 0), (500, 50), (200, 20)] {
        wb.set_cell("Sheet1", CellCoord::new(row, col), "present".into())
            .unwrap();
    }
    let before = serde_json::to_value(&wb.sheets).unwrap();
    let response = wb.get_viewport_panes(&req()).unwrap();
    assert_eq!(response.loaded_ranges.len(), 4);
    assert_eq!(response.viewport.cells.len(), 4);
    assert_eq!(response.loaded_ranges[0].start_row, 498);
    assert_eq!(response.loaded_ranges[0].end_row, 507);
    assert_eq!(response.loaded_ranges[0].start_col, 48);
    assert_eq!(response.loaded_ranges[0].end_col, 57);
    assert!(response
        .loaded_ranges
        .iter()
        .all(|r| r.end_row - r.start_row < 10 && r.end_col - r.start_col < 10));
    assert_eq!(serde_json::to_value(&wb.sheets).unwrap(), before);
    let mut r = req();
    r.viewport.start_row = 0;
    r.viewport.start_col = 0;
    r.viewport.end_row = 5;
    r.viewport.end_col = 5;
    assert_eq!(wb.get_viewport_panes(&r).unwrap().loaded_ranges.len(), 1);
    r.viewport.start_row = 500;
    r.viewport.end_row = 505;
    assert_eq!(wb.get_viewport_panes(&r).unwrap().loaded_ranges.len(), 2);
    r.freeze_top_row = false;
    assert_eq!(wb.get_viewport_panes(&r).unwrap().loaded_ranges.len(), 1);
}
#[test]
fn composite_rejects_unbounded_requests_and_clamps_sheet_edges() {
    let wb = Workbook::new();
    let mut r = req();
    r.viewport.overscan = 101;
    assert!(wb.get_viewport_panes(&r).is_err());
    r = req();
    r.viewport.start_row = 999;
    r.viewport.end_row = u32::MAX;
    r.viewport.start_col = 99;
    r.viewport.end_col = u32::MAX;
    let result = wb.get_viewport_panes(&r).unwrap();
    assert_eq!(result.loaded_ranges[0].end_row, 999);
    assert_eq!(result.loaded_ranges[0].end_col, 99);
    r = req();
    r.viewport.start_row = 0;
    r.viewport.start_col = 0;
    r.viewport.end_row = 999;
    r.viewport.end_col = 99;
    assert!(wb.get_viewport_panes(&r).is_err());
    r = req();
    r.viewport.start_row = 1000;
    assert!(wb.get_viewport_panes(&r).is_err());
    r = req();
    r.viewport.end_row = 10;
    assert!(wb.get_viewport_panes(&r).is_err());
}
#[test]
fn multiple_frozen_rows_and_columns_are_sparse_and_count_toward_limit() {
    let mut wb = Workbook::new();
    for (row, col) in [(2, 3), (2, 50), (500, 3), (500, 50), (200, 20)] {
        wb.set_cell("Sheet1", CellCoord::new(row, col), "present".into())
            .unwrap();
    }
    let mut r = req();
    r.visible_frozen_rows = Some(3);
    r.visible_frozen_columns = Some(4);
    let result = wb.get_viewport_panes(&r).unwrap();
    assert_eq!(result.loaded_ranges.len(), 4);
    assert_eq!(result.viewport.cells.len(), 4);
    assert_eq!(result.loaded_ranges[3].end_row, 2);
    assert_eq!(result.loaded_ranges[3].end_col, 3);

    r.viewport.start_row = 0;
    r.viewport.end_row = 0;
    r.viewport.start_col = 0;
    r.viewport.end_col = 0;
    r.viewport.overscan = 0;
    let result = wb.get_viewport_panes(&r).unwrap();
    assert_eq!(result.loaded_ranges.len(), 1);
    assert_eq!(result.loaded_ranges[0].end_row, 2);
    assert_eq!(result.loaded_ranges[0].end_col, 3);

    r.viewport.start_row = 499;
    r.viewport.end_row = 500;
    r.viewport.start_col = 49;
    r.viewport.end_col = 50;
    r.visible_frozen_rows = Some(u32::MAX);
    r.visible_frozen_columns = Some(u32::MAX);
    assert!(wb.get_viewport_panes(&r).is_err());
}

#[test]
fn frozen_and_scrolling_cells_share_current_staged_overlay() {
    use gridloom_core::db::sqlite::{ColumnMeta, QueryResult};
    use gridloom_core::lineage::LineageReport;
    use gridloom_core::table::{NamedSource, WorksheetTable};
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
            CellCoord::new(0, 0),
            vec!["id".into(), "label".into()],
            1,
        ),
    );
    let mut r = req();
    r.viewport.start_row = 1;
    r.viewport.end_row = 1;
    r.viewport.start_col = 1;
    r.viewport.end_col = 1;
    r.viewport.overscan = 0;
    let generation = wb.dep_graph.generation;
    wb.staged.stage_edit(
        "DB".into(),
        "items".into(),
        vec!["id".into()],
        vec![Value::Integer(1)],
        "label".into(),
        Value::Text("Original".into()),
        Value::Text("Proposed".into()),
    );
    let staged = serde_json::to_value(&wb.staged).unwrap();
    let response = wb.get_viewport_panes(&r).unwrap();
    assert_eq!(response.loaded_ranges.len(), 4);
    assert_eq!(response.viewport.cells["B2"].display_text, "Proposed");
    assert!(response.viewport.cells["B2"].is_staged);
    assert_eq!(response.viewport.cells["B1"].display_text, "label");
    assert_eq!(response.viewport.cells["A2"].display_text, "1");
    assert_eq!(response.viewport.cells["A1"].display_text, "id");
    assert_eq!(response.viewport.generation, generation);
    assert_eq!(serde_json::to_value(&wb.staged).unwrap(), staged);
}
