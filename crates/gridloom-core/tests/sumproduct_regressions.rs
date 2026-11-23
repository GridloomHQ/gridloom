use gridloom_core::{functions::arithmetic::sumproduct, CellCoord, CellError, Value, Workbook};
use rust_decimal::Decimal;
fn array(rows: &[&[i64]]) -> Value {
    Value::Array(
        rows.iter()
            .map(|row| row.iter().map(|n| Value::Integer(*n)).collect())
            .collect(),
    )
}
fn evaluate(formula: &str) -> String {
    let mut w = Workbook::new();
    w.set_cell("Sheet1", CellCoord::new(0, 0), formula.into())
        .unwrap();
    match &w.sheets["Sheet1"].cells["A1"].evaluated_value {
        Value::Decimal(d) => d.normalize().to_string(),
        other => other.to_string_repr(),
    }
}
#[test]
fn sumproduct_requires_identical_nonempty_rectangular_shapes() {
    for args in [
        vec![array(&[&[1, 2]]), array(&[&[1], &[2]])],
        vec![array(&[&[1, 2], &[3, 4]]), array(&[&[1, 2, 3, 4]])],
        vec![Value::Integer(2), array(&[&[1, 2]])],
        vec![array(&[&[1], &[2, 3]])],
        vec![array(&[])],
        vec![array(&[&[]])],
        vec![Value::Array(vec![vec![array(&[&[1]])]])],
        vec![],
    ] {
        assert_eq!(sumproduct(&args), Err(CellError::Value));
    }
    assert_eq!(
        sumproduct(&[Value::Integer(2), array(&[&[3]])]).unwrap(),
        Value::Decimal(Decimal::from(6))
    );
    assert_eq!(evaluate("=SUMPRODUCT({1,2;3,4},{5,6;7,8})"), "70");
}
#[test]
fn sumproduct_propagates_errors_in_every_argument_and_cell_even_with_zero() {
    for position in 0..3 {
        for cell in 0..2 {
            let mut args = vec![array(&[&[0, 1]]); 3];
            if let Value::Array(rows) = &mut args[position] {
                rows[0][cell] = Value::Error(CellError::Div0);
            }
            assert_eq!(sumproduct(&args), Err(CellError::Div0));
        }
    }
    assert_eq!(
        sumproduct(&[Value::Text("ignored".into()), Value::Error(CellError::Ref)]),
        Err(CellError::Ref)
    );
    assert_eq!(
        sumproduct(&[Value::Error(CellError::Num)]),
        Err(CellError::Num)
    );
}
#[test]
fn sumproduct_rejects_unconvertible_numeric_floats() {
    for f in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e100] {
        assert_eq!(
            sumproduct(&[Value::Float(f), Value::Integer(0)]),
            Err(CellError::Num)
        );
        assert_eq!(
            sumproduct(&[Value::Array(vec![vec![Value::Float(f)]])]),
            Err(CellError::Num)
        );
    }
    assert_eq!(
        sumproduct(&[Value::Float(0.5), Value::Integer(2)]).unwrap(),
        Value::Decimal(Decimal::ONE)
    );
}
#[test]
fn sumproduct_preserves_existing_mixed_type_coercions() {
    let values = Value::Array(vec![vec![
        Value::Text("2".into()),
        Value::Boolean(true),
        Value::Text("text".into()),
        Value::Blank,
        Value::Null,
        Value::EmptyText,
    ]]);
    assert_eq!(
        sumproduct(&[values, array(&[&[3, 4, 5, 6, 7, 8]])]).unwrap(),
        Value::Decimal(Decimal::from(10))
    );
    let date = Value::Date(chrono::NaiveDate::from_ymd_opt(1900, 1, 1).unwrap());
    assert_eq!(
        sumproduct(&[date, Value::Integer(3)]).unwrap(),
        Value::Decimal(Decimal::from(6))
    );
}
#[test]
fn sumproduct_product_and_sum_overflow_are_num_not_panics() {
    assert_eq!(
        evaluate("=SUMPRODUCT(79228162514264337593543950335,2)"),
        "#NUM!"
    );
    assert_eq!(
        evaluate("=SUMPRODUCT({79228162514264337593543950335,1})"),
        "#NUM!"
    );
    assert_eq!(
        evaluate("=SUMPRODUCT({-79228162514264337593543950335,-1})"),
        "#NUM!"
    );
    assert_eq!(
        evaluate("=SUMPRODUCT(79228162514264337593543950335,1)"),
        "79228162514264337593543950335"
    );
}
#[test]
fn sumproduct_retains_decimal_precision_rounding_and_underflow_policy() {
    assert_eq!(evaluate("=SUMPRODUCT({0.1,0.2},{1,1})"), "0.3");
    assert_eq!(
        evaluate("=SUMPRODUCT(0.0000000000000000000000000001,0.1)"),
        "0"
    );
    assert_eq!(evaluate("=SUMPRODUCT({1,-1},{2,2})"), "0");
}
#[test]
fn sumproduct_referenced_errors_recalculate_dependents_and_invalidate_old_reviews_on_open() {
    use gridloom_core::review::{ReviewDecision, ReviewRequest};
    let mut w = Workbook::new();
    for (col, input) in [(0, "=1/0"), (1, "=SUMPRODUCT(A1,0)"), (2, "=B1+1")] {
        w.set_cell("Sheet1", CellCoord::new(0, col), input.into())
            .unwrap();
    }
    assert_eq!(
        w.sheets["Sheet1"].cells["B1"].evaluated_value,
        Value::Error(CellError::Div0)
    );
    w.set_cell("Sheet1", CellCoord::new(0, 0), "2".into())
        .unwrap();
    assert_eq!(
        w.sheets["Sheet1"].cells["C1"].evaluated_value,
        Value::Decimal(Decimal::ONE)
    );
    // Old shape-flattening behavior produced 5 and allowed a review, with no
    // upstream error warning (an upstream error correctly blocks reviews already).
    let mut w = Workbook::new();
    w.set_cell(
        "Sheet1",
        CellCoord::new(0, 1),
        "=SUMPRODUCT({1,2},{1;2})".into(),
    )
    .unwrap();
    w.set_cell("Sheet1", CellCoord::new(0, 2), "=B1+1".into())
        .unwrap();
    let cells = &mut w.sheets.get_mut("Sheet1").unwrap().cells;
    cells.get_mut("B1").unwrap().evaluated_value = Value::Integer(5);
    cells.get_mut("C1").unwrap().evaluated_value = Value::Integer(6);
    let old = w.explain_cell("Sheet1", CellCoord::new(0, 1)).unwrap();
    w.record_review(ReviewRequest {
        sheet_name: "Sheet1".into(),
        row: 0,
        col: 1,
        expected_snapshot: old.snapshot,
        decision: ReviewDecision::Reviewed,
        reviewer: "Fixture".into(),
        note: "Old flattened shape".into(),
        evidence: vec![],
    })
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.tay");
    w.save_to_file(&path).unwrap();
    let loaded = Workbook::open_from_file(&path).unwrap();
    for addr in ["B1", "C1"] {
        assert_eq!(
            loaded.sheets["Sheet1"].cells[addr].evaluated_value,
            Value::Error(CellError::Value)
        );
    }
    assert!(loaded.reviews[0].invalidated);
}
