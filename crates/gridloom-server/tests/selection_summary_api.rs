use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use gridloom_core::{CellCoord, Workbook};
use gridloom_server::{routes::create_router, state::AppState};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn call(app: &Router, method: &str, path: &str, body: Value) -> (StatusCode, String) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn selection_summary_returns_decimal_strings_and_rejects_large_ranges() {
    let mut wb = Workbook::new();
    wb.set_cell("Sheet1", CellCoord::new(0, 0), "9007199254740993".into())
        .unwrap();
    let app = create_router(AppState::new(wb));
    let req = json!({"sheet_name":"Sheet1","start_row":0,"end_row":0,"start_col":0,"end_col":0});
    let (status, body) = call(&app, "POST", "/api/ranges/summary", req).await;
    assert_eq!(status, StatusCode::OK);
    let result: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(result["sum"]["value"], "9007199254740993");
    assert_eq!(result["sum"]["quality"], "exact");
    assert_eq!(result["numeric_count"], 1);
    let (status, _) = call(
        &app,
        "POST",
        "/api/ranges/summary",
        json!({"sheet_name":"Sheet1","start_row":0,"end_row":999,"start_col":0,"end_col":99}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
