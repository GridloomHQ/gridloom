use crate::state::AppState;
use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct InsertRows {
    sheet_name: String,
    before_row: u32,
    count: u32,
}

pub async fn insert_rows(
    State(state): State<AppState>,
    Json(input): Json<InsertRows>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let mut workbook = state.workbook.write().await;
    workbook
        .insert_rows(&input.sheet_name, input.before_row, input.count)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(serde_json::json!({"status":"inserted"})))
}
