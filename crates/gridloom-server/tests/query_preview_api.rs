use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use gridloom_core::{db::DbConnection, Workbook};
use gridloom_server::{routes::create_router, state::AppState};
use serde_json::{json, Value};
use tower::ServiceExt;

fn app() -> Router {
    let mut wb = Workbook::new();
    wb.connections
        .insert("SQLite".into(), DbConnection::open_sqlite_memory().unwrap());
    wb.connections
        .insert("DuckDB".into(), DbConnection::open_duckdb_memory().unwrap());
    create_router(AppState::new(wb))
}

async fn query(
    app: &Router,
    db: &str,
    sql: &str,
    offset: u64,
    limit: usize,
) -> (StatusCode, String) {
    let res = app
        .clone()
        .oneshot(
            Request::post("/api/query/preview")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({"connection_name":db, "sql":sql, "offset":offset, "limit":limit})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let body = to_bytes(res.into_body(), 32_768).await.unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn pagination_applies_after_filter_sort_and_user_limit_on_both_engines() {
    let app = app();
    let sql = "WITH items(id) AS (VALUES (1), (2), (3), (4), (5), (6))\n\
               SELECT id, 'café; test' AS note FROM items WHERE id > 1 ORDER BY id DESC LIMIT 3; -- Delimiter";
    for db in ["SQLite", "DuckDB"] {
        for (offset, ids, has_more) in [
            (0, vec![6, 5], true),
            (2, vec![4], false),
            (4, vec![], false),
        ] {
            let (status, body) = query(&app, db, sql, offset, 2).await;
            assert_eq!(status, StatusCode::OK, "{db}: {body}");
            let page: Value = serde_json::from_str(&body).unwrap();
            assert_eq!(page["offset"], offset);
            assert_eq!(page["has_more"], has_more);
            assert_eq!(page["columns"][0]["name"], "id");
            assert_eq!(page["columns"][1]["name"], "note");
            let rows = page["rows"].as_array().unwrap();
            assert_eq!(
                rows.iter()
                    .map(|r| r[0]["value"].as_i64().unwrap())
                    .collect::<Vec<_>>(),
                ids
            );
            assert!(rows.iter().all(|r| r[1]["value"] == "café; test"));
        }
        let (status, body) = query(&app, db, "SELECT 'é' AS v;-- final", 0, 1).await;
        assert_eq!(status, StatusCode::OK, "{db}: {body}");
        let page: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(page["has_more"], false);
    }
}

#[tokio::test]
async fn rejects_excessive_pages_and_multi_statement_requests() {
    let app = app();
    for (sql, offset, limit) in [
        ("SELECT 1", 0, 0),
        ("SELECT 1", 0, 101),
        ("SELECT 1", u64::MAX, 100),
        ("SELECT 1; SELECT 2", 0, 100),
        ("DELETE FROM invoices", 0, 100),
    ] {
        let (status, body) = query(&app, "SQLite", sql, offset, limit).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
    assert_eq!(
        query(&app, "Missing", "SELECT 1", 0, 100).await.0,
        StatusCode::NOT_FOUND
    );
}
