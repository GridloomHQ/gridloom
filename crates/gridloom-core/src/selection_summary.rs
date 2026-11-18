//! Read-only selection statistics. Numeric strings never round-trip through JavaScript numbers.
use crate::virtual_view::ViewportRequest;
use crate::{CellError, Value, Workbook};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
#[derive(Debug, Deserialize)]
pub struct SelectionSummaryRequest {
    pub sheet_name: String,
    pub start_row: u32,
    pub start_col: u32,
    pub end_row: u32,
    pub end_col: u32,
}
#[derive(Debug, Default, Serialize)]
pub struct SelectionCounts {
    pub integer: usize,
    pub decimal: usize,
    pub float: usize,
    pub text: usize,
    pub boolean: usize,
    pub date: usize,
    pub null: usize,
    pub blank: usize,
    pub error: usize,
    pub other: usize,
}
#[derive(Debug, Serialize)]
pub struct Aggregate {
    pub value: Option<String>,
    pub quality: &'static str,
    pub reason: Option<String>,
}
impl Aggregate {
    fn unavailable(reason: &str) -> Self {
        Self {
            value: None,
            quality: "unavailable",
            reason: Some(reason.into()),
        }
    }
    fn value(value: Decimal, quality: &'static str) -> Self {
        Self{value:Some(value.normalize().to_string()),quality,reason:match quality {"approximate"=>Some("Includes binary floating-point input converted to decimal; result is approximate.".into()),"rounded"=>Some("Average requires rounding to the supported decimal precision.".into()),_=>None}}
    }
}
#[derive(Debug, Serialize)]
pub struct SelectionSummary {
    pub selected_count: usize,
    pub numeric_count: usize,
    pub counts: SelectionCounts,
    pub sum: Aggregate,
    pub average: Aggregate,
}
fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}
fn exact_average(sum: Decimal, count: usize) -> bool {
    let sum = sum.normalize();
    let numerator = sum.mantissa();
    let divisor = gcd(numerator.unsigned_abs(), count as u128) as i128;
    let mut denominator = count as i128 / divisor;
    let mut twos = 0;
    let mut fives = 0;
    while denominator % 2 == 0 {
        denominator /= 2;
        twos += 1;
    }
    while denominator % 5 == 0 {
        denominator /= 5;
        fives += 1;
    }
    if denominator != 1 {
        return false;
    }
    let scale = sum.scale() + twos.max(fives);
    if scale > 28 {
        return false;
    }
    let numerator = numerator / divisor;
    let factor = if twos > fives {
        5i128.checked_pow(twos - fives)
    } else {
        2i128.checked_pow(fives - twos)
    };
    factor
        .and_then(|f| numerator.checked_mul(f))
        .is_some_and(|n| Decimal::try_from_i128_with_scale(n, scale).is_ok())
}
impl Workbook {
    pub fn selection_summary(
        &self,
        req: &SelectionSummaryRequest,
    ) -> Result<SelectionSummary, CellError> {
        let sheet = self.sheets.get(&req.sheet_name).ok_or(CellError::Ref)?;
        if req.start_row > req.end_row
            || req.start_col > req.end_col
            || req.end_row >= sheet.max_rows
            || req.end_col >= sheet.max_cols
        {
            return Err(CellError::Ref);
        }
        let selected = (u64::from(req.end_row - req.start_row) + 1)
            * (u64::from(req.end_col - req.start_col) + 1);
        if selected > 10_000 {
            return Err(CellError::Value);
        }
        let viewport = self.get_viewport(&ViewportRequest {
            sheet_name: req.sheet_name.clone(),
            start_row: req.start_row,
            start_col: req.start_col,
            end_row: req.end_row,
            end_col: req.end_col,
            overscan: 0,
        })?;
        let mut counts = SelectionCounts::default();
        let mut numbers = Vec::new();
        let mut failure = None;
        // Deterministic row-major traversal ensures even conservative overflow behavior is stable.
        for row in req.start_row..=req.end_row {
            for col in req.start_col..=req.end_col {
                let coord = crate::CellCoord::new(row, col);
                let value = viewport
                    .cells
                    .get(&coord.to_a1())
                    .map_or(&Value::Blank, |c| &c.raw_value);
                match value {
                    Value::Integer(n) => {
                        counts.integer += 1;
                        numbers.push(Decimal::from(*n));
                    }
                    Value::Decimal(n) => {
                        counts.decimal += 1;
                        numbers.push(*n);
                    }
                    Value::Float(n) => {
                        counts.float += 1;
                        if !n.is_finite() {
                            failure = Some("Selection contains a non-finite numeric value.");
                        } else {
                            match value.to_decimal(){Ok(d) if *n==0.0||!d.is_zero()=>numbers.push(d),_=>failure=Some("A floating-point value is outside supported decimal precision or range.")}
                        }
                    }
                    Value::Text(_) | Value::EmptyText => counts.text += 1,
                    Value::Boolean(_) => counts.boolean += 1,
                    Value::Date(_) | Value::Timestamp(_) => counts.date += 1,
                    Value::Null => counts.null += 1,
                    Value::Blank => counts.blank += 1,
                    Value::Error(_) => {
                        counts.error += 1;
                        failure = Some(
                            "Selection contains calculation errors; aggregates are unavailable.",
                        );
                    }
                    Value::Array(_) => {
                        counts.other += 1;
                        failure =
                            Some("Selection contains an unsupported unprojected array value.");
                    }
                }
            }
        }
        let numeric_count = counts.integer + counts.decimal + counts.float;
        let mut response = SelectionSummary {
            selected_count: selected as usize,
            numeric_count,
            counts,
            sum: Aggregate::unavailable("No numeric cells in selection."),
            average: Aggregate::unavailable("No numeric cells in selection."),
        };
        if let Some(reason) = failure {
            response.sum = Aggregate::unavailable(reason);
            response.average = Aggregate::unavailable(reason);
            return Ok(response);
        }
        if numbers.is_empty() {
            return Ok(response);
        }
        let numbers: Vec<_> = numbers.into_iter().map(|d| d.normalize()).collect();
        let scale = numbers.iter().map(Decimal::scale).max().unwrap_or(0);
        let accumulated = numbers.iter().try_fold(0i128, |total, n| {
            let factor = 10i128.checked_pow(scale - n.scale())?;
            total.checked_add(n.mantissa().checked_mul(factor)?)
        });
        let sum = accumulated.and_then(|mut n| {
            let mut s = scale;
            while s > 0 && n % 10 == 0 {
                n /= 10;
                s -= 1;
            }
            Decimal::try_from_i128_with_scale(n, s).ok()
        });
        let Some(sum) = sum else {
            let reason="Exact aligned accumulation exceeds supported decimal precision or range; no partial sum is shown.";
            response.sum = Aggregate::unavailable(reason);
            response.average = Aggregate::unavailable(reason);
            return Ok(response);
        };
        let Some(average) = sum.checked_div(Decimal::from(numeric_count)) else {
            let reason = "Average exceeds supported decimal precision or range.";
            response.sum = Aggregate::unavailable(reason);
            response.average = Aggregate::unavailable(reason);
            return Ok(response);
        };
        let approximate = response.counts.float > 0;
        response.sum = Aggregate::value(sum, if approximate { "approximate" } else { "exact" });
        response.average = Aggregate::value(
            average,
            if approximate {
                "approximate"
            } else if exact_average(sum, numeric_count) {
                "exact"
            } else {
                "rounded"
            },
        );
        Ok(response)
    }
}
