//! Bounded whole-row insertion. Positional data and formula references move together, or nothing changes.
use crate::{
    ast::Expr,
    parser::{parse_formula, Lexer, Token},
    value::{CellCoord, CellRange, Value},
    Workbook,
};

const MAX_INSERT_COUNT: u32 = 1_000;
// The editable grid uses a single 26px-per-row scroll surface. Keep it
// below 32 million CSS pixels; the separate ledger uses bounded blocks.
const MAX_WORKSHEET_ROWS: u32 = 1_200_000;

#[derive(Debug, PartialEq, Eq)]
pub struct RowInsertError(pub String);
impl std::fmt::Display for RowInsertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for RowInsertError {}

fn error(msg: impl Into<String>) -> RowInsertError {
    RowInsertError(msg.into())
}

#[derive(Clone, Copy)]
struct Shift {
    at: u32,
    count: u32,
}
impl Shift {
    fn row(self, row: u32) -> Result<u32, RowInsertError> {
        if row < self.at {
            return Ok(row);
        }
        row.checked_add(self.count)
            .filter(|r| *r < u32::MAX)
            .ok_or_else(|| error("A shifted reference would exceed the supported address range."))
    }

    fn cell(self, coord: CellCoord) -> Result<CellCoord, RowInsertError> {
        Ok(CellCoord::new(self.row(coord.row)?, coord.col))
    }

    fn range(self, range: CellRange) -> Result<CellRange, RowInsertError> {
        Ok(CellRange::new(
            self.row(range.start_row)?,
            range.start_col,
            self.row(range.end_row)?,
            range.end_col,
        ))
    }
}

/// Plan whether each parsed cell/range points to the affected worksheet. In
/// particular, a quoted or parenthesized sheet qualifier is not a string
/// search, and a structured name stays bound to the table rather than text.
fn plan_references(
    expr: &mut Expr,
    refers_to_target: bool,
    sheet: &str,
    shift: Shift,
    refs: &mut Vec<bool>,
) -> Result<(), RowInsertError> {
    match expr {
        Expr::SheetRef {
            sheet: target,
            expr,
        } => {
            plan_references(expr, target == sheet, sheet, shift, refs)?;
        }
        Expr::CellRef(coord) => {
            refs.push(refers_to_target);
            if refers_to_target {
                *coord = shift.cell(*coord)?;
            }
        }
        Expr::RangeRef(range) => {
            refs.push(refers_to_target);
            if refers_to_target {
                let shifted = shift.range(*range)?;
                if u64::from(shifted.height()) * u64::from(shifted.width()) > 100_000 {
                    return Err(error(
                        "The expanded reference exceeds the maximum formula range.",
                    ));
                }
                *range = shifted;
            }
        }
        Expr::SpillRef(_) => {
            return Err(error(
                "Spill references are not supported by row insertion.",
            ));
        }
        Expr::FunctionCall { name, args } => {
            if matches!(name.as_str(), "INDIRECT" | "OFFSET" | "ADDRESS") {
                return Err(error(
                    "Computed references are not supported by row insertion.",
                ));
            }
            for arg in args {
                plan_references(arg, refers_to_target, sheet, shift, refs)?;
            }
        }
        Expr::ArrayLiteral(rows) => {
            for value in rows.iter_mut().flatten() {
                plan_references(value, refers_to_target, sheet, shift, refs)?;
            }
        }
        Expr::Unary { expr, .. } => plan_references(expr, refers_to_target, sheet, shift, refs)?,
        Expr::Binary { left, right, .. } => {
            plan_references(left, refers_to_target, sheet, shift, refs)?;
            plan_references(right, refers_to_target, sheet, shift, refs)?;
        }
        Expr::Literal(_) | Expr::StructuredRef(_) => {}
    }
    Ok(())
}

/// Keep spelling, whitespace and absolute markers; only replace the row digits.
fn rewrite_ref_text(text: &str, shift: Shift) -> Result<String, RowInsertError> {
    let mut output = String::new();
    for (i, token) in text.split_inclusive(':').enumerate() {
        if i > 1 {
            return Err(error("Unsupported range reference syntax."));
        }
        let digits = token
            .find(|c: char| c.is_ascii_digit())
            .ok_or_else(|| error("Invalid reference row."))?;
        let suffix = token[digits..]
            .find(|c: char| !c.is_ascii_digit())
            .map_or(token.len(), |n| digits + n);
        let row = token[digits..suffix]
            .parse::<u32>()
            .ok()
            .and_then(|n| n.checked_sub(1))
            .ok_or_else(|| error("Invalid reference row."))?;
        let next = shift.row(row)?;
        output.push_str(&token[..digits]);
        if next != row {
            output.push_str(&(next + 1).to_string());
        } else {
            output.push_str(&token[digits..suffix]);
        }
        output.push_str(&token[suffix..]);
    }
    Ok(output)
}

fn rewrite_formula(
    input: &str,
    source_sheet: &str,
    target_sheet: &str,
    shift: Shift,
) -> Result<Option<String>, RowInsertError> {
    let malformed =
        || error("A formula cannot be parsed safely. Correct it before inserting rows.");
    let mut expected = parse_formula(input).map_err(|_| malformed())?;
    let mut references = Vec::new();
    plan_references(
        &mut expected,
        source_sheet == target_sheet,
        target_sheet,
        shift,
        &mut references,
    )?;
    if !references.iter().any(|target| *target) {
        return Ok(None);
    }

    let mut lexer = Lexer::new(input);
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_spanned().map_err(|_| malformed())?;
        if token.0 == Token::Eof {
            break;
        }
        tokens.push(token);
    }
    let mut reference_plan = references.into_iter();
    let mut out = String::with_capacity(input.len());
    let mut previous = 0;
    let mut i = 0;
    while i < tokens.len() {
        let (token, span) = &tokens[i];
        if matches!(token, Token::CellRef(_) | Token::RangeRef(_)) {
            let mut end = span.end;
            // With whitespace before the colon the parser builds a range from
            // three separate tokens. Both endpoints share the same qualifier.
            if matches!(token, Token::CellRef(_))
                && matches!(tokens.get(i + 1).map(|v| &v.0), Some(Token::Colon))
                && matches!(tokens.get(i + 2).map(|v| &v.0), Some(Token::CellRef(_)))
            {
                end = tokens[i + 2].1.end;
                i += 2;
            }
            if reference_plan.next().ok_or_else(malformed)? {
                out.push_str(&input[previous..span.start]);
                out.push_str(&rewrite_ref_text(&input[span.start..end], shift)?);
                previous = end;
            }
        }
        i += 1;
    }
    out.push_str(&input[previous..]);
    if reference_plan.next().is_some() || parse_formula(&out).map_err(|_| malformed())? != expected
    {
        return Err(error(
            "The rewritten formula could not be verified; no rows were inserted.",
        ));
    }
    Ok((out != input).then_some(out))
}

impl Workbook {
    /// Insert blank rows immediately before `at` (zero based), retaining the
    /// current selection address in UI and growing the worksheet row count.
    pub fn insert_rows(
        &mut self,
        sheet_name: &str,
        at: u32,
        count: u32,
    ) -> Result<(), RowInsertError> {
        let target = self
            .sheets
            .get(sheet_name)
            .ok_or_else(|| error("Worksheet not found."))?;
        if at >= target.max_rows || count == 0 || count > MAX_INSERT_COUNT {
            return Err(error(
                "Choose a worksheet row and insert between 1 and 1,000 rows.",
            ));
        }
        let rows = target
            .max_rows
            .checked_add(count)
            .filter(|new| *new <= MAX_WORKSHEET_ROWS)
            .ok_or_else(|| {
                error("The new worksheet size would exceed 1,200,000 editable worksheet rows.")
            })?;
        if self
            .close_periods
            .iter()
            .any(|p| p.sheet_name == sheet_name)
        {
            return Err(error(
                "Rows cannot be inserted into a worksheet managed by Monthly close.",
            ));
        }
        if self.sheets.values().any(|s| {
            !s.spills.active_spills.is_empty()
                || s.cells.values().any(|cell| {
                    matches!(
                        cell.evaluated_value,
                        Value::Array(_) | Value::Error(crate::error::CellError::Spill)
                    )
                })
        }) {
            return Err(error(
                "Resolve or remove dynamic spill formulas before inserting rows.",
            ));
        }
        if self.named_sources.values().any(|source| {
            source
                .parameters
                .iter()
                .any(|parameter| parameter.cell_binding.is_some())
        }) {
            return Err(error("A named query uses a worksheet cell binding. Row insertion cannot safely update those bindings."));
        }
        if self
            .reviews
            .iter()
            .any(|review| review.sheet_name == sheet_name && review.coord.row >= at)
        {
            return Err(error("This insertion would move a reviewed cell or open question. Its historical address cannot be rewritten safely."));
        }
        for table in target.tables.values() {
            if table.range.start_row < at && at <= table.range.end_row {
                return Err(error(format!("This insertion would split worksheet table '{}'. Choose a row above or below it.",table.name)));
            }
        }
        let shift = Shift { at, count };
        let mut formulas = Vec::new();
        for (name, sheet) in &self.sheets {
            for (key, cell) in &sheet.cells {
                if let Some(formula) = &cell.formula {
                    if let Some(updated) = rewrite_formula(formula, name, sheet_name, shift)
                        .map_err(|e| error(format!("{name}!{key}: {e}")))?
                    {
                        formulas.push((name.clone(), key.clone(), updated));
                    }
                }
            }
        }
        let mut sheets = self.sheets.clone();
        for (name, key, formula) in formulas {
            let cell = sheets
                .get_mut(&name)
                .and_then(|s| s.cells.get_mut(&key))
                .ok_or_else(|| error("Formula target disappeared."))?;
            let input = &cell.raw_input;
            let prefix = input.len() - input.trim_start().len();
            let suffix = input.trim_end().len();
            // Existing raw input may have optional whitespace around the formula.
            cell.raw_input = if prefix <= suffix && Some(input.trim()) == cell.formula.as_deref() {
                format!("{}{}{}", &input[..prefix], formula, &input[suffix..])
            } else {
                formula.clone()
            };
            cell.formula = Some(formula);
        }
        let shifted = sheets
            .get_mut(sheet_name)
            .ok_or_else(|| error("Worksheet not found."))?;
        let mut cells = std::collections::HashMap::with_capacity(shifted.cells.len());
        for (address, mut cell) in std::mem::take(&mut shifted.cells) {
            if address != cell.coord.to_a1() || cell.coord.row >= target.max_rows {
                return Err(error(
                    "The worksheet contains an inconsistent cell address.",
                ));
            }
            cell.coord = shift.cell(cell.coord)?;
            cells.insert(cell.coord.to_a1(), cell);
        }
        shifted.cells = cells;
        for table in shifted.tables.values_mut() {
            if !table.range.contains(table.anchor) || table.range.end_row >= target.max_rows {
                return Err(error(
                    "The worksheet contains an inconsistent table address.",
                ));
            }
            table.anchor = shift.cell(table.anchor)?;
            table.range = shift.range(table.range)?;
        }
        shifted.ledger_links = std::mem::take(&mut shifted.ledger_links)
            .into_iter()
            .map(|(address, link)| {
                let coord = CellCoord::from_a1(&address)
                    .map_err(|_| error("Invalid ledger link address."))?;
                if coord.row >= target.max_rows || address != coord.to_a1() {
                    return Err(error("Invalid ledger link address."));
                }
                Ok((shift.cell(coord)?.to_a1(), link))
            })
            .collect::<Result<_, RowInsertError>>()?;
        shifted.max_rows = rows;
        let previous_sheets = std::mem::replace(&mut self.sheets, sheets);
        let previous_graph = self.dep_graph.clone();
        if let Err(failure) = self.recalculate(sheet_name) {
            self.sheets = previous_sheets;
            self.dep_graph = previous_graph;
            return Err(error(format!(
                "Recalculation failed ({failure}); no rows were inserted."
            )));
        }
        // The existing per-sheet cell history cannot represent atomic structural edits.
        self.clear_edit_history();
        Ok(())
    }
}
