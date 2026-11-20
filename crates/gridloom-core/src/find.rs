//! Literal, read-only discovery over the current worksheet display, never live SQL.
use crate::virtual_view::ViewportRequest;
use crate::{CellCoord, CellError, Workbook};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const MAX_CANDIDATES: usize = 50_000;
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindLookIn {
    #[default]
    Values,
    Formulas,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FindRequest {
    pub query: String,
    pub sheet_name: Option<String>,
    #[serde(default)]
    pub look_in: FindLookIn,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub entire_cell: bool,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
}
fn default_limit() -> usize {
    100
}
#[derive(Debug, Serialize)]
pub struct FindMatch {
    pub sheet_name: String,
    pub coord: CellCoord,
    pub address: String,
    pub display_text: String,
    pub raw_input: Option<String>,
    pub formula: Option<String>,
    pub is_staged: bool,
    pub is_spill: bool,
    pub table_name: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct FindResponse {
    pub matches: Vec<FindMatch>,
    pub total_matches: usize,
    pub next_offset: Option<usize>,
    pub scanned_cells: usize,
    pub truncated: bool,
    pub warnings: Vec<String>,
}
impl Workbook {
    pub fn find_cells(&self, req: &FindRequest) -> Result<FindResponse, CellError> {
        if req.query.is_empty()
            || req.query.len() > 512
            || req.limit == 0
            || req.limit > 200
            || req.offset > MAX_CANDIDATES
        {
            return Err(CellError::Value);
        }
        if req
            .sheet_name
            .as_ref()
            .is_some_and(|name| !self.sheets.contains_key(name))
        {
            return Err(CellError::Ref);
        }
        let live_omitted = matches!(req.look_in, FindLookIn::Values) && self.sheets.iter().any(|(name,sheet)|
            req.sheet_name.as_ref().is_none_or(|selected|selected==name)
                && sheet.tables.values().any(|table|table.live.is_some()));
        let needle = if req.case_sensitive {
            req.query.clone()
        } else {
            req.query.to_lowercase()
        };
        // Keep only the first 50,001 coordinates in sheet-name/row/column order.
        // Sparse worksheet storage and cached occupied regions avoid scanning blank rectangles.
        let mut candidates = BTreeSet::new();
        let mut add = |sheet: &str, row: u32, col: u32| {
            candidates.insert((sheet.to_owned(), row, col));
            if candidates.len() > MAX_CANDIDATES + 1 {
                candidates.pop_last();
            }
        };
        for (name, sheet) in &self.sheets {
            if req.sheet_name.as_ref().is_some_and(|n| n != name) {
                continue;
            }
            for cell in sheet.cells.values() {
                if cell.coord.row < sheet.max_rows && cell.coord.col < sheet.max_cols {
                    add(name, cell.coord.row, cell.coord.col);
                }
            }
            if matches!(req.look_in, FindLookIn::Values) {
                for table in sheet.tables.values() {
                    // Match the cached viewport only: headers plus materialized source rows.
                    let rows = self
                        .named_sources
                        .get(&table.source_name)
                        .and_then(|s| s.last_result.as_ref())
                        .map_or(0, |r| r.rows.len());
                    let mut count = 0;
                    'table: for r in 0..=rows {
                        let Some(row) = table.anchor.row.checked_add(r as u32) else {
                            break;
                        };
                        if row >= sheet.max_rows || row > table.range.end_row {
                            break;
                        }
                        for c in 0..table.columns.len() {
                            let Some(col) = table.anchor.col.checked_add(c as u32) else {
                                break;
                            };
                            if col >= sheet.max_cols || col > table.range.end_col {
                                break;
                            }
                            add(name, row, col);
                            count += 1;
                            if count > MAX_CANDIDATES {
                                break 'table;
                            }
                        }
                    }
                }
                for spill in sheet.spills.active_spills.values() {
                    let mut count = 0;
                    'spill: for row in spill.range.start_row
                        ..=spill.range.end_row.min(sheet.max_rows.saturating_sub(1))
                    {
                        for col in spill.range.start_col
                            ..=spill.range.end_col.min(sheet.max_cols.saturating_sub(1))
                        {
                            add(name, row, col);
                            count += 1;
                            if count > MAX_CANDIDATES {
                                break 'spill;
                            }
                        }
                    }
                }
            }
        }
        let truncated = candidates.len() > MAX_CANDIDATES;
        let mut clipped = false;
        let mut preview = |text: &str| {
            let value: String = text.chars().take(1000).collect();
            if value.len() < text.len() {
                clipped = true;
            }
            value
        };
        let mut response = FindResponse {
            matches: vec![],
            total_matches: 0,
            next_offset: None,
            scanned_cells: 0,
            truncated,
            warnings: vec![],
        };
        for (name, row, col) in candidates.into_iter().take(MAX_CANDIDATES) {
            response.scanned_cells += 1;
            let coord = CellCoord::new(row, col);
            let address = coord.to_a1();
            let viewport = self.get_viewport(&ViewportRequest {
                sheet_name: name.clone(),
                start_row: row,
                end_row: row,
                start_col: col,
                end_col: col,
                overscan: 0,
            })?;
            let Some(cell) = viewport.cells.get(&address) else {
                continue;
            };
            let text = match req.look_in {
                FindLookIn::Values => cell.display_text.as_str(),
                FindLookIn::Formulas => cell.raw_input.as_deref().unwrap_or(""),
            };
            let haystack = if req.case_sensitive {
                text.to_owned()
            } else {
                text.to_lowercase()
            };
            if text.is_empty()
                || !(if req.entire_cell {
                    haystack == needle
                } else {
                    haystack.contains(&needle)
                })
            {
                continue;
            }
            let index = response.total_matches;
            response.total_matches += 1;
            if index < req.offset || response.matches.len() >= req.limit {
                continue;
            }
            response.matches.push(FindMatch {
                sheet_name: name,
                coord,
                address,
                display_text: preview(&cell.display_text),
                raw_input: cell.raw_input.as_deref().map(&mut preview),
                formula: cell.formula.as_deref().map(&mut preview),
                is_staged: cell.is_staged,
                is_spill: cell.is_spill,
                table_name: cell.table_name.clone(),
            });
        }
        let next = req.offset.saturating_add(response.matches.len());
        if next < response.total_matches {
            response.next_offset = Some(next);
        }
        if live_omitted {
            response.warnings.push("Worksheet Find omits live table values. Use SQL WHERE for live table filtering.".into());
        }
        if clipped {
            response.warnings.push("Result previews are limited to 1,000 characters per field; matching uses the complete cell text.".into());
        }
        if truncated {
            response.warnings.push("Search limited to the first 50,000 occupied candidate cells in sheet-name/row/column order. Results are incomplete; narrow the search to a sheet.".into());
        }
        response.warnings.push("Search uses current cached worksheet content, including staged display values. It does not refresh or search external databases, evidence attachments, charts or review notes. Pagination is recomputed from current workbook content.".into());
        Ok(response)
    }
}
