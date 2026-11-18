use crate::value::{CellCoord, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewportRequest {
    pub sheet_name: String,
    pub start_row: u32,
    pub start_col: u32,
    pub end_row: u32,
    pub end_col: u32,
    pub overscan: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellRenderData {
    pub coord: CellCoord,
    pub raw_input: Option<String>,
    #[serde(default)]
    pub style: crate::spreadsheet::CellStyle,
    pub display_text: String,
    pub raw_value: Value,
    pub formula: Option<String>,
    pub number_format: Option<String>,
    pub is_staged: bool,
    pub is_spill: bool,
    pub is_error: bool,
    pub table_name: Option<String>,
    pub is_table_header: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewportResponse {
    pub sheet_name: String,
    pub start_row: u32,
    pub start_col: u32,
    pub end_row: u32,
    pub end_col: u32,
    pub cells: HashMap<String, CellRenderData>, // Key is A1 notation
    pub total_rows: u32,
    pub total_cols: u32,
    pub generation: u64,
}

/// A single-lock composite viewport: scrolling region plus at most three frozen strips.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ViewportPanesRequest {
    #[serde(flatten)]
    pub viewport: ViewportRequest,
    pub freeze_top_row: bool,
    pub freeze_first_column: bool,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoadedRange {
    pub start_row: u32,
    pub start_col: u32,
    pub end_row: u32,
    pub end_col: u32,
}
#[derive(Debug, Clone, Serialize)]
pub struct ViewportPanesResponse {
    #[serde(flatten)]
    pub viewport: ViewportResponse,
    pub loaded_ranges: Vec<LoadedRange>,
}
impl crate::Workbook {
    /// Call under one read guard when serving concurrent mutations. No query refresh or mutation.
    pub fn get_viewport_panes(
        &self,
        req: &ViewportPanesRequest,
    ) -> Result<ViewportPanesResponse, crate::CellError> {
        let main = &req.viewport;
        let sheet = self
            .sheets
            .get(&main.sheet_name)
            .ok_or(crate::CellError::Ref)?;
        if sheet.max_rows == 0
            || sheet.max_cols == 0
            || main.start_row > main.end_row
            || main.start_col > main.end_col
            || main.start_row >= sheet.max_rows
            || main.start_col >= sheet.max_cols
            || main.overscan > 100
        {
            return Err(crate::CellError::Ref);
        }
        let bounds = LoadedRange {
            start_row: main.start_row.saturating_sub(main.overscan),
            start_col: main.start_col.saturating_sub(main.overscan),
            end_row: main
                .end_row
                .saturating_add(main.overscan)
                .min(sheet.max_rows - 1),
            end_col: main
                .end_col
                .saturating_add(main.overscan)
                .min(sheet.max_cols - 1),
        };
        let count = (u64::from(bounds.end_row - bounds.start_row) + 1)
            * (u64::from(bounds.end_col - bounds.start_col) + 1);
        if count > 20_000 {
            return Err(crate::CellError::Value);
        }
        let mut ranges = vec![bounds.clone()];
        let top = req.freeze_top_row && bounds.start_row > 0;
        let left = req.freeze_first_column && bounds.start_col > 0;
        if top {
            ranges.push(LoadedRange {
                start_row: 0,
                end_row: 0,
                start_col: bounds.start_col,
                end_col: bounds.end_col,
            });
        }
        if left {
            ranges.push(LoadedRange {
                start_row: bounds.start_row,
                end_row: bounds.end_row,
                start_col: 0,
                end_col: 0,
            });
        }
        if top && left {
            ranges.push(LoadedRange {
                start_row: 0,
                end_row: 0,
                start_col: 0,
                end_col: 0,
            });
        }
        let render = |range: &LoadedRange| {
            self.get_viewport(&ViewportRequest {
                sheet_name: main.sheet_name.clone(),
                start_row: range.start_row,
                end_row: range.end_row,
                start_col: range.start_col,
                end_col: range.end_col,
                overscan: 0,
            })
        };
        let mut viewport = render(&bounds)?;
        for range in ranges.iter().skip(1) {
            viewport.cells.extend(render(range)?.cells);
        }
        Ok(ViewportPanesResponse {
            viewport,
            loaded_ranges: ranges,
        })
    }
}
