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
async fn atomic_panes_endpoint_returns_existing_viewport_shape_with_loaded_ranges() {
    let mut wb = Workbook::new();
    wb.set_cell("Sheet1", CellCoord::new(0, 0), "corner".into())
        .unwrap();
    wb.set_cell("Sheet1", CellCoord::new(500, 50), "main".into())
        .unwrap();
    let app = create_router(AppState::new(wb));
    let req = json!({"sheet_name":"Sheet1","start_row":500,"end_row":505,"start_col":50,"end_col":55,"overscan":2,"freeze_top_row":true,"freeze_first_column":true});
    let (status, body) = call(&app, "POST", "/api/cells/viewport-panes", req).await;
    assert_eq!(status, StatusCode::OK);
    let result: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(result["sheet_name"], "Sheet1");
    assert_eq!(result["cells"]["A1"]["display_text"], "corner");
    assert_eq!(result["loaded_ranges"].as_array().unwrap().len(), 4);
    assert_eq!(result["start_row"], 498);
    let (status,_)=call(&app,"POST","/api/cells/viewport-panes",json!({"sheet_name":"Sheet1","start_row":0,"end_row":999,"start_col":0,"end_col":99,"overscan":0,"freeze_top_row":false,"freeze_first_column":false})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
