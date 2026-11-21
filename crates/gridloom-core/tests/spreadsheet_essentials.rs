use gridloom_core::functions::EvaluationContext;
use gridloom_core::spreadsheet::{rewrite_formula, CellStyle};
use gridloom_core::workbook::WorkbookEvalContext;
use gridloom_core::{CellCoord, CellError, CellRange, Value, Workbook};
fn a(s: &str) -> CellCoord {
    CellCoord::from_a1(s).unwrap()
}
fn range(s: &str) -> CellRange {
    CellRange::from_a1(s).unwrap()
}
fn set(w: &mut Workbook, s: &str, c: &str, v: &str) {
    w.set_cell(s, a(c), v.into()).unwrap()
}
fn value(w: &Workbook, s: &str, c: &str) -> String {
    WorkbookEvalContext {
        workbook: w,
        sheet_name: s,
        current_coord: a(c),
    }
    .get_cell_value(a(c))
    .unwrap()
    .to_string_repr()
}
#[test]
fn cross_sheet_chain_and_ranges_recalculate_in_dependency_order() {
    let mut w = Workbook::new();
    w.add_sheet("Inputs").unwrap();
    w.add_sheet("Final report").unwrap();
    set(&mut w, "Inputs", "A1", "3");
    set(&mut w, "Inputs", "A2", "4");
    set(&mut w, "Sheet1", "A1", "=SUM(Inputs!A1:A2)");
    set(&mut w, "Final report", "A1", "=Sheet1!A1*2");
    set(&mut w, "Sheet1", "B1", "='Final report'!A1+1");
    assert_eq!(value(&w, "Sheet1", "B1"), "15");
    set(&mut w, "Inputs", "A2", "9");
    assert_eq!(value(&w, "Sheet1", "B1"), "25");
}
#[test]
fn cross_sheet_cycle_rejected_and_entire_workbook_restored() {
    let mut w = Workbook::new();
    w.add_sheet("Inputs").unwrap();
    set(&mut w, "Inputs", "A1", "3");
    set(&mut w, "Sheet1", "A1", "=Inputs!A1");
    assert_eq!(
        w.set_cell("Inputs", a("A1"), "=Sheet1!A1".into()),
        Err(CellError::Cycle)
    );
    assert_eq!(value(&w, "Inputs", "A1"), "3");
    assert_eq!(value(&w, "Sheet1", "A1"), "3");
}
#[test]
fn copy_translates_absolute_mixed_and_sheet_refs_not_text_or_columns() {
    assert_eq!(
        rewrite_formula(
            "=SUM(A1,$A1,A$1,$A$1,'Sales Q1'!B2,Data[AB12],\"A1\")",
            2,
            3,
            None
        )
        .unwrap(),
        "=SUM(D3,$A3,D$1,$A$1,'Sales Q1'!E4,Data[AB12],\"A1\")"
    );
    assert!(rewrite_formula("=A1", -1, 0, None).is_err());
}
#[test]
fn range_copy_preserves_style_and_overlapping_copy_uses_snapshot() {
    let mut w = Workbook::new();
    w.edit_range("Sheet1", a("A1"), vec![vec!["1".into(), "=A1*2".into()]])
        .unwrap();
    let style = CellStyle {
        bold: true,
        italic: true,
        fill: Some("#abc123".into()),
        number_format: Some("0.00".into()),
    };
    w.format_range("Sheet1", range("B1"), style.clone())
        .unwrap();
    w.copy_range("Sheet1", range("A1:B1"), "Sheet1", a("B1"))
        .unwrap();
    assert_eq!(value(&w, "Sheet1", "B1"), "1");
    assert_eq!(value(&w, "Sheet1", "C1"), "2");
    assert_eq!(
        w.sheets["Sheet1"].cells["C1"].formula.as_deref(),
        Some("=B1*2")
    );
    assert_eq!(w.sheets["Sheet1"].cells["C1"].style, style);
    w.undo_sheet_edit().unwrap();
    assert_eq!(value(&w, "Sheet1", "B1"), "2");
    assert!(!w.sheets["Sheet1"].cells.contains_key("C1"));
    w.redo_sheet_edit().unwrap();
    assert_eq!(value(&w, "Sheet1", "C1"), "2");
}
#[test]
fn atomic_paste_rejects_spill_child_without_partial_edits() {
    let mut w = Workbook::new();
    set(&mut w, "Sheet1", "B2", "=SEQUENCE(2,1)");
    assert_eq!(
        w.edit_range("Sheet1", a("A3"), vec![vec!["5".into(), "6".into()]]),
        Err(CellError::Spill)
    );
    assert_eq!(value(&w, "Sheet1", "A3"), "");
    assert_eq!(value(&w, "Sheet1", "B3"), "2");
}
#[test]
fn atomic_paste_rejects_out_of_bounds_and_cycles() {
    let mut w = Workbook::new();
    assert!(w
        .edit_range(
            "Sheet1",
            CellCoord::new(999, 99),
            vec![vec!["1".into(), "2".into()]]
        )
        .is_err());
    assert!(w.sheets["Sheet1"].cells.is_empty());
    assert_eq!(
        w.edit_range("Sheet1", a("A1"), vec![vec!["=B1".into(), "=A1".into()]]),
        Err(CellError::Cycle)
    );
    assert!(w.sheets["Sheet1"].cells.is_empty());
}
#[test]
fn fill_repeats_source_and_translates_each_formula() {
    let mut w = Workbook::new();
    w.edit_range("Sheet1", a("A1"), vec![vec!["5".into(), "=A1*$D$1".into()]])
        .unwrap();
    set(&mut w, "Sheet1", "D1", "3");
    w.fill_range("Sheet1", range("A1:B1"), range("A2:B4"))
        .unwrap();
    assert_eq!(value(&w, "Sheet1", "B4"), "15");
    assert_eq!(w.sheets["Sheet1"].cells["B4"].raw_input, "=A4*$D$1");
    w.undo_sheet_edit().unwrap();
    assert_eq!(value(&w, "Sheet1", "B4"), "");
}
#[test]
fn sheet_rename_updates_references_but_not_quoted_text() {
    let mut w = Workbook::new();
    w.add_sheet("Inputs").unwrap();
    set(&mut w, "Inputs", "A1", "7");
    set(&mut w, "Sheet1", "A1", "=Inputs!A1");
    set(&mut w, "Sheet1", "A2", "=\"Inputs!A1\"");
    w.rename_sheet("Inputs", "O'Brien inputs").unwrap();
    assert_eq!(
        w.sheets["Sheet1"].cells["A1"].formula.as_deref(),
        Some("='O''Brien inputs'!A1")
    );
    assert_eq!(value(&w, "Sheet1", "A1"), "7");
    assert_eq!(value(&w, "Sheet1", "A2"), "Inputs!A1");
}
#[test]
fn styles_and_cross_sheet_formulas_persist_with_old_file_compatibility() {
    let mut w = Workbook::new();
    w.add_sheet("Inputs").unwrap();
    set(&mut w, "Inputs", "A1", "0.25");
    set(&mut w, "Sheet1", "A1", "=Inputs!A1");
    let style = CellStyle {
        bold: true,
        italic: false,
        fill: None,
        number_format: Some("0.00%".into()),
    };
    w.format_range("Sheet1", range("A1"), style.clone())
        .unwrap();
    let file = tempfile::NamedTempFile::new().unwrap();
    w.save_to_file(file.path()).unwrap();
    let loaded = Workbook::open_from_file(file.path()).unwrap();
    assert_eq!(value(&loaded, "Sheet1", "A1"), "0.25");
    assert_eq!(loaded.sheets["Sheet1"].cells["A1"].style, style);
    w.save_recovery_copy(file.path()).unwrap();
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(file.path()).unwrap()).unwrap();
    json["sheets"]["Sheet1"]["cells"]["A1"]
        .as_object_mut()
        .unwrap()
        .remove("style");
    std::fs::write(file.path(), serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(Workbook::open_from_file(file.path()).is_ok());
}
#[test]
fn plain_edit_undo_redo_and_redo_branch() {
    let mut w = Workbook::new();
    set(&mut w, "Sheet1", "A1", "1");
    set(&mut w, "Sheet1", "A1", "2");
    w.undo_sheet_edit().unwrap();
    assert_eq!(value(&w, "Sheet1", "A1"), "1");
    w.redo_sheet_edit().unwrap();
    assert_eq!(value(&w, "Sheet1", "A1"), "2");
    w.undo_sheet_edit().unwrap();
    set(&mut w, "Sheet1", "A1", "3");
    assert!(w.redo_sheet_edit().is_err());
}
#[test]
fn formatting_only_changes_display_not_values() {
    let mut w = Workbook::new();
    set(&mut w, "Sheet1", "A1", "0.125");
    w.format_range(
        "Sheet1",
        range("A1"),
        CellStyle {
            number_format: Some("0.00%".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let vp = w
        .get_viewport(&gridloom_core::virtual_view::ViewportRequest {
            sheet_name: "Sheet1".into(),
            start_row: 0,
            start_col: 0,
            end_row: 0,
            end_col: 0,
            overscan: 0,
        })
        .unwrap();
    assert_eq!(vp.cells["A1"].display_text, "12.50%");
    assert_eq!(vp.cells["A1"].raw_input.as_deref(), Some("0.125"));
    assert_eq!(value(&w, "Sheet1", "A1"), "0.125");
    assert_eq!(
        gridloom_core::spreadsheet::format_value(&Value::Blank, Some("0.00")),
        ""
    );
}
#[test]
fn cross_sheet_spills_and_children_recalculate() {
    let mut w = Workbook::new();
    w.add_sheet("Inputs").unwrap();
    set(&mut w, "Inputs", "A1", "=SEQUENCE(3)");
    set(&mut w, "Sheet1", "A1", "=SUM(Inputs!A1#)");
    set(&mut w, "Sheet1", "A2", "=Inputs!A3");
    assert_eq!(value(&w, "Sheet1", "A1"), "6");
    assert_eq!(value(&w, "Sheet1", "A2"), "3");
    set(&mut w, "Inputs", "A1", "=SEQUENCE(3,1,5)");
    assert_eq!(value(&w, "Sheet1", "A1"), "18");
    assert_eq!(value(&w, "Sheet1", "A2"), "7");
}

#[test]
fn copy_snapshot_rejects_formula_or_style_changes_even_with_equal_values() {
    let mut w = Workbook::new();
    set(&mut w, "Sheet1", "A1", "=1+1");
    let captured = w.capture_range("Sheet1", range("A1")).unwrap();
    assert_eq!(captured.values, vec![vec!["=1+1".to_string()]]);
    set(&mut w, "Sheet1", "A1", "=2");
    assert!(w
        .copy_range_snapshot_checked("Sheet1", range("A1"), "Sheet1", a("B1"), &captured.snapshot)
        .is_err());
    let captured = w.capture_range("Sheet1", range("A1")).unwrap();
    w.format_range(
        "Sheet1",
        range("A1"),
        CellStyle {
            bold: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(w
        .copy_range_snapshot_checked("Sheet1", range("A1"), "Sheet1", a("B1"), &captured.snapshot)
        .is_err());
    let captured = w.capture_range("Sheet1", range("A1")).unwrap();
    w.copy_range_snapshot_checked("Sheet1", range("A1"), "Sheet1", a("B1"), &captured.snapshot)
        .unwrap();
    assert_eq!(value(&w, "Sheet1", "B1"), "2");
}
#[test]
fn clear_history_prevents_reverting_external_mutations() {
    let mut w = Workbook::new();
    set(&mut w, "Sheet1", "A1", "1");
    w.clear_edit_history();
    assert!(w.undo_sheet_edit().is_err());
}

#[test]
fn undo_batch_can_remove_its_own_spill_without_overwriting_others() {
    let mut w = Workbook::new();
    w.edit_range(
        "Sheet1",
        a("A1"),
        vec![vec!["=SEQUENCE(2)".into()], vec!["".into()]],
    )
    .unwrap();
    assert_eq!(value(&w, "Sheet1", "A2"), "2");
    w.undo_sheet_edit().unwrap();
    assert_eq!(value(&w, "Sheet1", "A1"), "");
    assert_eq!(value(&w, "Sheet1", "A2"), "");
    w.redo_sheet_edit().unwrap();
    assert_eq!(value(&w, "Sheet1", "A2"), "2");
}
#[test]
fn unbounded_reference_ranges_are_rejected_before_dependency_expansion() {
    let mut w = Workbook::new();
    assert_eq!(
        w.set_cell("Sheet1", a("A1"), "=SUM(A2:XFD1048576)".into()),
        Err(CellError::Ref)
    );
    assert!(w.sheets["Sheet1"].cells.is_empty());
}

#[test]
fn formatting_extreme_numbers_and_huge_ranges_cannot_panic() {
    let value = Value::Decimal(rust_decimal::Decimal::MAX);
    assert_eq!(
        gridloom_core::spreadsheet::format_value(&value, Some("percent")),
        value.to_string_repr()
    );
    let mut w = Workbook::new();
    let huge = CellRange {
        start_row: 0,
        start_col: 0,
        end_row: u32::MAX,
        end_col: u32::MAX,
    };
    assert!(w
        .format_range("Sheet1", huge, CellStyle::default())
        .is_err());
    assert!(w.read_range("Sheet1", huge).is_err());
    assert!(w.sheets["Sheet1"].cells.is_empty());
}
#[test]
fn copying_spill_children_materializes_values_not_hidden_blank_inputs() {
    let mut w = Workbook::new();
    w.edit_range(
        "Sheet1",
        a("A1"),
        vec![vec!["=SEQUENCE(3)".into()], vec!["".into()]],
    )
    .unwrap();
    assert_eq!(
        w.read_range("Sheet1", range("A2:A3")).unwrap(),
        vec![vec!["2".to_string()], vec!["3".to_string()]]
    );
    w.copy_range("Sheet1", range("A2:A3"), "Sheet1", a("C1"))
        .unwrap();
    assert_eq!(value(&w, "Sheet1", "C1"), "2");
    assert_eq!(value(&w, "Sheet1", "C2"), "3");
    assert!(w
        .copy_range("Sheet1", range("C1:C2"), "Sheet1", a("A2"))
        .is_err());
}
#[test]
fn copying_readonly_source_table_materializes_literal_values_without_formula_injection() {
    let mut w = Workbook::new();
    w.add_sqlite_memory("db").unwrap();
    w.connections["db"]
        .execute_read_query("SELECT 1", &[])
        .unwrap();
    w.create_named_source("src", "db", "SELECT '=1+2' AS label, 7 AS amount")
        .unwrap();
    w.insert_table("Sheet1", "Data", "src", a("A1")).unwrap();
    w.copy_range("Sheet1", range("A1:B2"), "Sheet1", a("D1"))
        .unwrap();
    assert_eq!(value(&w, "Sheet1", "D2"), "=1+2");
    assert!(w.sheets["Sheet1"].cells["D2"].formula.is_none());
    assert_eq!(value(&w, "Sheet1", "E2"), "7");
    assert!(w
        .copy_range("Sheet1", range("D1:E2"), "Sheet1", a("A1"))
        .is_err());
}

#[test]
fn copying_entire_spill_keeps_formula_and_does_not_block_it_with_materialized_children() {
    let mut w = Workbook::new();
    set(&mut w, "Sheet1", "A1", "=SEQUENCE(3)");
    w.copy_range("Sheet1", range("A1:A3"), "Sheet1", a("C1"))
        .unwrap();
    assert_eq!(value(&w, "Sheet1", "C1"), "1");
    assert_eq!(value(&w, "Sheet1", "C3"), "3");
    assert_eq!(
        w.sheets["Sheet1"].cells["C1"].formula.as_deref(),
        Some("=SEQUENCE(3)")
    );
    w.undo_sheet_edit().unwrap();
    assert_eq!(value(&w, "Sheet1", "C1"), "");
    assert_eq!(value(&w, "Sheet1", "C3"), "");
}

#[test]
fn source_copy_snapshots_bind_types_and_copy_staged_values_as_displayed() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let db = rusqlite::Connection::open(file.path()).unwrap();
    db.execute_batch(
        "CREATE TABLE items(id INTEGER PRIMARY KEY, amount); INSERT INTO items VALUES(1,7)",
    )
    .unwrap();
    let mut w = Workbook::new();
    w.add_sqlite_connection("db", file.path()).unwrap();
    w.create_named_source("src", "db", "SELECT id,amount FROM items")
        .unwrap();
    w.insert_table("Sheet1", "Data", "src", a("A1")).unwrap();
    let old = w.capture_range("Sheet1", range("B2")).unwrap();
    db.execute_batch("UPDATE items SET amount='7'").unwrap();
    w.refresh_source("src").unwrap();
    assert_ne!(
        old.snapshot,
        w.capture_range("Sheet1", range("B2")).unwrap().snapshot
    );
    w.staged.stage_edit(
        "db".into(),
        "items".into(),
        vec!["id".into()],
        vec![Value::Integer(1)],
        "amount".into(),
        Value::Text("7".into()),
        Value::Text("proposed".into()),
    );
    assert_eq!(
        w.read_range("Sheet1", range("B2")).unwrap(),
        vec![vec!["proposed".to_string()]]
    );
    w.copy_range("Sheet1", range("B2"), "Sheet1", a("D1"))
        .unwrap();
    assert_eq!(value(&w, "Sheet1", "D1"), "proposed");
}
#[test]
fn cross_sheet_undo_restores_value_but_never_resurrects_review() {
    use gridloom_core::review::{ReviewDecision, ReviewRequest};
    let mut w = Workbook::new();
    w.add_sheet("Inputs").unwrap();
    set(&mut w, "Inputs", "A1", "10");
    set(&mut w, "Sheet1", "A1", "=Inputs!A1*2");
    let original = w.explain_cell("Sheet1", a("A1")).unwrap();
    w.record_review(ReviewRequest {
        sheet_name: "Sheet1".into(),
        row: 0,
        col: 0,
        expected_snapshot: original.snapshot.clone(),
        decision: ReviewDecision::Reviewed,
        reviewer: "Reviewer".into(),
        note: String::new(),
        evidence: vec![],
    })
    .unwrap();
    set(&mut w, "Inputs", "A1", "20");
    assert!(w.review_history()[0].invalidated);
    w.undo_sheet_edit().unwrap();
    assert_eq!(value(&w, "Sheet1", "A1"), "20");
    assert_eq!(
        w.explain_cell("Sheet1", a("A1")).unwrap().snapshot,
        original.snapshot
    );
    assert!(w.review_history()[0].invalidated);
}
#[test]
fn successful_source_refresh_clears_existing_worksheet_history() {
    let mut w = Workbook::new();
    w.add_sqlite_memory("db").unwrap();
    w.create_named_source("src", "db", "SELECT 1 AS amount")
        .unwrap();
    set(&mut w, "Sheet1", "A1", "7");
    w.refresh_source("src").unwrap();
    assert!(w.undo_sheet_edit().is_err());
    assert_eq!(value(&w, "Sheet1", "A1"), "7");
}

#[test]
fn format_attribute_preserves_mixed_styles_and_is_one_undo_operation() {
    let mut w = Workbook::new();
    w.edit_range("Sheet1", a("A1"), vec![vec!["0.5".into(), "25".into()]])
        .unwrap();
    let left = CellStyle {
        bold: false,
        italic: true,
        fill: Some("#123456".into()),
        number_format: Some("percent".into()),
    };
    let right = CellStyle {
        bold: false,
        italic: false,
        fill: Some("#abcdef".into()),
        number_format: Some("currency".into()),
    };
    w.format_range("Sheet1", range("A1"), left.clone()).unwrap();
    w.format_range("Sheet1", range("B1"), right.clone())
        .unwrap();
    w.format_range_attribute("Sheet1", range("A1:B1"), "bold", "true")
        .unwrap();
    assert_eq!(
        w.sheets["Sheet1"].cells["A1"].style,
        CellStyle {
            bold: true,
            ..left.clone()
        }
    );
    assert_eq!(
        w.sheets["Sheet1"].cells["B1"].style,
        CellStyle {
            bold: true,
            ..right.clone()
        }
    );
    w.undo_sheet_edit().unwrap();
    assert_eq!(w.sheets["Sheet1"].cells["A1"].style, left);
    assert_eq!(w.sheets["Sheet1"].cells["B1"].style, right);
    w.redo_sheet_edit().unwrap();
    w.format_range_attribute("Sheet1", range("A1:B1"), "fill", "")
        .unwrap();
    assert!(w.sheets["Sheet1"].cells["A1"].style.bold);
    assert!(w.sheets["Sheet1"].cells["A1"].style.fill.is_none());
    assert_eq!(
        w.sheets["Sheet1"].cells["B1"].number_format.as_deref(),
        Some("currency")
    );
    w.format_range_attribute("Sheet1", range("A1:B1"), "number_format", "0.00")
        .unwrap();
    assert!(w.sheets["Sheet1"].cells["A1"].style.italic);
    assert!(!w.sheets["Sheet1"].cells["B1"].style.italic);
    assert_eq!(
        w.sheets["Sheet1"].cells["B1"].number_format.as_deref(),
        Some("0.00")
    );
}
#[test]
fn invalid_format_attributes_or_collisions_reject_whole_range() {
    let mut w = Workbook::new();
    set(&mut w, "Sheet1", "B1", "=SEQUENCE(2)");
    for (attribute, value) in [
        ("bold", "yes"),
        ("fill", "#zz0000"),
        ("number_format", "bad"),
        ("unknown", "true"),
    ] {
        assert!(w
            .format_range_attribute("Sheet1", range("A1"), attribute, value)
            .is_err());
    }
    assert_eq!(
        w.format_range_attribute("Sheet1", range("A2:B2"), "bold", "true"),
        Err(CellError::Spill)
    );
    assert!(!w.sheets["Sheet1"].cells.contains_key("A2"));
    w.format_range_attribute("Sheet1", range("A1"), "italic", "true")
        .unwrap();
    assert!(w.sheets["Sheet1"].cells["A1"].style.italic);
    assert_eq!(value(&w, "Sheet1", "A1"), "");
}

#[test]
fn sheet_delete_preserves_remaining_data_and_keeps_at_least_one_sheet() {
    let mut w = Workbook::new();
    set(&mut w, "Sheet1", "B1", "24");
    w.add_sheet("Inputs").unwrap();
    set(&mut w, "Inputs", "A1", "7");
    w.delete_sheet("Inputs").unwrap();
    assert_eq!(w.active_sheet, "Sheet1");
    assert!(!w.sheets.contains_key("Inputs"));
    assert_eq!(value(&w, "Sheet1", "B1"), "24");
    assert!(w.undo_sheet_edit().is_err());
    assert!(w
        .delete_sheet("Sheet1")
        .unwrap_err()
        .contains("at least one"));
    assert!(w.delete_sheet("Missing").unwrap_err().contains("not found"));
}

#[test]
fn sheet_delete_leaves_active_sheet_and_rejects_live_formula_references() {
    let mut w = Workbook::new();
    w.add_sheet("Inputs").unwrap();
    w.add_sheet("Report").unwrap();
    set(&mut w, "Inputs", "A1", "7");
    set(&mut w, "Report", "A1", "=IF(1=1, Inputs!A1, 0)");
    set(&mut w, "Report", "B1", "=\"Inputs!A1\"");
    let generation = w.dep_graph.generation;
    assert!(w.delete_sheet("Inputs").unwrap_err().contains("Report!A1"));
    assert_eq!(w.dep_graph.generation, generation);
    assert_eq!(value(&w, "Report", "A1"), "7");
    set(&mut w, "Report", "A1", "7");
    w.delete_sheet("Inputs").unwrap();
    assert_eq!(w.active_sheet, "Report");
    assert_eq!(value(&w, "Report", "B1"), "Inputs!A1");
}
