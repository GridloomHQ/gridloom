use gridloom_core::functions::arithmetic::{ceiling_math, floor_math};
use gridloom_core::{CellCoord, CellError, Value, Workbook};
use rust_decimal::Decimal;
fn evaluate(formula: &str) -> String {
    let mut w = Workbook::new();
    w.set_cell("Sheet1", CellCoord::new(0, 0), formula.into())
        .unwrap();
    match &w.sheets["Sheet1"].cells["A1"].evaluated_value {
        Value::Decimal(d) => d.normalize().to_string(),
        v => v.to_string_repr(),
    }
}
#[test]
fn tiny_values_do_not_disappear_before_direction_is_chosen() {
    for (fun, sign, mode, expected) in [
        ("CEILING.MATH", "", 0, "10"),
        ("FLOOR.MATH", "", 0, "0"),
        ("CEILING.MATH", "-", 0, "0"),
        ("FLOOR.MATH", "-", 0, "-10"),
        ("CEILING.MATH", "-", 1, "-10"),
        ("FLOOR.MATH", "-", 1, "0"),
        ("CEILING.MATH", "-", -1, "-10"),
        ("FLOOR.MATH", "-", -1, "0"),
    ] {
        assert_eq!(
            evaluate(&format!(
                "={fun}({sign}0.0000000000000000000000000001,10,{mode})"
            )),
            expected
        );
    }
}
#[test]
fn exact_multiples_skip_overflowing_quotients() {
    for fun in ["CEILING.MATH", "FLOOR.MATH"] {
        for sign in ["", "-"] {
            assert_eq!(
                evaluate(&format!(
                    "={fun}({sign}79228162514264337593543950335,0.0000000000000000000000000001)"
                )),
                format!("{sign}79228162514264337593543950335")
            );
        }
    }
}
#[test]
fn final_overflow_or_unrepresentable_result_is_num() {
    assert_eq!(
        evaluate("=CEILING.MATH(79228162514264337593543950335,10)"),
        "#NUM!"
    );
    assert_eq!(
        evaluate("=FLOOR.MATH(79228162514264337593543950335,10)"),
        "79228162514264337593543950330"
    );
    assert_eq!(
        evaluate("=FLOOR.MATH(-79228162514264337593543950335,10)"),
        "#NUM!"
    );
    assert_eq!(
        evaluate("=CEILING.MATH(-79228162514264337593543950335,10)"),
        "-79228162514264337593543950330"
    );
    assert_eq!(
        evaluate("=FLOOR.MATH(10000000000000000000000000001,1.5)"),
        "#NUM!"
    );
}
#[test]
fn representable_final_result_survives_unrepresentable_naive_intermediates() {
    assert_eq!(
        evaluate("=CEILING.MATH(0.0000000000000000000000000001,79228162514264337593543950335)"),
        "79228162514264337593543950335"
    );
    assert_eq!(
        evaluate("=FLOOR.MATH(-0.0000000000000000000000000001,79228162514264337593543950335)"),
        "-79228162514264337593543950335"
    );
    assert_eq!(
        evaluate("=CEILING.MATH(10000000000000000000000000001,1.5)"),
        "10000000000000000000000000002"
    );
    assert_eq!(
        evaluate("=CEILING.MATH(10000000000000000000000000000,1.0000000000000000000000000001)"),
        "10000000000000000000000000001"
    );
    assert_eq!(
        evaluate("=FLOOR.MATH(-10000000000000000000000000000,1.0000000000000000000000000001)"),
        "-10000000000000000000000000001"
    );
}
#[test]
fn modes_defaults_significance_sign_zero_and_error_coercion_are_preserved() {
    for fun in ["CEILING.MATH", "FLOOR.MATH"] {
        assert_eq!(evaluate(&format!("={fun}(1,0,1/0)")), "0");
        assert_eq!(evaluate(&format!("={fun}(1/0,0)")), "#DIV/0!");
        assert_eq!(evaluate(&format!("={fun}(1,1,1/0)")), "#DIV/0!");
        assert_eq!(evaluate(&format!("={fun}(1,1/0)")), "#DIV/0!");
        assert_eq!(evaluate(&format!("={fun}(0,-3)")), "0");
    }
    assert_eq!(evaluate("=CEILING.MATH(1.1)"), "2");
    assert_eq!(evaluate("=FLOOR.MATH(1.1)"), "1");
    assert_eq!(evaluate("=CEILING.MATH(-1.1,-1,0.9)"), "-1");
    assert_eq!(evaluate("=FLOOR.MATH(-1.1,-1,1.9)"), "-1");
    for f in [ceiling_math, floor_math] {
        assert_eq!(f(&[]), Err(CellError::Value));
        assert_eq!(
            f(&[Value::Integer(2), Value::Blank]).unwrap(),
            Value::Decimal(Decimal::from(2))
        );
        assert_eq!(f(&[Value::Float(f64::INFINITY)]), Err(CellError::Num));
    }
}
#[test]
fn signed_mixed_scale_results_match_integer_rational_oracle() {
    for n in [-9999i128, -21, -1, 0, 1, 21, 9999] {
        for sig in [1i128, 7, 123] {
            for sn in 0..=4 {
                for ss in 0..=4 {
                    for mode in [-1i64, 0, 1] {
                        for ceiling in [false, true] {
                            let scale = sn.max(ss);
                            let ni = n * 10i128.pow(scale - sn);
                            let si = sig * 10i128.pow(scale - ss);
                            let rem = ni % si;
                            let away = if ni < 0 {
                                if ceiling {
                                    mode != 0
                                } else {
                                    mode == 0
                                }
                            } else {
                                ceiling
                            };
                            let expected = ni - rem
                                + if rem != 0 && away {
                                    if ni < 0 {
                                        -si
                                    } else {
                                        si
                                    }
                                } else {
                                    0
                                };
                            let args = [
                                Value::Decimal(Decimal::from_i128_with_scale(n, sn)),
                                Value::Decimal(Decimal::from_i128_with_scale(sig, ss)),
                                Value::Integer(mode),
                            ];
                            let actual = if ceiling {
                                ceiling_math(&args)
                            } else {
                                floor_math(&args)
                            }
                            .unwrap();
                            assert_eq!(
                                actual,
                                Value::Decimal(Decimal::from_i128_with_scale(expected, scale)),
                                "n{n} sig{sig} sn{sn} ss{ss} mode{mode} ceiling{ceiling}"
                            );
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn old_tiny_rounding_cache_recalculates_and_invalidates_reviews() {
    use gridloom_core::review::{ReviewDecision, ReviewRequest};
    let mut w = Workbook::new();
    w.set_cell(
        "Sheet1",
        CellCoord::new(0, 0),
        "=CEILING.MATH(0.0000000000000000000000000001,10)".into(),
    )
    .unwrap();
    w.set_cell("Sheet1", CellCoord::new(0, 1), "=A1+1".into())
        .unwrap();
    let cells = &mut w.sheets.get_mut("Sheet1").unwrap().cells;
    cells.get_mut("A1").unwrap().evaluated_value = Value::Integer(0);
    cells.get_mut("B1").unwrap().evaluated_value = Value::Integer(1);
    let old = w.explain_cell("Sheet1", CellCoord::new(0, 0)).unwrap();
    w.record_review(ReviewRequest {
        sheet_name: "Sheet1".into(),
        row: 0,
        col: 0,
        expected_snapshot: old.snapshot,
        decision: ReviewDecision::Reviewed,
        reviewer: "Fixture".into(),
        note: "Old rounded result".into(),
        evidence: vec![],
    })
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.gridloom");
    w.save_to_file(&path).unwrap();
    let loaded = Workbook::open_from_file(&path).unwrap();
    assert_eq!(
        loaded.sheets["Sheet1"].cells["A1"].evaluated_value,
        Value::Decimal(Decimal::from(10))
    );
    assert_eq!(
        loaded.sheets["Sheet1"].cells["B1"].evaluated_value,
        Value::Decimal(Decimal::from(11))
    );
    assert!(loaded.reviews[0].invalidated);
}
