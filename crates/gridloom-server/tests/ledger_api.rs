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

async fn call_trace(
    router: &axum::Router,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, String) {
    let builder = Request::builder().uri(path);
    let request = match body {
        Some(body) => builder
            .method("POST")
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string())),
        None => builder.body(Body::empty()),
    }
    .unwrap();
    let reply = router.clone().oneshot(request).await.unwrap();
    let status = reply.status();
    let body = to_bytes(reply.into_body(), 65536).await.unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn trace_saves_account_period_to_cell_and_uses_exact_amounts_after_reopen() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("accounting.sqlite");
    let target_file = tmp.path().join("review.tay");
    let writer = rusqlite::Connection::open(&path).unwrap();
    writer
        .execute_batch(
            "PRAGMA journal_mode=WAL;
        CREATE TABLE ledger(id INTEGER PRIMARY KEY, posted TEXT, amount INTEGER, account TEXT);
        INSERT INTO ledger VALUES(1,'2026-11-18',9007199254740993,'01'),
            (2,'2026-11-18',500,'1'), (3,'2026-11-18',10,'01'),
            (4,'2026-11-19',7,'01'), (5,'2026-11-20',8,'01'),
            (6,'2026-11-19',800,'1'), (7,'2026-11-18',327,'x'' OR 1=1 --');
        CREATE INDEX ordered_account ON ledger(account,posted,id);",
        )
        .unwrap();
    let mut workbook = Workbook::new();
    workbook.add_sqlite_connection("accounting", &path).unwrap();
    workbook
        .set_cell(
            "Sheet1",
            gridloom_core::CellCoord::new(0, 0),
            "90071992547410.10".into(),
        )
        .unwrap();
    workbook
        .set_cell(
            "Sheet1",
            gridloom_core::CellCoord::new(0, 1),
            "10.001".into(),
        )
        .unwrap();
    let state = AppState::new(workbook);
    let router = create_router(state.clone());
    let spec = json!({"connection":"accounting","table":"ledger","date":"posted","cents":"amount","from":"2026-11-18","through":"2026-11-19","account":{"column":"account","value":"01"}});
    let fetch = |offset, session| {
        call_trace(
            &router,
            "/api/ledger/page",
            Some(json!({"spec":spec,"session":session,"offset":offset,"limit":1})),
        )
    };
    let (status, first) = fetch(0, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let first: Value = serde_json::from_str(&first).unwrap();
    let id = first["session"].clone();
    assert_eq!(first["total_cents"], "9007199254741010");
    assert_eq!(first["count"], 3);
    for (offset, key) in [(1, "3"), (2, "4")] {
        let (status, body) = fetch(offset, id.clone()).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            serde_json::from_str::<Value>(&body).unwrap()["rows"][0][0],
            key
        );
    }
    let quoted=call_trace(&router,"/api/ledger/page",Some(json!({"spec":{ "connection":"accounting","table":"ledger","date":"posted","cents":"amount","account":{"column":"account","value":"x' OR 1=1 --"}},"session":null,"offset":0,"limit":1}))).await;
    assert_eq!(quoted.0, StatusCode::OK, "{}", quoted.1);
    assert_eq!(
        serde_json::from_str::<Value>(&quoted.1).unwrap()["total_cents"],
        "327"
    );

    let trace_api = "/api/ledger/trace";
    let target = json!({"sheet_name":"Sheet1","row":0,"col":0});
    let save = json!({"target":target,"session":id});
    let (status, msg) = call_trace(
        &router,
        trace_api,
        Some(json!({"target":{"sheet_name":"Sheet1","row":0,"col":1},"session":id})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{msg}");
    let (status, reply) = call_trace(&router, trace_api, Some(save.clone())).await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    assert_eq!(
        serde_json::from_str::<Value>(&reply).unwrap(),
        json!({"link":spec,"source_changed":false,"balance_cents":"9007199254741010"})
    );
    state
        .workbook
        .write()
        .await
        .save_to_file(&target_file)
        .unwrap();
    let mut reopened = Workbook::open_from_file(&target_file).unwrap();
    reopened
        .set_cell("Sheet1", gridloom_core::CellCoord::new(0, 0), "0".into())
        .unwrap();
    reopened
        .connection_configs
        .get_mut("accounting")
        .unwrap()
        .path
        .push_str(".moved");
    let reopened_router = create_router(AppState::new(reopened));
    let get = "/api/ledger/trace?sheet_name=Sheet1&row=0&col=0";
    let (status, reply) = call_trace(&reopened_router, get, None).await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    assert_eq!(
        serde_json::from_str::<Value>(&reply).unwrap(),
        json!({"link":spec,"source_changed":true,"balance_cents":"0"})
    );
    writer.execute("DELETE FROM ledger WHERE id=1", []).unwrap();
    assert_eq!(
        call_trace(&router, trace_api, Some(save)).await.0,
        StatusCode::CONFLICT
    );
    let (status, reply) = call_trace(
        &reopened_router,
        trace_api,
        Some(json!({"target":target,"session":null})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    assert_eq!(
        serde_json::from_str::<Value>(&reply).unwrap()["link"],
        Value::Null
    );
}
