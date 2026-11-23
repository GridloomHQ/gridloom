use gridloom_core::{CellCoord, Workbook};
fn evaluate(formula: &str) -> String {
    let mut wb = Workbook::new();
    wb.set_cell("Sheet1", CellCoord::new(0, 0), formula.into())
        .unwrap();
    match &wb.sheets["Sheet1"].cells["A1"].evaluated_value {
        gridloom_core::Value::Decimal(d) => d.normalize().to_string(),
        other => other.to_string_repr(),
    }
}
#[test]
fn product_overflow_returns_num_and_standard_decimal_behavior_is_preserved() {
    for sign in ["", "-"] {
        assert_eq!(
            evaluate(&format!("=PRODUCT({sign}79228162514264337593543950335,2)")),
            "#NUM!"
        );
        assert_eq!(
            evaluate(&format!("=PRODUCT({sign}79228162514264337593543950335,1)")),
            format!("{sign}79228162514264337593543950335")
        );
    }
    assert_eq!(evaluate("=PRODUCT(1.25,0.2)"), "0.25");
    assert_eq!(
        evaluate("=PRODUCT(0.0000000000000000000000000001,2)"),
        "0.0000000000000000000000000002"
    );
    // Existing decimal rounding/underflow is retained; this is not exact-product semantics.
    assert_eq!(
        evaluate("=PRODUCT(0.0000000000000000000000000001,0.1)"),
        "0"
    );
}
#[test]
fn product_unconvertible_float_is_not_silently_skipped_and_other_types_stay_ignored() {
    use gridloom_core::{functions::arithmetic::product, CellError, Value};
    for f in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e100] {
        assert_eq!(
            product(&[Value::Integer(2), Value::Float(f)]),
            Err(CellError::Num)
        );
    }
    assert_eq!(
        product(&[Value::Text("2".into()), Value::Blank, Value::Boolean(true)])
            .unwrap()
            .to_string_repr(),
        "0"
    );
    assert_eq!(
        product(&[Value::Integer(2), Value::Float(0.5)])
            .unwrap()
            .to_string_repr(),
        "1.0"
    );
    assert_eq!(
        product(&[Value::Error(CellError::Ref)]),
        Err(CellError::Ref)
    );
}
#[test]
fn mod_does_not_construct_an_overflowing_quotient() {
    for sign in ["", "-"] {
        for divisor in ["0.1", "0.0000000000000000000000000001"] {
            assert_eq!(
                evaluate(&format!(
                    "=MOD({sign}79228162514264337593543950335,{divisor})"
                )),
                "0"
            );
        }
    }
    assert_eq!(evaluate("=MOD(0.3,0.1)"), "0");
}
#[test]
fn mod_sign_and_exact_correction_respect_divisor() {
    for (n, d, expected) in [
        ("3", "2", "1"),
        ("-3", "2", "1"),
        ("3", "-2", "-1"),
        ("-3", "-2", "-1"),
        ("79228162514264337593543950335", "10", "5"),
        ("-79228162514264337593543950335", "10", "5"),
    ] {
        assert_eq!(evaluate(&format!("=MOD({n},{d})")), expected);
    }
    assert_eq!(
        evaluate("=MOD(-0.0000000000000000000000000001,1)"),
        "0.9999999999999999999999999999"
    );
    assert_eq!(
        evaluate("=MOD(0.0000000000000000000000000001,-1)"),
        "-0.9999999999999999999999999999"
    );
    for (n, d) in [
        (
            "-0.0000000000000000000000000001",
            "79228162514264337593543950335",
        ),
        (
            "0.0000000000000000000000000001",
            "-79228162514264337593543950335",
        ),
    ] {
        assert_eq!(evaluate(&format!("=MOD({n},{d})")), "#NUM!");
    }
}
#[test]
fn mod_matches_integer_rational_oracle_across_signed_mixed_scales() {
    use gridloom_core::{functions::arithmetic::mod_fn, Value};
    use rust_decimal::Decimal;
    for a in [-99999i128, -21, -1, 0, 1, 21, 99999] {
        for b in [-123i128, -7, 7, 123] {
            for sa in 0..=6 {
                for sb in 0..=6 {
                    let scale = sa.max(sb);
                    let n = a * 10i128.pow(scale - sa);
                    let d = b * 10i128.pow(scale - sb);
                    let mut r = n % d;
                    if r != 0 && r.signum() != d.signum() {
                        r += d;
                    }
                    let expected = Decimal::from_i128_with_scale(r, scale);
                    let actual = mod_fn(&[
                        Value::Decimal(Decimal::from_i128_with_scale(a, sa)),
                        Value::Decimal(Decimal::from_i128_with_scale(b, sb)),
                    ])
                    .unwrap();
                    assert_eq!(
                        actual,
                        Value::Decimal(expected),
                        "a={a} b={b} sa={sa} sb={sb}"
                    );
                }
            }
        }
    }
}
#[test]
fn mod_errors_propagate_without_panics() {
    assert_eq!(evaluate("=MOD(1,0)"), "#DIV/0!");
    assert_eq!(evaluate("=MOD(1/0,2)"), "#DIV/0!");
    assert_eq!(evaluate("=MOD(1,1/0)"), "#DIV/0!");
}
#[test]
fn reopened_old_mod_cache_invalidates_review_and_recalculates_dependents() {
    use gridloom_core::{
        review::{ReviewDecision, ReviewRequest},
        Value,
    };
    let mut wb = Workbook::new();
    let target = CellCoord::new(0, 0);
    wb.set_cell(
        "Sheet1",
        target,
        "=MOD(-0.0000000000000000000000000001,79228162514264337593543950335)".into(),
    )
    .unwrap();
    wb.set_cell("Sheet1", CellCoord::new(0, 1), "=A1+1".into())
        .unwrap();
    wb.sheets
        .get_mut("Sheet1")
        .unwrap()
        .cells
        .get_mut("A1")
        .unwrap()
        .evaluated_value = Value::Integer(0);
    wb.sheets
        .get_mut("Sheet1")
        .unwrap()
        .cells
        .get_mut("B1")
        .unwrap()
        .evaluated_value = Value::Integer(1);
    let old = wb.explain_cell("Sheet1", target).unwrap();
    wb.record_review(ReviewRequest {
        sheet_name: "Sheet1".into(),
        row: 0,
        col: 0,
        expected_snapshot: old.snapshot,
        decision: ReviewDecision::Reviewed,
        reviewer: "Fixture".into(),
        note: "Old wrong MOD".into(),
        evidence: vec![],
    })
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.tay");
    wb.save_to_file(&path).unwrap();
    let loaded = Workbook::open_from_file(&path).unwrap();
    assert!(loaded.reviews[0].invalidated);
    for addr in ["A1", "B1"] {
        assert_eq!(
            loaded.sheets["Sheet1"].cells[addr]
                .evaluated_value
                .to_string_repr(),
            "#NUM!"
        );
    }
}

#[test]
fn mod_normalizes_redundant_scale_before_alignment() {
    assert_eq!(
        evaluate("=MOD(-1.0000000000000000000000000000,10000000000000000000000000000)"),
        "9999999999999999999999999999"
    );
    assert_eq!(
        evaluate("=MOD(1.0000000000000000000000000000,-10000000000000000000000000000)"),
        "-9999999999999999999999999999"
    );
}
