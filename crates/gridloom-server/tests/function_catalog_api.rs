use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use gridloom_core::Workbook;
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
async fn function_catalog_is_sorted_unique_and_read_only() {
    let state = AppState::new(Workbook::new());
    let before = serde_json::to_value(&state.workbook.read().await.sheets).unwrap();
    let app = create_router(state.clone());
    let (status, body) = call(&app, "GET", "/api/functions", json!(null)).await;
    assert_eq!(status, StatusCode::OK);
    let result: Value = serde_json::from_str(&body).unwrap();
    let names: Vec<String> = serde_json::from_value(result["functions"].clone()).unwrap();
    let mut expected = gridloom_core::functions::FUNCTION_NAMES
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(names, expected);
    assert_eq!(names.len(), 95);
    assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(
        serde_json::to_value(&state.workbook.read().await.sheets).unwrap(),
        before
    );
}
