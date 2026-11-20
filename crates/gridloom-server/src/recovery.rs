//! Local recovery snapshots. Independent of explicit file saves and database writeback.
use gridloom_core::workbook::Workbook;
use serde::Serialize;
use std::{fs, io, path::PathBuf};
use uuid::Uuid;

pub struct Recovery {
    pub dir: PathBuf,
    pub current: Uuid,
    pub dirty: bool,
}

#[derive(Serialize)]
pub struct RecoveryEntry {
    pub id: Uuid,
    pub modified: String,
}

impl Recovery {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            current: Uuid::new_v4(),
            dirty: false,
        }
    }

    pub fn path(&self, id: Uuid) -> PathBuf {
        self.dir.join(format!("{id}.gridloom"))
    }

    pub fn checkpoint(&mut self, workbook: &mut Workbook) -> anyhow::Result<()> {
        if self.dirty {
            let mut dir = fs::DirBuilder::new();
            dir.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                dir.mode(0o700);
            }
            dir.create(&self.dir)?;
            workbook
                .save_recovery_copy(self.path(self.current))
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            self.dirty = false;
        }
        Ok(())
    }

    pub fn rotate(&mut self) {
        self.current = Uuid::new_v4();
        self.dirty = false;
    }

    pub fn list(&self) -> io::Result<Vec<RecoveryEntry>> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(e),
        };
        let mut snapshots = Vec::new();
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|x| x.to_str()) != Some("gridloom")
                || !entry.file_type()?.is_file()
            {
                continue;
            }
            let Some(id) = path
                .file_stem()
                .and_then(|x| x.to_str())
                .and_then(|x| Uuid::parse_str(x).ok())
            else {
                continue;
            };
            if id == self.current || path != self.path(id) {
                continue;
            }
            snapshots.push(RecoveryEntry {
                id,
                modified: chrono::DateTime::<chrono::Utc>::from(entry.metadata()?.modified()?)
                    .to_rfc3339(),
            });
        }
        snapshots.sort_unstable_by(|a, b| b.modified.cmp(&a.modified).then(a.id.cmp(&b.id)));
        Ok(snapshots)
    }
}

pub fn default_dir() -> anyhow::Result<PathBuf> {
    if let Some(dir) = std::env::var_os("GRIDLOOM_RECOVERY_DIR").filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|d| PathBuf::from(d).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|d| PathBuf::from(d).join(".local/state")))
    };
    Ok(base
        .ok_or_else(|| anyhow::anyhow!("Set GRIDLOOM_RECOVERY_DIR to a private local folder"))?
        .join("gridloom/recovery"))
}

use crate::state::AppState;
use axum::{
    extract::{Path, Request, State},
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};

pub fn start_worker(state: &AppState) {
    let wb = std::sync::Arc::downgrade(&state.workbook);
    let recovery = state.recovery.as_ref().map(std::sync::Arc::downgrade);
    let gate = std::sync::Arc::downgrade(&state.mutation_gate);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(2));
        loop {
            ticker.tick().await;
            let (Some(wb), Some(recovery), Some(gate)) = (
                wb.upgrade(),
                recovery.as_ref().and_then(|r| r.upgrade()),
                gate.upgrade(),
            ) else {
                break;
            };
            let _guard = gate.lock().await;
            if let Err(error) = checkpoint(wb, recovery).await {
                tracing::warn!(%error, "Gridloom autosave could not write a recovery copy");
            }
        }
    });
}

async fn checkpoint(
    wb: std::sync::Arc<tokio::sync::RwLock<Workbook>>,
    recovery: std::sync::Arc<tokio::sync::Mutex<Recovery>>,
) -> anyhow::Result<()> {
    let mut recovery = recovery.lock_owned().await;
    if !recovery.dirty {
        return Ok(());
    }
    let mut wb = wb.write_owned().await;
    tokio::task::spawn_blocking(move || recovery.checkpoint(&mut wb)).await?
}

pub async fn track_mutations(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let Some(recovery) = state.recovery.clone() else {
        return next.run(request).await;
    };
    let path = request.uri().path();
    let replacing = path == "/api/workbook/new"
        || path == "/api/workbook/open"
        || path.starts_with("/api/workbook/recovery/");
    let read_only_post = matches!(
        path,
        "/api/ranges/read"
            | "/api/ranges/summary"
            | "/api/ranges/capture"
            | "/api/query/execute"
            | "/api/query/analyze"
            | "/api/ledger/page"
            | "/api/sample"
            | "/api/sample/ledger"
            | "/api/pivots/refresh"
            | "/api/ranges/sort-filter"
            | "/api/cells/viewport"
            | "/api/cells/viewport-panes"
            | "/api/cells/find"
            | "/api/cells/explain"
            | "/api/cells/dependencies"
            | "/api/benchmark/run"
            | "/api/workbook/save"
    );
    if !path.starts_with("/api/")
        || !matches!(
            *request.method(),
            Method::POST | Method::DELETE | Method::PUT | Method::PATCH
        )
        || read_only_post
    {
        return next.run(request).await;
    }
    let restoring = path.starts_with("/api/workbook/recovery/");
    let _guard = state.mutation_gate.lock().await;
    if replacing {
        if let Err(error) = checkpoint(state.workbook.clone(), recovery.clone()).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Cannot replace workbook without preserving recovery copy: {error}"),
            )
                .into_response();
        }
    } else {
        // Even a failed operation can leave partial workbook changes.
        recovery.lock().await.dirty = true;
    }
    let response = next.run(request).await;
    if replacing && response.status().is_success() {
        let mut recovery = recovery.lock().await;
        recovery.rotate();
        recovery.dirty = restoring;
    }
    response
}

pub async fn list(
    State(state): State<AppState>,
) -> Result<Json<Vec<RecoveryEntry>>, (StatusCode, String)> {
    let recovery = state
        .recovery
        .ok_or((StatusCode::NOT_FOUND, "Recovery unavailable".into()))?;
    let entries = recovery
        .lock()
        .await
        .list()
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    Ok(Json(entries))
}

pub async fn restore(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let recovery = state
        .recovery
        .ok_or((StatusCode::NOT_FOUND, "Recovery unavailable".into()))?;
    let path = {
        let recovery = recovery.lock().await;
        let path = recovery.path(id);
        if id == recovery.current || !fs::symlink_metadata(&path).is_ok_and(|m| m.is_file()) {
            return Err((StatusCode::NOT_FOUND, "Recovery copy not found".into()));
        }
        path
    };
    let mut restored = tokio::task::spawn_blocking(move || Workbook::open_from_file(path))
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    // A recovery copy is not the explicit file associated with the document.
    restored.file_path = None;
    *state.workbook.write().await = restored;
    Ok(Json(serde_json::json!({"status":"restored"})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use serde_json::{json, Value};
    use std::sync::Arc;
    use tower::ServiceExt;

    async fn call(
        app: &axum::Router,
        method: &str,
        uri: &str,
        value: Value,
    ) -> (StatusCode, Value) {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header("content-type", "application/json")
                    .body(Body::from(value.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        (
            status,
            serde_json::from_slice(&body).unwrap_or_else(|_| json!(String::from_utf8_lossy(&body))),
        )
    }

    #[tokio::test]
    async fn checkpoint_and_restore_preserve_explicit_file_and_do_not_apply_database_edits() {
        let dir = tempfile::tempdir().unwrap();
        let manual = dir.path().join("named.gridloom");
        let db = dir.path().join("ledger.db");
        let db_conn = rusqlite::Connection::open(&db).unwrap();
        db_conn
            .execute(
                "CREATE TABLE entries (id INTEGER PRIMARY KEY, amount INTEGER)",
                [],
            )
            .unwrap();
        db_conn
            .execute("INSERT INTO entries VALUES (1, 11)", [])
            .unwrap();
        let mut workbook = Workbook::new();
        workbook
            .add_sqlite_connection("Ledger", db.to_str().unwrap())
            .unwrap();
        workbook.save_to_file(&manual).unwrap();
        let before = fs::read(&manual).unwrap();
        let mut state = AppState::new(workbook);
        let recovery_dir = dir.path().join("recovery");
        let recovery = Arc::new(tokio::sync::Mutex::new(Recovery::new(recovery_dir.clone())));
        state.recovery = Some(recovery.clone());
        let app = crate::routes::create_router(state.clone());
        let edit = call(
            &app,
            "POST",
            "/api/ranges/edit",
            json!({"sheet_name":"Sheet1","row":0,"col":0,"values":[["42"]]}),
        )
        .await;
        assert_eq!(edit.0, StatusCode::OK, "{edit:?}");
        let staged = call(
            &app,
            "POST",
            "/api/staged",
            json!({
                "connection_name":"Ledger","table_name":"entries","pk_columns":["id"],
                "pk_values":[{"type":"Decimal","value":"1"}],"column_name":"amount","original_value":{"type":"Decimal","value":"11"},"proposed_value":{"type":"Decimal","value":"99"}
            }),
        )
        .await;
        assert_eq!(staged.0, StatusCode::OK, "{staged:?}");
        checkpoint(state.workbook.clone(), recovery.clone())
            .await
            .unwrap();
        assert_eq!(
            state.workbook.read().await.file_path.as_deref(),
            Some(manual.as_path())
        );
        assert_eq!(fs::read(&manual).unwrap(), before);
        assert_eq!(
            db_conn
                .query_row("SELECT amount FROM entries", [], |r| r.get::<_, i32>(0))
                .unwrap(),
            11
        );

        let mut restarted = AppState::new(Workbook::new());
        restarted.recovery = Some(Arc::new(tokio::sync::Mutex::new(Recovery::new(
            recovery_dir,
        ))));
        let new_app = crate::routes::create_router(restarted.clone());
        let (status, copies) = call(&new_app, "GET", "/api/workbook/recovery", Value::Null).await;
        assert_eq!(status, StatusCode::OK);
        let uri = format!(
            "/api/workbook/recovery/{}",
            copies[0]["id"].as_str().unwrap()
        );
        let restored = call(&new_app, "POST", &uri, Value::Null).await;
        assert_eq!(restored.0, StatusCode::OK, "{restored:?}");
        assert!(restarted.workbook.read().await.file_path.is_none());
        assert_eq!(restarted.workbook.read().await.staged.patches.len(), 1);
        let (status, range) = call(
            &new_app,
            "POST",
            "/api/ranges/read",
            json!({
                "sheet_name":"Sheet1","range":{"start_row":0,"start_col":0,"end_row":0,"end_col":0}
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(range, json!([["42"]]));
        assert_eq!(
            db_conn
                .query_row("SELECT amount FROM entries", [], |r| r.get::<_, i32>(0))
                .unwrap(),
            11
        );
    }

    #[tokio::test]
    async fn ledger_browsing_leaves_workbook_recovery_clean() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ledger.db");
        rusqlite::Connection::open(&db)
            .unwrap()
            .execute_batch(
                "CREATE TABLE entries (id INTEGER PRIMARY KEY, posted TEXT, amount INTEGER);
                 INSERT INTO entries VALUES (1, '2026-11-19', 11);",
            )
            .unwrap();
        let mut workbook = Workbook::new();
        workbook.add_sqlite_connection("Ledger", &db).unwrap();
        let mut state = AppState::new(workbook);
        let copies = dir.path().join("recovery");
        let recovery = Arc::new(tokio::sync::Mutex::new(Recovery::new(copies.clone())));
        state.recovery = Some(recovery.clone());
        let app = crate::routes::create_router(state.clone());
        let (status, result) = call(
            &app,
            "POST",
            "/api/ledger/page",
            json!({
                "spec":{"connection":"Ledger","table":"entries","date":"posted","cents":"amount"},
                "offset":0,"limit":1
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{result:?}");
        assert_eq!(result["total_cents"], "11");
        checkpoint(state.workbook.clone(), recovery).await.unwrap();
        assert!(!copies.exists());
    }

    #[tokio::test]
    async fn untitled_edits_autosave_and_checkpoint_before_new() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new(Workbook::new());
        let recovery = Arc::new(tokio::sync::Mutex::new(Recovery::new(
            dir.path().to_owned(),
        )));
        state.recovery = Some(recovery.clone());
        start_worker(&state);
        let app = crate::routes::create_router(state.clone());
        let payload =
            |value: &str| json!({"sheet_name":"Sheet1","row":0,"col":0,"values":[[value]]});
        assert_eq!(
            call(&app, "POST", "/api/ranges/edit", payload("43"))
                .await
                .0,
            StatusCode::OK
        );
        let active = {
            let r = recovery.lock().await;
            r.path(r.current)
        };
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !active.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            call(&app, "POST", "/api/ranges/edit", payload("44"))
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(
            call(&app, "POST", "/api/workbook/new", Value::Null).await.0,
            StatusCode::OK
        );
        let (status, copies) = call(&app, "GET", "/api/workbook/recovery", Value::Null).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(copies.as_array().unwrap().len(), 1);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert!(fs::read_to_string(active).unwrap().contains("44"));
    }

    #[test]
    fn failed_checkpoint_keeps_dirty_for_retry() {
        let dir = tempfile::tempdir().unwrap();
        let blocked = dir.path().join("blocked");
        fs::write(&blocked, "not a directory").unwrap();
        let mut recovery = Recovery::new(blocked.clone());
        recovery.dirty = true;
        let mut wb = Workbook::new();
        assert!(recovery.checkpoint(&mut wb).is_err());
        assert!(recovery.dirty);
        fs::remove_file(blocked).unwrap();
        recovery.checkpoint(&mut wb).unwrap();
        assert!(!recovery.dirty);
    }
}
