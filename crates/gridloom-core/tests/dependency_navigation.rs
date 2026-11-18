use gridloom_core::{CellCoord, Workbook};
fn a(s: &str) -> CellCoord {
    CellCoord::from_a1(s).unwrap()
}
fn set(w: &mut Workbook, s: &str, c: &str, v: &str) {
    w.set_cell(s, a(c), v.into()).unwrap();
}
#[test]
fn direct_cross_sheet_navigation_is_grouped_and_branch_conservative() {
    let mut w = Workbook::new();
    w.add_sheet("Inputs").unwrap();
    set(&mut w, "Inputs", "A1", "10");
    set(&mut w, "Inputs", "A2", "20");
    set(&mut w, "Sheet1", "B1", "=SUM(Inputs!A1:A2)");
    set(&mut w, "Sheet1", "B2", "=IF(TRUE,B1,Inputs!A1)");
    let up = w.navigate_dependencies("Sheet1", a("B1")).unwrap();
    assert_eq!(up.upstream.len(), 1);
    assert_eq!(up.upstream[0].kind, "range");
    assert_eq!(up.upstream[0].range.unwrap().to_a1(), "A1:A2");
    let down = w.navigate_dependencies("Inputs", a("A1")).unwrap();
    assert_eq!(down.downstream.len(), 2);
    assert!(down.downstream.iter().any(|n| n.coord == Some(a("B2"))));
    let downstream_second = w.navigate_dependencies("Inputs", a("A2")).unwrap();
    assert_eq!(downstream_second.downstream.len(), 1); // only direct consumers
    assert!(!down.truncated);
    // Repeated navigation is stable and doesn't mutate the workbook.
    assert_eq!(
        serde_json::to_value(&down).unwrap(),
        serde_json::to_value(w.navigate_dependencies("Inputs", a("A1")).unwrap()).unwrap()
    );
    set(&mut w, "Sheet1", "B1", "3");
    assert!(w
        .navigate_dependencies("Inputs", a("A2"))
        .unwrap()
        .downstream
        .is_empty());
}
#[test]
fn blank_cells_can_have_consumers_and_invalid_locations_fail() {
    let mut w = Workbook::new();
    set(&mut w, "Sheet1", "B1", "=A1+1");
    assert_eq!(
        w.navigate_dependencies("Sheet1", a("A1"))
            .unwrap()
            .downstream
            .len(),
        1
    );
    assert!(w.navigate_dependencies("Missing", a("A1")).is_err());
    assert!(w
        .navigate_dependencies("Sheet1", CellCoord::new(1000, 0))
        .is_err());
}
#[test]
fn structured_columns_and_spill_children_have_navigable_dependencies() {
    let mut w = Workbook::new();
    w.add_sqlite_memory("db").unwrap();
    w.create_named_source("Items", "db", "SELECT 10 AS Amount, 20 AS Other".into())
        .unwrap();
    w.insert_table("Sheet1", "ItemsTable", "Items", a("A1"))
        .unwrap();
    set(&mut w, "Sheet1", "D1", "=SUM(ItemsTable[Amount])");
    let up = w.navigate_dependencies("Sheet1", a("D1")).unwrap();
    assert_eq!(up.upstream[0].kind, "table");
    assert_eq!(up.upstream[0].range.unwrap().to_a1(), "A2");
    assert_eq!(
        w.navigate_dependencies("Sheet1", a("A2"))
            .unwrap()
            .downstream
            .len(),
        1
    );
    assert!(w
        .navigate_dependencies("Sheet1", a("B2"))
        .unwrap()
        .downstream
        .is_empty());
    let source = w.navigate_dependencies("Sheet1", a("A2")).unwrap();
    assert_eq!(source.upstream[0].source_name.as_deref(), Some("Items"));
    set(&mut w, "Sheet1", "F1", "={1;2}");
    assert_eq!(
        w.navigate_dependencies("Sheet1", a("F2")).unwrap().upstream[0].coord,
        Some(a("F1"))
    );
    assert_eq!(
        w.navigate_dependencies("Sheet1", a("F1"))
            .unwrap()
            .downstream[0]
            .kind,
        "spill"
    );
}

#[test]
fn large_ranges_stay_grouped_and_neighbor_limits_are_explicit() {
    use gridloom_core::workbook::CellData;
    let mut w = Workbook::new();
    let sheet = w.sheets.get_mut("Sheet1").unwrap();
    sheet.max_rows = 2000;
    // Construct without recalculation: navigation must work on incomplete/cached states too.
    for row in 0..1001 {
        let coord = CellCoord::new(row, 1);
        sheet
            .cells
            .insert(coord.to_a1(), CellData::new(coord, "=A1".into()));
    }
    let coord = a("D1");
    sheet.cells.insert(
        coord.to_a1(),
        CellData::new(coord, "=SUM(A1:A1000000)".into()),
    );
    let nav = w.navigate_dependencies("Sheet1", a("A1")).unwrap();
    assert_eq!(nav.downstream.len(), 1000);
    assert!(nav.truncated);
    assert!(nav.warnings.iter().any(|s| s.contains("incomplete")));
    let grouped = w.navigate_dependencies("Sheet1", a("D1")).unwrap();
    assert_eq!(grouped.upstream.len(), 1);
    assert_eq!(grouped.upstream[0].kind, "range");
}

#[test]
fn unparseable_formulas_report_incomplete_scope_without_hiding_other_consumers() {
    use gridloom_core::workbook::CellData;
    let mut w = Workbook::new();
    set(&mut w, "Sheet1", "B1", "=A1+1");
    w.sheets
        .get_mut("Sheet1")
        .unwrap()
        .cells
        .insert("C1".into(), CellData::new(a("C1"), "=SUM(".into()));
    let nav = w.navigate_dependencies("Sheet1", a("A1")).unwrap();
    assert_eq!(nav.downstream.len(), 1);
    assert!(nav
        .warnings
        .iter()
        .any(|s| s.contains("Cannot parse Sheet1!C1")));
}

#[test]
fn dynamic_and_unknown_functions_disclose_unresolved_boundaries() {
    use gridloom_core::workbook::CellData;
    let mut w = Workbook::new();
    let mut c = CellData::new(a("B1"), "=INDIRECT(A1)".into());
    c.evaluated_value = gridloom_core::Value::Error(gridloom_core::CellError::Name);
    w.sheets
        .get_mut("Sheet1")
        .unwrap()
        .cells
        .insert("B1".into(), c);
    let nav = w.navigate_dependencies("Sheet1", a("B1")).unwrap();
    assert_eq!(nav.upstream.len(), 1);
    assert_eq!(nav.upstream[0].coord, Some(a("A1")));
    assert!(nav
        .warnings
        .iter()
        .any(|s| s.contains("dynamic-reference boundary")));
    assert!(nav
        .warnings
        .iter()
        .any(|s| s.contains("unrecognized formula")));
}
