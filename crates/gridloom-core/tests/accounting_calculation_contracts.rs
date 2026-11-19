use chrono::NaiveDate;
use gridloom_core::functions::EvaluationContext;
use gridloom_core::workbook::WorkbookEvalContext;
use gridloom_core::{CellCoord, CellError, Value, Workbook};
use rust_decimal::Decimal;

fn input(book: &mut Workbook, sheet: &str, cell: &str, text: &str) {
    book.set_cell(sheet, CellCoord::from_a1(cell).unwrap(), text.into())
        .unwrap();
}

fn value(book: &Workbook, sheet: &str, cell: &str) -> Value {
    let coord = CellCoord::from_a1(cell).unwrap();
    WorkbookEvalContext {
        workbook: book,
        sheet_name: sheet,
        current_coord: coord,
    }
    .get_cell_value(coord)
    .unwrap()
}

fn exact(book: &Workbook, sheet: &str, cell: &str, expected: &str) {
    let v = value(book, sheet, cell);
    assert!(
        matches!(v, Value::Decimal(_) | Value::Integer(_)),
        "{sheet}!{cell} did not retain an exact type: {v:?}"
    );
    assert_eq!(
        v.to_decimal().unwrap(),
        Decimal::from_str_exact(expected).unwrap()
    );
}

#[test]
fn close_recalculates_large_decimal_reconciliation_across_sheets_after_reopen() {
    let mut book = Workbook::new();
    book.add_sheet("Opening balance").unwrap();
    book.add_sheet("Adjustments").unwrap();
    input(&mut book, "Opening balance", "A1", "9007199254740993.21");
    input(&mut book, "Adjustments", "A1", "0.02");
    input(&mut book, "Adjustments", "A2", "-9007199254740993.10");

    input(
        &mut book,
        "Sheet1",
        "A1",
        "='Opening balance'!$A$1+Adjustments!A1",
    );
    input(&mut book, "Sheet1", "A2", "=A1+Adjustments!A2");
    input(&mut book, "Sheet1", "A3", "=A2*100");
    exact(&book, "Sheet1", "A1", "9007199254740993.23");
    exact(&book, "Sheet1", "A2", "0.13");
    exact(&book, "Sheet1", "A3", "13");

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("close.gridloom");
    book.save_to_file(&file).unwrap();
    let mut reopened = Workbook::open_from_file(&file).unwrap();
    exact(&reopened, "Sheet1", "A2", "0.13");
    input(&mut reopened, "Adjustments", "A1", "-0.12");
    exact(&reopened, "Sheet1", "A1", "9007199254740993.09");
    exact(&reopened, "Sheet1", "A2", "-0.01");
    exact(&reopened, "Sheet1", "A3", "-1");
}

#[test]
fn blank_zero_and_errors_remain_distinct_in_close_checks() {
    let mut book = Workbook::new();
    input(&mut book, "Sheet1", "A2", "0");
    input(&mut book, "Sheet1", "A3", "=1/0");
    input(&mut book, "Sheet1", "B1", "=ISBLANK(A1)");
    input(&mut book, "Sheet1", "B2", "=ISBLANK(A2)");
    input(&mut book, "Sheet1", "B3", "=COUNTBLANK(A1:A2)");
    input(&mut book, "Sheet1", "B4", "=IFERROR(A3,0.01)");
    input(&mut book, "Sheet1", "B5", "=IFNA(A3,0.01)");
    assert_eq!(value(&book, "Sheet1", "B1"), Value::Boolean(true));
    assert_eq!(value(&book, "Sheet1", "B2"), Value::Boolean(false));
    exact(&book, "Sheet1", "B3", "1");
    exact(&book, "Sheet1", "B4", "0.01");
    assert!(matches!(
        value(&book, "Sheet1", "B5"),
        Value::Error(CellError::Div0)
    ));
}

#[test]
fn period_end_and_financial_display_use_documented_rounding() {
    let mut book = Workbook::new();
    input(&mut book, "Sheet1", "A1", "=EOMONTH(DATE(2024,1,31),1)");
    input(&mut book, "Sheet1", "A2", "=DAYS(A1,DATE(2024,1,31))");
    input(
        &mut book,
        "Sheet1",
        "A3",
        "=ROUND(PMT(0.08/12,120,10000),2)",
    );
    input(&mut book, "Sheet1", "A4", "=ROUND(2.345,2)");
    input(&mut book, "Sheet1", "A5", "=ROUND(-2.355,2)");
    assert_eq!(
        value(&book, "Sheet1", "A1"),
        Value::Date(NaiveDate::from_ymd_opt(2024, 2, 29).unwrap())
    );
    exact(&book, "Sheet1", "A2", "29");
    exact(&book, "Sheet1", "A3", "-121.33");
    exact(&book, "Sheet1", "A4", "2.34");
    exact(&book, "Sheet1", "A5", "-2.36");
}
