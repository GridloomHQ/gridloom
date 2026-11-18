use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use gridloom_core::Workbook;
use gridloom_server::{routes::create_router, state::AppState};
use serde_json::{json, Value};
use tower::ServiceExt;
async fn call(app: &Router, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let res = app
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
    let status = res.status();
    let bytes = to_bytes(res.into_body(), 40 * 1024 * 1024).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into())),
    )
}
#[tokio::test]
async fn monthly_workflow_attached_review_compare_pack_and_reopen() {
    let app = create_router(AppState::new(Workbook::new()));
    let csv = "account,name,debit,credit\n1000,Cash,10000,0\n4000,Sales,0,15000\n5000,Costs,5000,0";
    let (status, p) = call(
        &app,
        "POST",
        "/api/close/import",
        json!({"period":"2026-10","csv":csv}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{p}");
    let id = p["id"].as_str().unwrap();
    let sheet = &p["sheet_name"];
    let (status,_)=call(&app,"POST","/api/close/map",json!({"period_id":id,"mappings":{"1000":"Balance sheet","4000":"Revenue","5000":"Cost of sales"}})).await;
    assert_eq!(status, StatusCode::OK);
    let location = json!({"sheet_name":sheet,"row":6,"col":8});
    let (_, e) = call(&app, "POST", "/api/cells/explain", location.clone()).await;
    assert_eq!(e["can_review"], true);
    let (status,review)=call(&app,"POST","/api/reviews/attached",json!({"review":{"sheet_name":sheet,"row":6,"col":8,"expected_snapshot":e["snapshot"],"decision":"Reviewed","reviewer":"Local reviewer","note":"Tie-out","evidence":[]},"attachments":[{"filename":"support.txt","media_type":"text/plain","bytes":[116,105,101]}]})).await;
    assert_eq!(status, StatusCode::OK, "{review}");
    assert_eq!(review["attachments"][0]["bytes"], json!([116, 105, 101]));
    assert!(review["input_snapshot"]["inputs"].is_object());
    let review_id = review["id"].as_str().unwrap();
    let (status, _) = call(
        &app,
        "POST",
        "/api/cells/set",
        json!({"sheet_name":sheet,"row":2,"col":4,"raw_input":"16000"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, diff) = call(
        &app,
        "GET",
        &format!("/api/reviews/{review_id}/compare"),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{diff}");
    assert!(!diff["changes"].as_array().unwrap().is_empty());
    let (_, pack) = call(&app, "GET", &format!("/api/close/{id}/pack"), Value::Null).await;
    assert_eq!(pack["format"], "gridloom-review-pack");
    assert_eq!(
        pack["reviews"][0]["attachments"][0]["filename"],
        "support.txt"
    );
    assert_eq!(pack["status"]["balanced"], false);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("monthly.gridloom");
    assert_eq!(
        call(&app, "POST", "/api/workbook/save", json!({"path":path}))
            .await
            .0,
        StatusCode::OK
    );
    call(&app, "POST", "/api/workbook/new", Value::Null).await;
    assert_eq!(
        call(&app, "POST", "/api/workbook/open", json!({"path":path}))
            .await
            .0,
        StatusCode::OK
    );
    let (_, periods) = call(&app, "GET", "/api/close/periods", Value::Null).await;
    assert_eq!(periods[0]["source_csv"], csv);
    let (_, reviews) = call(&app, "GET", "/api/reviews", Value::Null).await;
    assert_eq!(reviews[0]["invalidated"], true);
}
#[tokio::test]
async fn ranges_and_cross_sheet_are_functional_over_http() {
    let app = create_router(AppState::new(Workbook::new()));
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/ranges/edit",
            json!({"sheet_name":"Sheet1","row":0,"col":0,"values":[["2","=A1*3"],["4","=A2*3"]]})
        )
        .await
        .0,
        StatusCode::OK
    );
    let range = json!({"start_row":0,"start_col":0,"end_row":1,"end_col":1});
    assert_eq!(call(&app,"POST","/api/ranges/format",json!({"sheet_name":"Sheet1","range":range,"style":{"bold":true,"italic":false,"fill":"#ffeecc","number_format":"0.00"}})).await.0,StatusCode::OK);
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/ranges/format-attribute",
            json!({"sheet_name":"Sheet1","range":range,"attribute":"italic","value":"true"})
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_,viewport) = call(&app,"POST","/api/cells/viewport",json!({"sheet_name":"Sheet1","start_row":0,"start_col":0,"end_row":2,"end_col":2,"overscan":0})).await;
    assert_eq!(viewport["cells"]["A1"]["style"]["bold"], true);
    assert_eq!(viewport["cells"]["A1"]["style"]["italic"], true);
    assert_eq!(viewport["cells"]["A1"]["style"]["fill"], "#ffeecc");
    let (_, capture) = call(
        &app,
        "POST",
        "/api/ranges/capture",
        json!({"sheet_name":"Sheet1","range":range}),
    )
    .await;
    assert_eq!(capture["values"][0][1], "=A1*3");
    assert_eq!(
        call(&app, "POST", "/api/sheets", json!({"name":"Other sheet"}))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(call(&app,"POST","/api/ranges/copy",json!({"sheet_name":"Sheet1","range":range,"target_sheet":"Other sheet","target":{"row":0,"col":0},"expected_snapshot":capture["snapshot"]})).await.0,StatusCode::OK);
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/cells/set",
            json!({"sheet_name":"Other sheet","row":0,"col":3,"raw_input":"=Sheet1!B1"})
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, e) = call(
        &app,
        "POST",
        "/api/cells/explain",
        json!({"sheet_name":"Other sheet","row":0,"col":3}),
    )
    .await;
    assert!(
        e["value"] == json!({"type":"Integer","value":6})
            || e["value"] == json!({"type":"Decimal","value":"6"}),
        "{e}"
    );
    assert_eq!(e["can_review"], true);
    assert_eq!(
        call(&app, "POST", "/api/history/undo", Value::Null).await.0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, "POST", "/api/history/redo", Value::Null).await.0,
        StatusCode::OK
    );
    call(
        &app,
        "POST",
        "/api/cells/set",
        json!({"sheet_name":"Sheet1","row":0,"col":0,"raw_input":"3"}),
    )
    .await;
    assert_eq!(call(&app,"POST","/api/ranges/copy",json!({"sheet_name":"Sheet1","range":range,"target_sheet":"Other sheet","target":{"row":3,"col":0},"expected_snapshot":capture["snapshot"]})).await.0,StatusCode::BAD_REQUEST);
}
#[tokio::test]
async fn local_server_rejects_cross_origin_and_rebinding() {
    let app = gridloom_server::build_app(false).await.unwrap();
    for (host, origin, status) in [
        ("127.0.0.1:3001", None, StatusCode::OK),
        ("evil.example:3001", None, StatusCode::FORBIDDEN),
        (
            "127.0.0.1:3001",
            Some("https://evil.example"),
            StatusCode::FORBIDDEN,
        ),
        (
            "localhost:3001",
            Some("http://localhost:3001"),
            StatusCode::OK,
        ),
    ] {
        let mut req = Request::builder().uri("/api/health").header("Host", host);
        if let Some(origin) = origin {
            req = req.header("Origin", origin);
        }
        assert_eq!(
            app.clone()
                .oneshot(req.body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status(),
            status
        );
    }
}

#[tokio::test]
async fn working_paper_question_adjustment_impact_and_reproducible_export() {
    let app = create_router(AppState::new(Workbook::new()));
    let csv = "account,name,debit,credit\n1000,Cash,100,0\n4000,Sales,0,100";
    let (status, p) = call(
        &app,
        "POST",
        "/api/close/import",
        json!({"period":"2026-10","csv":csv}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let id = &p["id"];
    let sheet = &p["sheet_name"];
    let (status, configured) = call(
        &app,
        "POST",
        "/api/close/configure",
        json!({
            "period_id":id,"entity":"Sample Ltd","currency":"BRL","units":"units",
            "report_lines":[
                {"id":"cash","label":"Cash","parent_id":null,"normal_balance":"Debit"},
                {"id":"revenue","label":"Revenue","parent_id":null,"normal_balance":"Credit"}],
            "mappings":{}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{configured}");
    let cash = json!({"sheet_name":sheet,"row":1,"col":8});
    let (_, before) = call(&app, "POST", "/api/cells/explain", cash.clone()).await;
    assert_eq!(before["can_review"], false);
    let mut question_request = cash.clone();
    question_request["expected_snapshot"] = before["snapshot"].clone();
    question_request["decision"] = json!("Question");
    question_request["reviewer"] = json!("Reviewer");
    question_request["note"] = json!("Check last transaction");
    question_request["evidence"] = json!([]);
    let (status, question) = call(&app, "POST", "/api/reviews", question_request).await;
    assert_eq!(status, StatusCode::OK, "{question}");
    let (status, queue) = call(&app, "GET", "/api/questions", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(queue[0]["id"], question["id"]);
    let (status, deps) = call(&app, "POST", "/api/cells/dependencies", cash.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(deps["upstream"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["kind"] == "range"));
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/close/map",
            json!({"period_id":id,"mappings":{"1000":"cash","4000":"revenue"}})
        )
        .await
        .0,
        StatusCode::OK
    );
    let revenue = json!({"sheet_name":sheet,"row":2,"col":8});
    let (_, before_adjustment) = call(&app, "POST", "/api/cells/explain", revenue.clone()).await;
    let (status,review)=call(&app,"POST","/api/reviews",json!({
        "sheet_name":sheet,"row":2,"col":8,"expected_snapshot":before_adjustment["snapshot"],
        "decision":"Reviewed","reviewer":"Controller","note":"Reviewed before adjustment","evidence":[]
    })).await;
    assert_eq!(status, StatusCode::OK, "{review}");
    let (status,receipt)=call(&app,"POST","/api/close/adjustments",json!({
        "period_id":id,"author":"Preparer","rationale":"Accrued invoice",
        "lines":[{"account":"1000","debit":"25","credit":"0"},{"account":"4000","debit":"0","credit":"25"}],
        "evidence":[{"filename":"invoice.txt","media_type":"text/plain","bytes":[65,66]}]
    })).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(receipt["affected_lines"].as_array().unwrap().len(), 2);
    assert_eq!(receipt["invalidated_reviews"][0]["id"], review["id"]);
    let (_, comparison) = call(
        &app,
        "GET",
        &format!("/api/reviews/{}/compare", review["id"].as_str().unwrap()),
        Value::Null,
    )
    .await;
    assert!(comparison["summaries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["category"] == "adjustment"));
    let (_, current) = call(&app, "POST", "/api/cells/explain", cash.clone()).await;
    let path = format!(
        "/api/questions/{}/transition",
        question["id"].as_str().unwrap()
    );
    let (status,resolved)=call(&app,"POST",&path,json!({"action":"Resolved","actor":"Reviewer","note":"Verified invoice and accrual","expected_snapshot":current["snapshot"]})).await;
    assert_eq!(status, StatusCode::OK, "{resolved}");
    let (_, queue) = call(&app, "GET", "/api/questions", Value::Null).await;
    assert_eq!(queue, json!([]));
    let (_, pack) = call(
        &app,
        "GET",
        &format!("/api/close/{}/pack", id.as_str().unwrap()),
        Value::Null,
    )
    .await;
    assert_eq!(pack["version"], 2);
    assert_eq!(pack["status"]["period"]["source_csv"], csv);
    assert_eq!(
        pack["status"]["live_accounts"][0]["reported_balance"],
        "125"
    );
    assert_eq!(
        pack["status"]["period"]["adjustments"][0]["evidence"][0]["bytes"],
        json!([65, 66])
    );
    let recorded_question = pack["reviews"]
        .as_array()
        .unwrap()
        .iter()
        .find(|review| review["id"] == question["id"])
        .expect("Question retained in review pack");
    assert_eq!(
        recorded_question["question_events"][0]["action"],
        "Resolved"
    );
}
