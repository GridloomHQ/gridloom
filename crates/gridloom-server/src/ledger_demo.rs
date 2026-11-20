//! On-demand synthetic demo, outside the workbook and separate from real data.
use crate::state::AppState;
use anyhow::{bail, Context};
use axum::{extract::State, http::StatusCode, Json};
use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::Instant,
};
use uuid::Uuid;

const ROWS: u32 = 10_000_000;
const BATCH: u32 = 100_000;
const FILE_NAME: &str = "ledger-10m-v1.sqlite";
const APPLICATION_ID: i32 = 0x474c444d; // GLDM
const SCHEMA_VERSION: i32 = 1;
const SCHEMA: &str = "CREATE TABLE entries (
    id INTEGER PRIMARY KEY,
    posted TEXT NOT NULL,
    account TEXT NOT NULL,
    amount_cents INTEGER NOT NULL
) STRICT;";
const INSERT: &str = "WITH RECURSIVE ids(id) AS (
    VALUES(?1) UNION ALL SELECT id + 1 FROM ids WHERE id < ?2
)
INSERT INTO entries
SELECT id, date('2026-01-01', printf('+%d days', (id - 1) / ?3)),
       printf('%d', 1100 + ((id - 1) % 10) * 100),
       CASE WHEN (id - 1) % 3 = 0 THEN -1 ELSE 1 END *
           (10000 + (id - 1) % 3701)
FROM ids;";

#[derive(Clone, Default, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Status {
    #[default]
    Idle,
    Generating {
        rows_written: u32,
        target_rows: u32,
        phase: &'static str,
    },
    Cancelling,
    Ready {
        #[serde(skip)]
        path: PathBuf,
        stats: Stats,
    },
    Cancelled,
    Failed {
        message: String,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct Stats {
    rows: u32,
    bytes: u64,
    generation_ms: Option<f64>,
    totals_ms: f64,
    total_cents: String,
    filtered_rows: u32,
    filtered_cents: String,
    filter_ms: f64,
    seek_ms: f64,
}

#[derive(Default)]
struct Job {
    status: Status,
    cancel: Option<Arc<AtomicBool>>,
}
#[derive(Default)]
pub struct Demo(Mutex<Job>);

type Failure = (StatusCode, String);
fn error(e: impl std::fmt::Display) -> Failure {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
impl Demo {
    fn lock(&self) -> Result<MutexGuard<'_, Job>, Failure> {
        self.0
            .lock()
            .map_err(|_| error("Sample job is unavailable"))
    }

    pub(crate) fn ready_path(&self) -> Result<PathBuf, Failure> {
        match &self.lock()?.status {
            Status::Ready { path, .. } => Ok(path.clone()),
            _ => Err((
                StatusCode::CONFLICT,
                "Generate the sample in Help → About → Sample first.".into(),
            )),
        }
    }
}

pub async fn status(State(state): State<AppState>) -> Result<Json<Status>, Failure> {
    Ok(Json(state.demo.lock()?.status.clone()))
}

pub async fn start(State(state): State<AppState>) -> Result<Json<Status>, Failure> {
    let path = crate::recovery::default_dir()
        .map_err(error)?
        .join("samples")
        .join(FILE_NAME);
    let mut job = state.demo.lock()?;
    if job.cancel.is_some() || matches!(&job.status, Status::Ready { path, .. } if path.is_file()) {
        return Ok(Json(job.status.clone()));
    }
    let cancel = Arc::new(AtomicBool::new(false));
    job.cancel = Some(cancel.clone());
    job.status = Status::Generating {
        rows_written: 0,
        target_rows: ROWS,
        phase: "Preparing",
    };
    let status = job.status.clone();
    drop(job);

    let demo = state.demo.clone();
    tokio::task::spawn_blocking(move || {
        let ensure_not_cancelled = |rows, phase| {
            let mut job = demo
                .lock()
                .map_err(|(_, message)| anyhow::anyhow!(message))?;
            if cancel.load(Ordering::Relaxed) {
                bail!("Sample generation cancelled");
            }
            job.status = Status::Generating {
                rows_written: rows,
                target_rows: ROWS,
                phase,
            };
            Ok(())
        };
        let result = build_or_reuse(&path, ROWS, ensure_not_cancelled, || {
            cancel.load(Ordering::Relaxed)
        });
        if let Ok(mut job) = demo.lock() {
            job.status = match result {
                Ok(stats) => Status::Ready { path, stats },
                Err(_) if cancel.load(Ordering::Relaxed) => Status::Cancelled,
                Err(error) => Status::Failed {
                    message: format!("{error:#}"),
                },
            };
            job.cancel = None;
        }
    });
    Ok(Json(status))
}

pub async fn cancel(State(state): State<AppState>) -> Result<Json<Status>, Failure> {
    let mut job = state.demo.lock()?;
    if let Some(cancel) = &job.cancel {
        cancel.store(true, Ordering::Relaxed);
        job.status = Status::Cancelling;
    }
    Ok(Json(job.status.clone()))
}

// Only delete a private directory created by this invocation, never an existing sample.
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn build_or_reuse(
    output: &Path,
    rows: u32,
    progress: impl Fn(u32, &'static str) -> anyhow::Result<()>,
    cancelled: impl Fn() -> bool,
) -> anyhow::Result<Stats> {
    debug_assert!(rows > 0 && rows <= ROWS);
    if fs::symlink_metadata(output).is_ok() {
        progress(rows, "Measuring existing sample")?;
        return measure(output, rows, None)
            .context("Cannot reuse the existing sample file; it was left untouched");
    }

    let parent = output.parent().context("Invalid sample data directory")?;
    fs::create_dir_all(parent).context("Cannot create sample directory")?;
    let scratch = parent.join(format!(".gridloom-sample-{}", Uuid::new_v4()));
    fs::create_dir(&scratch).context("Cannot reserve temporary sample directory")?;
    let scratch = Scratch(scratch);
    let temp = scratch.0.join(FILE_NAME);
    let generation_started = Instant::now();
    let db = Connection::open(&temp)?;
    // Only the disposable database uses these settings. All row generation is
    // inside SQLite, with a bounded page cache. Index sorting can spill to disk.
    db.execute_batch(
        "PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF; PRAGMA cache_size=-8192;
         PRAGMA temp_store=FILE; BEGIN;",
    )?;
    db.pragma_update(None, "application_id", APPLICATION_ID)?;
    db.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    db.execute_batch(SCHEMA)?;
    for start in (1..=rows).step_by(BATCH as usize) {
        progress(start - 1, "Writing entries")?;
        db.execute(INSERT, params![start, (start + BATCH - 1).min(rows), BATCH])
            .context("Unable to write sample rows; check available disk space")?;
    }
    progress(rows, "Indexing posting dates")?;
    db.execute_batch("CREATE INDEX entries_posted ON entries(posted);")?;
    progress(rows, "Indexing account codes")?;
    db.execute_batch("CREATE INDEX entries_account_posted ON entries(account, posted); COMMIT;")?;
    db.close().map_err(|(_, error)| error)?;
    OpenOptions::new().write(true).open(&temp)?.sync_all()?;
    let generation_ms = elapsed_ms(generation_started);
    progress(rows, "Measuring sample")?;
    let stats = measure(&temp, rows, Some(generation_ms))?;
    if cancelled() {
        bail!("Sample generation cancelled");
    }

    // The complete file becomes visible atomically. Unlike rename, hard_link
    // will not overwrite a destination published by another running instance.
    match fs::hard_link(&temp, output) {
        Ok(()) => Ok(stats),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            measure(output, rows, None).context("An existing sample file was left untouched")
        }
        Err(error) => Err(error).context("Could not publish the completed sample"),
    }
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn measure(path: &Path, expected_rows: u32, generation_ms: Option<f64>) -> anyhow::Result<Stats> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() {
        bail!("Sample must be a regular SQLite file");
    }
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    db.execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF; PRAGMA cache_size=-8192;")?;
    let id: i32 = db.pragma_query_value(None, "application_id", |r| r.get(0))?;
    let version: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if id != APPLICATION_ID || version != SCHEMA_VERSION {
        bail!("File is not this version of the Gridloom sample");
    }
    let tick = Instant::now();
    let (rows, total): (u32, i64) =
        db.query_row("SELECT COUNT(*), SUM(amount_cents) FROM entries", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    let totals_ms = elapsed_ms(tick);
    if rows != expected_rows {
        bail!("Expected {expected_rows} rows, found {rows}");
    }
    let tick = Instant::now();
    let (filtered_rows, filtered): (u32, i64) = db.query_row(
        "SELECT COUNT(*), SUM(amount_cents)
         FROM entries WHERE account = '1100' AND posted BETWEEN '2026-01-01' AND '2026-01-07'",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let filter_ms = elapsed_ms(tick);
    let tick = Instant::now();
    let last: i64 = db.query_row(
        "SELECT id FROM entries ORDER BY posted DESC, id DESC LIMIT 1",
        [],
        |r| r.get(0),
    )?;
    let seek_ms = elapsed_ms(tick);
    if last != i64::from(rows) {
        bail!("Sample's last entry does not match the expected row count");
    }
    Ok(Stats {
        rows,
        bytes: meta.len(),
        generation_ms,
        totals_ms,
        total_cents: total.to_string(),
        filtered_rows,
        filtered_cents: filtered.to_string(),
        filter_ms,
        seek_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_amounts_and_existing_sample_reuse() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let stats = build_or_reuse(&path, 10, |_, _| Ok(()), || false).unwrap();
        let cents: i64 = (0..10)
            .map(|i| {
                if i % 3 == 0 {
                    -(10_000 + i)
                } else {
                    10_000 + i
                }
            })
            .sum();
        assert_eq!(stats.rows, 10);
        assert_eq!(stats.total_cents, cents.to_string());
        assert_eq!(stats.filtered_rows, 1);
        assert_eq!(stats.filtered_cents, "-10000");
        assert!(stats.generation_ms.is_some());
        let reused = build_or_reuse(&path, 10, |_, _| Ok(()), || false).unwrap();
        assert_eq!(reused.total_cents, stats.total_cents);
        assert!(reused.generation_ms.is_none());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn never_overwrites_and_removes_cancelled_work() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        fs::write(&path, b"unrelated file").unwrap();
        assert!(build_or_reuse(&path, 10, |_, _| Ok(()), || false).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"unrelated file");
        fs::remove_file(&path).unwrap();
        assert!(build_or_reuse(&path, 10, |_, _| Ok(()), || true).is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn demo_page_does_not_connect_the_workbook() {
        use axum::{
            body::{to_bytes, Body},
            http::Request,
        };
        use serde_json::{json, Value};
        use tower::ServiceExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let state = AppState::new(gridloom_core::Workbook::new());
        let router = crate::routes::create_router(state.clone());
        let page_request = |url| {
            Request::builder()
                .method("POST")
                .uri(url)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "spec": {
                            "connection": "ignored", "table": "ignored", "date": "ignored",
                            "cents": "ignored", "from": "2026-01-01", "through": "2026-01-07",
                            "account": {"column": "ignored", "value": "1100"}
                        },
                        "offset": 1,
                        "limit": 2
                    })
                    .to_string(),
                ))
                .unwrap()
        };
        let r = router
            .clone()
            .oneshot(page_request("/api/sample/ledger"))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::CONFLICT);
        let stats = build_or_reuse(&path, 100, |_, _| Ok(()), || false).unwrap();
        state.demo.lock().unwrap().status = Status::Ready { path, stats };
        let r = router
            .clone()
            .oneshot(page_request("/api/sample/ledger"))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        let page: Value =
            serde_json::from_slice(&to_bytes(r.into_body(), 4096).await.unwrap()).unwrap();
        assert_eq!(page["count"], 10);
        assert_eq!(page["rows"][0][0], "11");
        assert_eq!(page["rows"][1][0], "21");
        assert!(state.workbook.read().await.connection_configs.is_empty());
        let r = router
            .oneshot(page_request("/api/ledger/page"))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    #[ignore = "generates and measures the full ten-million-row sample (~700 MB)"]
    fn full_demo() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let stats = build_or_reuse(&path, ROWS, |_, _| Ok(()), || false).unwrap();
        eprintln!("{stats:?}");
        assert_eq!(stats.rows, ROWS);
        assert_eq!(stats.filtered_rows, 70_000);
    }
}
