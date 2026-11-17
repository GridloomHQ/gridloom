pub mod benchmark;
pub mod routes;
pub mod sample_data;
pub mod state;

use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use gridloom_core::workbook::Workbook;
use sample_data::ensure_sample_databases;
use state::AppState;
use std::net::SocketAddr;

pub async fn build_app(open_samples: bool) -> anyhow::Result<axum::Router> {
    let mut workbook = Workbook::new();

    if open_samples {
        if let Ok((sqlite_p, duckdb_p)) = ensure_sample_databases() {
            let _ = workbook.add_sqlite_connection("SalesDB", &sqlite_p);
            let _ = workbook.add_duckdb_connection("AnalyticsDB", &duckdb_p);

            // Pre-seed sample tables and queries
            let _ = workbook.create_named_source(
                "Invoices",
                "SalesDB",
                "SELECT id, customer_name, issued_on, amount, status FROM invoices",
            );

            let _ = workbook.insert_table(
                "Sheet1",
                "Invoices",
                "Invoices",
                gridloom_core::value::CellCoord::new(1, 1), // B2
            );
            sample_data::seed_sample_financial_model(&mut workbook)?;
        }
    }

    let state = AppState::new(workbook);
    let mut router = routes::create_router(state).layer(middleware::from_fn(local_boundary));

    // Installed bundles work outside the source checkout; the launcher also supports
    // a user-provided asset location without changing the workbook's file paths.
    let dist_path = std::env::var_os("GRIDLOOM_FRONTEND_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("frontend/dist")))
                .filter(|p| p.exists())
        })
        .unwrap_or_else(|| std::path::PathBuf::from("frontend/dist"));
    if dist_path.exists() {
        router = router.fallback_service(tower_http::services::ServeDir::new(dist_path));
    }

    Ok(router)
}

pub async fn start_server(
    port: u16,
    open_samples: bool,
) -> anyhow::Result<(u16, tokio::task::JoinHandle<()>)> {
    let app = build_app(open_samples).await?;
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let actual_port = listener.local_addr()?.port();

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    Ok((actual_port, handle))
}

/// A loopback-only application is not a remote multi-user service. Reject browser
/// requests from other origins and DNS-rebinding hosts, not just CORS reads.
async fn local_boundary(request: Request, next: Next) -> Response {
    let local_authority = |authority: &str| {
        let host = authority.split(':').next().unwrap_or("");
        host.eq_ignore_ascii_case("localhost")
            || host == "127.0.0.1"
            || authority == "[::1]"
            || authority.starts_with("[::1]:")
    };
    let allowed_host = request
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .is_some_and(local_authority);
    let allowed_origin = request.headers().get(header::ORIGIN).is_none_or(|origin| {
        origin
            .to_str()
            .ok()
            .and_then(|s| s.strip_prefix("http://"))
            .is_some_and(|s| !s.contains('/') && !s.contains('@') && local_authority(s))
    });
    if !allowed_host || !allowed_origin {
        return (StatusCode::FORBIDDEN, "Gridloom accepts only loopback hosts and local browser origins. This is not a remote multi-user service.").into_response();
    }
    next.run(request).await
}
