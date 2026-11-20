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
            formats: Vec::new(),
        }
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
