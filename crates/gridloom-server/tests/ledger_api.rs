use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use gridloom_core::Workbook;
use gridloom_server::{routes::create_router, state::AppState};
use serde_json::{json, Value};
use tower::ServiceExt;

#[tokio::test]
async fn ledger_paging_uses_registered_sqlite_with_exact_totals_and_rejects_stale_sessions() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("accounting.sqlite");
    let writer = rusqlite::Connection::open(&path).unwrap();
    writer
        .execute_batch(
            "PRAGMA journal_mode=WAL;
        CREATE TABLE ledger(id INTEGER PRIMARY KEY, posted TEXT, amount INTEGER);
        INSERT INTO ledger VALUES(2,'2026-11-18',9007199254740993),(3,'2026-11-19',99),
            (1,'2026-11-18',4);
        CREATE INDEX ordered ON ledger(posted,id);",
        )
        .unwrap();
    let mut workbook = Workbook::new();
    workbook.add_sqlite_connection("accounting", &path).unwrap();
    let router = create_router(AppState::new(workbook));
    let spec = json!({"connection":"accounting","table":"ledger","date":"posted","cents":"amount","from":"2026-11-18","through":"2026-11-18"});
    let call = |session: Value, offset: u64, limit| {
        let request = Request::builder()
            .method("POST")
            .uri("/api/ledger/page")
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({"spec":spec,"session":session,"offset":offset,"limit":limit}).to_string(),
            ))
            .unwrap();
        router.clone().oneshot(request)
    };
    let response = call(Value::Null, 0, 1).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let first: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
    assert_eq!(first["count"], 2);
    assert_eq!(first["total_cents"], "9007199254740997");
    assert_eq!(first["rows"], json!([["1", "2026-11-18", "4"]]));
    let response = call(first["session"].clone(), 1, 1).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let second: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
    assert_eq!(
        second["rows"],
        json!([["2", "2026-11-18", "9007199254740993"]])
    );
    writer.execute("DELETE FROM ledger WHERE id=1", []).unwrap();
    assert_eq!(
        call(first["session"].clone(), 0, 1).await.unwrap().status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(Value::Null, 0, 201).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
}
