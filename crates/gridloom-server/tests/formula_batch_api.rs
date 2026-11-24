use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use gridloom_core::{CellCoord, Workbook};
use gridloom_server::{routes::create_router, state::AppState};
use rusqlite::Connection;
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

const PLAN: &str = "/api/formula-batch?sheet_name=Sheet1";
const RUN: &str = "/api/formula-batch";
async fn inspect(app: &Router) -> String {
    let (status, body) = call(app, "GET", PLAN, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    serde_json::from_str::<Value>(&body).unwrap()["revision"]
        .as_str()
        .unwrap()
        .to_owned()
}
async fn run(app: &Router, revision: &str, expected: StatusCode) -> String {
    let (status, body) = call(
        app,
        "POST",
        RUN,
        json!({"sheet_name":"Sheet1","expected_revision":revision}),
    )
    .await;
    assert_eq!(status, expected, "{body}");
    body
}
async fn edit(app: &Router, row: usize, col: usize, raw: &str) {
    let (status, body) = call(
        app,
        "POST",
        "/api/cells/set",
        json!({"sheet_name":"Sheet1","row":row,"col":col,"raw_input":raw}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}
async fn explain(app: &Router) -> Value {
    let (status, body) = call(
        app,
        "POST",
        "/api/cells/explain",
        json!({"sheet_name":"Sheet1","row":5,"col":1}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    serde_json::from_str(&body).unwrap()
}

#[tokio::test]
async fn same_live_formula_moves_review_and_committed_results_and_rejects_partial_commit() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("input.sqlite");
    let sql = Connection::open(&file).unwrap();
    sql.execute_batch("CREATE TABLE source (id INTEGER PRIMARY KEY, units INTEGER NOT NULL) STRICT;
        INSERT INTO source VALUES(1,3),(2,5);
        CREATE TABLE output(id INTEGER PRIMARY KEY REFERENCES source(id), total INTEGER NOT NULL CHECK(total>=0)) STRICT;
        INSERT INTO output VALUES(1,42);").unwrap();
    let mut wb = Workbook::new();
    wb.add_sqlite_connection("data", &file).unwrap();
    for (r, c, value) in [
        (0, 0, "TAYRIVA FORMULA BATCH 1"),
        (1, 0, "data"),
        (1, 1, "source"),
        (1, 2, "output"),
        (1, 3, "id"),
        (3, 1, "2"),
        (4, 0, "in:units"),
        (4, 1, "out:total"),
        (
            5,
            0,
            r#"=SQLVALUE("data","SELECT units FROM source WHERE id=?",$B$4)"#,
        ),
        (5, 1, "=ROUND(A6/2,0)"),
    ] {
        wb.set_cell("Sheet1", CellCoord::new(r, c), value.to_string())
            .unwrap();
    }
    let app = create_router(AppState::new(wb));
    let review = explain(&app).await;
    assert_eq!(review["value"]["value"], "2");
    let nodes = review["nodes"].to_string();
    assert!(
        nodes.contains("ROUND") && nodes.contains("SELECT units FROM source"),
        "{nodes}"
    );
    let original = inspect(&app).await;
    let output = || {
        sql.prepare("SELECT id,total FROM output ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(output(), vec![(1, 42)]);
    let result: Value = serde_json::from_str(&run(&app, &original, StatusCode::OK).await).unwrap();
    assert_eq!(result["rows"], 2);
    assert_eq!(output(), vec![(1, 2), (2, 2)]);

    edit(&app, 5, 1, "=A6*2").await;
    assert_eq!(explain(&app).await["value"]["value"], "10");
    run(&app, &original, StatusCode::CONFLICT).await;
    assert_eq!(output(), vec![(1, 2), (2, 2)]);
    run(&app, &inspect(&app).await, StatusCode::OK).await;
    assert_eq!(output(), vec![(1, 6), (2, 10)]);

    // The first new result is valid, then the second violates SQLite CHECK.
    // The earlier persisted set must survive the initial DELETE and first INSERT.
    edit(&app, 5, 1, "=4-A6").await;
    run(&app, &inspect(&app).await, StatusCode::BAD_REQUEST).await;
    assert_eq!(output(), vec![(1, 6), (2, 10)]);

    // Fractional cents must not silently truncate. Source table is never changed.
    edit(&app, 5, 1, "=A6/2").await;
    run(&app, &inspect(&app).await, StatusCode::BAD_REQUEST).await;
    assert_eq!(output(), vec![(1, 6), (2, 10)]);
    assert_eq!(
        sql.query_row("SELECT sum(units) FROM source", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        8
    );

    // Reject SQLite objects that can affect unrelated data on replacement.
    sql.execute_batch("CREATE TRIGGER bad AFTER DELETE ON output BEGIN DELETE FROM source; END;")
        .unwrap();
    edit(&app, 5, 1, "=A6").await;
    let message = run(&app, &inspect(&app).await, StatusCode::BAD_REQUEST).await;
    assert!(message.contains("triggers"));
    assert_eq!(output(), vec![(1, 6), (2, 10)]);

    edit(&app, 5, 1, "=B4+1").await;
    let (invalid, _) = call(&app, "GET", PLAN, Value::Null).await;
    assert_eq!(invalid, StatusCode::BAD_REQUEST);
}
