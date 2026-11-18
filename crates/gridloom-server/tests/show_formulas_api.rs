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
async fn show_formulas_flag_are_validated_and_exposed_in_summary() {
    let app = create_router(AppState::new(Workbook::new()));
    let (_, body) = call(&app, "GET", "/api/workbook", json!(null)).await;
    let response: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(response["sheets"][0]["show_formulas"], false);
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/sheets/show-formulas",
            json!({"sheet_name":"Sheet1","show_formulas":true})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/sheets/show-formulas",
            json!({"sheet_name":"missing","show_formulas":false})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/sheets/show-formulas",
            json!({"sheet_name":"Sheet1"})
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (_, body) = call(&app, "GET", "/api/workbook", json!(null)).await;
    let response: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(response["sheets"][0]["show_formulas"], true);
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/sheets/show-formulas",
            json!({"sheet_name":"Sheet1","show_formulas":false})
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, body) = call(&app, "GET", "/api/workbook", json!(null)).await;
    let response: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(response["sheets"][0]["show_formulas"], false);
}
