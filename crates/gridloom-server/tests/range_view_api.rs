use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use gridloom_core::{CellCoord, CellRange, Workbook};
use gridloom_server::{routes::create_router, state::AppState};
use serde_json::{json, Value};
use tower::ServiceExt;

#[tokio::test]
async fn sort_filter_endpoint_is_read_only_and_reports_original_positions() {
    let mut wb = Workbook::new();
    wb.edit_range(
        "Sheet1",
        CellCoord::new(0, 0),
        vec![
            vec!["Account".into(), "Amount".into()],
            vec!["Cash".into(), "10".into()],
            vec!["Other".into(), "9".into()],
            vec!["Cash".into(), "2".into()],
        ],
    )
    .unwrap();
    let range = CellRange::new(0, 0, 3, 1);
    let before = wb.capture_range("Sheet1", range).unwrap().snapshot;
    let state = AppState::new(wb);
    let router = create_router(state.clone());
    let request = |col| {
        Request::builder().method("POST").uri("/api/ranges/sort-filter")
        .header("Content-Type","application/json").body(Body::from(json!({
            "sheet_name":"Sheet1","range":range,"first_row_header":true,
            "sort":{"col":col,"descending":false},"filter":{"col":0,"mode":"contains","text":"CASH"}
        }).to_string())).unwrap()
    };
    let response = router.clone().oneshot(request(1)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let view: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
    assert_eq!(
        view["rows"],
        json!([{"row":3,"values":["Cash","2"]},{"row":1,"values":["Cash","10"]}])
    );
    assert_eq!(view["total_rows"], 3);
    assert_eq!(
        state
            .workbook
            .read()
            .await
            .capture_range("Sheet1", range)
            .unwrap()
            .snapshot,
        before
    );
    assert_eq!(
        router.oneshot(request(2)).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
}
