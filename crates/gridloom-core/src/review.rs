//! Local, snapshot-bound review notes. These are not authenticated signatures or audit assurance.
use crate::ast::Expr;
use crate::error::CellError;
use crate::functions::EvaluationContext;
use crate::parser::parse_formula;
use crate::value::{CellCoord, Value};
use crate::workbook::{Workbook, WorkbookEvalContext};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

const MAX_NODES: usize = 256;
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReviewDecision {
    Reviewed,
    Question,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceReference {
    pub label: String,
    pub reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRecord {
    pub id: Uuid,
    pub sheet_name: String,
    pub coord: CellCoord,
    pub decision: ReviewDecision,
    pub reviewer: String,
    pub note: String,
    pub evidence: Vec<EvidenceReference>,
    pub created_at: DateTime<Utc>,
    pub snapshot: String,
    pub invalidated: bool,
    pub invalidation_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReviewRequest {
    pub sheet_name: String,
    pub row: u32,
    pub col: u32,
    pub expected_snapshot: String,
    pub decision: ReviewDecision,
    pub reviewer: String,
    pub note: String,
    pub evidence: Vec<EvidenceReference>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExplanationNode {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub formula: Option<String>,
    pub value: Option<Value>,
    pub depends_on: Vec<String>,
    pub detail: Option<String>,
    pub coord: Option<CellCoord>,
}

#[derive(Debug, Serialize)]
pub struct CellExplanation {
    pub sheet_name: String,
    pub coord: CellCoord,
    pub value: Value,
    pub formula: Option<String>,
    pub nodes: Vec<ExplanationNode>,
    pub warnings: Vec<String>,
    pub snapshot: String,
    pub can_review: bool,
    pub review: Option<ReviewRecord>,
}

impl Workbook {
    /// Uses parsed, conservative formula dependencies and the cached calculation only.
    /// It intentionally does not execute SQL or claim row-level aggregate provenance.
    pub fn explain_cell(
        &self,
        sheet_name: &str,
        coord: CellCoord,
    ) -> Result<CellExplanation, CellError> {
        let sheet = self.sheets.get(sheet_name).ok_or(CellError::Ref)?;
        if coord.row >= sheet.max_rows || coord.col >= sheet.max_cols {
            return Err(CellError::Ref);
        }
        let ctx = WorkbookEvalContext {
            workbook: self,
            sheet_name,
            current_coord: coord,
        };
        let value = ctx.get_cell_value(coord)?;
        let formula = sheet.get_cell(&coord).and_then(|c| c.formula.clone());
        let mut trace = TraceBuilder {
            workbook: self,
            sheet_name,
            nodes: BTreeMap::new(),
            parts: BTreeMap::new(),
            warnings: BTreeSet::new(),
            visiting: BTreeSet::new(),
            can_review: !value.is_blank() && !contains_error(&value),
            sql_counter: 0,
        };
        trace.cell(coord, 0)?;
        if self.stale_calculation_sheets.contains(sheet_name) {
            trace.can_review = false;
            trace.warnings.insert("This sheet could not be recalculated after database writeback. Recalculate successfully before reviewing.".into());
        }
        if value.is_blank() {
            trace
                .warnings
                .insert("Blank cells have no value to review.".into());
        }
        if !trace.can_review && !value.is_blank() {
            trace.warnings.insert("The calculation or one of its dependencies contains an error or an incomplete trace.".into());
        }
        trace.warnings.insert("Dependencies are conservative: all formula branches are included, not just the branch used by the current result.".into());
        trace.warnings.insert("Evidence references and reviewer names are entered locally, not verified or authenticated. External documents are not monitored for changes.".into());
        // Staged overlays are currently separate from formula evaluation. Never label that mixed view reviewed.
        if !self.staged.patches.is_empty() {
            trace.can_review = false;
            trace.warnings.insert("Pending database edits are not included in formula calculations. Apply or discard them, refresh sources, then review.".into());
        }
        trace.parts.insert(
            "pending_database_edits".into(),
            serde_json::json!(!self.staged.patches.is_empty()),
        );
        let nodes: Vec<_> = trace.nodes.into_values().collect();
        let warnings: Vec<_> = trace.warnings.into_iter().collect();
        // Versioned, canonical content digest. Not a signature or proof that a database/document is current.
        let bytes = serde_json::to_vec(&(1, &trace.parts, &nodes, &warnings, trace.can_review))
            .map_err(|_| CellError::Value)?;
        let snapshot = format!("v1:{:x}", Sha256::digest(bytes));
        let review = self.reviews.iter().rev().find(|r| r.sheet_name == sheet_name && r.coord == coord).cloned().map(|mut r| {
            if r.snapshot != snapshot || !trace.can_review {
                r.invalidated = true;
                r.invalidation_reason.get_or_insert_with(|| "The calculation, dependencies, or reviewable state changed. Review again.".into());
            }
            r
        });
        Ok(CellExplanation {
            sheet_name: sheet_name.into(),
            coord,
            value: preview_value(&value),
            formula,
            nodes,
            warnings,
            snapshot,
            can_review: trace.can_review,
            review,
        })
    }

    pub fn record_review(&mut self, mut request: ReviewRequest) -> Result<ReviewRecord, String> {
        request.reviewer = request.reviewer.trim().into();
        request.note = request.note.trim().into();
        if request.reviewer.is_empty() || request.reviewer.len() > 120 || request.note.len() > 4000
        {
            return Err("Enter a reviewer name (up to 120 characters) and a note of at most 4000 characters.".into());
        }
        if request.decision == ReviewDecision::Question && request.note.is_empty() {
            return Err("Describe the question you are circling up.".into());
        }
        if request.evidence.len() > 20 {
            return Err("Attach at most 20 evidence references.".into());
        }
        for evidence in &mut request.evidence {
            evidence.label = evidence.label.trim().into();
            evidence.reference = evidence.reference.trim().into();
            if evidence.label.is_empty()
                || evidence.label.len() > 200
                || evidence.reference.is_empty()
                || evidence.reference.len() > 2000
            {
                return Err("Each evidence reference needs a label (up to 200 characters) and a reference (up to 2000 characters).".into());
            }
        }
        let coord = CellCoord::new(request.row, request.col);
        let explanation = self
            .explain_cell(&request.sheet_name, coord)
            .map_err(|e| e.message().to_string())?;
        if explanation.snapshot != request.expected_snapshot {
            return Err("This calculation changed while you were reviewing it. Reload its explanation and review again.".into());
        }
        if !explanation.can_review {
            return Err(
                "This calculation cannot be reviewed yet. Resolve its warnings first.".into(),
            );
        }
        let record = ReviewRecord {
            id: Uuid::new_v4(),
            sheet_name: request.sheet_name,
            coord,
            decision: request.decision,
            reviewer: request.reviewer,
            note: request.note,
            evidence: request.evidence,
            created_at: Utc::now(),
            snapshot: explanation.snapshot,
            invalidated: false,
            invalidation_reason: None,
        };
        self.reviews.push(record.clone());
        Ok(record)
    }

    /// Sticky invalidation: restoring the old value must not silently resurrect an old sign-off.
    pub fn invalidate_changed_reviews(&mut self) {
        let changed: Vec<_> = self
            .reviews
            .iter()
            .enumerate()
            .filter(|(_, r)| !r.invalidated)
            .filter_map(|(i, r)| match self.explain_cell(&r.sheet_name, r.coord) {
                Ok(explanation) if explanation.can_review && explanation.snapshot == r.snapshot => {
                    None
                }
                _ => Some(i),
            })
            .collect();
        for i in changed {
            self.reviews[i].invalidated = true;
            self.reviews[i].invalidation_reason = Some(
                "The calculation, dependencies, or reviewable state changed. Review again.".into(),
            );
        }
    }

    pub fn review_history(&self) -> Vec<ReviewRecord> {
        self.reviews.iter().rev().cloned().map(|mut record| {
            if !record.invalidated && !matches!(self.explain_cell(&record.sheet_name, record.coord), Ok(e) if e.can_review && e.snapshot == record.snapshot) {
                record.invalidated = true;
                record.invalidation_reason = Some("The calculation, dependencies, or reviewable state changed. Review again.".into());
            }
            record
        }).collect()
    }
}

fn contains_error(value: &Value) -> bool {
    match value {
        Value::Error(_) => true,
        Value::Array(rows) => rows.iter().flatten().any(contains_error),
        _ => false,
    }
}

fn preview_value(value: &Value) -> Value {
    match value {
        Value::Array(rows) => Value::Array(
            rows.iter()
                .take(10)
                .map(|r| r.iter().take(10).map(preview_value).collect())
                .collect(),
        ),
        other => other.clone(),
    }
}

struct TraceBuilder<'a> {
    workbook: &'a Workbook,
    sheet_name: &'a str,
    nodes: BTreeMap<String, ExplanationNode>,
    parts: BTreeMap<String, serde_json::Value>,
    warnings: BTreeSet<String>,
    visiting: BTreeSet<String>,
    can_review: bool,
    sql_counter: usize,
}

impl TraceBuilder<'_> {
    fn edge(&mut self, parent: &str, child: String) {
        if let Some(node) = self.nodes.get_mut(parent) {
            if !node.depends_on.contains(&child) {
                node.depends_on.push(child);
                node.depends_on.sort();
            }
        }
    }

    fn limit(&mut self, depth: usize) -> bool {
        if self.nodes.len() >= MAX_NODES || depth > MAX_DEPTH {
            self.can_review = false;
            self.warnings.insert("The dependency trace exceeds the 256-node or 64-level review limit. A partial trace cannot be signed off.".into());
            true
        } else {
            false
        }
    }

    fn cell(&mut self, coord: CellCoord, depth: usize) -> Result<String, CellError> {
        let id = format!("cell:{}", coord.to_a1());
        if self.visiting.contains(&id) {
            self.can_review = false;
            self.warnings
                .insert("Circular dependency: this calculation cannot be signed off.".into());
            return Ok(id);
        }
        if self.nodes.contains_key(&id) || self.limit(depth) {
            return Ok(id);
        }
        let sheet = &self.workbook.sheets[self.sheet_name];
        let cell = sheet.get_cell(&coord);
        let ctx = WorkbookEvalContext {
            workbook: self.workbook,
            sheet_name: self.sheet_name,
            current_coord: coord,
        };
        let value = ctx.get_cell_value(coord)?;
        if contains_error(&value) {
            self.can_review = false;
        }
        let formula = cell.and_then(|c| c.formula.clone());
        let raw_input = cell.map(|c| c.raw_input.clone());
        let stored_value = cell.map(|c| c.evaluated_value.clone());
        self.parts.insert(id.clone(), serde_json::json!({"raw_input": raw_input, "value": value, "stored_value": stored_value}));
        self.nodes.insert(
            id.clone(),
            ExplanationNode {
                id: id.clone(),
                kind: "cell".into(),
                label: format!("{}!{}", self.sheet_name, coord.to_a1()),
                formula: formula.clone(),
                value: Some(preview_value(&value)),
                depends_on: vec![],
                detail: None,
                coord: Some(coord),
            },
        );
        self.visiting.insert(id.clone());
        if let Some(table) = sheet.tables.values().find(|t| t.range.contains(coord)) {
            let name = table.name.clone();
            let table_id = self.table(&name, depth + 1)?;
            self.edge(&id, table_id);
        } else if let Some(spill) = sheet
            .spills
            .get_spill_at(&coord)
            .filter(|s| s.anchor != coord)
        {
            let anchor = spill.anchor;
            let anchor_id = self.cell(anchor, depth + 1)?;
            self.edge(&id, anchor_id);
            self.nodes.get_mut(&id).unwrap().detail =
                Some(format!("Spill result from {}", anchor.to_a1()));
        } else if let Some(formula) = formula {
            match parse_formula(&formula) {
                Ok(ast) => self.expr(&id, &ast, coord, depth + 1)?,
                Err(_) => {
                    self.can_review = false;
                    self.warnings.insert(
                        "A formula could not be parsed; its dependencies are incomplete.".into(),
                    );
                }
            }
        }
        self.visiting.remove(&id);
        Ok(id)
    }

    fn table(&mut self, name: &str, depth: usize) -> Result<String, CellError> {
        let sheet = &self.workbook.sheets[self.sheet_name];
        let table = sheet
            .tables
            .values()
            .find(|t| t.name.eq_ignore_ascii_case(name))
            .ok_or(CellError::Ref)?;
        let id = format!("table:{}", table.name);
        if self.nodes.contains_key(&id) || self.limit(depth) {
            return Ok(id);
        }
        self.parts.insert(
            id.clone(),
            serde_json::to_value(table).map_err(|_| CellError::Value)?,
        );
        self.nodes.insert(
            id.clone(),
            ExplanationNode {
                id: id.clone(),
                kind: "table".into(),
                label: table.name.clone(),
                formula: None,
                value: None,
                depends_on: vec![],
                detail: Some(format!(
                    "Worksheet table at {}; cached data source {}",
                    table.anchor.to_a1(),
                    table.source_name
                )),
                coord: None,
            },
        );
        let source_name = table.source_name.clone();
        let source_id = self.source(&source_name, depth + 1)?;
        self.edge(&id, source_id);
        Ok(id)
    }

    fn source(&mut self, name: &str, depth: usize) -> Result<String, CellError> {
        let id = format!("source:{name}");
        if self.nodes.contains_key(&id) || self.limit(depth) {
            return Ok(id);
        }
        let source = self
            .workbook
            .named_sources
            .get(name)
            .ok_or(CellError::Ref)?;
        let stale = self.workbook.stale_sources.contains(name);
        self.parts
            .insert(format!("source_stale:{name}"), serde_json::json!(stale));
        if stale {
            self.can_review = false;
            self.warnings.insert(format!("Data source {name} is out of date after database writeback. Refresh it before reviewing this calculation."));
        }
        self.parts.insert(
            id.clone(),
            serde_json::to_value(source).map_err(|_| CellError::Value)?,
        );
        self.parts.insert(
            format!("connection:{}", source.connection_name),
            serde_json::to_value(
                self.workbook
                    .connection_configs
                    .get(&source.connection_name),
            )
            .map_err(|_| CellError::Value)?,
        );
        self.nodes.insert(
            id.clone(),
            ExplanationNode {
                id: id.clone(),
                kind: "source".into(),
                label: source.name.clone(),
                formula: None,
                value: None,
                depends_on: vec![],
                detail: Some(format!(
                    "Connection: {}\nCached rows: {}\nSQL: {}",
                    source.connection_name,
                    source.last_result.as_ref().map_or(0, |r| r.rows.len()),
                    source.sql
                )),
                coord: None,
            },
        );
        self.warnings.insert("SQL provenance stops at the named query and its cached result. Contributing database rows are not inferred, including for aggregates or joins. Refresh to observe database changes.".into());
        if source.last_result.is_none() {
            self.can_review = false;
        }
        Ok(id)
    }

    fn expr(
        &mut self,
        parent: &str,
        expr: &Expr,
        current: CellCoord,
        depth: usize,
    ) -> Result<(), CellError> {
        if self.limit(depth) {
            return Ok(());
        }
        match expr {
            Expr::CellRef(coord) => {
                let child = self.cell(*coord, depth)?;
                self.edge(parent, child);
            }
            Expr::RangeRef(range) => {
                'rows: for row in range.start_row..=range.end_row {
                    for col in range.start_col..=range.end_col {
                        if self.limit(depth) {
                            break 'rows;
                        }
                        let child = self.cell(CellCoord::new(row, col), depth)?;
                        self.edge(parent, child);
                    }
                }
            }
            Expr::SpillRef(coord) => {
                let child = self.cell(*coord, depth)?;
                self.edge(parent, child);
            }
            Expr::StructuredRef(sref) => {
                let child = self.table(&sref.table_name, depth)?;
                self.edge(parent, child);
                // Resolve to verify the exact column/row scope; retain it in the snapshot.
                let ctx = WorkbookEvalContext {
                    workbook: self.workbook,
                    sheet_name: self.sheet_name,
                    current_coord: current,
                };
                if ctx.get_table_column(sref).is_err() {
                    self.can_review = false;
                }
            }
            Expr::Unary { expr, .. } => self.expr(parent, expr, current, depth + 1)?,
            Expr::Binary { left, right, .. } => {
                self.expr(parent, left, current, depth + 1)?;
                self.expr(parent, right, current, depth + 1)?;
            }
            Expr::FunctionCall { name, args } => {
                let mut argument_parent = parent.to_string();
                if name.eq_ignore_ascii_case("SQLVALUE") || name.eq_ignore_ascii_case("SQLTABLE") {
                    let id = format!("sql:{}", self.sql_counter);
                    self.sql_counter += 1;
                    let detail = match (args.first(), args.get(1)) {
                        (Some(Expr::Literal(connection)), Some(Expr::Literal(sql))) => format!("Connection: {}\nSQL: {}\nResult is the cached formula calculation, not a live query or a list of contributing rows.", connection.to_string_repr(), sql.to_string_repr()),
                        _ => "Connection/query are formula expressions. Inspect their dependencies and formula text; only the cached calculation is reviewed, not live database rows.".into(),
                    };
                    self.nodes.insert(
                        id.clone(),
                        ExplanationNode {
                            id: id.clone(),
                            kind: "sql".into(),
                            label: name.to_uppercase(),
                            formula: None,
                            value: None,
                            depends_on: vec![],
                            detail: Some(detail),
                            coord: None,
                        },
                    );
                    // Dynamic SQL can select any configured connection. Bind the conservative set, canonically.
                    let configs: BTreeMap<_, _> = self.workbook.connection_configs.iter().collect();
                    self.parts.insert(
                        "sql_connections".into(),
                        serde_json::to_value(configs).map_err(|_| CellError::Value)?,
                    );
                    self.warnings.insert("SQL formula explanations do not re-run queries. They show the cached calculation and query boundary, not contributing database rows. Recalculate to observe database changes.".into());
                    self.edge(parent, id.clone());
                    argument_parent = id;
                }
                if name.eq_ignore_ascii_case("TODAY") || name.eq_ignore_ascii_case("NOW") {
                    self.warnings.insert("Volatile date/time formulas cover the last calculation only; recalculate before relying on a time-sensitive result.".into());
                }
                for arg in args {
                    self.expr(&argument_parent, arg, current, depth + 1)?;
                }
            }
            Expr::ArrayLiteral(rows) => {
                for expr in rows.iter().flatten() {
                    self.expr(parent, expr, current, depth + 1)?;
                }
            }
            Expr::Literal(_) => {}
        }
        Ok(())
    }
}
