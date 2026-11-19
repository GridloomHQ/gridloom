use crate::value::{CellCoord, CellRange, Value};
use crate::virtual_view::{CellRenderData, ViewportRequest};
use crate::Workbook;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

const MAX_ROWS: u64 = 1_000;
const MAX_COLS: u64 = 50;
const MAX_CELLS: u64 = 5_000;
const MAX_TEXT: usize = 500;

#[derive(Debug, Deserialize)]
pub struct RangeViewRequest {
    pub sheet_name: String,
    pub range: CellRange,
    pub first_row_header: bool,
    pub sort: Option<RowSort>,
    pub filter: Option<RowFilter>,
}

#[derive(Debug, Deserialize)]
pub struct RowSort {
    pub col: u32,
    pub descending: bool,
}

#[derive(Debug, Deserialize)]
pub struct RowFilter {
    pub col: u32,
    pub mode: FilterMode,
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterMode {
    Contains,
    Equals,
    Empty,
    NonEmpty,
    AtLeast,
    AtMost,
}

#[derive(Debug, Serialize)]
pub struct RangeView {
    pub generation: u64,
    pub headers: Vec<String>,
    pub total_rows: u32,
    pub rows: Vec<ViewRow>,
}

#[derive(Debug, Serialize)]
pub struct ViewRow {
    pub row: u32,
    pub values: Vec<String>,
}

fn rank(value: &Value) -> u8 {
    match value {
        Value::Integer(_) | Value::Decimal(_) => 0,
        Value::Float(f) if f.is_finite() => 1,
        Value::Date(_) => 2,
        Value::Timestamp(_) => 3,
        Value::Text(s) if !s.is_empty() => 4,
        Value::Boolean(_) => 5,
        Value::Array(_) => 6,
        Value::Error(_) | Value::Float(_) => 7,
        Value::Blank | Value::EmptyText | Value::Null | Value::Text(_) => 8,
    }
}

fn compare(a: &Value, b: &Value, descending: bool) -> Ordering {
    let (ra, rb) = (rank(a), rank(b));
    // Reverse within type, but keep the same group order in both directions.
    ra.cmp(&rb).then_with(|| {
        let ord = match (a, b) {
            (Value::Integer(a), Value::Integer(b)) => a.cmp(b),
            (Value::Decimal(a), Value::Decimal(b)) => a.cmp(b),
            (Value::Integer(a), Value::Decimal(b)) => Decimal::from(*a).cmp(b),
            (Value::Decimal(a), Value::Integer(b)) => a.cmp(&Decimal::from(*b)),
            (Value::Float(a), Value::Float(b)) if a.is_finite() && b.is_finite() => {
                a.partial_cmp(b).unwrap_or(Ordering::Equal)
            }
            (Value::Date(a), Value::Date(b)) => a.cmp(b),
            (Value::Timestamp(a), Value::Timestamp(b)) => a.cmp(b),
            (Value::Text(a), Value::Text(b)) => a.to_lowercase().cmp(&b.to_lowercase()),
            (Value::Boolean(a), Value::Boolean(b)) => a.cmp(b),
            _ => Ordering::Equal,
        };
        if descending {
            ord.reverse()
        } else {
            ord
        }
    })
}

fn matches_filter(
    cell: Option<&CellRenderData>,
    filter: &RowFilter,
    needle: &str,
    threshold: Option<Decimal>,
) -> bool {
    let value = cell.map(|c| &c.raw_value).unwrap_or(&Value::Blank);
    let empty = rank(value) == 8;
    match filter.mode {
        FilterMode::Empty => empty,
        FilterMode::NonEmpty => !empty,
        FilterMode::AtLeast | FilterMode::AtMost => {
            let value = match value {
                Value::Integer(n) => Decimal::from(*n),
                Value::Decimal(n) => *n,
                _ => return false,
            };
            threshold.is_some_and(|threshold| {
                if matches!(filter.mode, FilterMode::AtLeast) {
                    value >= threshold
                } else {
                    value <= threshold
                }
            })
        }
        FilterMode::Contains | FilterMode::Equals => {
            let text = cell
                .map(|c| c.display_text.as_str())
                .unwrap_or("")
                .to_lowercase();
            !empty
                && if matches!(filter.mode, FilterMode::Equals) {
                    text == needle
                } else {
                    text.contains(needle)
                }
        }
    }
}

fn preview(raw: &str) -> String {
    let mut chars = raw.chars();
    let mut text: String = chars.by_ref().take(MAX_TEXT).collect();
    if chars.next().is_some() {
        text.push('…');
    }
    text
}

impl Workbook {
    /// Read one bounded snapshot without modifying cell addresses, formulas, or writeback state.
    pub fn sort_filter_view(&self, req: &RangeViewRequest) -> Result<RangeView, &'static str> {
        let r = req.range;
        let sheet = self
            .sheets
            .get(&req.sheet_name)
            .ok_or("Worksheet not found")?;
        if r.start_row > r.end_row
            || r.start_col > r.end_col
            || r.end_row >= sheet.max_rows
            || r.end_col >= sheet.max_cols
        {
            return Err("Select a valid worksheet range");
        }
        let (height, width) = (u64::from(r.height()), u64::from(r.width()));
        if height > MAX_ROWS || width > MAX_COLS || height * width > MAX_CELLS {
            return Err(
                "Sort & filter supports at most 1,000 rows, 50 columns, and 5,000 cells per view",
            );
        }
        let in_range = |col| (r.start_col..=r.end_col).contains(&col);
        if req.sort.as_ref().is_some_and(|s| !in_range(s.col))
            || req.filter.as_ref().is_some_and(|f| {
                !in_range(f.col)
                    || f.text.len() > 1_000
                    || (matches!(f.mode, FilterMode::Contains | FilterMode::Equals)
                        && f.text.is_empty())
            })
        {
            return Err("Use a selected column and enter a valid filter");
        }
        if req.first_row_header && height == 1 {
            return Err("Select at least one data row beneath the header");
        }
        if self.stale_calculation_sheets.contains(&req.sheet_name) {
            return Err("Recalculate this worksheet before sorting or filtering");
        }
        let data = self
            .get_viewport(&ViewportRequest {
                sheet_name: req.sheet_name.clone(),
                start_row: r.start_row,
                end_row: r.end_row,
                start_col: r.start_col,
                end_col: r.end_col,
                overscan: 0,
            })
            .map_err(|_| "Could not read the selected range")?;
        let cells: std::collections::HashMap<_, _> =
            data.cells.values().map(|cell| (cell.coord, cell)).collect();
        let cell = |row, col| cells.get(&CellCoord::new(row, col)).copied();
        let display = |row, col| {
            cell(row, col)
                .map(|c| c.display_text.as_str())
                .unwrap_or("")
        };
        let headers = (r.start_col..=r.end_col)
            .map(|col| {
                let label = CellCoord::new(0, col).to_a1();
                let column = &label[..label.len() - 1]; // A1 → A, AA1 → AA.
                let heading = if req.first_row_header {
                    display(r.start_row, col)
                } else {
                    ""
                };
                if heading.is_empty() {
                    column.to_owned()
                } else {
                    format!("{} ({column})", preview(heading))
                }
            })
            .collect();
        let first = r.start_row + u32::from(req.first_row_header);
        let mut rows: Vec<_> = (first..=r.end_row).collect();
        if let Some(filter) = &req.filter {
            let needle = filter.text.to_lowercase();
            let threshold = if matches!(filter.mode, FilterMode::AtLeast | FilterMode::AtMost) {
                Some(
                    Decimal::from_str_exact(filter.text.trim())
                        .map_err(|_| "Enter a valid exact decimal threshold")?,
                )
            } else {
                None
            };
            rows.retain(|&row| matches_filter(cell(row, filter.col), filter, &needle, threshold));
        }
        if let Some(sort) = &req.sort {
            let key = |row| {
                cell(row, sort.col)
                    .map(|c| &c.raw_value)
                    .unwrap_or(&Value::Blank)
            };
            rows.sort_by(|&a, &b| compare(key(a), key(b), sort.descending).then_with(|| a.cmp(&b)));
        }
        let rows = rows
            .into_iter()
            .map(|row| ViewRow {
                row,
                values: (r.start_col..=r.end_col)
                    .map(|col| preview(display(row, col)))
                    .collect(),
            })
            .collect();
        Ok(RangeView {
            generation: data.generation,
            headers,
            total_rows: r.end_row - first + 1,
            rows,
        })
    }
}
