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

const MAX_NODES: usize = 8192;
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
    #[serde(default)]
    pub input_snapshot: Option<ReviewInputSnapshot>,
    #[serde(default)]
    pub attachments: Vec<EvidenceAttachment>,
    #[serde(default)]
    pub question_events: Vec<QuestionEvent>,
}

/// Local issue history, not authenticated approval. Events never overwrite the original question.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum QuestionAction {
    Resolved,
    Reopened,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionEvent {
    pub id: Uuid,
    pub action: QuestionAction,
    pub actor: String,
    pub note: String,
    pub created_at: DateTime<Utc>,
    pub snapshot: String,
    pub input_snapshot: ReviewInputSnapshot,
}

#[derive(Debug, Deserialize)]
pub struct QuestionTransitionRequest {
    pub action: QuestionAction,
    pub actor: String,
    pub note: String,
    pub expected_snapshot: String,
    pub expected_last_event_id: Option<Uuid>,
}

impl ReviewRecord {
    pub fn question_is_open(&self) -> bool {
        self.decision == ReviewDecision::Question
            && !matches!(
                self.question_events.last().map(|e| &e.action),
                Some(QuestionAction::Resolved)
            )
    }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplanationNode {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub formula: Option<String>,
    pub value: Option<Value>,
    pub depends_on: Vec<String>,
    pub detail: Option<String>,
    pub coord: Option<CellCoord>,
    #[serde(default)]
    pub sheet_name: Option<String>,
    #[serde(default)]
    pub source_name: Option<String>,
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
    pub input_snapshot: ReviewInputSnapshot,
}

/// Full captured calculation inputs, not a live database backup or a signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewInputSnapshot {
    pub version: u32,
    pub sheet_name: String,
    pub coord: CellCoord,
    pub value: Value,
    pub formula: Option<String>,
    pub inputs: BTreeMap<String, serde_json::Value>,
    pub nodes: Vec<ExplanationNode>,
    pub warnings: Vec<String>,
    pub can_review: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceAttachment {
    pub id: Uuid,
    pub version: u32,
    pub previous_version_id: Option<Uuid>,
    pub filename: String,
    pub media_type: String,
    pub sha256: String,
    pub bytes: Vec<u8>,
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, Deserialize)]
pub struct EvidenceUpload {
    pub filename: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
    #[serde(default)]
    pub replaces: Option<Uuid>,
}
#[derive(Debug, Serialize)]
pub struct SnapshotChange {
    /// JSON pointer into the retained snapshot. Values are complete, not display previews.
    pub path: String,
    pub before: Option<serde_json::Value>,
    pub after: Option<serde_json::Value>,
}
#[derive(Debug, Serialize)]
pub struct ReviewComparison {
    pub before_review_id: Uuid,
    pub after_review_id: Option<Uuid>,
    pub before_snapshot: String,
    pub after_snapshot: String,
    pub changes: Vec<SnapshotChange>,
    pub summaries: Vec<ChangeSummary>,
    /// Changed cells in this captured dependency trace, not a workbook-wide impact analysis.
    pub affected_cells: Vec<AffectedCell>,
}
#[derive(Debug, Serialize)]
pub struct ChangeSummary {
    pub category: String,
    pub label: String,
    pub sheet_name: Option<String>,
    pub coord: Option<CellCoord>,
    pub before: Option<serde_json::Value>,
    pub after: Option<serde_json::Value>,
}
#[derive(Debug, Serialize)]
pub struct AffectedCell {
    pub sheet_name: String,
    pub coord: CellCoord,
    pub label: String,
}
#[derive(Debug, Serialize)]
pub struct SourceDrilldownRow {
    pub cached_row_index: usize,
    pub primary_key: BTreeMap<String, Value>,
    pub values: Vec<Value>,
}
#[derive(Debug, Serialize)]
pub struct SourceDrilldown {
    pub source_name: String,
    pub connection_name: String,
    pub base_table: String,
    pub sql: String,
    pub columns: Vec<String>,
    pub rows: Vec<SourceDrilldownRow>,
    pub total_rows: usize,
    pub offset: usize,
    pub warning: String,
}

impl Workbook {
    /// Uses parsed, conservative formula dependencies and the cached calculation only.
    /// It intentionally does not execute SQL or claim row-level aggregate provenance.
    pub fn explain_cell(
        &self,
        sheet_name: &str,
        coord: CellCoord,
    ) -> Result<CellExplanation, CellError> {
        self.build_explanation(sheet_name, coord, true)
    }

    /// Native review snapshot without cloning files from the latest review.
    pub fn explain_review_snapshot(
        &self,
        sheet_name: &str,
        coord: CellCoord,
    ) -> Result<CellExplanation, CellError> {
        self.build_explanation(sheet_name, coord, false)
    }

    fn build_explanation(
        &self,
        sheet_name: &str,
        coord: CellCoord,
        include_latest_review: bool,
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
            sheet_name: sheet_name.into(),
            root_sheet_name: sheet_name.into(),
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
        // Include close configuration, import identity and adjustment evidence for every
        // referenced close sheet, not just the root cell's sheet.
        for period in &self.close_periods {
            if trace
                .nodes
                .values()
                .any(|n| n.sheet_name.as_deref() == Some(period.sheet_name.as_str()))
            {
                let issues = self.close_integrity_issues(period.id);
                if !issues.is_empty() {
                    trace.can_review = false;
                    trace.warnings.insert(format!(
                        "Working paper {} has {} integrity issue(s): {}",
                        period.sheet_name,
                        issues.len(),
                        issues[0]
                    ));
                }
                // Avoid even temporarily serializing potentially large CSV and
                // attachment bytes into each line's capture. Those bytes live in
                // the working paper; this snapshot binds to their retained hashes.
                let journals: Vec<_> = period.adjustments.iter().map(|journal| {
                    let evidence: Vec<_> = journal.evidence.iter().map(|attachment| {
                        serde_json::json!({
                            "id":attachment.id, "version":attachment.version,
                            "previous_version_id":attachment.previous_version_id,
                            "filename":attachment.filename, "media_type":attachment.media_type,
                            "sha256":attachment.sha256, "created_at":attachment.created_at,
                        })
                    }).collect();
                    serde_json::json!({
                        "id":journal.id, "created_at":journal.created_at, "author":journal.author,
                        "rationale":journal.rationale, "lines":journal.lines, "evidence":evidence,
                    })
                }).collect();
                trace.parts.insert(
                    format!("close_adjustments:{}", period.id),
                    serde_json::Value::Array(journals),
                );
                trace.parts.insert(
                    format!("close_working_paper:{}", period.id),
                    serde_json::json!({
                        "id":period.id, "period":period.period, "version":period.version,
                        "sheet_name":period.sheet_name, "created_at":period.created_at,
                        "source_sha256":period.source_sha256, "accounts":period.accounts,
                        "prior_period_id":period.prior_period_id, "entity":period.entity,
                        "currency":period.currency, "units":period.units,
                        "report_lines":period.report_lines,
                        "reconciliation_version":period.reconciliation_version,
                    }),
                );
                // These protected worksheet inputs may not be on the result's numeric
                // path. Capture attempted tampering exactly even if totals stay equal.
                let sheet = &self.sheets[&period.sheet_name];
                let mut import_cells = BTreeMap::new();
                for row in 1..=period.accounts.len() as u32 {
                    for col in 0..=5 {
                        let coord = CellCoord::new(row, col);
                        let value = sheet.get_cell(&coord).map(|cell| {
                            serde_json::json!({
                                "raw_input": cell.raw_input, "formula": cell.formula,
                                "value": cell.evaluated_value
                            })
                        });
                        import_cells.insert(coord.to_a1(), value);
                    }
                }
                trace.parts.insert(
                    format!("close_import_cells:{}", period.id),
                    serde_json::to_value(import_cells).map_err(|_| CellError::Value)?,
                );
            }
        }
        let nodes: Vec<_> = trace.nodes.into_values().collect();
        let warnings: Vec<_> = trace.warnings.into_iter().collect();
        // Versioned, canonical content digest. Not a signature or proof that a database/document is current.
        let bytes = serde_json::to_vec(&(1, &trace.parts, &nodes, &warnings, trace.can_review))
            .map_err(|_| CellError::Value)?;
        let snapshot = format!("v1:{:x}", Sha256::digest(bytes));
        let input_snapshot = ReviewInputSnapshot {
            version: 1,
            sheet_name: sheet_name.into(),
            coord,
            value: value.clone(),
            formula: formula.clone(),
            inputs: trace.parts,
            nodes: nodes.clone(),
            warnings: warnings.clone(),
            can_review: trace.can_review,
        };
        let review = include_latest_review.then(|| self.reviews.iter().rev().find(|r| r.sheet_name == sheet_name && r.coord == coord)).flatten().cloned().map(|mut r| {
            if r.snapshot != snapshot || (r.decision == ReviewDecision::Reviewed && !trace.can_review) {
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
            input_snapshot,
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
            .explain_review_snapshot(&request.sheet_name, coord)
            .map_err(|e| e.message().to_string())?;
        if explanation.snapshot != request.expected_snapshot {
            return Err("This calculation changed while you were reviewing it. Reload its explanation and review again.".into());
        }
        if request.decision == ReviewDecision::Reviewed
            && self.reviews.iter().any(|r| {
                r.sheet_name == request.sheet_name && r.coord == coord && r.question_is_open()
            })
        {
            return Err(
                "Resolve the open questions on this cell before marking it reviewed.".into(),
            );
        }
        if request.decision == ReviewDecision::Reviewed && !explanation.can_review {
            return Err(
                "This calculation cannot be reviewed yet. Resolve its warnings first.".into(),
            );
        }
        if serde_json::to_vec(&explanation.input_snapshot)
            .map_err(|e| e.to_string())?
            .len()
            > 16 * 1024 * 1024
        {
            return Err("Review inputs exceed the 16 MiB snapshot limit; narrow the source query before reviewing.".into());
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
            input_snapshot: Some(explanation.input_snapshot),
            attachments: vec![],
            question_events: vec![],
        };
        self.reviews.push(record.clone());
        Ok(record)
    }

    /// Queue membership is independent of sign-off validity or later reviews on the same cell.
    pub fn open_questions(&self) -> Vec<ReviewRecord> {
        self.review_history()
            .into_iter()
            .filter(ReviewRecord::question_is_open)
            .collect()
    }

    pub fn transition_question(
        &mut self,
        question_id: Uuid,
        mut request: QuestionTransitionRequest,
    ) -> Result<ReviewRecord, String> {
        request.actor = request.actor.trim().into();
        request.note = request.note.trim().into();
        if request.actor.is_empty()
            || request.actor.len() > 120
            || request.note.is_empty()
            || request.note.len() > 4000
        {
            return Err("Enter a local actor label (up to 120 characters) and a reason (1–4000 characters).".into());
        }
        let index = self
            .reviews
            .iter()
            .position(|r| r.id == question_id)
            .ok_or("Question not found")?;
        let question = &self.reviews[index];
        if question.decision != ReviewDecision::Question {
            return Err("This record is not a question.".into());
        }
        if question.question_events.last().map(|e| e.id) != request.expected_last_event_id {
            return Err("Question history changed. Reload before retrying.".into());
        }
        if (request.action == QuestionAction::Resolved) != question.question_is_open() {
            return Err("Question is already in the requested state.".into());
        }
        let current = self
            .explain_review_snapshot(&question.sheet_name, question.coord)
            .map_err(|e| e.to_string())?;
        if current.snapshot != request.expected_snapshot {
            return Err("Calculation changed. Reload before updating the question.".into());
        }
        if serde_json::to_vec(&current.input_snapshot)
            .map_err(|e| e.to_string())?
            .len()
            > 16 * 1024 * 1024
        {
            return Err("Question event inputs exceed the 16 MiB snapshot limit.".into());
        }
        self.reviews[index].question_events.push(QuestionEvent {
            id: Uuid::new_v4(),
            action: request.action,
            actor: request.actor,
            note: request.note,
            created_at: Utc::now(),
            snapshot: current.snapshot,
            input_snapshot: current.input_snapshot,
        });
        Ok(self.reviews[index].clone())
    }

    /// Sticky invalidation: restoring the old value must not silently resurrect an old sign-off.
    pub fn invalidate_changed_reviews(&mut self) {
        let changed: Vec<_> = self
            .reviews
            .iter()
            .enumerate()
            .filter(|(_, r)| !r.invalidated)
            .filter_map(
                |(i, r)| match self.explain_review_snapshot(&r.sheet_name, r.coord) {
                    Ok(explanation)
                        if (r.decision == ReviewDecision::Question || explanation.can_review)
                            && explanation.snapshot == r.snapshot =>
                    {
                        None
                    }
                    _ => Some(i),
                },
            )
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
            if !record.invalidated && !matches!(self.explain_review_snapshot(&record.sheet_name, record.coord), Ok(e) if (record.decision == ReviewDecision::Question || e.can_review) && e.snapshot == record.snapshot) {
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
    sheet_name: String,
    root_sheet_name: String,
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
            self.warnings.insert("The dependency trace exceeds the 8192-node or 64-level review limit. A partial trace cannot be signed off.".into());
            true
        } else {
            false
        }
    }

    fn cell(&mut self, coord: CellCoord, depth: usize) -> Result<String, CellError> {
        let id = if self.sheet_name == self.root_sheet_name {
            format!("cell:{}", coord.to_a1())
        } else {
            format!("cell:{}!{}", self.sheet_name, coord.to_a1())
        };
        if self.visiting.contains(&id) {
            self.can_review = false;
            self.warnings
                .insert("Circular dependency: this calculation cannot be signed off.".into());
            return Ok(id);
        }
        if self.nodes.contains_key(&id) || self.limit(depth) {
            return Ok(id);
        }
        if self
            .workbook
            .stale_calculation_sheets
            .contains(&self.sheet_name)
        {
            self.can_review = false;
            self.warnings.insert(format!("Referenced sheet {} has a stale calculation; recalculate successfully before reviewing.", self.sheet_name));
        }
        let sheet = &self.workbook.sheets[&self.sheet_name];
        let cell = sheet.get_cell(&coord);
        let ctx = WorkbookEvalContext {
            workbook: self.workbook,
            sheet_name: &self.sheet_name,
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
                sheet_name: Some(self.sheet_name.clone()),
                source_name: None,
            },
        );
        self.visiting.insert(id.clone());
        let follows_formula =
            if let Some(table) = sheet.tables.values().find(|t| t.range.contains(coord)) {
                let local_body = table.local.is_some() && coord.row != table.anchor.row;
                let table_id = self.table(&table.name, depth + 1)?;
                self.edge(&id, table_id);
                local_body
            } else {
                true
            };
        if follows_formula {
            if let Some(spill) = sheet
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
                            "A formula could not be parsed; its dependencies are incomplete."
                                .into(),
                        );
                    }
                }
            }
        }
        self.visiting.remove(&id);
        Ok(id)
    }

    fn table(&mut self, name: &str, depth: usize) -> Result<String, CellError> {
        let sheet = &self.workbook.sheets[&self.sheet_name];
        let table = sheet
            .tables
            .values()
            .find(|t| t.name.eq_ignore_ascii_case(name))
            .ok_or(CellError::Ref)?;
        let id = if self.sheet_name == self.root_sheet_name {
            format!("table:{}", table.name)
        } else {
            format!("table:{}!{}", self.sheet_name, table.name)
        };
        if self.nodes.contains_key(&id) || self.limit(depth) {
            return Ok(id);
        }
        let metadata = if let Some(local) = &table.local {
            // Retain schema, not formatting. Resolved cell values are traced separately.
            serde_json::json!({"name":table.name,"anchor":table.anchor,"range":table.range,
                "columns":table.columns,"has_headers":table.has_headers,
                "has_totals":table.has_totals,"local":local})
        } else {
            serde_json::to_value(table).map_err(|_| CellError::Value)?
        };
        self.parts.insert(id.clone(), metadata);
        self.nodes.insert(
            id.clone(),
            ExplanationNode {
                id: id.clone(),
                kind: "table".into(),
                label: table.name.clone(),
                formula: None,
                value: None,
                depends_on: vec![],
                detail: Some(if table.local.is_some() {
                    format!("Workbook-owned table at {}. Resolved cells are recorded in the calculation trace.", table.anchor.to_a1())
                } else {
                    format!("Worksheet table at {}; cached data source {}", table.anchor.to_a1(), table.source_name)
                }),
                coord: None,
                sheet_name: Some(self.sheet_name.clone()),
                source_name: None,
            },
        );
        if table.local.is_none() {
            let source_id = self.source(&table.source_name, depth + 1)?;
            self.edge(&id, source_id);
        }
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
                sheet_name: Some(self.sheet_name.clone()),
                source_name: None,
            },
        );
        self.nodes.get_mut(&id).unwrap().source_name = Some(name.into());
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
            Expr::SheetRef { sheet, expr } => {
                if !self.workbook.sheets.contains_key(sheet) {
                    self.can_review = false;
                    return Err(CellError::Ref);
                }
                let previous = std::mem::replace(&mut self.sheet_name, sheet.clone());
                let result = self.expr(parent, expr, current, depth + 1);
                self.sheet_name = previous;
                result?;
            }
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
                let table = self.workbook.sheets[&self.sheet_name]
                    .tables
                    .values()
                    .find(|t| t.name.eq_ignore_ascii_case(&sref.table_name))
                    .ok_or(CellError::Ref)?;
                if table.local.is_some() {
                    if let Ok(range) = table.resolve_structured_ref(sref, Some(current)) {
                        'rows: for row in range.start_row..=range.end_row {
                            for col in range.start_col..=range.end_col {
                                if self.limit(depth) {
                                    break 'rows;
                                }
                                let child = self.cell(CellCoord::new(row, col), depth)?;
                                self.edge(parent, child);
                            }
                        }
                    } else {
                        self.can_review = false;
                    }
                } else {
                    let ctx = WorkbookEvalContext {
                        workbook: self.workbook,
                        sheet_name: &self.sheet_name,
                        current_coord: current,
                    };
                    if ctx.get_table_column(sref).is_err() {
                        self.can_review = false;
                    }
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
                            sheet_name: Some(self.sheet_name.clone()),
                            source_name: None,
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

impl Workbook {
    /// Attach bytes to a newly created review. Existing records/versions are never overwritten.
    pub fn record_review_with_attachments(
        &mut self,
        request: ReviewRequest,
        uploads: Vec<EvidenceUpload>,
    ) -> Result<ReviewRecord, String> {
        if uploads.len() > 20
            || uploads.iter().map(|u| u.bytes.len()).sum::<usize>() > 8 * 1024 * 1024
        {
            return Err("Attach at most 20 files totaling 8 MiB per review.".into());
        }
        let mut attachments = Vec::new();
        let mut replaced = BTreeSet::new();
        for upload in uploads {
            let filename = upload.filename.trim();
            let media_type = upload.media_type.trim();
            if filename.is_empty()
                || filename.len() > 255
                || filename.contains(['/', '\\'])
                || filename.chars().any(char::is_control)
                || media_type.is_empty()
                || media_type.len() > 120
                || media_type.chars().any(char::is_control)
                || upload.bytes.len() > 2 * 1024 * 1024
            {
                return Err("Each attachment needs a safe filename, a media type, and at most 2 MiB of bytes.".into());
            }
            let version = if let Some(id) = upload.replaces {
                if !replaced.insert(id) {
                    return Err(
                        "An attachment version can only be replaced once in a review.".into(),
                    );
                }
                if self
                    .reviews
                    .iter()
                    .flat_map(|r| &r.attachments)
                    .any(|a| a.previous_version_id == Some(id))
                {
                    return Err("This evidence version has already been superseded. Replace its latest version instead.".into());
                }
                let prior = self
                    .reviews
                    .iter()
                    .filter(|r| {
                        r.sheet_name == request.sheet_name
                            && r.coord == CellCoord::new(request.row, request.col)
                    })
                    .flat_map(|r| &r.attachments)
                    .find(|a| a.id == id)
                    .ok_or(
                        "Previous evidence version does not belong to this cell's review history.",
                    )?;
                prior
                    .version
                    .checked_add(1)
                    .ok_or("Evidence version overflow")?
            } else {
                1
            };
            attachments.push(EvidenceAttachment {
                id: Uuid::new_v4(),
                version,
                previous_version_id: upload.replaces,
                filename: filename.into(),
                media_type: media_type.into(),
                sha256: format!("{:x}", Sha256::digest(&upload.bytes)),
                bytes: upload.bytes,
                created_at: Utc::now(),
            });
        }
        let mut record = self.record_review(request)?;
        record.attachments = attachments;
        *self.reviews.last_mut().expect("record_review appended") = record.clone();
        Ok(record)
    }

    pub fn compare_review(&self, review_id: Uuid) -> Result<ReviewComparison, String> {
        self.compare_to_current(review_id, true)
    }

    /// Comparison summaries without cloning retained attachment payloads.
    pub fn compare_review_summary(
        &self,
        before_id: Uuid,
        after_id: Option<Uuid>,
    ) -> Result<ReviewComparison, String> {
        match after_id {
            Some(after) => self.compare_versions(before_id, after, false),
            None => self.compare_to_current(before_id, false),
        }
    }

    fn compare_to_current(&self, review_id: Uuid, raw: bool) -> Result<ReviewComparison, String> {
        let before = self
            .reviews
            .iter()
            .find(|r| r.id == review_id)
            .ok_or("Review not found")?;
        let retained = before.input_snapshot.as_ref().ok_or("This legacy review retained only a digest, not its inputs. Exact comparison is unavailable.")?;
        let current = self
            .explain_review_snapshot(&before.sheet_name, before.coord)
            .map_err(|e| e.message().to_string())?;
        let (summaries, affected_cells) = self.summarize_changes(retained, &current.input_snapshot);
        Ok(ReviewComparison {
            summaries,
            affected_cells,
            before_review_id: before.id,
            after_review_id: None,
            before_snapshot: before.snapshot.clone(),
            after_snapshot: current.snapshot,
            changes: if raw {
                snapshot_diff(retained, &current.input_snapshot)?
            } else {
                Vec::new()
            },
        })
    }

    pub fn compare_review_versions(
        &self,
        before_id: Uuid,
        after_id: Uuid,
    ) -> Result<ReviewComparison, String> {
        self.compare_versions(before_id, after_id, true)
    }

    fn compare_versions(
        &self,
        before_id: Uuid,
        after_id: Uuid,
        raw: bool,
    ) -> Result<ReviewComparison, String> {
        let before = self
            .reviews
            .iter()
            .find(|r| r.id == before_id)
            .ok_or("Before review not found")?;
        let after = self
            .reviews
            .iter()
            .find(|r| r.id == after_id)
            .ok_or("After review not found")?;
        if before.sheet_name != after.sheet_name || before.coord != after.coord {
            return Err("Compare reviews of the same cell.".into());
        }
        let a = before
            .input_snapshot
            .as_ref()
            .ok_or("Before review has no retained inputs")?;
        let b = after
            .input_snapshot
            .as_ref()
            .ok_or("After review has no retained inputs")?;
        let evidence_metadata = |record: &ReviewRecord| {
            serde_json::json!({
                "references":record.evidence,
                "attachments":record.attachments.iter().map(|item| serde_json::json!({
                    "id":item.id,"filename":item.filename,"version":item.version,
                    "sha256":item.sha256,"media_type":item.media_type,
                    "previous_version_id":item.previous_version_id,"byte_count":item.bytes.len(),
                })).collect::<Vec<_>>(),
            })
        };
        let (old_files, new_files) = if raw {
            (
                serde_json::to_value(&before.attachments).map_err(|e| e.to_string())?,
                serde_json::to_value(&after.attachments).map_err(|e| e.to_string())?,
            )
        } else {
            (evidence_metadata(before), evidence_metadata(after))
        };
        let mut changes = if raw {
            snapshot_diff(a, b)?
        } else {
            Vec::new()
        };
        if raw {
            diff_json(
                "/attachments",
                Some(&old_files),
                Some(&new_files),
                &mut changes,
            );
        }
        let (mut summaries, affected_cells) = self.summarize_changes(a, b);
        if old_files != new_files {
            summaries.push(ChangeSummary {
                category: "evidence".into(),
                label: "Retained supporting evidence changed".into(),
                sheet_name: Some(before.sheet_name.clone()),
                coord: Some(before.coord),
                before: Some(old_files),
                after: Some(new_files),
            });
        }
        Ok(ReviewComparison {
            summaries,
            affected_cells,
            before_review_id: before.id,
            after_review_id: Some(after.id),
            before_snapshot: before.snapshot.clone(),
            after_snapshot: after.snapshot.clone(),
            changes,
        })
    }

    fn summarize_changes(
        &self,
        before: &ReviewInputSnapshot,
        after: &ReviewInputSnapshot,
    ) -> (Vec<ChangeSummary>, Vec<AffectedCell>) {
        let mut summaries = Vec::new();
        let mut affected = BTreeMap::new();
        let keys: BTreeSet<_> = before
            .inputs
            .keys()
            .chain(after.inputs.keys())
            .cloned()
            .collect();
        let old_nodes: BTreeMap<_, _> = before
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        let new_nodes: BTreeMap<_, _> = after
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        for key in keys {
            let old = before.inputs.get(&key);
            let new = after.inputs.get(&key);
            if old == new {
                continue;
            }
            let old_node = old_nodes.get(key.as_str()).copied();
            let new_node = new_nodes.get(key.as_str()).copied();
            let node = new_node.or(old_node);
            let sheet = node.and_then(|n| n.sheet_name.clone());
            let coord = node.and_then(|n| n.coord);
            let category = if key.starts_with("cell:") {
                if old_node.and_then(|n| n.formula.as_ref())
                    != new_node.and_then(|n| n.formula.as_ref())
                {
                    "formula"
                } else if old.and_then(|v| v.get("raw_input"))
                    != new.and_then(|v| v.get("raw_input"))
                {
                    if sheet
                        .as_ref()
                        .is_some_and(|s| self.close_periods.iter().any(|p| &p.sheet_name == s))
                        && coord.is_some_and(|c| c.col == 2 && c.row > 0)
                    {
                        "mapping"
                    } else {
                        "input"
                    }
                } else {
                    "calculation"
                }
            } else if key.starts_with("close_working_paper:") {
                "close_configuration"
            } else if key.starts_with("close_adjustments:") {
                "adjustment"
            } else if key.starts_with("close_import_cells:") {
                "working_paper_input"
            } else if key.starts_with("source:") {
                "source"
            } else if key.starts_with("source_stale:") {
                "source_state"
            } else if key.starts_with("connection:") {
                "connection"
            } else if key.starts_with("table:") {
                "table"
            } else {
                "calculation_state"
            };
            let label = node.map(|n| n.label.clone()).unwrap_or_else(|| key.clone());
            if let (Some(sheet), Some(coord)) = (&sheet, coord) {
                affected.insert(
                    (sheet.clone(), coord.row, coord.col),
                    AffectedCell {
                        sheet_name: sheet.clone(),
                        coord,
                        label: label.clone(),
                    },
                );
            }
            summaries.push(ChangeSummary {
                category: category.into(),
                label,
                sheet_name: sheet,
                coord,
                before: old.cloned(),
                after: new.cloned(),
            });
        }
        if before.can_review != after.can_review || before.warnings != after.warnings {
            summaries.push(ChangeSummary {
                category: "reviewability".into(),
                label: "Calculation warnings or sign-off eligibility changed".into(),
                sheet_name: Some(after.sheet_name.clone()),
                coord: Some(after.coord),
                before: Some(
                    serde_json::json!({"can_review":before.can_review,"warnings":before.warnings}),
                ),
                after: Some(
                    serde_json::json!({"can_review":after.can_review,"warnings":after.warnings}),
                ),
            });
        }
        (summaries, affected.into_values().collect())
    }

    /// Only returns observed, direct-projection result rows with complete declared primary keys.
    /// This is not aggregate contribution inference and never runs a new query.
    pub fn source_drilldown(
        &self,
        name: &str,
        offset: usize,
        limit: usize,
    ) -> Result<SourceDrilldown, String> {
        let source = self.named_sources.get(name).ok_or("Source not found")?;
        let lineage = source.lineage.as_ref().filter(|l| l.is_editable && !l.primary_keys.is_empty() && l.read_only_columns.is_empty())
            .ok_or("Row drilldown requires a direct single-table projection with declared primary keys; aggregates, joins and computed projections have no inferred transaction lineage.")?;
        if self.stale_sources.contains(name) {
            return Err("Refresh this stale source before drilling down.".into());
        }
        let result = source
            .last_result
            .as_ref()
            .ok_or("Refresh the source first")?;
        let indices: Vec<_> = lineage
            .primary_keys
            .iter()
            .map(|pk| {
                result
                    .columns
                    .iter()
                    .position(|c| c.name == *pk)
                    .ok_or("The cached result is missing a primary key column")
            })
            .collect::<Result<_, _>>()?;
        let mut keys = BTreeSet::new();
        for row in &result.rows {
            let key: Vec<_> = indices
                .iter()
                .map(|i| {
                    row.get(*i)
                        .filter(|v| !matches!(v, Value::Null | Value::Blank))
                        .ok_or("Cached row has an incomplete primary key")
                })
                .collect::<Result<_, _>>()?;
            if !keys.insert(serde_json::to_string(&key).map_err(|e| e.to_string())?) {
                return Err(
                    "Cached primary keys are not unique; transaction lineage is unavailable."
                        .into(),
                );
            }
        }
        let rows = result
            .rows
            .iter()
            .enumerate()
            .skip(offset)
            .take(limit.clamp(1, 500))
            .map(|(i, row)| SourceDrilldownRow {
                cached_row_index: i,
                primary_key: lineage
                    .primary_keys
                    .iter()
                    .zip(indices.iter())
                    .map(|(pk, idx)| (pk.clone(), row[*idx].clone()))
                    .collect(),
                values: row.clone(),
            })
            .collect();
        Ok(SourceDrilldown { source_name: name.into(), connection_name: source.connection_name.clone(),
            base_table: lineage.base_table.clone().ok_or("No base table")?, sql: source.sql.clone(),
            columns: result.columns.iter().map(|c| c.name.clone()).collect(), rows, total_rows: result.rows.len(), offset,
            warning: "Cached source rows, not a live database read. These rows are not claimed to contribute to any selected aggregate formula. Refresh to observe external changes.".into() })
    }
}

fn snapshot_diff(
    a: &ReviewInputSnapshot,
    b: &ReviewInputSnapshot,
) -> Result<Vec<SnapshotChange>, String> {
    let a = serde_json::to_value(a).map_err(|e| e.to_string())?;
    let b = serde_json::to_value(b).map_err(|e| e.to_string())?;
    let mut changes = Vec::new();
    diff_json("", Some(&a), Some(&b), &mut changes);
    Ok(changes)
}
fn diff_json(
    path: &str,
    a: Option<&serde_json::Value>,
    b: Option<&serde_json::Value>,
    changes: &mut Vec<SnapshotChange>,
) {
    use serde_json::Value as J;
    if a == b {
        return;
    }
    match (a, b) {
        (Some(J::Object(a)), Some(J::Object(b))) => {
            let keys: BTreeSet<_> = a.keys().chain(b.keys()).collect();
            for key in keys {
                diff_json(
                    &format!("{}/{}", path, key.replace('~', "~0").replace('/', "~1")),
                    a.get(key),
                    b.get(key),
                    changes,
                );
            }
        }
        (Some(J::Array(a)), Some(J::Array(b))) => {
            for i in 0..a.len().max(b.len()) {
                diff_json(&format!("{path}/{i}"), a.get(i), b.get(i), changes);
            }
        }
        _ => changes.push(SnapshotChange {
            path: path.into(),
            before: a.cloned(),
            after: b.cloned(),
        }),
    }
}
