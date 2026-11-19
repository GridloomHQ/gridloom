use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use gridloom_core::Workbook;
use gridloom_server::{routes::create_router, state::AppState};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn post(app: &Router, path: &str, body: Value) -> (StatusCode, String) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
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
async fn paste_special_requires_snapshot_and_preserves_precision_end_to_end() {
    let app = create_router(AppState::new(Workbook::new()));
    let edit = "/api/ranges/edit";
    let copy = "/api/ranges/copy";
    let capture = "/api/ranges/capture";
    let fixture =
        json!({"sheet_name":"Sheet1","row":0,"col":0,"values":[["=9007199254740993.123456"]]});
    assert_eq!(post(&app, edit, fixture).await.0, StatusCode::OK);
    let range = json!({"start_row":0,"start_col":0,"end_row":0,"end_col":0});
    let (status, body) = post(&app, capture, json!({"sheet_name":"Sheet1","range":range})).await;
    assert_eq!(status, StatusCode::OK);
    let captured: Value = serde_json::from_str(&body).unwrap();
    let mut paste = json!({"sheet_name":"Sheet1","target_sheet":"Sheet1","range":range,
        "target":{"row":1,"col":0},"mode":"values"});
    assert_eq!(
        post(&app, copy, paste.clone()).await.0,
        StatusCode::BAD_REQUEST
    );
    paste["expected_snapshot"] = captured["snapshot"].clone();
    let (status, body) = post(&app, copy, paste.clone()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(
        &app,
        "/api/cells/viewport",
        json!({
            "sheet_name":"Sheet1","start_row":1,"start_col":0,"end_row":1,"end_col":0,"overscan":0
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let cells: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        cells["cells"]["A2"]["raw_value"],
        json!({"type":"Decimal","value":"9007199254740993.123456"})
    );
    assert!(cells["cells"]["A2"]["formula"].is_null());
    assert_eq!(
        post(
            &app,
            edit,
            json!({"sheet_name":"Sheet1","row":0,"col":0,"values":[["17"]]})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(post(&app, copy, paste).await.0, StatusCode::BAD_REQUEST);
}
