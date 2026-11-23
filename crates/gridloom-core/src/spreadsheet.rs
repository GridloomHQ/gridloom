//! Atomic worksheet operations. History never restores database writes, evidence or reviews.
use crate::table::{FormatChange, TableFormat};
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
/// Tayriva-only paste modes. Ordinary copy still carries formula and styling.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PasteMode {
    Values,
    Formulas,
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
    table_formats: Vec<(String, TableFormat)>,
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
fn valid_fill(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
}
fn valid_number_format(value: &str) -> bool {
    [
        "general", "number", "percent", "currency", "0.00", "0%", "0.00%", "$0.00",
    ]
    .contains(&value)
}
fn intersection(a: CellRange, b: CellRange) -> CellRange {
    CellRange::new(
        a.start_row.max(b.start_row),
        a.start_col.max(b.start_col),
        a.end_row.min(b.end_row),
        a.end_col.min(b.end_col),
    )
}
fn without(block: CellRange, area: CellRange) -> Vec<CellRange> {
    if !block.intersects(&area) {
        return vec![block];
    }
    let cut = intersection(block, area);
    let mut remains = Vec::new();
    if block.start_row < cut.start_row {
        remains.push(CellRange::new(
            block.start_row,
            block.start_col,
            cut.start_row - 1,
            block.end_col,
        ));
    }
    if cut.end_row < block.end_row {
        remains.push(CellRange::new(
            cut.end_row + 1,
            block.start_col,
            block.end_row,
            block.end_col,
        ));
    }
    if block.start_col < cut.start_col {
        remains.push(CellRange::new(
            cut.start_row,
            block.start_col,
            cut.end_row,
            cut.start_col - 1,
        ));
    }
    if cut.end_col < block.end_col {
        remains.push(CellRange::new(
            cut.start_row,
            cut.end_col + 1,
            cut.end_row,
            block.end_col,
        ));
    }
    remains
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
        || sheet.tables.values().any(|t| t.blocks_cell_edits(coord))
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
    sheet.tables.values().any(|t| t.blocks_cell_edits(coord))
        || sheet
            .spills
            .get_spill_at(&coord)
            .is_some_and(|s| s.anchor != coord)
}
impl Workbook {
    /// Resize presentation only; never changes cells, calculations, review state or edit history.
    pub fn set_column_width(
        &mut self,
        name: &str,
        column: u32,
        width: Option<u16>,
    ) -> Result<(), CellError> {
        let sheet = self.sheets.get_mut(name).ok_or(CellError::Ref)?;
        if column >= sheet.max_cols || width.is_some_and(|value| !(48..=600).contains(&value)) {
            return Err(CellError::Value);
        }
        match width {
            Some(value) if value != 110 => {
                sheet.column_widths.insert(column, value);
            }
            _ => {
                sheet.column_widths.remove(&column);
            }
        }
        Ok(())
    }
    /// Freeze only presentation; never changes cells, calculations, review state or history.
    pub fn set_freeze_panes(
        &mut self,
        name: &str,
        freeze_top_row: bool,
        freeze_first_column: bool,
    ) -> Result<(), CellError> {
        self.set_frozen_counts(
            name,
            u32::from(freeze_top_row),
            u32::from(freeze_first_column),
        )
    }

    pub fn set_frozen_counts(
        &mut self,
        name: &str,
        rows: u32,
        columns: u32,
    ) -> Result<(), CellError> {
        let sheet = self.sheets.get_mut(name).ok_or(CellError::Ref)?;
        if rows >= sheet.max_rows || columns >= sheet.max_cols {
            return Err(CellError::Ref);
        }
        // Keep compatibility with readers that only know how to pin the first cell.
        sheet.freeze_top_row = rows > 0;
        sheet.freeze_first_column = columns > 0;
        // One and zero already have an equivalent legacy representation.
        sheet.frozen_rows = (rows > 1).then_some(rows);
        sheet.frozen_columns = (columns > 1).then_some(columns);
        Ok(())
    }

    /// Presentation-only formula visibility; leaves calculation and review state unchanged.
    pub fn set_show_formulas(&mut self, name: &str, show_formulas: bool) -> Result<(), CellError> {
        let sheet = self.sheets.get_mut(name).ok_or(CellError::Ref)?;
        sheet.show_formulas = show_formulas;
        Ok(())
    }
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
    /// Remove an ordinary worksheet without breaking references or discarding recorded evidence.
    /// Data sources and pending database edits belong to the workbook, not the sheet.
    pub fn delete_sheet(&mut self, name: &str) -> Result<(), String> {
        let removed = self.sheets.get(name).ok_or("Worksheet not found.")?;
        if self.sheets.len() <= 1 {
            return Err("Keep at least one worksheet in the workbook.".into());
        }
        if self
            .close_periods
            .iter()
            .any(|period| period.sheet_name == name)
        {
            return Err(
                "This worksheet belongs to Monthly close and cannot be deleted here.".into(),
            );
        }
        if self.reviews.iter().any(|review| review.sheet_name == name) {
            return Err(
                "This worksheet has recorded reviews or evidence and cannot be deleted.".into(),
            );
        }

        let refers_to_removed = |expr: &crate::ast::Expr| {
            fn check(expr: &crate::ast::Expr, name: &str, removed: &Sheet) -> bool {
                use crate::ast::Expr;
                match expr {
                    Expr::SheetRef { sheet, expr } => sheet == name || check(expr, name, removed),
                    Expr::StructuredRef(reference) => {
                        removed.tables.contains_key(&reference.table_name)
                    }
                    Expr::Unary { expr, .. } => check(expr, name, removed),
                    Expr::Binary { left, right, .. } => {
                        check(left, name, removed) || check(right, name, removed)
                    }
                    Expr::FunctionCall { args, .. } => {
                        args.iter().any(|arg| check(arg, name, removed))
                    }
                    Expr::ArrayLiteral(rows) => {
                        rows.iter().flatten().any(|cell| check(cell, name, removed))
                    }
                    _ => false,
                }
            }
            check(expr, name, removed)
        };
        let first_reference = self
            .sheets
            .iter()
            .filter(|(sheet, _)| sheet.as_str() != name)
            .flat_map(|(sheet, data)| {
                data.cells.values().filter_map(move |cell| {
                    let formula = cell.formula.as_deref()?;
                    crate::parser::parse_formula(formula)
                        .ok()
                        .filter(&refers_to_removed)
                        .map(|_| (sheet, cell.coord.to_a1()))
                })
            })
            .min();
        if let Some((sheet, cell)) = first_reference {
            return Err(format!(
                "Remove the reference from {sheet}!{cell} before deleting this worksheet."
            ));
        }

        let original_sheets = self.sheets.clone();
        let original_graph = self.dep_graph.clone();
        let original_stale = self.stale_calculation_sheets.clone();
        self.sheets.remove(name);
        self.stale_calculation_sheets.remove(name);
        let next = if self.active_sheet == name {
            // Choose the next active worksheet deterministically.
            self.sheets.keys().min().expect("a sheet remains").clone()
        } else {
            self.active_sheet.clone()
        };
        if let Err(error) = self.recalculate(&next) {
            self.sheets = original_sheets;
            self.dep_graph = original_graph;
            self.stale_calculation_sheets = original_stale;
            return Err(format!(
                "Could not recalculate after removing the worksheet: {error}."
            ));
        }
        self.active_sheet = next;
        self.clear_edit_history();
        Ok(())
    }
    /// Explicit NULL for an editable local-table column. Ordinary clearing
    /// remains Blank; literal "NULL" in a Text column stays literal.
    pub fn set_local_table_null(&mut self, name: &str, coord: CellCoord) -> Result<(), CellError> {
        let sheet = self.sheets.get(name).ok_or(CellError::Ref)?;
        if !sheet.tables.values().any(|table| table.local_column(coord).is_some()) {
            return Err(CellError::Sql("Select a local table data cell to set NULL.".into()));
        }
        let mut data = CellData::new(coord, String::new());
        if let Some(old) = sheet.get_cell(&coord) {
            data.style = old.style.clone();
            data.number_format = old.number_format.clone();
        }
        data.evaluated_value = Value::Null;
        self.commit_cells(name, vec![(coord, Some(data))], true)
    }

    fn commit_cells(
        &mut self,
        name: &str,
        mut changes: Vec<(CellCoord, Option<CellData>)>,
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
        for (coord, data) in &mut changes {
            if let Some(data) = data {
                if let Some((column, kind)) = sheet.tables.values().find_map(|t| t.local_column(*coord)) {
                    kind.enforce(data, column)?;
                }
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
            self.record_cell_change(CellChange {
                sheet: name.into(),
                before,
                after,
                table_formats: Vec::new(),
            });
        }
        self.invalidate_changed_reviews();
        Ok(())
    }
    fn record_cell_change(&mut self, change: CellChange) {
        self.edit_history.undo.push(change);
        while self.edit_history.undo.len() > 100
            || self
                .edit_history
                .undo
                .iter()
                .map(|c| c.before.len() + c.after.len() + c.table_formats.len())
                .sum::<usize>()
                > 100_000
        {
            self.edit_history.undo.remove(0);
        }
        self.edit_history.redo.clear();
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
    /// Apply a sparse batch of exact workbook inputs using the same atomic path
    /// as UI edits, preserving existing cell styles and typed column rules.
    pub fn edit_typed_cells(
        &mut self,
        name: &str,
        mut cells: Vec<CellData>,
    ) -> Result<(), CellError> {
        let sheet = self.sheets.get(name).ok_or(CellError::Ref)?;
        let mut seen = std::collections::BTreeSet::new();
        for data in &mut cells {
            if !seen.insert(data.coord) {
                return Err(CellError::Value);
            }
            if let Some(old) = sheet.get_cell(&data.coord) {
                data.style = old.style.clone();
                data.number_format = old.number_format.clone();
            }
        }
        self.commit_cells(name, cells.into_iter().map(|d| (d.coord, Some(d))).collect(), true)
    }

    /// Changes one attribute across a range without flattening mixed styles.
    pub fn format_range_attribute(
        &mut self,
        name: &str,
        range: CellRange,
        attribute: &str,
        value: &str,
    ) -> Result<(), CellError> {
        let change = match attribute {
            "bold" if matches!(value, "true" | "false") => FormatChange::Bold(value == "true"),
            "italic" if matches!(value, "true" | "false") => FormatChange::Italic(value == "true"),
            "fill" if value.is_empty() => FormatChange::Fill(None),
            "fill" if valid_fill(value) => FormatChange::Fill(Some(value.into())),
            "number_format" if valid_number_format(value) => {
                FormatChange::NumberFormat(value.into())
            }
            _ => return Err(CellError::Value),
        };
        self.apply_range_format(name, range, change)
    }
    pub fn format_range(
        &mut self,
        name: &str,
        range: CellRange,
        style: CellStyle,
    ) -> Result<(), CellError> {
        if style
            .fill
            .as_deref()
            .is_some_and(|value| !valid_fill(value))
            || style
                .number_format
                .as_deref()
                .is_some_and(|value| !valid_number_format(value))
        {
            return Err(CellError::Value);
        }
        self.apply_range_format(name, range, FormatChange::Style(style))
    }

    fn apply_range_format(
        &mut self,
        name: &str,
        range: CellRange,
        change: FormatChange,
    ) -> Result<(), CellError> {
        let sheet = self.sheets.get(name).ok_or(CellError::Ref)?;
        if range.start_row > range.end_row
            || range.start_col > range.end_col
            || range.end_row >= sheet.max_rows
            || range.end_col >= sheet.max_cols
        {
            return Err(CellError::Ref);
        }

        // Cut table rectangles out of the ordinary selection without visiting table rows.
        let mut ordinary = vec![range];
        let mut table_formats = Vec::new();
        for (name, table) in &sheet.tables {
            if !table.range.intersects(&range) {
                continue;
            }
            let area = intersection(range, table.range);
            let anchor = table.anchor;
            let start_row = area.start_row - anchor.row;
            table_formats.push((
                name.clone(),
                TableFormat {
                    range: CellRange::new(
                        start_row,
                        area.start_col - anchor.col,
                        area.end_row - anchor.row,
                        area.end_col - anchor.col,
                    ),
                    // A point edit remains a point edit, even for a one-row result.
                    through_end: start_row <= 1
                        && area.end_row == table.range.end_row
                        && area.start_row < area.end_row,
                    change: change.clone(),
                },
            ));
            ordinary = ordinary
                .into_iter()
                .flat_map(|block| without(block, area))
                .collect();
        }
        let mut count: u64 = 0;
        let mut changes = Vec::new();
        for block in ordinary {
            count = count.saturating_add(u64::from(block.width()) * u64::from(block.height()));
            if count > 10_000 {
                return Err(CellError::Value);
            }
            for coord in coords(block)? {
                let mut data = sheet
                    .get_cell(&coord)
                    .cloned()
                    .unwrap_or_else(|| CellData::new(coord, String::new()));
                // Older workbooks can store the number format only in CellData.
                data.style.number_format = data.style.number_format.or(data.number_format);
                change.apply(&mut data.style);
                data.number_format = data.style.number_format.clone();
                changes.push((coord, Some(data)));
            }
        }

        if changes.is_empty() {
            self.record_cell_change(CellChange {
                sheet: name.into(),
                before: HashMap::new(),
                after: HashMap::new(),
                table_formats: Vec::new(),
            });
        } else {
            // Validate and commit the whole ordinary portion before changing presentation.
            self.commit_cells(name, changes, true)?;
        }
        let sheet = self.sheets.get_mut(name).unwrap();
        for (table, format) in &table_formats {
            sheet
                .tables
                .get_mut(table)
                .unwrap()
                .formats
                .push(format.clone());
        }
        self.edit_history.undo.last_mut().unwrap().table_formats = table_formats;
        Ok(())
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
                    .map(|c| (&c.raw_value, c.is_staged, &c.style, &c.number_format))
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
                    let rendered = viewport.cells.get(&coord.to_a1());
                    let value = rendered
                        .map(|c| c.raw_value.clone())
                        .unwrap_or(Value::Blank);
                    let mut data = CellData::new(coord, value.to_string_repr());
                    data.formula = None;
                    data.evaluated_value = value;
                    data.is_dirty = false;
                    if let Some(cell) = rendered.filter(|c| c.table_name.is_some()) {
                        data.style = cell.style.clone();
                        data.number_format = cell.number_format.clone();
                    }
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
    /// Paste source values or formulas into an ordinary range, without changing
    /// destination styles. A snapshot binds the operation to the copied content.
    pub fn paste_special(
        &mut self,
        name: &str,
        range: CellRange,
        target_sheet: &str,
        target: CellCoord,
        expected_snapshot: &str,
        mode: PasteMode,
    ) -> Result<(), CellError> {
        if self.capture_range(name, range)?.snapshot != expected_snapshot
            || (matches!(mode, PasteMode::Values) && self.stale_calculation_sheets.contains(name))
        {
            return Err(CellError::Value);
        }
        let source = self.sheets.get(name).ok_or(CellError::Ref)?;
        let dest_sheet = self.sheets.get(target_sheet).ok_or(CellError::Ref)?;
        let bottom = target
            .row
            .checked_add(range.end_row - range.start_row)
            .ok_or(CellError::Ref)?;
        let right = target
            .col
            .checked_add(range.end_col - range.start_col)
            .ok_or(CellError::Ref)?;
        if bottom >= dest_sheet.max_rows || right >= dest_sheet.max_cols {
            return Err(CellError::Ref);
        }
        let viewport = self.clipboard_viewport(name, range)?;
        let mut changes = Vec::new();
        for coord in coords(range)? {
            let dest = CellCoord::new(
                target.row + (coord.row - range.start_row),
                target.col + (coord.col - range.start_col),
            );
            let mut data = match mode {
                PasteMode::Values => {
                    let value = viewport
                        .cells
                        .get(&coord.to_a1())
                        .map(|cell| cell.raw_value.clone())
                        .unwrap_or(Value::Blank);
                    // Never parse display strings: "0012", "=literal", exact
                    // decimals, SQL NULL and dates must retain their types.
                    CellData {
                        coord: dest,
                        raw_input: value.to_string_repr(),
                        evaluated_value: value,
                        formula: None,
                        is_dirty: false,
                        style: Default::default(),
                        number_format: None,
                    }
                }
                PasteMode::Formulas => {
                    if source_is_materialized(source, coord) {
                        continue;
                    }
                    let Some(formula) = source
                        .get_cell(&coord)
                        .and_then(|cell| cell.formula.as_ref())
                    else {
                        continue;
                    };
                    CellData::new(
                        dest,
                        rewrite_formula(
                            formula,
                            i64::from(dest.row) - i64::from(coord.row),
                            i64::from(dest.col) - i64::from(coord.col),
                            None,
                        )?,
                    )
                }
            };
            if let Some(old) = dest_sheet.get_cell(&dest) {
                data.style = old.style.clone();
                data.number_format = old.number_format.clone();
            }
            changes.push((dest, Some(data)));
        }
        if changes.is_empty() {
            return Err(CellError::Value);
        }
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
        if let Err(e) = self.replay_cell_change(&change, edits, false) {
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
        if let Err(e) = self.replay_cell_change(&change, edits, true) {
            self.edit_history.redo.push(change);
            return Err(e);
        }
        self.edit_history.undo.push(change);
        Ok(())
    }

    fn replay_cell_change(
        &mut self,
        change: &CellChange,
        edits: Vec<(CellCoord, Option<CellData>)>,
        redo: bool,
    ) -> Result<(), CellError> {
        let sheet = self.sheets.get(&change.sheet).ok_or(CellError::Ref)?;
        for (name, _) in &change.table_formats {
            if !sheet
                .tables
                .get(name)
                .is_some_and(|table| redo || !table.formats.is_empty())
            {
                return Err(CellError::Ref);
            }
        }
        if !edits.is_empty() {
            self.commit_cells(&change.sheet, edits, false)?;
        }
        let sheet = self.sheets.get_mut(&change.sheet).unwrap();
        for (name, format) in &change.table_formats {
            let formats = &mut sheet.tables.get_mut(name).unwrap().formats;
            if redo {
                formats.push(format.clone());
            } else {
                formats.pop();
            }
        }
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
