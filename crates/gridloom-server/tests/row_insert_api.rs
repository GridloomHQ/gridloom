use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use gridloom_core::{CellCoord, Workbook};
use gridloom_server::{routes::create_router, state::AppState};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn api_inserts_rows_atomically_and_reports_rejections() {
    let mut w = Workbook::new();
    w.set_cell("Sheet1", CellCoord::new(1, 0), "5".into())
        .unwrap();
    w.set_cell("Sheet1", CellCoord::new(2, 0), "=A2".into())
        .unwrap();
    let state = AppState::new(w);
    let router = create_router(state.clone());
    let insert = |row, count| {
        router.clone().oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/sheets/insert-rows")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"sheet_name":"Sheet1","before_row":row,"count":count}).to_string(),
                ))
                .unwrap(),
        )
    };
    let response = insert(1, 2).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let reply = insert(1, 0).await.unwrap();
    assert_eq!(reply.status(), StatusCode::BAD_REQUEST);
    let text = to_bytes(reply.into_body(), 1024).await.unwrap();
    assert!(String::from_utf8_lossy(&text).contains("between 1 and 1,000 rows"));
    let workbook = state.workbook.read().await;
    let sheet = &workbook.sheets["Sheet1"];
    assert_eq!(sheet.max_rows, 1002);
    assert_eq!(sheet.cells["A5"].raw_input, "=A4");
    assert_eq!(sheet.cells["A5"].evaluated_value.to_string_repr(), "5");
}
