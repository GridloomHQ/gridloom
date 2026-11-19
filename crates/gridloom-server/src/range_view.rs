use crate::state::AppState;
use axum::{extract::State, http::StatusCode, Json};
use gridloom_core::range_view::{RangeView, RangeViewRequest};

pub async fn sort_filter(
    State(state): State<AppState>,
    Json(input): Json<RangeViewRequest>,
) -> Result<Json<RangeView>, (StatusCode, &'static str)> {
    state
        .workbook
        .read()
        .await
        .sort_filter_view(&input)
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}
