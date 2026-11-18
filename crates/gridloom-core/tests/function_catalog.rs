use gridloom_core::ast::StructuredRef;
use gridloom_core::functions::{call_function, EvaluationContext, FUNCTION_NAMES};
use gridloom_core::{CellCoord, CellError, CellRange, Value};
use std::collections::BTreeSet;
struct NoSources;
impl EvaluationContext for NoSources {
    fn get_cell_value(&self, _: CellCoord) -> Result<Value, CellError> {
        panic!("catalog test must not read cells")
    }
    fn get_range_values(&self, _: CellRange) -> Result<Value, CellError> {
        panic!("catalog test must not read ranges")
    }
    fn get_table_column(&self, _: &StructuredRef) -> Result<Value, CellError> {
        panic!("catalog test must not read tables")
    }
    fn execute_sql_value(&self, _: &str, _: &str, _: &[Value]) -> Result<Value, CellError> {
        panic!("catalog test must not execute SQL")
    }
    fn execute_sql_table(&self, _: &str, _: &str, _: &[Value]) -> Result<Value, CellError> {
        panic!("catalog test must not execute SQL")
    }
}
#[test]
fn all_advertised_names_dispatch_and_accept_lowercase() {
    let names: BTreeSet<_> = FUNCTION_NAMES.iter().copied().collect();
    assert_eq!(names.len(), FUNCTION_NAMES.len());
    assert_eq!(names.len(), 95);
    for name in FUNCTION_NAMES {
        let upper = call_function(name, &[], &NoSources);
        let lower = call_function(&name.to_lowercase(), &[], &NoSources);
        assert_ne!(upper, Err(CellError::Name), "{name} must be implemented");
        if *name == "NOW" {
            assert!(matches!(upper, Ok(Value::Timestamp(_))));
            assert!(matches!(lower, Ok(Value::Timestamp(_))));
        } else {
            assert_eq!(upper, lower, "{name} case behavior changed");
        }
    }
    assert_eq!(
        call_function("NOT_IMPLEMENTED", &[], &NoSources),
        Err(CellError::Name)
    );
}
#[test]
fn aliases_and_evaluator_special_functions_are_advertised() {
    for name in [
        "CEILING",
        "CEILING.MATH",
        "FLOOR",
        "FLOOR.MATH",
        "STDEV",
        "STDEV.S",
        "STDEVP",
        "STDEV.P",
        "VAR",
        "VAR.S",
        "VARP",
        "VAR.P",
        "CONCAT",
        "CONCATENATE",
        "IF",
        "IFS",
        "IFERROR",
        "IFNA",
        "SQLVALUE",
        "SQLTABLE",
    ] {
        assert!(FUNCTION_NAMES.contains(&name), "missing {name}");
    }
    for (canonical, alias) in [
        ("CEILING.MATH", "CEILING"),
        ("FLOOR.MATH", "FLOOR"),
        ("STDEV.S", "STDEV"),
        ("STDEV.P", "STDEVP"),
        ("VAR.S", "VAR"),
        ("VAR.P", "VARP"),
        ("CONCAT", "CONCATENATE"),
    ] {
        let args = [Value::Integer(3), Value::Integer(2)];
        assert_eq!(
            call_function(canonical, &args, &NoSources),
            call_function(alias, &args, &NoSources)
        );
    }
}
