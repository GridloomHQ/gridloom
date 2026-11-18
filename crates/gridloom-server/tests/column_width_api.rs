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
async fn column_width_mutation_is_exposed_in_sheet_summary() {
    let app = create_router(AppState::new(Workbook::new()));
    let body = json!({"sheet_name":"Sheet1","column":3,"width":240});
    assert_eq!(
        call(&app, "POST", "/api/sheets/column-width", body).await.0,
        StatusCode::OK
    );
    let (status, body) = call(&app, "GET", "/api/workbook", json!(null)).await;
    assert_eq!(status, StatusCode::OK);
    let response: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(response["sheets"][0]["column_widths"]["3"], 240);
    for body in [
        json!({"sheet_name":"Sheet1","column":3,"width":10}),
        json!({"sheet_name":"Sheet1","column":100,"width":240}),
        json!({"sheet_name":"missing","column":3,"width":240}),
    ] {
        assert_eq!(
            call(&app, "POST", "/api/sheets/column-width", body).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    let (_, body) = call(&app, "GET", "/api/workbook", json!(null)).await;
    let response: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(response["sheets"][0]["column_widths"]["3"], 240);
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/sheets/column-width",
            json!({"sheet_name":"Sheet1","column":3,"width":null})
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, body) = call(&app, "GET", "/api/workbook", json!(null)).await;
    let response: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(response["sheets"][0]["column_widths"], json!({}));
}
