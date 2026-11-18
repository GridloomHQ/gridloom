//! Bounded, conservative expression-dependency navigation, not contribution attribution.
use crate::{ast::Expr, parser::parse_formula, CellCoord, CellError, CellRange, Workbook};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const MAX_NEIGHBORS: usize = 1000;
const MAX_SCAN_CELLS: usize = 100_000;

#[derive(Debug, Clone, Serialize)]
pub struct DependencyTarget {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub sheet_name: Option<String>,
    pub coord: Option<CellCoord>,
    pub range: Option<CellRange>,
    pub formula: Option<String>,
    pub source_name: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct DependencyNavigation {
    pub sheet_name: String,
    pub coord: CellCoord,
    pub upstream: Vec<DependencyTarget>,
    pub downstream: Vec<DependencyTarget>,
    pub warnings: Vec<String>,
    pub truncated: bool,
}
fn target(kind: &str, sheet: &str, range: CellRange, label: String) -> DependencyTarget {
    DependencyTarget {
        id: serde_json::to_string(&(kind, sheet, &label, range)).unwrap(),
        kind: kind.into(),
        label,
        sheet_name: Some(sheet.into()),
        coord: Some(CellCoord::new(range.start_row, range.start_col)),
        range: Some(range),
        formula: None,
        source_name: None,
    }
}
fn cell(sheet: &str, coord: CellCoord) -> DependencyTarget {
    target(
        "cell",
        sheet,
        CellRange::single(coord),
        format!("{sheet}!{}", coord.to_a1()),
    )
}
struct Collector<'a> {
    workbook: &'a Workbook,
    targets: BTreeMap<String, DependencyTarget>,
    warnings: BTreeSet<String>,
    truncated: bool,
}
impl<'a> Collector<'a> {
    fn new(workbook: &'a Workbook) -> Self {
        Self {
            workbook,
            targets: BTreeMap::new(),
            warnings: BTreeSet::new(),
            truncated: false,
        }
    }
    fn add(&mut self, t: DependencyTarget) {
        if self.targets.len() < MAX_NEIGHBORS {
            self.targets.insert(t.id.clone(), t);
        } else {
            self.truncated = true;
        }
    }
    fn expr(&mut self, sheet: &str, current: CellCoord, expr: &Expr, depth: usize) {
        if depth > 64 {
            self.truncated = true;
            return;
        }
        match expr {
            Expr::SheetRef { sheet, expr } => self.expr(sheet, current, expr, depth + 1),
            Expr::CellRef(c) => self.add(cell(sheet, *c)),
            Expr::RangeRef(r) => {
                self.add(target("range", sheet, *r, format!("{sheet}!{}", r.to_a1())))
            }
            Expr::SpillRef(c) => {
                let r = self
                    .workbook
                    .sheets
                    .get(sheet)
                    .and_then(|s| s.spills.active_spills.get(c))
                    .map_or(CellRange::single(*c), |s| s.range);
                self.add(target("spill", sheet, r, format!("{sheet}!{}#", c.to_a1())));
            }
            Expr::StructuredRef(sref) => {
                let table = self.workbook.sheets.get(sheet).and_then(|s| {
                    s.tables
                        .values()
                        .find(|t| t.name.eq_ignore_ascii_case(&sref.table_name))
                });
                match table.and_then(|t| {
                    t.resolve_structured_ref(sref, Some(current))
                        .ok()
                        .map(|r| (t, r))
                }) {
                    Some((t, r)) => {
                        let mut out = target(
                            "table",
                            sheet,
                            r,
                            format!(
                                "{sheet}!{}[{}{}]",
                                t.name,
                                if sref.is_row_relative { "@" } else { "" },
                                sref.column_name
                            ),
                        );
                        out.source_name = Some(t.source_name.clone());
                        self.add(out);
                    }
                    None => {
                        self.warnings.insert(format!(
                            "Unresolved table reference on {sheet}: {}[{}].",
                            sref.table_name, sref.column_name
                        ));
                    }
                }
            }
            Expr::Unary { expr, .. } => self.expr(sheet, current, expr, depth + 1),
            Expr::Binary { left, right, .. } => {
                self.expr(sheet, current, left, depth + 1);
                self.expr(sheet, current, right, depth + 1);
            }
            Expr::FunctionCall { name, args } => {
                if name.eq_ignore_ascii_case("INDIRECT") || name.eq_ignore_ascii_case("OFFSET") {
                    self.warnings.insert(format!("{name} is an unsupported dynamic-reference boundary. Only explicit argument references are shown; computed reference targets are not inferred."));
                }
                if name.eq_ignore_ascii_case("SQLVALUE") || name.eq_ignore_ascii_case("SQLTABLE") {
                    self.warnings.insert("SQL is a cached-query boundary. Database rows and aggregate/join contributions are not inferred.".into());
                }
                for arg in args {
                    self.expr(sheet, current, arg, depth + 1);
                }
            }
            Expr::ArrayLiteral(rows) => {
                for expr in rows.iter().flatten() {
                    self.expr(sheet, current, expr, depth + 1);
                }
            }
            Expr::Literal(_) => {}
        }
    }
    fn at(&mut self, sheet_name: &str, coord: CellCoord) {
        let Some(sheet) = self.workbook.sheets.get(sheet_name) else {
            return;
        };
        // Table cells and spill children are calculated projections, not independent inputs.
        if let Some(t) = sheet.tables.values().find(|t| t.range.contains(coord)) {
            let mut out = target(
                "source",
                sheet_name,
                t.range,
                format!("{} · cached source {}", t.name, t.source_name),
            );
            out.source_name = Some(t.source_name.clone());
            // A source boundary is not a cell to recurse into.
            out.coord = None;
            self.add(out);
            self.warnings.insert(
                "Table cells come from cached sources; source-row contribution is not inferred."
                    .into(),
            );
            return;
        }
        if let Some(spill) = sheet.spills.get_spill_at(&coord) {
            if spill.anchor != coord {
                self.add(cell(sheet_name, spill.anchor));
                return;
            }
        }
        if let Some(c) = sheet.get_cell(&coord) {
            if c.evaluated_value == crate::Value::Error(CellError::Name) {
                self.warnings.insert("An unrecognized formula/name is present. Navigation shows parsed references only, not an inferred meaning for unknown functions.".into());
            }
        }
        if let Some(formula) = sheet.get_cell(&coord).and_then(|c| c.formula.as_ref()) {
            match parse_formula(formula) {
                Ok(expr) => self.expr(sheet_name, coord, &expr, 0),
                Err(_) => {
                    self.warnings.insert(format!(
                        "Cannot parse {sheet_name}!{}; its dependencies are incomplete.",
                        coord.to_a1()
                    ));
                }
            }
        }
    }
}
impl Workbook {
    /// Direct neighbors only. Follow a cell to explore another hop. Ranges remain grouped.
    /// Scans current formulas instead of relying on the recalculation graph's cached state.
    pub fn navigate_dependencies(
        &self,
        sheet_name: &str,
        coord: CellCoord,
    ) -> Result<DependencyNavigation, CellError> {
        let sheet = self.sheets.get(sheet_name).ok_or(CellError::Ref)?;
        if coord.row >= sheet.max_rows || coord.col >= sheet.max_cols {
            return Err(CellError::Ref);
        }
        let mut upstream = Collector::new(self);
        upstream.at(sheet_name, coord);
        let mut warnings = upstream.warnings.clone();
        warnings.insert("Direct expression dependencies include all branches, not just actual contributions. Ranges are grouped; inspect a cell to follow another hop.".into());
        warnings.insert("Downstream lists formula cells and spill projections, not pivot/chart consumers or external database changes.".into());
        let mut downstream = BTreeMap::new();
        let mut truncated = upstream.truncated;
        let mut scanned = 0;
        let sorted: BTreeMap<_, _> = self.sheets.iter().collect();
        'sheets: for (name, sheet) in sorted {
            let cells: BTreeMap<_, _> = sheet.cells.iter().collect();
            for c in cells.values() {
                scanned += 1;
                if scanned > MAX_SCAN_CELLS {
                    truncated = true;
                    break 'sheets;
                }
                if c.formula.is_none() {
                    continue;
                }
                let mut deps = Collector::new(self);
                deps.at(name, c.coord);
                warnings.extend(deps.warnings);
                truncated |= deps.truncated;
                if deps.targets.values().any(|t| {
                    t.kind != "source"
                        && t.sheet_name.as_deref() == Some(sheet_name)
                        && t.range.is_some_and(|r| r.contains(coord))
                }) {
                    let mut t = cell(name, c.coord);
                    t.formula = c.formula.clone();
                    if downstream.len() < MAX_NEIGHBORS {
                        downstream.insert(t.id.clone(), t);
                    } else {
                        truncated = true;
                    }
                }
            }
        }
        // Spill projections aren't stored as cells. Show the region as one group.
        for spill in sheet
            .spills
            .active_spills
            .values()
            .filter(|s| s.anchor == coord && (s.rows > 1 || s.cols > 1))
        {
            let mut t = target(
                "spill",
                sheet_name,
                spill.range,
                format!("{sheet_name}!{} · spill output", spill.range.to_a1()),
            );
            t.coord = None;
            downstream.insert(t.id.clone(), t);
        }
        if truncated {
            warnings.insert("Navigation is incomplete: a 1,000-neighbor, 100,000-cell scan or 64-level expression limit was reached.".into());
        }
        Ok(DependencyNavigation {
            sheet_name: sheet_name.into(),
            coord,
            upstream: upstream.targets.into_values().collect(),
            downstream: downstream.into_values().collect(),
            warnings: warnings.into_iter().collect(),
            truncated,
        })
    }
}
