use crate::state::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use gridloom_core::formula_batch::{BatchPlan, BatchReport, FormulaBatch};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Selection {
    sheet_name: String,
}

#[derive(Deserialize)]
pub struct Run {
    sheet_name: String,
    expected_revision: String,
}

type ApiError = (StatusCode, String);

pub async fn inspect(
    State(state): State<AppState>,
    Query(req): Query<Selection>,
) -> Result<Json<BatchPlan>, ApiError> {
    let wb = state.workbook.read().await;
    let batch = FormulaBatch::compile(&wb, &req.sheet_name)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.message().to_owned()))?;
    Ok(Json(batch.plan))
}

pub async fn run(
    State(state): State<AppState>,
    Json(req): Json<Run>,
) -> Result<Json<BatchReport>, ApiError> {
    // Keep the workbook stable from template compilation until the write-back.
    // The database work runs on the blocking thread pool.
    let mut wb = state.workbook.write_owned().await;
    let batch = FormulaBatch::compile(&wb, &req.sheet_name)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.message().to_owned()))?;
    if batch.plan.revision != req.expected_revision {
        return Err((
            StatusCode::CONFLICT,
            "The worksheet template changed. Inspect it again before running.".into(),
        ));
    }
    let connection = batch.plan.connection.clone();
    let report = tokio::task::spawn_blocking(move || {
        let report = batch.run()?;
        wb.invalidate_after_external_database_write(&connection);
        Ok::<_, gridloom_core::CellError>(report)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::BAD_REQUEST, e.message().to_owned()))?;
    Ok(Json(report))
}
