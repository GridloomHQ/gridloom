use gridloom_core::selection_summary::SelectionSummaryRequest;
use gridloom_core::workbook::CellData;
use gridloom_core::{CellCoord, CellError, Value, Workbook};
use rust_decimal::Decimal;
fn req(end: u32) -> SelectionSummaryRequest {
    SelectionSummaryRequest {
        sheet_name: "Sheet1".into(),
        start_row: 0,
        start_col: 0,
        end_row: end,
        end_col: 0,
    }
}
fn insert(wb: &mut Workbook, row: u32, value: Value) {
    let coord = CellCoord::new(row, 0);
    let mut cell = CellData::new(coord, "fixture".into());
    cell.evaluated_value = value;
    wb.sheets
        .get_mut("Sheet1")
        .unwrap()
        .cells
        .insert(coord.to_a1(), cell);
}
fn decimal(n: &str) -> Value {
    Value::Decimal(n.parse().unwrap())
}
#[test]
fn exact_decimals_large_integers_and_cancellation() {
    let mut wb = Workbook::new();
    insert(&mut wb, 0, decimal("0.1"));
    insert(&mut wb, 1, decimal("0.2"));
    let result = wb.selection_summary(&req(1)).unwrap();
    assert_eq!(result.sum.value.as_deref(), Some("0.3"));
    assert_eq!(result.average.value.as_deref(), Some("0.15"));
    assert_eq!(result.sum.quality, "exact");
    assert_eq!(result.average.quality, "exact");
    insert(&mut wb, 0, Value::Integer(9_007_199_254_740_993));
    insert(&mut wb, 1, Value::Integer(1));
    assert_eq!(
        wb.selection_summary(&req(1)).unwrap().sum.value.as_deref(),
        Some("9007199254740994")
    );
    insert(&mut wb, 0, decimal("-123.45"));
    insert(&mut wb, 1, decimal("123.45"));
    assert_eq!(
        wb.selection_summary(&req(1)).unwrap().sum.value.as_deref(),
        Some("0")
    );
}
#[test]
fn repeating_averages_and_float_inputs_are_labeled() {
    let mut wb = Workbook::new();
    insert(&mut wb, 0, Value::Integer(1));
    insert(&mut wb, 1, Value::Integer(0));
    insert(&mut wb, 2, Value::Integer(0));
    let result = wb.selection_summary(&req(2)).unwrap();
    assert_eq!(result.sum.quality, "exact");
    assert_eq!(result.average.quality, "rounded");
    assert!(result.average.value.unwrap().starts_with("0.333333"));
    insert(&mut wb, 0, Value::Integer(3));
    assert_eq!(
        wb.selection_summary(&req(2)).unwrap().average.quality,
        "exact"
    );
    insert(&mut wb, 1, Value::Float(0.1));
    let result = wb.selection_summary(&req(2)).unwrap();
    assert_eq!(result.sum.quality, "approximate");
    assert_eq!(result.average.quality, "approximate");
}
#[test]
fn errors_overflow_or_lost_precision_never_show_partial_aggregates() {
    for values in [
        vec![Value::Integer(1), Value::Error(CellError::Div0)],
        vec![Value::Integer(1), Value::Float(f64::INFINITY)],
        vec![Value::Integer(1), Value::Float(f64::NAN)],
        vec![Value::Integer(1), Value::Float(1e-100)],
        vec![Value::Decimal(Decimal::MAX), Value::Decimal(Decimal::MAX)],
        vec![
            Value::Integer(10),
            decimal("0.0000000000000000000000000001"),
        ],
        vec![
            Value::Decimal(Decimal::MAX),
            decimal("0.0000000000000000000000000001"),
        ],
    ] {
        let mut wb = Workbook::new();
        for (r, v) in values.into_iter().enumerate() {
            insert(&mut wb, r as u32, v);
        }
        let result = wb.selection_summary(&req(1)).unwrap();
        assert!(result.sum.value.is_none());
        assert!(result.average.value.is_none());
        assert_eq!(result.sum.quality, "unavailable");
        assert!(result.sum.reason.is_some());
    }
}
#[test]
fn exclusions_empty_selection_and_bounded_read_only_behavior() {
    let mut wb = Workbook::new();
    insert(&mut wb, 0, Value::Text("12".into()));
    insert(&mut wb, 1, Value::Boolean(true));
    insert(&mut wb, 2, Value::Null);
    insert(&mut wb, 3, Value::EmptyText);
    insert(
        &mut wb,
        4,
        Value::Date(chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
    );
    let before = serde_json::to_value(&wb.sheets).unwrap();
    let result = wb.selection_summary(&req(5)).unwrap();
    assert_eq!(result.selected_count, 6);
    assert_eq!(result.numeric_count, 0);
    assert_eq!(result.counts.text, 2);
    assert_eq!(result.counts.boolean, 1);
    assert_eq!(result.counts.null, 1);
    assert_eq!(result.counts.blank, 1);
    assert_eq!(result.counts.date, 1);
    assert!(result.sum.value.is_none());
    assert!(result.average.value.is_none());
    assert_eq!(serde_json::to_value(&wb.sheets).unwrap(), before);
    let mut r = req(999);
    r.end_col = 9;
    assert_eq!(wb.selection_summary(&r).unwrap().selected_count, 10_000);
    r.end_col = 10;
    assert!(wb.selection_summary(&r).is_err());
    r = req(1);
    r.start_row = 2;
    assert!(wb.selection_summary(&r).is_err());
}
#[test]
fn dynamic_spill_counts_each_visible_number_once() {
    let mut wb = Workbook::new();
    wb.set_cell("Sheet1", CellCoord::new(0, 0), "=SEQUENCE(2,2,1)".into())
        .unwrap();
    let mut r = req(1);
    r.end_col = 1;
    let result = wb.selection_summary(&r).unwrap();
    assert_eq!(result.numeric_count, 4);
    assert_eq!(result.sum.value.as_deref(), Some("10"));
    assert_eq!(result.average.value.as_deref(), Some("2.5"));
}

#[test]
fn staged_cached_values_are_counted_without_external_writes() {
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

    wb.staged.stage_edit(
        "DB".into(),
        "items".into(),
        vec!["id".into()],
        vec![Value::Integer(1)],
        "label".into(),
        Value::Text("Original".into()),
        Value::Integer(24),
    );
    let before = serde_json::to_value(&wb.staged).unwrap();
    let mut r = req(1);
    r.start_row = 1;
    r.end_col = 1;
    let result = wb.selection_summary(&r).unwrap();
    assert_eq!(result.numeric_count, 2);
    assert_eq!(result.sum.value.as_deref(), Some("25"));
    assert_eq!(serde_json::to_value(&wb.staged).unwrap(), before);
}
