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
fn rounding_preserves_half_even_and_signed_directions() {
    for (n, expected) in [
        ("1.25", "1.2"),
        ("1.35", "1.4"),
        ("-1.25", "-1.2"),
        ("-1.35", "-1.4"),
        ("25", "20"),
        ("35", "40"),
        ("-25", "-20"),
        ("-35", "-40"),
    ] {
        let digits = if n.contains('.') { 1 } else { -1 };
        assert_eq!(evaluate(&format!("=ROUND({n},{digits})")), expected);
    }
    for (fun, pos, neg) in [
        ("ROUNDUP", "1.3", "-1.3"),
        ("ROUNDDOWN", "1.2", "-1.2"),
        ("TRUNC", "1.2", "-1.2"),
    ] {
        assert_eq!(evaluate(&format!("={fun}(1.25,1)")), pos);
        assert_eq!(evaluate(&format!("={fun}(-1.25,1)")), neg);
    }
}
#[test]
fn high_precision_never_overflows_or_wraps_digits() {
    for fun in ["ROUND", "ROUNDUP", "ROUNDDOWN", "TRUNC"] {
        for digits in [19i64, 28, 29, 4294967296, i64::MAX] {
            assert_eq!(evaluate(&format!("={fun}(1.25,{digits})")), "1.25");
            assert_eq!(
                evaluate(&format!("={fun}(79228162514264337593543950335,{digits})")),
                "79228162514264337593543950335"
            );
        }
    }
}
#[test]
fn negative_extreme_precision_returns_correct_zero_or_num_without_panics() {
    for fun in ["ROUND", "ROUNDUP", "ROUNDDOWN", "TRUNC"] {
        for digits in [-19i64, -28, -29, -4294967296, i64::MIN] {
            let expected = if fun != "ROUNDUP" {
                "0".to_string()
            } else if digits >= -28 {
                format!("1{}", "0".repeat((-digits) as usize))
            } else {
                "#NUM!".into()
            };
            assert_eq!(evaluate(&format!("={fun}(1,{digits})")), expected);
            assert_eq!(evaluate(&format!("={fun}(0,{digits})")), "0");
        }
    }
}
#[test]
fn tiny_nonzero_remainders_are_not_lost_before_rounding() {
    for sign in ["", "-"] {
        assert_eq!(
            evaluate(&format!(
                "=ROUNDUP({sign}0.0000000000000000000000000001,-1)"
            )),
            format!("{sign}10")
        );
        for fun in ["ROUND", "ROUNDDOWN", "TRUNC"] {
            assert_eq!(
                evaluate(&format!("={fun}({sign}0.0000000000000000000000000001,-1)")),
                "0"
            );
        }
        assert_eq!(
            evaluate(&format!(
                "=ROUNDUP({sign}0.0000000000000000000000000001,27)"
            )),
            format!("{sign}0.000000000000000000000000001")
        );
    }
}
#[test]
fn unrepresentable_result_is_num_but_representable_max_is_not_scaled() {
    for sign in ["", "-"] {
        for fun in ["ROUND", "ROUNDUP", "ROUNDDOWN", "TRUNC"] {
            let n = format!("{sign}79228162514264337593543950335");
            assert_eq!(evaluate(&format!("={fun}({n},1)")), n);
            let expected = if fun == "ROUND" || fun == "ROUNDUP" {
                "#NUM!".into()
            } else {
                format!("{sign}79228162514264337593543950330")
            };
            assert_eq!(evaluate(&format!("={fun}({n},-1)")), expected);
        }
    }
}
#[test]
fn existing_digits_coercion_defaults_and_errors_are_preserved() {
    assert_eq!(evaluate("=ROUND(1.25,1.9)"), "1.2");
    assert_eq!(evaluate("=ROUNDUP(11,-1.9)"), "20");
    assert_eq!(evaluate("=TRUNC(-1.9)"), "-1");
    for fun in ["ROUND", "ROUNDUP", "ROUNDDOWN", "TRUNC"] {
        assert_eq!(evaluate(&format!("={fun}(1,1/0)")), "#DIV/0!");
        assert_eq!(evaluate(&format!("={fun}(1/0,1)")), "#DIV/0!");
    }
}
#[test]
fn reopened_old_rounding_cache_recalculates_and_invalidates_review() {
    use gridloom_core::{
        review::{ReviewDecision, ReviewRequest},
        Value,
    };
    let mut wb = Workbook::new();
    let target = CellCoord::new(0, 0);
    wb.set_cell(
        "Sheet1",
        target,
        "=ROUNDUP(0.0000000000000000000000000001,-1)".into(),
    )
    .unwrap();
    wb.sheets
        .get_mut("Sheet1")
        .unwrap()
        .cells
        .get_mut("A1")
        .unwrap()
        .evaluated_value = Value::Integer(0);
    let old = wb.explain_cell("Sheet1", target).unwrap();
    wb.record_review(ReviewRequest {
        sheet_name: "Sheet1".into(),
        row: 0,
        col: 0,
        expected_snapshot: old.snapshot,
        decision: ReviewDecision::Reviewed,
        reviewer: "Fixture reviewer".into(),
        note: "Old rounded result".into(),
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
        "10"
    );
    assert!(loaded.reviews[0].invalidated);
}

#[test]
fn scale_28_drop_29_halfway_boundary_and_drop_30_are_exact() {
    for sign in ["", "-"] {
        for (n, expected) in [
            ("4.9999999999999999999999999999", "0"),
            ("5.0000000000000000000000000000", "0"),
            ("5.0000000000000000000000000001", "10"),
        ] {
            let result = if expected == "0" {
                "0".into()
            } else {
                format!("{sign}{expected}")
            };
            assert_eq!(evaluate(&format!("=ROUND({sign}{n},-1)")), result);
            assert_eq!(evaluate(&format!("=ROUND({sign}{n},-2)")), "0");
            assert_eq!(
                evaluate(&format!("=ROUNDUP({sign}{n},-2)")),
                format!("{sign}100")
            );
        }
    }
}
