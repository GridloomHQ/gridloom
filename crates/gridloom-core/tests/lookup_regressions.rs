use gridloom_core::{CellCoord, Workbook};
fn evaluate(formula: &str) -> String {
    let mut wb = Workbook::new();
    wb.set_cell("Sheet1", CellCoord::new(0, 0), formula.into())
        .unwrap();
    wb.sheets["Sheet1"].cells["A1"]
        .evaluated_value
        .to_string_repr()
}
#[test]
fn xlookup_approximate_returns_nearest_smaller_not_first_smaller() {
    assert_eq!(
        evaluate("=XLOOKUP(25,{10;20;30},{100;200;300},0,-1)"),
        "200"
    );
}
#[test]
fn xlookup_approximate_prioritizes_exact_match_even_if_later() {
    assert_eq!(
        evaluate("=XLOOKUP(20,{10;20;30},{100;200;300},0,-1)"),
        "200"
    );
}
#[test]
fn legacy_approximate_lookups_work_under_their_sorted_input_contracts() {
    assert_eq!(
        evaluate("=VLOOKUP(25,{10,100;20,200;30,300},2,TRUE)"),
        "200"
    );
    assert_eq!(evaluate("=MATCH(25,{10;20;30},1)"), "2");
    assert_eq!(evaluate("=MATCH(25,{30;20;10},-1)"), "1");
}

#[test]
fn vlookup_out_of_bounds_return_column_is_reference_error_not_blank() {
    assert_eq!(evaluate("=VLOOKUP(10,{10,100;20,200},3,FALSE)"), "#REF!");
}

#[test]
fn xlookup_mismatched_axis_returns_value_error_not_blank() {
    assert_eq!(evaluate("=XLOOKUP(20,{10;20},{100})"), "#VALUE!");
}
#[test]
fn xlookup_horizontal_lookup_projects_a_column_from_multirow_return() {
    use gridloom_core::Value;
    let mut wb = Workbook::new();
    wb.set_cell(
        "Sheet1",
        CellCoord::new(0, 0),
        "=XLOOKUP(20,{10,20},{100,200;300,400})".into(),
    )
    .unwrap();
    assert_eq!(
        wb.sheets["Sheet1"].cells["A1"].evaluated_value,
        Value::Array(vec![vec![Value::Integer(200)], vec![Value::Integer(400)]])
    );
}
#[test]
fn xlookup_mixed_large_integer_float_does_not_false_match() {
    use gridloom_core::Value;
    let mut wb = Workbook::new();
    for (coord, input) in [
        (CellCoord::new(0, 0), "9007199254740992"),
        (CellCoord::new(1, 0), "9007199254740993"),
        (CellCoord::new(0, 1), "wrong"),
        (CellCoord::new(1, 1), "right"),
    ] {
        wb.set_cell("Sheet1", coord, input.into()).unwrap();
    }
    wb.sheets
        .get_mut("Sheet1")
        .unwrap()
        .cells
        .get_mut("A1")
        .unwrap()
        .evaluated_value = Value::Float(9007199254740992.0);
    wb.set_cell(
        "Sheet1",
        CellCoord::new(0, 2),
        "=XLOOKUP(9007199254740993,A1:A2,B1:B2)".into(),
    )
    .unwrap();
    assert_eq!(
        wb.sheets["Sheet1"].cells["C1"]
            .evaluated_value
            .to_string_repr(),
        "right"
    );
}

#[test]
fn xlookup_unsorted_nearest_larger_smaller_and_duplicate_direction() {
    assert_eq!(
        evaluate("=XLOOKUP(25,{30;10;20},{300;100;200},0,-1)"),
        "200"
    );
    assert_eq!(evaluate("=XLOOKUP(25,{30;10;20},{300;100;200},0,1)"), "300");
    assert_eq!(evaluate("=XLOOKUP(20,{30;10;20},{300;100;200},0,1)"), "200");
    assert_eq!(evaluate("=XLOOKUP(25,{20;20;10},{1;2;3},0,-1,-1)"), "2");
    assert_eq!(evaluate("=XLOOKUP(20,{20;20;10},{1;2;3},0,0,-1)"), "2");
    assert_eq!(evaluate("=XLOOKUP(5,{20;20;10},{1;2;3},99,-1)"), "99");
    assert_eq!(evaluate("=XLOOKUP(5,{20;20;10},{1;2;3})"), "#N/A");
}
#[test]
fn xlookup_rejects_unsupported_fractional_modes_and_shapes() {
    for formula in [
        "=XLOOKUP(10,{10;20},{1;2},0,2)",
        "=XLOOKUP(10,{10;20},{1;2},0,0,2)",
        "=XLOOKUP(10,{10;20},{1;2},0,0,-2)",
        "=XLOOKUP(10,{10;20},{1;2},0,0.5)",
        "=XLOOKUP(10,{10;20},{1;2},0,0,1.5)",
        "=XLOOKUP(10,{10,20;30,40},{1,2;3,4})",
    ] {
        assert_eq!(evaluate(formula), "#VALUE!", "{formula}");
    }
}
#[test]
fn xlookup_comparison_is_precise_and_conservatively_handles_floats() {
    use gridloom_core::functions::lookup::xlookup;
    use gridloom_core::{CellError, Value};
    let column = |values: Vec<Value>| Value::Array(values.into_iter().map(|v| vec![v]).collect());
    let result = Value::Integer(7);
    for float in [0.5, -0.5, 0.0, -0.0, 9007199254740992.0] {
        let exact = Value::Decimal(rust_decimal::Decimal::from_f64_retain(float).unwrap());
        assert_eq!(
            xlookup(&[
                exact,
                column(vec![Value::Float(float)]),
                column(vec![result.clone()])
            ])
            .unwrap(),
            result
        );
    }
    assert_eq!(
        xlookup(&[
            Value::Decimal("0.1".parse().unwrap()),
            column(vec![Value::Float(0.1)]),
            column(vec![result.clone()])
        ]),
        Err(CellError::Num)
    );
    for float in [f64::from_bits(1), 1e100] {
        assert_eq!(
            xlookup(&[
                Value::Integer(0),
                column(vec![Value::Float(float)]),
                column(vec![result.clone()])
            ]),
            Err(CellError::Num)
        );
    }
    for float in [0.1, 1e100] {
        assert_eq!(
            xlookup(&[
                Value::Float(float),
                column(vec![Value::Float(float)]),
                column(vec![result.clone()])
            ])
            .unwrap(),
            result
        );
    }
    for float in [f64::NAN, f64::INFINITY] {
        assert_eq!(
            xlookup(&[
                Value::Float(float),
                column(vec![Value::Integer(1)]),
                column(vec![result.clone()])
            ]),
            Err(CellError::Num)
        );
    }
}
#[test]
fn xlookup_errors_follow_search_direction_and_only_selected_return_or_fallback() {
    use gridloom_core::functions::lookup::xlookup;
    use gridloom_core::{CellError, Value};
    let column = |values: Vec<Value>| Value::Array(values.into_iter().map(|v| vec![v]).collect());
    let one = Value::Integer(1);
    let two = Value::Integer(2);
    let error = Value::Error(CellError::Div0);
    let mut args = vec![
        one.clone(),
        column(vec![one.clone(), error.clone()]),
        column(vec![two.clone(), error.clone()]),
        error.clone(),
    ];
    assert_eq!(xlookup(&args).unwrap(), two);
    args.extend([Value::Integer(0), Value::Integer(-1)]);
    assert_eq!(xlookup(&args), Err(CellError::Div0));
    args[1] = column(vec![error.clone(), one.clone()]);
    args[2] = column(vec![error.clone(), two.clone()]);
    assert_eq!(xlookup(&args).unwrap(), two);
    args[0] = Value::Integer(3);
    assert_eq!(xlookup(&args), Err(CellError::Div0));
    assert_eq!(
        xlookup(&[
            one.clone(),
            column(vec![one.clone()]),
            column(vec![error.clone()])
        ])
        .unwrap(),
        error
    );
    assert_eq!(
        xlookup(&[
            Value::Integer(3),
            column(vec![one.clone()]),
            column(vec![two]),
            error.clone()
        ])
        .unwrap(),
        error
    );
    assert_eq!(
        xlookup(&[error.clone(), column(vec![one.clone()]), column(vec![one])]),
        Err(CellError::Div0)
    );
}
#[test]
fn reopened_old_lookup_cache_is_recalculated_and_old_review_invalidated() {
    use gridloom_core::{
        review::{ReviewDecision, ReviewRequest},
        Value,
    };
    let mut wb = Workbook::new();
    let target = CellCoord::new(0, 0);
    wb.set_cell(
        "Sheet1",
        target,
        "=XLOOKUP(25,{10;20;30},{100;200;300},0,-1)".into(),
    )
    .unwrap();
    // Simulate a package produced by the old evaluator, not a current user workbook.
    wb.sheets
        .get_mut("Sheet1")
        .unwrap()
        .cells
        .get_mut("A1")
        .unwrap()
        .evaluated_value = Value::Integer(100);
    let old = wb.explain_cell("Sheet1", target).unwrap();
    wb.record_review(ReviewRequest {
        sheet_name: "Sheet1".into(),
        row: 0,
        col: 0,
        expected_snapshot: old.snapshot,
        decision: ReviewDecision::Reviewed,
        reviewer: "Fixture reviewer".into(),
        note: "Old evaluator result".into(),
        evidence: vec![],
    })
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.tay");
    wb.save_to_file(&path).unwrap();
    let loaded = Workbook::open_from_file(&path).unwrap();
    assert_eq!(
        loaded.sheets["Sheet1"].cells["A1"]
            .evaluated_value
            .to_string_repr(),
        "200"
    );
    assert!(loaded.reviews[0].invalidated);
}
