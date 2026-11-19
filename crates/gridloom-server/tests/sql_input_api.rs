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

async fn ok(app: &Router, method: &str, path: &str, body: Value) -> Value {
    let (status, result) = call(app, method, path, body).await;
    assert_eq!(status, StatusCode::OK, "{method} {path}: {result}");
    serde_json::from_str(&result).unwrap()
}

fn patch(column: &str, original: Value, raw: &str) -> Value {
    json!({
        "connection_name":"ledger","table_name":"entries","pk_columns":["id"],
        "pk_values":[{"type":"Integer","value":1}], "column_name":column,
        "original_value":original,"raw_input":raw
    })
}

async fn rejects(app: &Router, body: Value, expected: &str) {
    let (status, detail) = call(app, "POST", "/api/staged", body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{detail}");
    assert!(detail.contains(expected), "{detail}");
}

#[tokio::test]
async fn sqlite_declared_types_validate_even_if_original_is_null_and_retain_text() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("ledger.sqlite");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TABLE entries (
             id INTEGER PRIMARY KEY, code TEXT, count INTEGER NOT NULL, amount DECIMAL(24,2),
             posted DATE, active BOOL, attachment BLOB,
             calculated INTEGER GENERATED ALWAYS AS (count + 1) STORED
         );
         INSERT INTO entries (id, count) VALUES (1, 4);",
    )
    .unwrap();
    let mut workbook = Workbook::new();
    workbook
        .add_sqlite_connection("ledger", path.to_str().unwrap())
        .unwrap();
    let app = create_router(AppState::new(workbook));
    let null = json!({"type":"Null"});
    let original_count = json!({"type":"Integer","value":4});

    rejects(
        &app,
        patch("count", original_count.clone(), "4.5"),
        "whole number",
    )
    .await;
    rejects(
        &app,
        patch("count", original_count.clone(), "NULL"),
        "NULL is not allowed",
    )
    .await;
    rejects(
        &app,
        patch("count", original_count.clone(), "9223372036854775808"),
        "whole number",
    )
    .await;
    rejects(
        &app,
        patch("posted", null.clone(), "2025-02-29"),
        "valid calendar date",
    )
    .await;
    rejects(
        &app,
        patch("active", null.clone(), "maybe"),
        "TRUE or FALSE",
    )
    .await;
    rejects(&app, patch("attachment", null.clone(), "text"), "read-only").await;
    rejects(
        &app,
        patch("calculated", json!({"type":"Integer","value":5}), "6"),
        "not an editable",
    )
    .await;
    rejects(
        &app,
        patch("id", json!({"type":"Integer","value":1}), "2"),
        "Primary key columns",
    )
    .await;
    rejects(&app, patch("amount", null.clone(), "0.012"), "scale 2").await;
    rejects(
        &app,
        patch("amount", null.clone(), "9007199254740992.01"),
        "SQLite would change",
    )
    .await;
    assert_eq!(ok(&app, "GET", "/api/staged", Value::Null).await, json!([]));

    let code = ok(
        &app,
        "POST",
        "/api/staged",
        patch("code", null.clone(), "0007"),
    )
    .await;
    ok(
        &app,
        "POST",
        "/api/staged",
        patch("amount", null.clone(), "0.01"),
    )
    .await;
    ok(
        &app,
        "POST",
        "/api/staged",
        patch("posted", null.clone(), "2024-02-29"),
    )
    .await;
    ok(
        &app,
        "POST",
        "/api/staged",
        patch("active", null.clone(), "TRUE"),
    )
    .await;
    assert_eq!(
        db.query_row("SELECT code FROM entries", [], |row| row
            .get::<_, Option<String>>(0))
            .unwrap(),
        None,
        "staging never writes"
    );

    for (raw, expect) in [
        ("", json!({"type":"Text","value":""})),
        ("NULL", json!({"type":"Text","value":"NULL"})),
        ("0007", json!({"type":"Text","value":"0007"})),
    ] {
        let id = ok(
            &app,
            "POST",
            "/api/staged",
            patch("code", null.clone(), raw),
        )
        .await;
        assert_eq!(id, code);
        let current = ok(&app, "GET", "/api/staged", Value::Null).await;
        let edited = current
            .as_array()
            .unwrap()
            .iter()
            .find(|patch| patch["id"] == id["id"])
            .unwrap();
        assert_eq!(edited["proposed_value"], expect);
    }

    ok(
        &app,
        "POST",
        "/api/writeback/apply",
        json!({"connection_name":"ledger"}),
    )
    .await;
    let state = db
        .query_row(
            "SELECT code, amount, posted, active FROM entries",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i32>(3)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(state, ("0007".into(), 0.01, "2024-02-29".into(), 1));
    let mut set_null = patch("code", json!({"type":"Text","value":"0007"}), "");
    set_null.as_object_mut().unwrap().remove("raw_input");
    set_null["proposed_value"] = null;
    ok(&app, "POST", "/api/staged", set_null).await;
    let staged = ok(&app, "GET", "/api/staged", Value::Null).await;
    assert_eq!(staged[0]["proposed_value"], json!({"type":"Null"}));
}

#[tokio::test]
async fn duckdb_rejects_rounding_and_applies_exact_decimal_above_binary64_precision() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("ledger.duckdb");
    {
        let db = duckdb::Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE entries (
                id BIGINT PRIMARY KEY, amount DECIMAL(24,2), count SMALLINT,
                calculated SMALLINT GENERATED ALWAYS AS (count+1)
             );
             INSERT INTO entries (id, amount, count) VALUES (1, 0.00, 4);",
        )
        .unwrap();
    }
    let mut workbook = Workbook::new();
    workbook
        .add_duckdb_connection("ledger", path.to_str().unwrap())
        .unwrap();
    let app = create_router(AppState::new(workbook));

    rejects(
        &app,
        patch("count", json!({"type":"Integer","value":4}), "32768"),
        "whole number",
    )
    .await;
    rejects(
        &app,
        patch("calculated", json!({"type":"Integer","value":5}), "6"),
        "read-only",
    )
    .await;
    let original = json!({"type":"Decimal","value":"0.00"});
    rejects(&app, patch("amount", original.clone(), "1.001"), "scale 2").await;
    rejects(
        &app,
        patch("amount", original.clone(), "10000000000000000000000.00"),
        "precision 24",
    )
    .await;
    ok(
        &app,
        "POST",
        "/api/staged",
        patch("amount", original, "9007199254740992.01"),
    )
    .await;
    let mut overprecise = patch("amount", json!({"type":"Decimal","value":"0.00"}), "");
    overprecise.as_object_mut().unwrap().remove("raw_input");
    overprecise["proposed_value"] =
        json!({"type":"Decimal","value":"0.010000000000000000000000000001"});
    assert_eq!(
        call(&app, "POST", "/api/staged", overprecise).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let staged = ok(&app, "GET", "/api/staged", Value::Null).await;
    assert_eq!(
        staged[0]["proposed_value"],
        json!({"type":"Decimal","value":"9007199254740992.01"})
    );
    let applied = ok(
        &app,
        "POST",
        "/api/writeback/apply",
        json!({"connection_name":"ledger"}),
    )
    .await;
    assert_eq!(applied["status"], "AppliedSuccessfully");
    drop(app);
    let db = duckdb::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT amount::VARCHAR FROM entries", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "9007199254740992.01"
    );
}

#[tokio::test]
async fn approximate_or_fractional_keys_cannot_guess_a_row() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("ledger.duckdb");
    {
        let db = duckdb::Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE entries (id DECIMAL(24,2) PRIMARY KEY, label VARCHAR);
             INSERT INTO entries VALUES (9007199254740992.00, 'first'), (9007199254740992.01, 'second');",
        )
        .unwrap();
    }
    let mut workbook = Workbook::new();
    workbook
        .add_duckdb_connection("ledger", path.to_str().unwrap())
        .unwrap();
    let app = create_router(AppState::new(workbook));
    let mut edit = patch(
        "label",
        json!({"type":"Text","value":"second"}),
        "not applied",
    );
    edit["pk_values"][0] = json!({"type":"Decimal","value":"9007199254740992.01"});
    rejects(&app, edit, "Fractional decimal keys are read-only").await;
    assert_eq!(ok(&app, "GET", "/api/staged", Value::Null).await, json!([]));
    let mut edit = patch(
        "label",
        json!({"type":"Text","value":"first"}),
        "updated first",
    );
    edit["pk_values"][0] = json!({"type":"Decimal","value":"9007199254740992.00"});
    ok(&app, "POST", "/api/staged", edit).await;
    let applied = ok(
        &app,
        "POST",
        "/api/writeback/apply",
        json!({"connection_name":"ledger"}),
    )
    .await;
    assert_eq!(applied["status"], "AppliedSuccessfully");
    drop(app);
    let db = duckdb::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT label FROM entries WHERE id=9007199254740992.01",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "second"
    );
}
