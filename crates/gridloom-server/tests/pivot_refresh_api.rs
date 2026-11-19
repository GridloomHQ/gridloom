use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use gridloom_core::{value::CellCoord, Workbook};
use gridloom_server::{routes::create_router, state::AppState};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn call(app: &Router, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
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
    let data = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
    (status, data)
}

#[tokio::test]
async fn refresh_reuses_saved_pivot_and_does_not_change_source_snapshot_or_apply_staged_edits() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let writer = rusqlite::Connection::open(file.path()).unwrap();
    writer
        .execute_batch(
            "CREATE TABLE ledger(id INTEGER PRIMARY KEY, account TEXT, cents INTEGER);
        INSERT INTO ledger VALUES(1,'A',9007199254740993);",
        )
        .unwrap();
    let mut workbook = Workbook::new();
    workbook
        .add_sqlite_connection("Ledger", file.path())
        .unwrap();
    workbook
        .create_named_source("Entries", "Ledger", "SELECT id, account, cents FROM ledger")
        .unwrap();
    workbook
        .insert_table("Sheet1", "EntriesTable", "Entries", CellCoord::new(0, 0))
        .unwrap();
    let app = create_router(AppState::new(workbook));

    let def = json!({"name": "Balances", "source_table": "Entries", "row_fields": ["account"],
        "col_fields": [], "value_field": "cents", "aggregation": "Sum"});
    let (status, saved) = call(&app, "POST", "/api/pivots", def.clone()).await;
    assert_eq!(status, StatusCode::OK, "{saved:?}");
    assert_eq!(
        saved["result"]["rows"][0][1],
        json!({"type":"Decimal","value":"9007199254740993"})
    );
    let (status, edited) = call(
        &app,
        "POST",
        "/api/staged",
        json!({
            "connection_name":"Ledger", "table_name":"ledger", "pk_columns":["id"],
            "pk_values":[{"type":"Integer","value":1}], "column_name":"cents",
            "original_value":{"type":"Integer","value":9007199254740993i64},
            "proposed_value":{"type":"Integer","value":1}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{edited:?}");

    writer
        .execute("INSERT INTO ledger VALUES(2,'A',2),(3,'B',5)", [])
        .unwrap();
    let (status, result) = call(
        &app,
        "POST",
        "/api/pivots/refresh",
        json!({"name":"Balances"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result:?}");
    assert_eq!(result["headers"], saved["result"]["headers"]);
    assert_eq!(
        result["rows"][0][1],
        json!({"type":"Decimal","value":"9007199254740995"})
    );
    assert_eq!(result["rows"].as_array().unwrap().len(), 2);
    assert_eq!(
        call(&app, "GET", "/api/pivots/Balances/data", Value::Null)
            .await
            .1,
        saved["result"]
    );
    assert_eq!(
        call(&app, "GET", "/api/pivots", Value::Null).await.1,
        json!([def.clone()])
    );
    assert_eq!(
        call(&app, "GET", "/api/staged", Value::Null)
            .await
            .1
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        writer
            .query_row("SELECT cents FROM ledger WHERE id=1", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        9007199254740993
    );

    writer.execute("DROP TABLE ledger", []).unwrap();
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/pivots/refresh",
            json!({"name":"Balances"})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let mut invalid = def.clone();
    invalid["value_field"] = json!("missing");
    assert_eq!(
        call(&app, "POST", "/api/pivots", invalid).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(&app, "GET", "/api/pivots/Balances/data", Value::Null)
            .await
            .1,
        saved["result"]
    );
    assert_eq!(
        call(&app, "GET", "/api/pivots", Value::Null).await.1,
        json!([def])
    );
}
