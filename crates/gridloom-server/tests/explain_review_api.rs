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
async fn explain_review_conflict_history_and_new_workbook_contract() {
    let mut wb = Workbook::new();
    wb.set_cell("Sheet1", CellCoord::new(0, 0), "10".into())
        .unwrap();
    wb.set_cell("Sheet1", CellCoord::new(0, 1), "=A1*2".into())
        .unwrap();
    let app = create_router(AppState::new(wb));
    let location = json!({"sheet_name":"Sheet1", "row":0,"col":1});
    let (status, text) = call(&app, "POST", "/api/cells/explain", location.clone()).await;
    assert_eq!(status, StatusCode::OK);
    let explanation: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(explanation["can_review"], true);
    let review = json!({"sheet_name":"Sheet1","row":0,"col":1,"expected_snapshot":explanation["snapshot"],
        "decision":"Question","reviewer":"Reviewer","note":"Confirm assumption","evidence":[{"label":"Budget","reference":"budget.pdf p3"}]});
    let (status, _) = call(&app, "POST", "/api/reviews", review.clone()).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(
        &app,
        "POST",
        "/api/cells/set",
        json!({"sheet_name":"Sheet1","row":0,"col":0,"raw_input":"11"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, text) = call(&app, "POST", "/api/reviews", review).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(text.contains("changed"));
    let (_, text) = call(&app, "GET", "/api/reviews", json!(null)).await;
    let history: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(history.as_array().unwrap().len(), 1);
    assert_eq!(history[0]["invalidated"], true);
    assert_eq!(history[0]["evidence"][0]["reference"], "budget.pdf p3");
    let (_, text) = call(&app, "POST", "/api/cells/explain", location).await;
    let explanation: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(explanation["review"]["invalidated"], true);
    let (status, _) = call(&app, "POST", "/api/workbook/new", json!(null)).await;
    assert_eq!(status, StatusCode::OK);
    let (_, text) = call(&app, "GET", "/api/reviews", json!(null)).await;
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), json!([]));
}
