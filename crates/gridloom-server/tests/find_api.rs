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
async fn find_api_defaults_pagination_and_validation() {
    let mut wb = Workbook::new();
    wb.set_cell("Sheet1", CellCoord::new(0, 0), "revenue".into())
        .unwrap();
    wb.set_cell("Sheet1", CellCoord::new(1, 0), "Revenue".into())
        .unwrap();
    let app = create_router(AppState::new(wb));
    let (status, body) = call(
        &app,
        "POST",
        "/api/cells/find",
        json!({"query":"revenue","limit":1}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let result: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(result["total_matches"], 2);
    assert_eq!(result["matches"][0]["address"], "A1");
    assert_eq!(result["next_offset"], 1);
    for body in [
        json!({"query":""}),
        json!({"query":"revenue","limit":999}),
        json!({"query":"revenue","sheet_name":"missing"}),
    ] {
        assert_eq!(
            call(&app, "POST", "/api/cells/find", body).await.0,
            StatusCode::BAD_REQUEST
        );
    }
}
