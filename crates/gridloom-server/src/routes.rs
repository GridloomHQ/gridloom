use crate::benchmark::run_performance_benchmarks;
use crate::state::AppState;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use gridloom_core::chart::ChartDefinition;
use gridloom_core::close::{
    ConfigureCloseRequest, ImportTrialBalanceRequest, MapAccountsRequest, PostAdjustmentRequest,
    RollforwardRequest,
};
use gridloom_core::db::EngineKind;
use gridloom_core::lineage::analyze_lineage;
use gridloom_core::pivot::PivotDefinition;
use gridloom_core::review::{EvidenceUpload, QuestionTransitionRequest, ReviewRequest};
use gridloom_core::spreadsheet::CellStyle;
use gridloom_core::value::{CellCoord, CellRange, Value};
use gridloom_core::virtual_view::ViewportRequest;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn create_router(state: AppState) -> Router {
    Router::new()
        // Health
        .route("/api/health", get(health_handler))
        .route("/api/functions", get(list_functions))
        // Workbook
        .route("/api/workbook", get(get_workbook_summary))
        .route("/api/workbook/new", post(new_workbook))
        .route("/api/workbook/open", post(open_workbook))
        .route("/api/workbook/save", post(save_workbook))
        .route("/api/workbook/recovery", get(crate::recovery::list))
        .route(
            "/api/workbook/recovery/{id}",
            post(crate::recovery::restore),
        )
        .route("/api/sheets", post(add_sheet))
        .route("/api/sheets/select", post(select_sheet))
        .route("/api/sheets/insert-rows", post(crate::row_insert::insert_rows))
        .route("/api/sheets/rename", post(rename_sheet))
        .route("/api/sheets/delete", post(delete_sheet))
        .route("/api/sheets/column-width", post(set_column_width))
        .route("/api/sheets/freeze", post(set_freeze_panes))
        .route("/api/sheets/show-formulas", post(set_show_formulas))
        .route("/api/ranges/edit", post(edit_range))
        .route("/api/ranges/read", post(read_range))
        .route("/api/ranges/summary", post(selection_summary))
        .route("/api/ranges/capture", post(capture_range))
        .route("/api/ranges/sort-filter", post(crate::range_view::sort_filter))
        .route("/api/ranges/format", post(format_range))
        .route("/api/ranges/format-attribute", post(format_range_attribute))
        .route("/api/ranges/copy", post(copy_range))
        .route("/api/ranges/fill", post(fill_range))
        .route("/api/history/undo", post(undo))
        .route("/api/history/redo", post(redo))
        .route("/api/close/periods", get(list_close_periods))
        .route("/api/close/import", post(import_trial_balance))
        .route("/api/close/map", post(map_close_accounts))
        .route("/api/close/configure", post(configure_close))
        .route("/api/close/adjustments", post(post_close_adjustment))
        .route("/api/close/rollforward", post(rollforward_close))
        .route("/api/close/{id}/status", get(close_status))
        .route("/api/close/{id}/pack", get(export_review_pack))
        // Database Connections & Schema
        .route("/api/connections", get(list_connections).post(connect_db))
        .route("/api/connections/{name}/tables", get(list_tables))
        .route(
            "/api/connections/{name}/tables/{table}/schema",
            get(get_table_schema),
        )
        // Query Execution & Lineage
        .route("/api/query/execute", post(execute_query))
        .route(
            "/api/sample",
            get(crate::ledger_demo::status)
                .post(crate::ledger_demo::start)
                .delete(crate::ledger_demo::cancel),
        )
        .route("/api/sample/ledger", post(crate::ledger::browse_sample))
        .route("/api/ledger/page", post(crate::ledger::browse))
        .route("/api/ledger/trace", get(crate::ledger::get_trace).post(crate::ledger::update_trace))
        .route("/api/query/analyze", post(analyze_query))
        // Named Data Sources
        .route("/api/sources", get(list_sources).post(create_source))
        .route("/api/sources/{name}/refresh", post(refresh_source))
        // Worksheet Tables
        .route("/api/tables/insert", post(insert_table))
        // Cells & Virtual Viewport
        .route("/api/cells/set", post(set_cell))
        .route("/api/cells/viewport", post(get_viewport))
        .route("/api/cells/viewport-panes", post(get_viewport_panes))
        .route("/api/cells/find", post(find_cells))
        .route("/api/cells/explain", post(explain_cell))
        .route("/api/cells/dependencies", post(navigate_dependencies))
        .route("/api/questions", get(list_open_questions))
        .route("/api/questions/{id}/transition", post(transition_question))
        .route("/api/reviews", get(list_reviews).post(record_review))
        .route("/api/reviews/attached", post(record_attached_review))
        .route("/api/reviews/{id}/compare", get(compare_review))
        .route("/api/sources/{name}/drilldown", get(source_drilldown))
        // Staged Writeback & Review
        .route(
            "/api/staged",
            get(list_staged).post(stage_edit).delete(discard_all_staged),
        )
        .route("/api/staged/{id}", delete(discard_staged_item))
        .route("/api/writeback/review", get(review_writeback))
        .route("/api/writeback/apply", post(apply_writeback))
        .route("/api/audit", get(list_audit_log))
        // Pivots & Charts
        .route("/api/pivots", get(list_pivots).post(save_pivot))
        .route("/api/pivots/{name}/data", get(get_pivot_data))
        .route("/api/pivots/refresh", post(refresh_pivot_data))
        .route("/api/charts", get(list_charts).post(save_chart))
        .route("/api/charts/{id}/data", get(get_chart_data))
        // Performance Acceptance Benchmark Runner
        .route("/api/benchmark/run", post(run_benchmarks))
        .layer(DefaultBodyLimit::max(40 * 1024 * 1024))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::recovery::track_mutations,
        ))
        .with_state(state)
}

async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "app": "Gridloom",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

#[derive(Serialize)]
struct SheetSummary {
    name: String,
    column_widths: std::collections::HashMap<u32, u16>,
    freeze_top_row: bool,
    show_formulas: bool,
    freeze_first_column: bool,
    frozen_rows: u32,
    frozen_columns: u32,
    tables: Vec<gridloom_core::table::WorksheetTable>,
}

#[derive(Serialize)]
struct WorkbookSummary {
    active_sheet: String,
    sheets: Vec<SheetSummary>,
    connections: Vec<String>,
    sources: Vec<String>,
    staged_patches_count: usize,
    file_path: Option<String>,
}

async fn get_workbook_summary(State(state): State<AppState>) -> impl IntoResponse {
    let wb = state.workbook.read().await;
    let summary = WorkbookSummary {
        active_sheet: wb.active_sheet.clone(),
        sheets: wb
            .sheets
            .values()
            .map(|s| SheetSummary {
                name: s.name.clone(),
                column_widths: s.column_widths.clone(),
                freeze_top_row: s.freeze_top_row,
                show_formulas: s.show_formulas,
                freeze_first_column: s.freeze_first_column,
                frozen_rows: s.frozen_row_count(),
                frozen_columns: s.frozen_column_count(),
                tables: s.tables.values().cloned().collect(),
            })
            .collect(),
        connections: wb.connections.keys().cloned().collect(),
        sources: wb.named_sources.keys().cloned().collect(),
        staged_patches_count: wb.staged.patches.len(),
        file_path: wb
            .file_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
    };
    Json(summary)
}

async fn new_workbook(State(state): State<AppState>) -> impl IntoResponse {
    let mut wb = state.workbook.write().await;
    *wb = gridloom_core::Workbook::new();
    Json(serde_json::json!({ "status": "created" }))
}

#[derive(Deserialize)]
struct PathPayload {
    path: String,
}

async fn open_workbook(
    State(state): State<AppState>,
    Json(payload): Json<PathPayload>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let loaded = gridloom_core::Workbook::open_from_file(&payload.path)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let mut wb = state.workbook.write().await;
    *wb = loaded;
    Ok(Json(
        serde_json::json!({ "status": "opened", "path": payload.path }),
    ))
}

async fn save_workbook(
    State(state): State<AppState>,
    Json(payload): Json<PathPayload>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut wb = state.workbook.write().await;
    wb.save_to_file(&payload.path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(
        serde_json::json!({ "status": "saved", "path": payload.path }),
    ))
}

#[derive(Deserialize)]
struct ConnectDbRequest {
    name: String,
    kind: String, // "sqlite" or "duckdb"
    path: String,
}

async fn connect_db(
    State(state): State<AppState>,
    Json(req): Json<ConnectDbRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut wb = state.workbook.write().await;
    match req.kind.to_lowercase().as_str() {
        "sqlite" => {
            if req.path == ":memory:" {
                wb.add_sqlite_memory(&req.name)
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
            } else {
                wb.add_sqlite_connection(&req.name, &req.path)
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
            }
        }
        "duckdb" => {
            if req.path == ":memory:" {
                wb.add_duckdb_memory(&req.name)
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
            } else {
                wb.add_duckdb_connection(&req.name, &req.path)
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
            }
        }
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "Invalid engine kind, use 'sqlite' or 'duckdb'".to_string(),
            ))
        }
    }
    Ok(Json(
        serde_json::json!({ "status": "connected", "name": req.name }),
    ))
}

async fn list_connections(State(state): State<AppState>) -> impl IntoResponse {
    let wb = state.workbook.read().await;
    let list: Vec<_> = wb.connection_configs.values().cloned().collect();
    Json(list)
}

async fn list_tables(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let wb = state.workbook.read().await;
    let conn = wb.connections.get(&name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("Connection '{}' not found", name),
        )
    })?;
    let tables = conn
        .list_tables()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(tables))
}

async fn get_table_schema(
    State(state): State<AppState>,
    Path((name, table)): Path<(String, String)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let wb = state.workbook.read().await;
    let conn = wb.connections.get(&name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("Connection '{}' not found", name),
        )
    })?;
    let schema = conn
        .get_table_schema(&table)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(schema))
}

#[derive(Deserialize)]
struct QueryRequest {
    connection_name: String,
    sql: String,
}

async fn execute_query(
    State(state): State<AppState>,
    Json(req): Json<QueryRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let wb = state.workbook.read().await;
    let conn = wb.connections.get(&req.connection_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("Connection '{}' not found", req.connection_name),
        )
    })?;
    let res = conn
        .execute_read_query(&req.sql, &[])
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(res))
}

async fn analyze_query(
    State(state): State<AppState>,
    Json(req): Json<QueryRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let wb = state.workbook.read().await;
    let conn = wb.connections.get(&req.connection_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("Connection '{}' not found", req.connection_name),
        )
    })?;

    let dialect = match conn.kind() {
        EngineKind::Sqlite => "sqlite",
        EngineKind::DuckDb => "duckdb",
    };
    let schema_fn = |t: &str| conn.get_table_schema(t).ok();
    let report = analyze_lineage(&req.sql, dialect, schema_fn);
    Ok(Json(report))
}

#[derive(Deserialize)]
struct CreateSourceRequest {
    name: String,
    connection_name: String,
    sql: String,
}

async fn create_source(
    State(state): State<AppState>,
    Json(req): Json<CreateSourceRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut wb = state.workbook.write().await;
    let source = wb
        .create_named_source(&req.name, &req.connection_name, &req.sql)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.message().to_string()))?;
    Ok(Json(source))
}

async fn list_sources(State(state): State<AppState>) -> impl IntoResponse {
    let wb = state.workbook.read().await;
    let sources: Vec<_> = wb.named_sources.values().cloned().collect();
    Json(sources)
}

async fn refresh_source(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut wb = state.workbook.write().await;
    wb.refresh_source(&name)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    Ok(Json(
        serde_json::json!({ "status": "refreshed", "source": name }),
    ))
}

#[derive(Deserialize)]
struct InsertTableRequest {
    sheet_name: String,
    table_name: String,
    source_name: String,
    row: u32,
    col: u32,
}

async fn insert_table(
    State(state): State<AppState>,
    Json(req): Json<InsertTableRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut wb = state.workbook.write().await;
    let table = wb
        .insert_table(
            &req.sheet_name,
            &req.table_name,
            &req.source_name,
            CellCoord::new(req.row, req.col),
        )
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(table))
}

#[derive(Deserialize)]
struct SetCellRequest {
    sheet_name: String,
    row: u32,
    col: u32,
    raw_input: String,
}

async fn set_cell(
    State(state): State<AppState>,
    Json(req): Json<SetCellRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut wb = state.workbook.write().await;
    wb.set_cell(
        &req.sheet_name,
        CellCoord::new(req.row, req.col),
        req.raw_input,
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

async fn get_viewport(
    State(state): State<AppState>,
    Json(req): Json<ViewportRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let wb = state.workbook.read().await;
    let vp = wb
        .get_viewport(&req)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(vp))
}

// Request decimals must be exact: Decimal's general-purpose deserializer can round.
struct ExactStagingValue(Value);

impl<'de> Deserialize<'de> for ExactStagingValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = serde_json::Value::deserialize(deserializer)?;
        if wire.get("type").and_then(|value| value.as_str()) == Some("Decimal") {
            let raw = wire.get("value").and_then(|value| value.as_str())
                .ok_or_else(|| serde::de::Error::custom("Staged decimals must be exact strings."))?;
            return rust_decimal::Decimal::from_str_exact(raw)
                .map(|value| Self(Value::Decimal(value)))
                .map_err(|_| serde::de::Error::custom("The staged decimal exceeds exact representable precision."));
        }
        serde_json::from_value(wire).map(Self).map_err(serde::de::Error::custom)
    }
}

#[derive(Deserialize)]
struct StageEditRequest {
    connection_name: String,
    table_name: String,
    pk_columns: Vec<String>,
    pk_values: Vec<ExactStagingValue>,
    column_name: String,
    original_value: ExactStagingValue,
    raw_input: Option<String>,
    proposed_value: Option<ExactStagingValue>,
}

async fn stage_edit(
    State(state): State<AppState>,
    Json(req): Json<StageEditRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    use gridloom_core::db::input::SqlEditInput;
    let input = match (req.raw_input, req.proposed_value) {
        (Some(raw), None) => SqlEditInput::Raw(raw),
        (None, Some(value)) => SqlEditInput::Typed(value.0),
        _ => return Err((StatusCode::BAD_REQUEST, "Provide one raw input or explicit typed value.".into())),
    };
    let id = state.workbook.write().await.stage_sql_edit(
        req.connection_name,
        req.table_name,
        req.pk_columns,
        req.pk_values.into_iter().map(|value| value.0).collect(),
        req.column_name,
        req.original_value.0,
        input,
    ).map_err(|error| (StatusCode::BAD_REQUEST, error.message().into()))?;
    Ok(Json(serde_json::json!({ "status": "staged", "id": id })))
}

async fn list_staged(State(state): State<AppState>) -> impl IntoResponse {
    let wb = state.workbook.read().await;
    let patches = wb.staged.list_patches();
    Json(patches)
}

async fn discard_staged_item(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut wb = state.workbook.write().await;
    let removed = wb.staged.discard_patch(&id);
    wb.invalidate_changed_reviews();
    Json(serde_json::json!({ "status": "discarded", "removed": removed }))
}

async fn discard_all_staged(State(state): State<AppState>) -> impl IntoResponse {
    let mut wb = state.workbook.write().await;
    wb.staged.discard_all();
    wb.invalidate_changed_reviews();
    Json(serde_json::json!({ "status": "all_discarded" }))
}

async fn review_writeback(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let wb = state.workbook.read().await;
    let mut items = Vec::new();
    for name in wb
        .staged
        .patches
        .values()
        .map(|p| &p.connection_name)
        .collect::<std::collections::HashSet<_>>()
    {
        let conn = wb.connections.get(name).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Target connection '{}' not found", name),
            )
        })?;
        items.extend(wb.staged.review(conn, name));
    }
    items.sort_by_key(|i| i.patch.staged_at);
    Ok(Json(items))
}

#[derive(Deserialize)]
struct ApplyWritebackRequest {
    connection_name: String,
}

async fn apply_writeback(
    State(state): State<AppState>,
    Json(req): Json<ApplyWritebackRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut wb = state.workbook.write().await;
    let rec = wb
        .apply_writeback(&req.connection_name)
        .map_err(|e| (StatusCode::CONFLICT, e.to_string()))?;

    Ok(Json(rec))
}

async fn list_audit_log(State(state): State<AppState>) -> impl IntoResponse {
    let wb = state.workbook.read().await;
    Json(wb.staged.audit_log.clone())
}

async fn list_pivots(State(state): State<AppState>) -> impl IntoResponse {
    let wb = state.workbook.read().await;
    let mut pivots: Vec<_> = wb.pivots.values().cloned().collect();
    pivots.sort_by(|a, b| a.name.cmp(&b.name));
    Json(pivots)
}

async fn save_pivot(
    State(state): State<AppState>,
    Json(def): Json<PivotDefinition>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if def.name.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Enter a pivot name".into()));
    }
    let mut wb = state.workbook.write().await;
    let result = wb
        .preview_pivot(&def)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let name = def.name.clone();
    wb.pivots.insert(name.clone(), def);
    Ok(Json(serde_json::json!({ "status": "saved", "name": name, "result": result })))
}

async fn get_pivot_data(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let wb = state.workbook.read().await;
    let res = wb
        .compute_pivot_result(&name)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(res))
}

#[derive(Deserialize)]
struct RefreshPivotRequest {
    name: String,
}

async fn refresh_pivot_data(
    State(state): State<AppState>,
    Json(req): Json<RefreshPivotRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let wb = state.workbook.read_owned().await;
    let result = tokio::task::spawn_blocking(move || wb.refresh_pivot_result(&req.name))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(result))
}

async fn list_charts(State(state): State<AppState>) -> impl IntoResponse {
    let wb = state.workbook.read().await;
    let charts: Vec<_> = wb.charts.values().cloned().collect();
    Json(charts)
}

async fn save_chart(
    State(state): State<AppState>,
    Json(def): Json<ChartDefinition>,
) -> impl IntoResponse {
    let mut wb = state.workbook.write().await;
    wb.charts.insert(def.id.clone(), def.clone());
    Json(serde_json::json!({ "status": "saved", "id": def.id }))
}

async fn get_chart_data(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let wb = state.workbook.read().await;
    let data = wb
        .compute_chart_data(&id)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(data))
}

async fn run_benchmarks() -> impl IntoResponse {
    let report = run_performance_benchmarks();
    Json(report)
}

#[derive(Deserialize)]
struct ExplainCellRequest {
    sheet_name: String,
    row: u32,
    col: u32,
}

async fn explain_cell(
    State(state): State<AppState>,
    Json(req): Json<ExplainCellRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let wb = state.workbook.read().await;
    let explanation = wb
        .explain_cell(&req.sheet_name, CellCoord::new(req.row, req.col))
        .map_err(|e| (StatusCode::BAD_REQUEST, e.message().to_string()))?;
    Ok(Json(explanation))
}

async fn navigate_dependencies(
    State(state): State<AppState>,
    Json(req): Json<ExplainCellRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let wb = state.workbook.read().await;
    Ok(Json(
        wb.navigate_dependencies(&req.sheet_name, CellCoord::new(req.row, req.col))
            .map_err(|e| bad_request(e.message()))?,
    ))
}

async fn list_open_questions(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.workbook.read().await.open_questions())
}

async fn transition_question(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<QuestionTransitionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let mut wb = state.workbook.write().await;
    Ok(Json(
        wb.transition_question(id, req)
            .map_err(|message| (StatusCode::CONFLICT, message))?,
    ))
}

async fn record_review(
    State(state): State<AppState>,
    Json(req): Json<ReviewRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut wb = state.workbook.write().await;
    let review = wb
        .record_review(req)
        .map_err(|message| (StatusCode::CONFLICT, message))?;
    Ok(Json(review))
}

async fn list_reviews(State(state): State<AppState>) -> impl IntoResponse {
    let wb = state.workbook.read().await;
    Json(wb.review_history())
}

type ApiError = (StatusCode, String);
fn bad_request(error: impl std::fmt::Display) -> ApiError {
    (StatusCode::BAD_REQUEST, error.to_string())
}

#[derive(Deserialize)]
struct SheetNameRequest {
    name: String,
}
#[derive(Deserialize)]
struct RenameSheetRequest {
    old_name: String,
    new_name: String,
}
async fn add_sheet(
    State(state): State<AppState>,
    Json(req): Json<SheetNameRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .workbook
        .write()
        .await
        .add_sheet(&req.name)
        .map_err(bad_request)?;
    Ok(Json(serde_json::json!({"status":"created"})))
}
async fn select_sheet(
    State(state): State<AppState>,
    Json(req): Json<SheetNameRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .workbook
        .write()
        .await
        .select_sheet(&req.name)
        .map_err(bad_request)?;
    Ok(Json(serde_json::json!({"status":"selected"})))
}
async fn rename_sheet(
    State(state): State<AppState>,
    Json(req): Json<RenameSheetRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .workbook
        .write()
        .await
        .rename_sheet(&req.old_name, &req.new_name)
        .map_err(bad_request)?;
    Ok(Json(serde_json::json!({"status":"renamed"})))
}
async fn delete_sheet(
    State(state): State<AppState>,
    Json(req): Json<SheetNameRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut workbook = state.workbook.write().await;
    workbook.delete_sheet(&req.name).map_err(bad_request)?;
    Ok(Json(serde_json::json!({"active_sheet":workbook.active_sheet})))
}
#[derive(Deserialize)]
struct EditRangeRequest {
    sheet_name: String,
    row: u32,
    col: u32,
    values: Vec<Vec<String>>,
}
#[derive(Deserialize)]
struct ReadRangeRequest {
    sheet_name: String,
    range: CellRange,
}
#[derive(Deserialize)]
struct FormatRangeRequest {
    sheet_name: String,
    range: CellRange,
    style: CellStyle,
}
#[derive(Deserialize)]
struct CopyRangeRequest {
    sheet_name: String,
    range: CellRange,
    target_sheet: String,
    target: CellCoord,
    expected_values: Option<Vec<Vec<String>>>,
    expected_snapshot: Option<String>,
    mode: Option<gridloom_core::spreadsheet::PasteMode>,
}
#[derive(Deserialize)]
struct FillRangeRequest {
    sheet_name: String,
    source: CellRange,
    target: CellRange,
}
async fn edit_range(
    State(state): State<AppState>,
    Json(req): Json<EditRangeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .workbook
        .write()
        .await
        .edit_range(
            &req.sheet_name,
            CellCoord::new(req.row, req.col),
            req.values,
        )
        .map_err(bad_request)?;
    Ok(Json(serde_json::json!({"status":"updated"})))
}
async fn read_range(
    State(state): State<AppState>,
    Json(req): Json<ReadRangeRequest>,
) -> Result<Json<Vec<Vec<String>>>, ApiError> {
    Ok(Json(
        state
            .workbook
            .read()
            .await
            .read_range(&req.sheet_name, req.range)
            .map_err(bad_request)?,
    ))
}
async fn format_range(
    State(state): State<AppState>,
    Json(req): Json<FormatRangeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .workbook
        .write()
        .await
        .format_range(&req.sheet_name, req.range, req.style)
        .map_err(bad_request)?;
    Ok(Json(serde_json::json!({"status":"formatted"})))
}
async fn copy_range(
    State(state): State<AppState>,
    Json(req): Json<CopyRangeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut wb = state.workbook.write().await;
    if let Some(mode) = req.mode {
        let snapshot = req
            .expected_snapshot
            .ok_or_else(|| bad_request("Paste Special needs a copied worksheet selection."))?;
        wb.paste_special(
            &req.sheet_name,
            req.range,
            &req.target_sheet,
            req.target,
            &snapshot,
            mode,
        )
    } else if let Some(snapshot) = req.expected_snapshot {
        wb.copy_range_snapshot_checked(
            &req.sheet_name,
            req.range,
            &req.target_sheet,
            req.target,
            &snapshot,
        )
    } else if let Some(values) = req.expected_values {
        wb.copy_range_checked(
            &req.sheet_name,
            req.range,
            &req.target_sheet,
            req.target,
            &values,
        )
    } else {
        wb.copy_range(&req.sheet_name, req.range, &req.target_sheet, req.target)
    }
    .map_err(bad_request)?;
    Ok(Json(serde_json::json!({"status":"copied"})))
}
async fn fill_range(
    State(state): State<AppState>,
    Json(req): Json<FillRangeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .workbook
        .write()
        .await
        .fill_range(&req.sheet_name, req.source, req.target)
        .map_err(bad_request)?;
    Ok(Json(serde_json::json!({"status":"filled"})))
}
async fn undo(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .workbook
        .write()
        .await
        .undo_sheet_edit()
        .map_err(bad_request)?;
    Ok(Json(serde_json::json!({"status":"undone"})))
}
async fn redo(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .workbook
        .write()
        .await
        .redo_sheet_edit()
        .map_err(bad_request)?;
    Ok(Json(serde_json::json!({"status":"redone"})))
}
async fn list_close_periods(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.workbook.read().await.close_periods.clone())
}
async fn import_trial_balance(
    State(state): State<AppState>,
    Json(req): Json<ImportTrialBalanceRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        state
            .workbook
            .write()
            .await
            .import_trial_balance(req)
            .map_err(bad_request)?,
    ))
}
async fn map_close_accounts(
    State(state): State<AppState>,
    Json(req): Json<MapAccountsRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        state
            .workbook
            .write()
            .await
            .map_close_accounts(req)
            .map_err(bad_request)?,
    ))
}
async fn configure_close(
    State(state): State<AppState>,
    Json(req): Json<ConfigureCloseRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let mut wb = state.workbook.write().await;
    Ok(Json(wb.configure_close(req).map_err(bad_request)?))
}

async fn post_close_adjustment(
    State(state): State<AppState>,
    Json(req): Json<PostAdjustmentRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let mut wb = state.workbook.write().await;
    let period_id = req.period_id;
    let before = wb.close_status(period_id).map_err(bad_request)?;
    let current: std::collections::HashSet<_> = wb
        .reviews
        .iter()
        .filter(|record| {
            !record.invalidated
                && record.decision == gridloom_core::review::ReviewDecision::Reviewed
        })
        .map(|record| record.id)
        .collect();
    let adjustment = wb.post_close_adjustment(req).map_err(bad_request)?;
    let after = wb.close_status(period_id).map_err(bad_request)?;
    let after_sheet = &after.period.sheet_name;
    let invalidated_reviews: Vec<_> = wb.reviews.iter()
        .filter(|record| record.invalidated && current.contains(&record.id))
        .map(|record| serde_json::json!({"id":record.id,"sheet_name":record.sheet_name,"coord":record.coord,"reviewer":record.reviewer})).collect();
    let affected_lines: Vec<_> = after.queue.iter().zip(before.queue.iter())
        .filter(|(after, before)| after.value != before.value || after.status != before.status
            || wb.reviews.iter().any(|review| &review.sheet_name == after_sheet && review.coord == after.coord && review.invalidated && current.contains(&review.id)))
        .map(|(after, before)| serde_json::json!({"label":after.label,"coord":after.coord,"before":before.value,"after":after.value,"before_status":before.status,"after_status":after.status})).collect();
    Ok(Json(
        serde_json::json!({"adjustment":adjustment,"affected_lines":affected_lines,"invalidated_reviews":invalidated_reviews}),
    ))
}

async fn rollforward_close(
    State(state): State<AppState>,
    Json(req): Json<RollforwardRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        state
            .workbook
            .write()
            .await
            .rollforward_close(req)
            .map_err(bad_request)?,
    ))
}
async fn close_status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        state
            .workbook
            .read()
            .await
            .close_status(id)
            .map_err(bad_request)?,
    ))
}
async fn export_review_pack(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        state
            .workbook
            .read()
            .await
            .export_review_pack(id)
            .map_err(bad_request)?,
    ))
}
#[derive(Deserialize)]
struct AttachedReviewRequest {
    review: ReviewRequest,
    attachments: Vec<EvidenceUpload>,
}
async fn record_attached_review(
    State(state): State<AppState>,
    Json(req): Json<AttachedReviewRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        state
            .workbook
            .write()
            .await
            .record_review_with_attachments(req.review, req.attachments)
            .map_err(|message| (StatusCode::CONFLICT, message))?,
    ))
}
#[derive(Deserialize)]
struct CompareQuery {
    after: Option<Uuid>,
}
async fn compare_review(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<CompareQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let wb = state.workbook.read().await;
    let comparison = if let Some(after) = query.after {
        wb.compare_review_versions(id, after)
    } else {
        wb.compare_review(id)
    };
    Ok(Json(comparison.map_err(bad_request)?))
}
#[derive(Deserialize)]
struct DrilldownQuery {
    #[serde(default)]
    offset: usize,
    #[serde(default = "drilldown_limit")]
    limit: usize,
}
fn drilldown_limit() -> usize {
    50
}
async fn source_drilldown(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<DrilldownQuery>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        state
            .workbook
            .read()
            .await
            .source_drilldown(&name, query.offset, query.limit)
            .map_err(bad_request)?,
    ))
}

async fn capture_range(
    State(state): State<AppState>,
    Json(req): Json<ReadRangeRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        state
            .workbook
            .read()
            .await
            .capture_range(&req.sheet_name, req.range)
            .map_err(bad_request)?,
    ))
}

#[derive(Deserialize)]
struct FormatAttributeRequest {
    sheet_name: String,
    range: CellRange,
    attribute: String,
    value: String,
}
async fn format_range_attribute(
    State(state): State<AppState>,
    Json(req): Json<FormatAttributeRequest>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .workbook
        .write()
        .await
        .format_range_attribute(&req.sheet_name, req.range, &req.attribute, &req.value)
        .map_err(bad_request)?;
    Ok(Json(serde_json::json!({"status":"formatted"})))
}

async fn find_cells(
    State(state): State<AppState>,
    Json(req): Json<gridloom_core::find::FindRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let wb = state.workbook.read().await;
    let result = wb
        .find_cells(&req)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(result))
}

#[derive(Deserialize)]
struct ColumnWidthRequest {
    sheet_name: String,
    column: u32,
    width: Option<u16>,
}
async fn set_column_width(
    State(state): State<AppState>,
    Json(req): Json<ColumnWidthRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    state
        .workbook
        .write()
        .await
        .set_column_width(&req.sheet_name, req.column, req.width)
        .map_err(bad_request)?;
    Ok(Json(serde_json::json!({"status":"resized"})))
}

#[derive(Deserialize)]
#[serde(untagged)]
enum FreezePanesRequest {
    Counts {
        sheet_name: String,
        frozen_rows: u32,
        frozen_columns: u32,
    },
    Legacy {
        sheet_name: String,
        freeze_top_row: bool,
        freeze_first_column: bool,
    },
}
async fn set_freeze_panes(
    State(state): State<AppState>,
    Json(req): Json<FreezePanesRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let (sheet_name, rows, columns) = match req {
        FreezePanesRequest::Counts {
            sheet_name,
            frozen_rows,
            frozen_columns,
        } => (sheet_name, frozen_rows, frozen_columns),
        FreezePanesRequest::Legacy {
            sheet_name,
            freeze_top_row,
            freeze_first_column,
        } => (
            sheet_name,
            u32::from(freeze_top_row),
            u32::from(freeze_first_column),
        ),
    };
    state
        .workbook
        .write()
        .await
        .set_frozen_counts(&sheet_name, rows, columns)
        .map_err(bad_request)?;
    Ok(Json(serde_json::json!({"status":"updated"})))
}

#[derive(Deserialize)]
struct ShowFormulasRequest {
    sheet_name: String,
    show_formulas: bool,
}
async fn set_show_formulas(
    State(state): State<AppState>,
    Json(req): Json<ShowFormulasRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    state
        .workbook
        .write()
        .await
        .set_show_formulas(&req.sheet_name, req.show_formulas)
        .map_err(bad_request)?;
    Ok(Json(serde_json::json!({"status":"updated"})))
}

async fn get_viewport_panes(
    State(state): State<AppState>,
    Json(req): Json<gridloom_core::virtual_view::ViewportPanesRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let wb = state.workbook.read().await;
    let response = wb.get_viewport_panes(&req).map_err(bad_request)?;
    Ok(Json(response))
}

async fn selection_summary(
    State(state): State<AppState>,
    Json(req): Json<gridloom_core::selection_summary::SelectionSummaryRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let wb = state.workbook.read().await;
    Ok(Json(wb.selection_summary(&req).map_err(bad_request)?))
}

async fn list_functions() -> impl IntoResponse {
    let mut functions = gridloom_core::functions::FUNCTION_NAMES.to_vec();
    functions.sort_unstable();
    Json(serde_json::json!({"functions": functions}))
}
