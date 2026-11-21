use crate::ast::StructuredRef;
use crate::db::sqlite::QueryResult;
use crate::error::CellError;
use crate::lineage::LineageReport;
use crate::spreadsheet::CellStyle;
use crate::value::{CellCoord, CellRange, Value};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceParam {
    pub name: String,
    pub cell_binding: Option<CellCoord>,
    pub constant_value: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedSource {
    pub name: String,
    pub connection_name: String,
    pub sql: String,
    pub parameters: Vec<SourceParam>,
    pub last_result: Option<QueryResult>,
    pub lineage: Option<LineageReport>,
}

/// Each change is a rectangle relative to the table, not one entry per cell.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TableFormat {
    pub range: CellRange,
    /// When applied to an entire data column, include rows added by future refreshes.
    pub through_end: bool,
    pub change: FormatChange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum FormatChange {
    Style(CellStyle),
    Bold(bool),
    Italic(bool),
    Fill(Option<String>),
    NumberFormat(String),
}

impl FormatChange {
    pub(crate) fn apply(&self, style: &mut CellStyle) {
        match self {
            Self::Style(value) => *style = value.clone(),
            Self::Bold(value) => style.bold = *value,
            Self::Italic(value) => style.italic = *value,
            Self::Fill(value) => style.fill = value.clone(),
            Self::NumberFormat(value) => style.number_format = Some(value.clone()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalColumnType {
    Any,
    Text,
    Integer,
    Decimal,
    Boolean,
    Date,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalColumn {
    pub name: String,
    pub kind: LocalColumnType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalTable {
    pub id: uuid::Uuid,
    pub column_types: Vec<LocalColumnType>,
}

impl LocalColumnType {
    pub(crate) fn enforce(self, data: &mut crate::workbook::CellData, column: &str) -> Result<(), CellError> {
        use LocalColumnType::*;
        if self == Any {
            return Ok(());
        }
        let fail = || CellError::Sql(format!("{column}: enter a {:?} literal, or leave the cell blank.", self));
        let raw = &data.raw_input;
        if raw.is_empty() {
            // Explicit local NULL is stored as a typed value with empty raw
            // input. An ordinary existing or newly cleared cell is Blank.
            if !matches!(data.evaluated_value, Value::Null) {
                data.evaluated_value = Value::Blank;
            }
            return Ok(());
        }
        if data.formula.is_some() && self != Text {
            return Err(fail());
        }
        let input = raw.trim();
        data.evaluated_value = match self {
            Text => {
                data.formula = None;
                if raw == "\"\"" {
                    Value::EmptyText
                } else {
                    Value::Text(raw.clone())
                }
            }
            Integer => Value::Integer(input.parse().map_err(|_| fail())?),
            Decimal => Value::Decimal(rust_decimal::Decimal::from_str_exact(input).map_err(|_| fail())?),
            Boolean if input.eq_ignore_ascii_case("true") => Value::Boolean(true),
            Boolean if input.eq_ignore_ascii_case("false") => Value::Boolean(false),
            Date if input.len() == 10 => Value::Date(chrono::NaiveDate::parse_from_str(input, "%Y-%m-%d").map_err(|_| fail())?),
            _ => return Err(fail()),
        };
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorksheetTable {
    pub name: String,
    pub source_name: String,
    pub anchor: CellCoord,
    pub range: CellRange,
    pub columns: Vec<String>,
    pub has_headers: bool,
    pub has_totals: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live: Option<crate::live_table::LiveTable>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local: Option<LocalTable>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) formats: Vec<TableFormat>,
}

impl WorksheetTable {
    pub fn new(
        name: String,
        source_name: String,
        anchor: CellCoord,
        columns: Vec<String>,
        row_count: u32,
    ) -> Self {
        let col_count = columns.len() as u32;
        // 1 row for header + row_count for data
        let total_rows = 1 + row_count.max(1);
        let range = CellRange::new(
            anchor.row,
            anchor.col,
            anchor.row + total_rows - 1,
            anchor.col + col_count.saturating_sub(1),
        );
        Self {
            name,
            source_name,
            anchor,
            range,
            columns,
            has_headers: true,
            has_totals: false,
            live: None,
            local: None,
            formats: Vec::new(),
        }
    }

    pub(crate) fn blocks_cell_edits(&self, coord: CellCoord) -> bool {
        self.range.contains(coord) && (self.local.is_none() || self.anchor.row == coord.row)
    }

    pub(crate) fn local_column(&self, coord: CellCoord) -> Option<(&str, LocalColumnType)> {
        let local = self.local.as_ref()?;
        if !self.range.contains(coord) || coord.row == self.anchor.row {
            return None;
        }
        let col = (coord.col - self.anchor.col) as usize;
        Some((self.columns.get(col)?, *local.column_types.get(col)?))
    }

    pub(crate) fn cell_style(&self, coord: CellCoord) -> CellStyle {
        let row = coord.row - self.anchor.row;
        let col = coord.col - self.anchor.col;
        let mut style = CellStyle {
            bold: row == 0,
            ..CellStyle::default()
        };
        for format in &self.formats {
            if row >= format.range.start_row
                && (format.through_end || row <= format.range.end_row)
                && col >= format.range.start_col
                && col <= format.range.end_col
            {
                format.change.apply(&mut style);
            }
        }
        style
    }

    /// Resolve a structured reference such as Invoices[Amount] into a CellRange
    pub fn resolve_structured_ref(
        &self,
        sref: &StructuredRef,
        eval_coord: Option<CellCoord>,
    ) -> Result<CellRange, CellError> {
        let col_idx = self
            .columns
            .iter()
            .position(|c| c.eq_ignore_ascii_case(&sref.column_name))
            .ok_or(CellError::Ref)?;

        let target_col = self.anchor.col + col_idx as u32;

        if sref.is_row_relative {
            // @Column - evaluates for the current row
            let cur = eval_coord.ok_or(CellError::Value)?;
            if cur.row <= self.anchor.row || cur.row > self.range.end_row {
                return Err(CellError::Value);
            }
            Ok(CellRange::single(CellCoord::new(cur.row, target_col)))
        } else {
            let start_row = if sref.include_headers {
                self.anchor.row
            } else {
                self.anchor.row + 1
            };
            let end_row = self.range.end_row;
            Ok(CellRange::new(start_row, target_col, end_row, target_col))
        }
    }
}
