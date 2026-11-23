use super::{core_error, parse, parse_cell, present, preview_at, Json, Reply, Session};
use gridloom_core::review::{
    EvidenceReference, ExplanationNode, QuestionAction, QuestionTransitionRequest, ReviewDecision,
    ReviewRecord, ReviewRequest,
};
use gridloom_core::{Value, Workbook};
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
use uuid::Uuid;

pub(super) const TOOLS: &[(&str, bool)] = &[
    ("review_cell", false),
    ("list_reviews", false),
    ("get_review", false),
    ("compare_review", false),
    ("record_review", true),
    ("transition_question", true),
];

const MODEL_PREFIX: &str = "AI-assisted (MCP): ";
const ASSURANCE: &str =
    "Local review record only. Reviewer labels are not authenticated; not audit assurance.";
const COVERAGE: &str =
    "The conservative transitive calculation trace includes both conditional branches and resolved workbook inputs. Displayed pages are excerpts, not causal attribution, a whole-workbook review, or independent verification of evidence.";

#[derive(Deserialize, Default)]
#[serde(default)]
struct Page {
    offset: usize,
    limit: Option<usize>,
}

impl Page {
    fn limits(&self, default: usize, max: usize) -> Result<(usize, usize), String> {
        let limit = self.limit.unwrap_or(default);
        if limit == 0 || limit > max {
            return Err(format!("Page limit must be between 1 and {max}."));
        }
        Ok((self.offset, limit))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewCell {
    sheet: String,
    cell: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Filter {
    Reviewed,
    Question,
    OpenQuestions,
    Stale,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct List {
    sheet: Option<String>,
    cell: Option<String>,
    status: Option<Filter>,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Details {
    review_id: Uuid,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Compare {
    review_id: Uuid,
    after_review_id: Option<Uuid>,
    expected_revision: Option<u64>,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Decision {
    Reviewed,
    Question,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    label: String,
    reference: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    revision: u64,
    sheet: String,
    cell: String,
    expected_snapshot: String,
    decision: Decision,
    agent_label: String,
    note: String,
    #[serde(default)]
    evidence: Vec<Reference>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum TransitionAction {
    Resolve,
    Reopen,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Transition {
    revision: u64,
    review_id: Uuid,
    expected_snapshot: String,
    // First event must explicitly name a null prior ID, not omit it.
    expected_last_event_id: Json,
    action: TransitionAction,
    agent_label: String,
    note: String,
}

fn model_actor(label: &str) -> Result<String, String> {
    let label = label.trim();
    if label.is_empty() || label.len() > 100 || label.chars().any(char::is_control) {
        return Err("Use a nonempty agent_label of up to 100 bytes, without control characters. Do not use a human's identity.".into());
    }
    Ok(format!("{MODEL_PREFIX}{label}"))
}

fn snapshot_valid(snapshot: &str) -> Result<(), String> {
    if snapshot.len() != 67
        || !snapshot.starts_with("v1:")
        || !snapshot[3..].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("Supply the exact snapshot string returned by review_cell.".into());
    }
    Ok(())
}

fn page(
    offset: Option<usize>,
    limit: Option<usize>,
    default: usize,
    maximum: usize,
) -> Result<(usize, usize), String> {
    Page {
        offset: offset.unwrap_or_default(),
        limit,
    }
    .limits(default, maximum)
}

fn page_result(offset: usize, count: usize, total: usize) -> Json {
    let next = offset.saturating_add(count);
    json!({"offset":offset,"count":count,"total":total,"next_offset":(next<total).then_some(next)})
}

fn state(r: &ReviewRecord) -> &'static str {
    match (
        r.decision == ReviewDecision::Question,
        r.question_is_open(),
        r.invalidated,
    ) {
        (true, true, _) => "question_open",
        (true, false, _) => "question_resolved",
        (false, _, true) => "review_stale",
        _ => "review_current",
    }
}

fn record_summary(r: &ReviewRecord) -> Json {
    json!({
        "review_id":r.id, "sheet":r.sheet_name, "cell":r.coord.to_a1(),
        "decision":r.decision,"status":state(r),
        "recorded_by":preview_at(&r.reviewer,180),
        "ai_assisted_mcp":r.reviewer.starts_with(MODEL_PREFIX),
        "snapshot":r.snapshot, "created_at":r.created_at,
        "snapshot_stale":r.invalidated, "invalidation_reason":r.invalidation_reason,
        "last_question_event_id":r.question_events.last().map(|event|event.id),
        "references":r.evidence.len(), "attachments":r.attachments.len(),
        "events":r.question_events.len(), "has_retained_input_snapshot":r.input_snapshot.is_some(),
    })
}

fn trace_node(node: &ExplanationNode) -> Json {
    json!({"id":preview_at(&node.id,256),"kind":node.kind,"label":preview_at(&node.label,256),
        "cell":node.coord.map(|coord|coord.to_a1()),"sheet":node.sheet_name,
        "formula":node.formula.as_deref().map(|s|preview_at(s,512)),
        "value":node.value.as_ref().map(bounded_value),
        "details":node.detail.as_deref().map(|s|preview_at(s,512)),
        "depends_on":node.depends_on.iter().take(8).map(|x|preview_at(x,256)).collect::<Vec<_>>(),
        "dependency_count":node.depends_on.len()})
}

fn bounded_value(value: &Value) -> Json {
    if let Value::Text(text) = value {
        json!({"type":"Text","value":preview_at(text,512)})
    } else if let Value::Array(rows) = value {
        json!({"type":"Array","rows":rows.len(),"columns":rows.first().map_or(0,Vec::len),
            "value":rows.first().and_then(|r|r.first()).map(bounded_value),"summary":true})
    } else {
        present(value)
    }
}

fn compare_value(value: Option<&Json>) -> Json {
    let Some(value) = value else {
        return Json::Null;
    };
    // The native summary retains tagged typed cells. Preserve exact integers
    // and decimals, but return only bounded cell values, not opaque snapshots.
    if let Some(tagged) = value.get("value") {
        let parsed = serde_json::from_value::<Value>(tagged.clone()).ok();
        return json!({
            "raw_input":value.get("raw_input").and_then(Json::as_str).map(|x|preview_at(x,512)),
            "value":parsed.as_ref().map(bounded_value),
        });
    }
    if let Some(attachments) = value["attachments"].as_array() {
        return json!({"attachment_metadata":attachments.iter().take(4).collect::<Vec<_>>(),
            "attachment_count":attachments.len(),
            "reference_count":value["references"].as_array().map_or(0, Vec::len)});
    }
    let printed = value.to_string();
    if printed.len() <= 768 {
        value.clone()
    } else {
        json!({"summary":"Larger native detail exists in the workbook's Explain & Review panel.", "utf8_bytes":printed.len()})
    }
}

fn review(wb: &Workbook, id: Uuid) -> Result<&ReviewRecord, String> {
    wb.reviews
        .iter()
        .find(|record| record.id == id)
        .ok_or_else(|| "Review not found in the active workbook.".into())
}

impl Session {
    pub(super) fn call_review(&mut self, name: &str, args: Json) -> Reply {
        match name {
            "review_cell" => {
                let ReviewCell {
                    sheet,
                    cell,
                    offset,
                    limit,
                } = parse(args)?;
                let (offset, limit) = page(offset, limit, 12, 24)?;
                let coord = parse_cell(&cell)?;
                let active = self.state()?;
                let wb = &active.workbook;
                let e = wb
                    .explain_review_snapshot(&sheet, coord)
                    .map_err(core_error)?;
                let mut records = wb
                    .reviews
                    .iter()
                    .rev()
                    .filter(|r| r.sheet_name == sheet && r.coord == coord);
                let mut open_questions = 0;
                let mut latest_marked_review = None;
                for r in records.by_ref() {
                    if r.question_is_open() {
                        open_questions += 1;
                    }
                    if latest_marked_review.is_none() && r.decision == ReviewDecision::Reviewed {
                        latest_marked_review = Some(r);
                    }
                }
                let latest = latest_marked_review.map(|r| {
                    let mut summary = record_summary(r);
                    if r.snapshot != e.snapshot || !e.can_review {
                        summary["snapshot_stale"] = json!(true);
                        summary["status"] = json!("review_stale");
                    }
                    summary
                });
                let condition = if open_questions > 0 {
                    "open_questions"
                } else if latest
                    .as_ref()
                    .is_some_and(|r| r["snapshot_stale"] == false)
                {
                    "review_current"
                } else if latest.is_some() {
                    "review_stale"
                } else {
                    "not_reviewed"
                };
                let trace: Vec<_> = e
                    .nodes
                    .iter()
                    .skip(offset)
                    .take(limit)
                    .map(trace_node)
                    .collect();
                let pagination = page_result(offset, trace.len(), e.nodes.len());
                Ok(
                    json!({"revision":active.revision,"sheet":sheet,"cell":coord.to_a1(),
                    "snapshot":e.snapshot,"status":condition,"can_mark_reviewed":e.can_review && open_questions==0,
                    "calculation_reviewable":e.can_review,"open_questions":open_questions,
                    "latest_review":latest,"value":bounded_value(&e.input_snapshot.value),
                    "formula":e.formula.as_deref().map(|s|preview_at(s,1024)),
                    "coverage":COVERAGE,"assurance":ASSURANCE,
                    "warnings":e.warnings.iter().take(12).map(|x|preview_at(x,640)).collect::<Vec<_>>(),
                    "warning_count":e.warnings.len(),"trace":trace,"trace_page":pagination}),
                )
            }
            "list_reviews" => {
                let List {
                    sheet,
                    cell,
                    status,
                    offset,
                    limit,
                } = parse(args)?;
                let (offset, limit) = page(offset, limit, 12, 24)?;
                if cell.is_some() && sheet.is_none() {
                    return Err("Provide the worksheet name when filtering by cell.".into());
                }
                let cell = cell.as_deref().map(parse_cell).transpose()?;
                let active = self.state()?;
                if sheet
                    .as_ref()
                    .is_some_and(|name| !active.workbook.sheets.contains_key(name))
                {
                    return Err("Worksheet not found.".into());
                }
                let iter = active.workbook.reviews.iter().rev().filter(|record| {
                    sheet.as_ref().is_none_or(|s| record.sheet_name == *s)
                        && cell.is_none_or(|c| record.coord == c)
                        && status.as_ref().is_none_or(|filter| match filter {
                            Filter::Reviewed => record.decision == ReviewDecision::Reviewed,
                            Filter::Question => record.decision == ReviewDecision::Question,
                            Filter::OpenQuestions => record.question_is_open(),
                            Filter::Stale => record.invalidated,
                        })
                });
                let total = iter.clone().count();
                let records: Vec<_> = iter.skip(offset).take(limit).map(record_summary).collect();
                Ok(
                    json!({"revision":active.revision,"page":page_result(offset,records.len(),total),
                    "assurance":ASSURANCE,"records":records}),
                )
            }
            "get_review" => {
                let Details {
                    review_id,
                    offset,
                    limit,
                } = parse(args)?;
                let (offset, limit) = page(offset, limit, 4, 6)?;
                let active = self.state()?;
                let r = review(&active.workbook, review_id)?;
                let refs: Vec<_> = r
                    .evidence
                    .iter()
                    .skip(offset)
                    .take(limit)
                    .map(|e| json!({"label":e.label,"reference":e.reference,"verified":false}))
                    .collect();
                let attachments: Vec<_> = r.attachments.iter().skip(offset).take(limit).map(|a|json!({
                    "id":a.id,"filename":a.filename,"media_type":a.media_type,"sha256":a.sha256,
                    "bytes_retained":a.bytes.len(),"version":a.version,"previous_version_id":a.previous_version_id,
                    "created_at":a.created_at,"contents_verified":false
                })).collect();
                let events: Vec<_> = r.question_events.iter().rev().skip(offset).take(limit).map(|event|json!({
                    "id":event.id,"action":event.action,"by":event.actor,"created_at":event.created_at,
                    "ai_assisted_mcp":event.actor.starts_with(MODEL_PREFIX),
                    "note":preview_at(&event.note,512),"snapshot":event.snapshot
                })).collect();
                Ok(
                    json!({"revision":active.revision,"record":record_summary(r),"note":preview_at(&r.note,4000),
                    "retained":r.input_snapshot.as_ref().map(|s|json!({"value":bounded_value(&s.value),
                        "formula":s.formula.as_deref().map(|f|preview_at(f,1024)),"trace_nodes":s.nodes.len()})),
                    "evidence_references":refs,"attachments_metadata":attachments,"question_events":events,
                    "pages":{"references":page_result(offset,refs.len(),r.evidence.len()),
                        "attachments":page_result(offset,attachments.len(),r.attachments.len()),
                        "events":page_result(offset,events.len(),r.question_events.len())},
                    "assurance":ASSURANCE,"evidence_scope":"References and retained metadata only; no file or URL is opened, read, or verified."}),
                )
            }
            "compare_review" => {
                let Compare {
                    review_id,
                    after_review_id,
                    expected_revision,
                    offset,
                    limit,
                } = parse(args)?;
                let (offset, limit) = page(offset, limit, 12, 24)?;
                let active = self.state()?;
                if expected_revision.is_some_and(|expected| expected != active.revision) {
                    return Err("Stale revision: restart compare_review at offset 0.".into());
                }
                let mut cmp = active
                    .workbook
                    .compare_review_summary(review_id, after_review_id)?;
                let mut category_counts = BTreeMap::new();
                for change in &cmp.summaries {
                    *category_counts.entry(change.category.clone()).or_insert(0) += 1;
                }
                // Stable sort preserves the native order within each tier.
                cmp.summaries
                    .sort_by_key(|change| match change.category.as_str() {
                        "input" | "mapping" | "working_paper_input" | "adjustment" | "formula" => 0,
                        "calculation" | "calculation_state" => 2,
                        _ => 1,
                    });
                let changes:Vec<_>=cmp.summaries.iter().skip(offset).take(limit).map(|change|json!({
                    "category":change.category,"label":preview_at(&change.label,256),
                    "sheet":change.sheet_name,"cell":change.coord.map(|c|c.to_a1()),
                    "before":compare_value(change.before.as_ref()),"after":compare_value(change.after.as_ref())
                })).collect();
                Ok(
                    json!({"revision":active.revision,"before_review_id":cmp.before_review_id,
                    "after_review_id":cmp.after_review_id,"before_snapshot":cmp.before_snapshot,
                    "after_snapshot":cmp.after_snapshot,"snapshot_changed":cmp.before_snapshot!=cmp.after_snapshot,
                    "affected_cells":cmp.affected_cells.len(),"changes":changes,"category_counts":category_counts,
                    "page":page_result(offset,changes.len(),cmp.summaries.len()),
                    "coverage":"Native categorized changes inside the captured trace; not a workbook-wide impact report."}),
                )
            }
            "record_review" => {
                let r: Record = parse(args)?;
                self.require_revision(r.revision)?;
                let reviewer = model_actor(&r.agent_label)?;
                if r.note.trim().is_empty() {
                    return Err("Record the check you performed or the question you are raising in the note.".into());
                }
                snapshot_valid(&r.expected_snapshot)?;
                let coord = parse_cell(&r.cell)?;
                let record =
                    self.active
                        .as_mut()
                        .unwrap()
                        .workbook
                        .record_review(ReviewRequest {
                            sheet_name: r.sheet,
                            row: coord.row,
                            col: coord.col,
                            expected_snapshot: r.expected_snapshot,
                            decision: match r.decision {
                                Decision::Reviewed => ReviewDecision::Reviewed,
                                Decision::Question => ReviewDecision::Question,
                            },
                            reviewer,
                            note: r.note,
                            evidence: r
                                .evidence
                                .into_iter()
                                .map(|e| EvidenceReference {
                                    label: e.label,
                                    reference: e.reference,
                                })
                                .collect(),
                        })?;
                self.update_revision(true);
                Ok(
                    json!({"revision":self.state()?.revision,"dirty":true,"saved":false,
                    "record":record_summary(&record),"assurance":ASSURANCE}),
                )
            }
            "transition_question" => {
                let t: Transition = parse(args)?;
                self.require_revision(t.revision)?;
                let actor = model_actor(&t.agent_label)?;
                snapshot_valid(&t.expected_snapshot)?;
                let expected_last_event_id = serde_json::from_value(t.expected_last_event_id)
                    .map_err(|_| {
                        "expected_last_event_id must be the last event UUID or explicit null."
                            .to_owned()
                    })?;
                let record = self.active.as_mut().unwrap().workbook.transition_question(
                    t.review_id,
                    QuestionTransitionRequest {
                        action: match t.action {
                            TransitionAction::Resolve => QuestionAction::Resolved,
                            TransitionAction::Reopen => QuestionAction::Reopened,
                        },
                        actor,
                        note: t.note,
                        expected_snapshot: t.expected_snapshot,
                        expected_last_event_id,
                    },
                )?;
                self.update_revision(true);
                Ok(
                    json!({"revision":self.state()?.revision,"dirty":true,"saved":false,"record":record_summary(&record),
                    "assurance":ASSURANCE,"effect":"This question action does not mark the calculation reviewed."}),
                )
            }
            _ => Err("Unknown tool.".into()),
        }
    }
}

pub(super) fn definitions(read_only: bool) -> Vec<Json> {
    let string = |limit| json!({"type":"string","minLength":1,"maxLength":limit});
    let (sheet, cell) = (string(256), string(20));
    let id = json!({"type":"string","format":"uuid"});
    let note = json!({"type":"string","maxLength":4000,"description":"Local review notes. Record verification performed and any caveats."});
    let model = json!({"type":"string","minLength":1,"maxLength":100,"description":"Your agent or tool label, not the user's name. Stamped as AI-assisted."});
    let snapshot = json!({"type":"string","pattern":"^v1:[a-f0-9]{64}$","description":"Exact snapshot returned by review_cell."});
    let revision = json!({"type":"integer","minimum":1});
    let offset = json!({"type":"integer","minimum":0});
    let limit = |n| json!({"type":"integer","minimum":1,"maximum":n});
    TOOLS.iter().filter(|(_,write)|!(*write&&read_only)).map(|(name,write)| {
        let (description, properties, required) = match *name {
            "review_cell"=>("Start a self-review here after re-reading the exact typed result. Inspect current native snapshot, eligibility, questions and paged transitive trace; compare with an independently calculated expectation before recording Reviewed.",json!({"sheet":sheet,"cell":cell,"offset":offset,"limit":limit(24)}),vec!["sheet","cell"]),
            "list_reviews"=>("List history or open questions across the workbook, optionally filtered by worksheet and cell. Newest first; page with next_offset.",json!({"sheet":sheet,"cell":cell,"status":{"enum":["reviewed","question","open_questions","stale"]},"offset":offset,"limit":limit(24)}),vec![]),
            "get_review"=>("Inspect recorded notes, captured result, evidence references, retained file metadata and question events. No external paths, URLs or evidence bytes are accessed.",json!({"review_id":id,"offset":offset,"limit":limit(6)}),vec!["review_id"]),
            "compare_review"=>("Compare a retained review snapshot with current calculation or another review of that cell, with input/formula changes first. Use page.next_offset and expected_revision from the prior result's revision for later pages. Excludes raw attachment bytes.",json!({"review_id":id,"after_review_id":id,"expected_revision":revision,"offset":offset,"limit":limit(24)}),vec!["review_id"]),
            "record_review"=>("Append a Question or explicitly requested local AI-assisted Reviewed record to the exact inspected snapshot. Reviewed is blocked on errors or unresolved questions. Note the independent check performed or the problem found. Evidence references are literal text. Does not save or signify human approval.",json!({"revision":revision,"sheet":sheet,"cell":cell,
                "decision":{"enum":["reviewed","question"]},"agent_label":model,"note":note,"expected_snapshot":snapshot,
                "evidence":{"type":"array","maxItems":20,"items":{"type":"object","required":["label","reference"],"additionalProperties":false,"properties":{"label":string(200),"reference":string(2000)}}}}),
                vec!["revision","sheet","cell","decision","agent_label","note","expected_snapshot"]),
            "transition_question"=>("Append an explicit resolve or reopen event with a reason. Match the latest event UUID (null only for first action), exact snapshot, and revision. Does not approve or save.",json!({"revision":revision,"review_id":id,"note":note,"agent_label":model,"action":{"enum":["resolve","reopen"]},"expected_snapshot":snapshot,
                "expected_last_event_id":{"anyOf":[id,{"type":"null"}]}}),vec!["revision","review_id","note","agent_label","action","expected_snapshot","expected_last_event_id"]),
            _=>unreachable!()
        };
        json!({"name":name,"description":description,
            "inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},
            "annotations":{"readOnlyHint": !write,"destructiveHint":false,"openWorldHint":false}})
    }).collect()
}
