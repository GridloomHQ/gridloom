//! Atomic worksheet operations. History never restores database writes, evidence or reviews.
use crate::workbook::{CellData, Sheet};
use crate::{CellCoord, CellError, CellRange, Value, Workbook};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellStyle {
    pub bold: bool,
    pub italic: bool,
    pub fill: Option<String>,
    pub number_format: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct RangeCapture {
    pub values: Vec<Vec<String>>,
    pub snapshot: String,
}
#[derive(Default)]
pub struct EditHistory {
    undo: Vec<CellChange>,
    redo: Vec<CellChange>,
}
#[derive(Clone)]
struct CellChange {
    sheet: String,
    before: HashMap<String, Option<CellData>>,
    after: HashMap<String, Option<CellData>>,
}

pub fn format_value(value: &Value, format: Option<&str>) -> String {
    if !matches!(
        value,
        Value::Integer(_) | Value::Decimal(_) | Value::Float(_)
    ) {
        return value.to_string_repr();
    }
    let Ok(n) = value.to_decimal() else {
        return value.to_string_repr();
    };
    match format {
        Some("0.00") | Some("number") => format!("{:.2}", n),
        Some("0%") | Some("percent") => n
            .checked_mul(rust_decimal::Decimal::from(100))
            .map(|p| format!("{:.0}%", p))
            .unwrap_or_else(|| value.to_string_repr()),
        Some("0.00%") => n
            .checked_mul(rust_decimal::Decimal::from(100))
            .map(|p| format!("{:.2}%", p))
            .unwrap_or_else(|| value.to_string_repr()),
        Some("$0.00") | Some("currency") => format!("$ {:.2}", n),
        _ => value.to_string_repr(),
    }
}
fn coords(range: CellRange) -> Result<Vec<CellCoord>, CellError> {
    if range.start_row > range.end_row || range.start_col > range.end_col {
        return Err(CellError::Ref);
    }
    let count = (range.end_row as u64 - range.start_row as u64 + 1)
        .checked_mul(range.end_col as u64 - range.start_col as u64 + 1)
        .ok_or(CellError::Value)?;
    if count > 10_000 {
        return Err(CellError::Value);
    }
    Ok((range.start_row..=range.end_row)
        .flat_map(|r| (range.start_col..=range.end_col).map(move |c| CellCoord::new(r, c)))
        .collect())
}
fn validate(sheet: &Sheet, coord: CellCoord) -> Result<(), CellError> {
    if coord.row >= sheet.max_rows
        || coord.col >= sheet.max_cols
        || sheet.tables.values().any(|t| t.range.contains(coord))
    {
        return Err(CellError::Ref);
    }
    if sheet
        .spills
        .get_spill_at(&coord)
        .is_some_and(|s| s.anchor != coord)
    {
        return Err(CellError::Spill);
    }
    Ok(())
}
fn source_is_materialized(sheet: &Sheet, coord: CellCoord) -> bool {
    sheet.tables.values().any(|t| t.range.contains(coord))
        || sheet
            .spills
            .get_spill_at(&coord)
            .is_some_and(|s| s.anchor != coord)
}
impl Workbook {
    pub fn clear_edit_history(&mut self) {
        self.edit_history = Default::default();
    }
    pub fn add_sheet(&mut self, name: &str) -> Result<(), CellError> {
        if name.trim() != name
            || name.is_empty()
            || name.len() > 80
            || name
                .chars()
                .any(|c| "[]:*?/\\!".contains(c) || c.is_control())
            || self.sheets.keys().any(|n| n.eq_ignore_ascii_case(name))
        {
            return Err(CellError::Value);
        }
        self.sheets.insert(name.into(), Sheet::new(name.into()));
        self.active_sheet = name.into();
        Ok(())
    }
    pub fn select_sheet(&mut self, name: &str) -> Result<(), CellError> {
        if !self.sheets.contains_key(name) {
            return Err(CellError::Ref);
        }
        self.active_sheet = name.into();
        Ok(())
    }
    pub fn rename_sheet(&mut self, old: &str, new: &str) -> Result<(), CellError> {
        if old == new {
            return self.select_sheet(old);
        }
        if self.close_periods.iter().any(|p| p.sheet_name == old) {
            return Err(CellError::Value);
        }
        if !self.sheets.contains_key(old) {
            return Err(CellError::Ref);
        }
        // Reuse validated naming; replace references only outside strings/structured references.
        let active = self.active_sheet.clone();
        self.add_sheet(new)?;
        let mut sheet = self.sheets.remove(old).ok_or(CellError::Ref)?;
        sheet.name = new.into();
        self.sheets.insert(new.into(), sheet);
        for sheet in self.sheets.values_mut() {
            for cell in sheet.cells.values_mut() {
                if let Some(formula) = &cell.formula {
                    let updated = rewrite_formula(formula, 0, 0, Some((old, new)))?;
                    cell.formula = Some(updated.clone());
                    cell.raw_input = updated;
                }
            }
        }
        for review in &mut self.reviews {
            if review.sheet_name == old {
                review.sheet_name = new.into();
                review.invalidated = true;
                review.invalidation_reason = Some("Worksheet renamed; review again.".into());
            }
        }
        self.active_sheet = if active == old { new.into() } else { active };
        self.edit_history = Default::default();
        self.recalculate(new)
    }
    fn commit_cells(
        &mut self,
        name: &str,
        changes: Vec<(CellCoord, Option<CellData>)>,
        history: bool,
    ) -> Result<(), CellError> {
        if changes.is_empty() || changes.len() > 10_000 {
            return Err(CellError::Value);
        }
        let sheet = self.sheets.get(name).ok_or(CellError::Ref)?;
        for (coord, _) in &changes {
            match validate(sheet, *coord) {
                Err(CellError::Spill)
                    if !history
                        && sheet.spills.get_spill_at(coord).is_some_and(|spill| {
                            changes.iter().any(|(c, _)| *c == spill.anchor)
                        }) => {}
                result => result?,
            }
        }
        let original = self.sheets.clone();
        let graph = self.dep_graph.clone();
        let mut before = HashMap::new();
        let mut after = HashMap::new();
        let sheet = self.sheets.get_mut(name).unwrap();
        for (coord, data) in changes {
            let key = coord.to_a1();
            before.insert(key.clone(), sheet.cells.get(&key).cloned());
            sheet.spills.clear_anchor(&coord);
            after.insert(key.clone(), data.clone());
            if let Some(data) = data {
                sheet.cells.insert(key, data);
            } else {
                sheet.cells.remove(&key);
            }
        }
        if let Err(e) = self.recalculate(name) {
            self.sheets = original;
            self.dep_graph = graph;
            return Err(e);
        }
        if history {
            self.edit_history.undo.push(CellChange {
                sheet: name.into(),
                before,
                after,
            });
            while self.edit_history.undo.len() > 100
                || self
                    .edit_history
                    .undo
                    .iter()
                    .map(|c| c.before.len() + c.after.len())
                    .sum::<usize>()
                    > 100_000
            {
                self.edit_history.undo.remove(0);
            }
            self.edit_history.redo.clear();
        }
        self.invalidate_changed_reviews();
        Ok(())
    }
    fn clipboard_viewport(
        &self,
        name: &str,
        range: CellRange,
    ) -> Result<crate::virtual_view::ViewportResponse, CellError> {
        coords(range)?;
        let sheet = self.sheets.get(name).ok_or(CellError::Ref)?;
        if range.end_row >= sheet.max_rows || range.end_col >= sheet.max_cols {
            return Err(CellError::Ref);
        }
        self.get_viewport(&crate::virtual_view::ViewportRequest {
            sheet_name: name.into(),
            start_row: range.start_row,
            start_col: range.start_col,
            end_row: range.end_row,
            end_col: range.end_col,
            overscan: 0,
        })
    }
    pub fn read_range(&self, name: &str, range: CellRange) -> Result<Vec<Vec<String>>, CellError> {
        let sheet = self.sheets.get(name).ok_or(CellError::Ref)?;
        coords(range)?;
        if range.end_row >= sheet.max_rows || range.end_col >= sheet.max_cols {
            return Err(CellError::Ref);
        }
        let viewport = self.clipboard_viewport(name, range)?;
        let mut rows = Vec::new();
        for r in range.start_row..=range.end_row {
            let mut row = Vec::new();
            for c in range.start_col..=range.end_col {
                let coord = CellCoord::new(r, c);
                row.push(
                    sheet
                        .get_cell(&coord)
                        .filter(|_| !source_is_materialized(sheet, coord))
                        .map(|c| c.raw_input.clone())
                        .unwrap_or_else(|| {
                            viewport
                                .cells
                                .get(&coord.to_a1())
                                .map(|c| c.raw_value.to_string_repr())
                                .unwrap_or_default()
                        }),
                );
            }
            rows.push(row);
        }
        Ok(rows)
    }
    pub fn edit_range(
        &mut self,
        name: &str,
        start: CellCoord,
        values: Vec<Vec<String>>,
    ) -> Result<(), CellError> {
        let width = values.first().map_or(0, Vec::len);
        if width == 0
            || values.len().checked_mul(width).is_none_or(|n| n > 10_000)
            || values.iter().any(|r| r.len() != width)
        {
            return Err(CellError::Value);
        }
        let sheet = self.sheets.get(name).ok_or(CellError::Ref)?;
        let mut changes = Vec::new();
        for (r, row) in values.into_iter().enumerate() {
            for (c, input) in row.into_iter().enumerate() {
                let coord = CellCoord::new(
                    start.row.checked_add(r as u32).ok_or(CellError::Ref)?,
                    start.col.checked_add(c as u32).ok_or(CellError::Ref)?,
                );
                let mut data = CellData::new(coord, input);
                if let Some(old) = sheet.get_cell(&coord) {
                    data.style = old.style.clone();
                    data.number_format = old.number_format.clone();
                }
                changes.push((coord, Some(data)));
            }
        }
        self.commit_cells(name, changes, true)
    }
    /// Changes one attribute across a range without flattening mixed styles.
    pub fn format_range_attribute(
        &mut self,
        name: &str,
        range: CellRange,
        attribute: &str,
        value: &str,
    ) -> Result<(), CellError> {
        match attribute {
            "bold" | "italic" if matches!(value, "true" | "false") => {}
            "fill"
                if value.is_empty()
                    || (value.len() == 7
                        && value.starts_with('#')
                        && value[1..].bytes().all(|b| b.is_ascii_hexdigit())) => {}
            "number_format"
                if [
                    "general", "number", "percent", "currency", "0.00", "0%", "0.00%", "$0.00",
                ]
                .contains(&value) => {}
            _ => return Err(CellError::Value),
        }
        let sheet = self.sheets.get(name).ok_or(CellError::Ref)?;
        let changes = coords(range)?
            .into_iter()
            .map(|coord| {
                let mut data = sheet
                    .get_cell(&coord)
                    .cloned()
                    .unwrap_or_else(|| CellData::new(coord, String::new()));
                match attribute {
                    "bold" => data.style.bold = value == "true",
                    "italic" => data.style.italic = value == "true",
                    "fill" => {
                        data.style.fill = if value.is_empty() {
                            None
                        } else {
                            Some(value.into())
                        }
                    }
                    "number_format" => {
                        data.style.number_format = Some(value.into());
                        data.number_format = Some(value.into());
                    }
                    _ => unreachable!(),
                }
                (coord, Some(data))
            })
            .collect();
        self.commit_cells(name, changes, true)
    }
    pub fn format_range(
        &mut self,
        name: &str,
        range: CellRange,
        style: CellStyle,
    ) -> Result<(), CellError> {
        if style.fill.as_ref().is_some_and(|s| {
            s.len() != 7 || !s.starts_with('#') || !s[1..].bytes().all(|b| b.is_ascii_hexdigit())
        }) {
            return Err(CellError::Value);
        }
        if style.number_format.as_ref().is_some_and(|s| {
            ![
                "general", "number", "percent", "currency", "0.00", "0%", "0.00%", "$0.00",
            ]
            .contains(&s.as_str())
        }) {
            return Err(CellError::Value);
        }
        let sheet = self.sheets.get(name).ok_or(CellError::Ref)?;
        let changes = coords(range)?
            .into_iter()
            .map(|coord| {
                let mut data = sheet
                    .get_cell(&coord)
                    .cloned()
                    .unwrap_or_else(|| CellData::new(coord, String::new()));
                data.style = style.clone();
                data.number_format = style.number_format.clone();
                (coord, Some(data))
            })
            .collect();
        self.commit_cells(name, changes, true)
    }
    pub fn capture_range(&self, name: &str, range: CellRange) -> Result<RangeCapture, CellError> {
        use sha2::{Digest, Sha256};
        let values = self.read_range(name, range)?;
        let sheet = self.sheets.get(name).ok_or(CellError::Ref)?;
        let cells: Vec<_> = coords(range)?
            .into_iter()
            .map(|c| sheet.get_cell(&c))
            .collect();
        let viewport = self.clipboard_viewport(name, range)?;
        let observed: Vec<_> = coords(range)?
            .into_iter()
            .map(|c| {
                viewport
                    .cells
                    .get(&c.to_a1())
                    .map(|c| (&c.raw_value, c.is_staged))
            })
            .collect();
        let bytes = serde_json::to_vec(&(name, range, &values, cells, observed))
            .map_err(|_| CellError::Value)?;
        Ok(RangeCapture {
            values,
            snapshot: format!("v1:{:x}", Sha256::digest(bytes)),
        })
    }
    pub fn copy_range_snapshot_checked(
        &mut self,
        name: &str,
        range: CellRange,
        target_sheet: &str,
        target: CellCoord,
        expected_snapshot: &str,
    ) -> Result<(), CellError> {
        if self.capture_range(name, range)?.snapshot != expected_snapshot {
            return Err(CellError::Value);
        }
        self.copy_range(name, range, target_sheet, target)
    }
    pub fn copy_range_checked(
        &mut self,
        name: &str,
        range: CellRange,
        target_sheet: &str,
        target: CellCoord,
        expected_values: &[Vec<String>],
    ) -> Result<(), CellError> {
        if self.read_range(name, range)? != expected_values {
            return Err(CellError::Value);
        }
        self.copy_range(name, range, target_sheet, target)
    }
    pub fn copy_range(
        &mut self,
        name: &str,
        range: CellRange,
        target_sheet: &str,
        target: CellCoord,
    ) -> Result<(), CellError> {
        let source = self.sheets.get(name).ok_or(CellError::Ref)?;
        let viewport = self.clipboard_viewport(name, range)?;
        let changes = coords(range)?
            .into_iter()
            .map(|coord| {
                if coord.row >= source.max_rows || coord.col >= source.max_cols {
                    return Err(CellError::Ref);
                }
                let dest = CellCoord::new(
                    target
                        .row
                        .checked_add(coord.row - range.start_row)
                        .ok_or(CellError::Ref)?,
                    target
                        .col
                        .checked_add(coord.col - range.start_col)
                        .ok_or(CellError::Ref)?,
                );
                // The copied anchor regenerates its spill; materializing its children would block it.
                if source
                    .spills
                    .get_spill_at(&coord)
                    .is_some_and(|spill| spill.anchor != coord && range.contains(spill.anchor))
                {
                    return Ok((dest, None));
                }
                let materialized = source_is_materialized(source, coord);
                let data = if materialized {
                    let value = viewport
                        .cells
                        .get(&coord.to_a1())
                        .map(|c| c.raw_value.clone())
                        .unwrap_or(Value::Blank);
                    let mut data = CellData::new(coord, value.to_string_repr());
                    data.formula = None;
                    data.evaluated_value = value;
                    data.is_dirty = false;
                    Some(data)
                } else {
                    source.get_cell(&coord).cloned()
                };
                let data = data
                    .map(|mut data| {
                        data.coord = dest;
                        if let Some(formula) = data.formula.as_ref() {
                            let f = rewrite_formula(
                                formula,
                                dest.row as i64 - coord.row as i64,
                                dest.col as i64 - coord.col as i64,
                                None,
                            )?;
                            data.raw_input = f.clone();
                            data.formula = Some(f);
                        }
                        Ok::<_, CellError>(data)
                    })
                    .transpose()?;
                Ok((dest, data))
            })
            .collect::<Result<Vec<_>, CellError>>()?;
        self.commit_cells(target_sheet, changes, true)
    }
    pub fn fill_range(
        &mut self,
        name: &str,
        source: CellRange,
        target: CellRange,
    ) -> Result<(), CellError> {
        let source_coords = coords(source)?;
        let source_sheet = self.sheets.get(name).ok_or(CellError::Ref)?;
        for c in source_coords {
            validate(source_sheet, c)?;
        }
        let height = source.end_row - source.start_row + 1;
        let width = source.end_col - source.start_col + 1;
        let changes = coords(target)?
            .into_iter()
            .map(|dest| {
                let coord = CellCoord::new(
                    source.start_row + (dest.row - target.start_row) % height,
                    source.start_col + (dest.col - target.start_col) % width,
                );
                let data = source_sheet
                    .get_cell(&coord)
                    .cloned()
                    .map(|mut data| {
                        data.coord = dest;
                        if let Some(f) = &data.formula {
                            let f = rewrite_formula(
                                f,
                                dest.row as i64 - coord.row as i64,
                                dest.col as i64 - coord.col as i64,
                                None,
                            )?;
                            data.raw_input = f.clone();
                            data.formula = Some(f);
                        }
                        Ok::<_, CellError>(data)
                    })
                    .transpose()?;
                Ok((dest, data))
            })
            .collect::<Result<Vec<_>, CellError>>()?;
        self.commit_cells(name, changes, true)
    }
    pub fn undo_sheet_edit(&mut self) -> Result<(), CellError> {
        let change = self.edit_history.undo.pop().ok_or(CellError::Value)?;
        let edits = change
            .before
            .iter()
            .map(|(key, data)| Ok((CellCoord::from_a1(key)?, data.clone())))
            .collect::<Result<Vec<_>, CellError>>()?;
        if let Err(e) = self.commit_cells(&change.sheet, edits, false) {
            self.edit_history.undo.push(change);
            return Err(e);
        }
        self.edit_history.redo.push(change);
        Ok(())
    }
    pub fn redo_sheet_edit(&mut self) -> Result<(), CellError> {
        let change = self.edit_history.redo.pop().ok_or(CellError::Value)?;
        let edits = change
            .after
            .iter()
            .map(|(key, data)| Ok((CellCoord::from_a1(key)?, data.clone())))
            .collect::<Result<Vec<_>, CellError>>()?;
        if let Err(e) = self.commit_cells(&change.sheet, edits, false) {
            self.edit_history.redo.push(change);
            return Err(e);
        }
        self.edit_history.undo.push(change);
        Ok(())
    }
}

/// Translate only reference tokens; quoted text, sheet names and structured columns stay literal.
pub fn rewrite_formula(
    input: &str,
    dr: i64,
    dc: i64,
    rename: Option<(&str, &str)>,
) -> Result<String, CellError> {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '"' || chars[i] == '\'' {
            let quote = chars[i];
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == quote {
                    i += 1;
                    if i < chars.len() && chars[i] == quote {
                        i += 1;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
            let token: String = chars[start..i].iter().collect();
            if quote == '\'' && i < chars.len() && chars[i] == '!' {
                let sheet = token[1..token.len() - 1].replace("''", "'");
                if let Some((old, new)) = rename.filter(|(old, _)| *old == sheet) {
                    let _ = old;
                    out.push_str(&format!("'{}'", new.replace("'", "''")));
                } else {
                    out.push_str(&token)
                }
            } else {
                out.push_str(&token)
            }
            continue;
        }
        if chars[i] == '[' {
            let start = i;
            let mut depth = 0;
            while i < chars.len() {
                if chars[i] == '[' {
                    depth += 1
                }
                if chars[i] == ']' {
                    depth -= 1
                }
                i += 1;
                if depth == 0 {
                    break;
                }
            }
            out.extend(chars[start..i].iter());
            continue;
        }
        if chars[i].is_ascii_alphabetic() || chars[i] == '_' || chars[i] == '$' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || "_.$".contains(chars[i]))
            {
                i += 1;
            }
            let token: String = chars[start..i].iter().collect();
            if i < chars.len() && chars[i] == '!' {
                if let Some((_, new)) = rename.filter(|(old, _)| *old == token) {
                    out.push_str(&format!("'{}'", new.replace("'", "''")))
                } else {
                    out.push_str(&token)
                }
            } else if (i == chars.len() || !['(', '['].contains(&chars[i]))
                && CellCoord::from_a1(&token).is_ok()
            {
                let coord = CellCoord::from_a1(&token)?;
                let abs_col = token.starts_with('$');
                let abs_row = token.trim_start_matches('$').contains('$');
                let row = coord.row as i64 + if abs_row { 0 } else { dr };
                let col = coord.col as i64 + if abs_col { 0 } else { dc };
                if row < 0 || col < 0 || row >= u32::MAX as i64 || col >= u32::MAX as i64 {
                    return Err(CellError::Ref);
                }
                let a1 = CellCoord::new(row as u32, col as u32).to_a1();
                let split = a1.find(|c: char| c.is_ascii_digit()).unwrap();
                out.push_str(&format!(
                    "{}{}{}{}",
                    if abs_col { "$" } else { "" },
                    &a1[..split],
                    if abs_row { "$" } else { "" },
                    &a1[split..]
                ));
            } else {
                out.push_str(&token)
            }
        } else {
            out.push(chars[i]);
            i += 1
        }
    }
    Ok(out)
}
