//! Expand the server-owned fixture in place; never accept a database or table from a request.
use crate::{
    sample_data::{INVOICE_SEEDS, SALES_FIXTURE_ID},
    state::AppState,
};
use anyhow::{bail, Context};
use axum::{extract::State, http::StatusCode, Json};
use rusqlite::{params, Connection, OpenFlags, TransactionBehavior};
use serde::Serialize;
use std::{
    fs,
    path::Path,
    sync::{Mutex, MutexGuard},
    time::Duration,
};

const TARGET: u32 = 10_000_000;
const BATCH: u32 = 100_000;
const INSERT: &str = "WITH RECURSIVE ids(id) AS (
    VALUES(?1) UNION ALL SELECT id + 1 FROM ids WHERE id < ?2
)
INSERT INTO invoices(id,customer_name,issued_on,amount,status)
SELECT id, printf('Sample customer %05d', id % 10000),
       date('2026-01-01', printf('+%d days', id % 365)),
       (10000 + id % 990001) / 100.0,
       CASE id % 3 WHEN 0 THEN 'Paid' WHEN 1 THEN 'Approved' ELSE 'Pending' END
FROM ids";

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
        rows: u32,
        added: u32,
    },
    Cancelled,
    Failed {
        message: String,
    },
}

#[derive(Default)]
pub struct Job(Mutex<Status>);
type Failure = (StatusCode, String);

impl Job {
    fn lock(&self) -> Result<MutexGuard<'_, Status>, Failure> {
        self.0.lock().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Invoice sample job unavailable".into(),
            )
        })
    }

    fn progress(
        &self,
        rows_written: u32,
        target_rows: u32,
        phase: &'static str,
    ) -> anyhow::Result<()> {
        let mut status = self
            .lock()
            .map_err(|(_, message)| anyhow::anyhow!(message))?;
        if matches!(*status, Status::Cancelling) {
            bail!("Invoice sample generation cancelled");
        }
        *status = Status::Generating {
            rows_written,
            target_rows,
            phase,
        };
        Ok(())
    }
}

fn require_fixture(state: &AppState) -> Result<&Path, Failure> {
    state.invoice_sample_path.as_deref().ok_or((
        StatusCode::NOT_FOUND,
        "This server was started without the built-in invoice sample.".into(),
    ))
}

pub async fn status(State(state): State<AppState>) -> Result<Json<Status>, Failure> {
    require_fixture(&state)?;
    Ok(Json(state.invoice_sample_job.lock()?.clone()))
}

pub async fn start(State(state): State<AppState>) -> Result<Json<Status>, Failure> {
    let path = require_fixture(&state)?.to_path_buf();
    let job = state.invoice_sample_job.clone();
    let mut status = job.lock()?;
    if matches!(*status, Status::Generating { .. } | Status::Cancelling) {
        return Ok(Json(status.clone()));
    }
    *status = Status::Generating {
        rows_written: 0,
        target_rows: TARGET,
        phase: "Checking existing invoices",
    };
    let response = status.clone();
    drop(status);

    tokio::task::spawn_blocking(move || {
        if let Err(error) = populate(&path, TARGET, &job, |_| {}) {
            if let Ok(mut status) = job.lock() {
                *status = if matches!(*status, Status::Cancelling) {
                    Status::Cancelled
                } else {
                    Status::Failed {
                        message: format!("{error:#}"),
                    }
                };
            }
        }
    });
    Ok(Json(response))
}

pub async fn cancel(State(state): State<AppState>) -> Result<Json<Status>, Failure> {
    require_fixture(&state)?;
    let mut status = state.invoice_sample_job.lock()?;
    if matches!(*status, Status::Generating { .. }) {
        *status = Status::Cancelling;
    }
    Ok(Json(status.clone()))
}

fn verify_file_path(path: &Path) -> anyhow::Result<()> {
    let parent = path.parent().context("Invalid invoice fixture location")?;
    let file = fs::symlink_metadata(path).context("The built-in invoice fixture is missing")?;
    if path
        .file_name()
        .is_none_or(|name| name != "sample_sales.sqlite")
        || !fs::symlink_metadata(parent)?.is_dir()
        || !file.is_file()
    {
        bail!("The built-in invoice fixture must be an ordinary local SQLite file.");
    }
    #[cfg(unix)]
    if std::os::unix::fs::MetadataExt::nlink(&file) != 1 {
        bail!("The built-in invoice fixture must not be hard-linked.");
    }
    Ok(())
}

fn verify_schema(db: &Connection) -> anyhow::Result<()> {
    let ordinary: i64 = db.query_row("SELECT COUNT(*) FROM pragma_table_list WHERE schema='main' AND name='invoices' AND type='table' AND ncol=5 AND wr=0", [], |r| r.get(0))?;
    let triggers: i64 = db.query_row(
        "SELECT COUNT(*) FROM sqlite_schema WHERE type='trigger' AND tbl_name='invoices'",
        [],
        |r| r.get(0),
    )?;
    let mut columns = db.prepare("SELECT name, upper(type), \"notnull\", pk, hidden FROM pragma_table_xinfo('invoices') ORDER BY cid")?;
    let actual = columns
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, u8>(2)?,
                r.get::<_, u8>(3)?,
                r.get::<_, u8>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let expected = [
        ("id", "INTEGER", 0, 1, 0),
        ("customer_name", "TEXT", 1, 0, 0),
        ("issued_on", "TEXT", 1, 0, 0),
        ("amount", "REAL", 1, 0, 0),
        ("status", "TEXT", 1, 0, 0),
    ];
    if ordinary != 1
        || triggers != 0
        || !actual
            .iter()
            .map(|(a, b, c, d, e)| (a.as_str(), b.as_str(), *c, *d, *e))
            .eq(expected)
    {
        bail!("The invoice database is not the supported built-in fixture schema.");
    }
    Ok(())
}

fn verify_legacy(db: &Connection, rows: u32) -> anyhow::Result<()> {
    if rows != INVOICE_SEEDS.len() as u32 {
        bail!("An unmarked invoice file does not match the original fixture; it was not changed.");
    }
    let objects: i64 = db.query_row(
        "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
        [],
        |r| r.get(0),
    )?;
    if objects != 1 {
        bail!("An unmarked invoice file has extra database objects; it was not changed.");
    }
    for (id, name, date, amount, status) in INVOICE_SEEDS {
        let found: (String, String, f64, String) = db
            .query_row(
                "SELECT customer_name, issued_on, amount, status FROM invoices WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .context("An unmarked invoice file is missing a required starter entry")?;
        if (
            found.0.as_str(),
            found.1.as_str(),
            found.2,
            found.3.as_str(),
        ) != (name, date, amount, status)
        {
            bail!(
                "An unmarked invoice file does not match the original fixture; it was not changed."
            );
        }
    }
    Ok(())
}

fn populate(path: &Path, target: u32, job: &Job, after_batch: impl Fn(u32)) -> anyhow::Result<()> {
    verify_file_path(path)?;
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW;
    let mut db = Connection::open_with_flags(path, flags)?;
    db.busy_timeout(Duration::from_secs(2))?;
    db.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA cache_size=-8192; PRAGMA temp_store=FILE; PRAGMA synchronous=FULL;")?;
    job.progress(0, target, "Checking existing invoices")?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    verify_schema(&tx)?;
    let (count, last): (u32, i64) = tx.query_row(
        "SELECT count(*), coalesce(max(id),0) FROM invoices",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let id: i32 = tx.pragma_query_value(None, "application_id", |r| r.get(0))?;
    if id == 0 {
        verify_legacy(&tx, count)?;
        tx.pragma_update(None, "application_id", SALES_FIXTURE_ID)?;
    } else if id != SALES_FIXTURE_ID {
        bail!("This file is not the built-in invoice fixture.");
    }
    if count > target {
        bail!("There are already more than {target} invoices; no rows were removed.");
    }
    let added = target - count;
    let last = last.max(10);
    last.checked_add(i64::from(added))
        .context("Invoice ID range exhausted")?;
    let mut written = 0;
    while written < added {
        job.progress(count + written, target, "Writing invoices")?;
        let batch = (added - written).min(BATCH);
        tx.execute(
            INSERT,
            params![
                last + i64::from(written) + 1,
                last + i64::from(written + batch)
            ],
        )?;
        written += batch;
        after_batch(count + written);
    }

    // Cancellation and commit linearize under this short lock. A cancellation accepted before
    // commit rolls the transaction back; a request after commit reports Ready, never Cancelled.
    let mut status = job
        .lock()
        .map_err(|(_, message)| anyhow::anyhow!(message))?;
    if matches!(*status, Status::Cancelling) {
        bail!("Invoice sample generation cancelled");
    }
    tx.commit()?;
    *status = Status::Ready {
        rows: target,
        added,
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample_data::create_sample_sqlite;

    fn summary(path: &Path) -> (u32, i64, String) {
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
            .query_row("SELECT count(*),max(id),(SELECT customer_name FROM invoices WHERE id=1) FROM invoices",
                [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap()
    }

    #[test]
    fn preserves_existing_rows_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample_sales.sqlite");
        create_sample_sqlite(&path).unwrap();
        let db = Connection::open(&path).unwrap();
        db.execute(
            "UPDATE invoices SET customer_name='Edited starter' WHERE id=1",
            [],
        )
        .unwrap();
        drop(db);
        let job = Job::default();
        populate(&path, 243, &job, |_| {}).unwrap();
        assert_eq!(summary(&path), (243, 243, "Edited starter".into()));
        assert!(matches!(
            *job.lock().unwrap(),
            Status::Ready {
                rows: 243,
                added: 233
            }
        ));
        populate(&path, 243, &job, |_| {}).unwrap();
        assert!(matches!(
            *job.lock().unwrap(),
            Status::Ready {
                rows: 243,
                added: 0
            }
        ));
    }

    #[test]
    fn cancellation_rolls_back_even_after_the_last_batch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample_sales.sqlite");
        create_sample_sqlite(&path).unwrap();
        let job = Job::default();
        assert!(populate(&path, 243, &job, |_| {
            *job.lock().unwrap() = Status::Cancelling;
        })
        .is_err());
        assert_eq!(summary(&path), (10, 10, "Acme Logistics".into()));
    }

    #[test]
    fn adopts_exact_legacy_fixture_but_refuses_an_unrelated_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample_sales.sqlite");
        create_sample_sqlite(&path).unwrap();
        let db = Connection::open(&path).unwrap();
        db.pragma_update(None, "application_id", 0).unwrap();
        db.execute(
            "UPDATE invoices SET customer_name='Someone else' WHERE id=1",
            [],
        )
        .unwrap();
        assert!(populate(&path, 30, &Job::default(), |_| {}).is_err());
        assert_eq!(summary(&path), (10, 10, "Someone else".into()));
        db.execute(
            "UPDATE invoices SET customer_name='Acme Logistics' WHERE id=1",
            [],
        )
        .unwrap();
        populate(&path, 30, &Job::default(), |_| {}).unwrap();
        assert_eq!(summary(&path), (30, 30, "Acme Logistics".into()));
    }

    #[tokio::test]
    async fn invoice_routes_require_enabled_fixture() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;
        let app = crate::routes::create_router(AppState::new(gridloom_core::Workbook::new()));
        for method in ["GET", "POST", "DELETE"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri("/api/sample/invoices")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[test]
    #[ignore = "generates ten million invoices on temporary disk"]
    fn full_invoice_sample() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample_sales.sqlite");
        create_sample_sqlite(&path).unwrap();
        let start = std::time::Instant::now();
        let job = Job::default();
        populate(&path, TARGET, &job, |_| {}).unwrap();
        println!(
            "full invoice sample in {:?}, {} bytes",
            start.elapsed(),
            fs::metadata(&path).unwrap().len()
        );
        assert_eq!(
            summary(&path),
            (TARGET, i64::from(TARGET), "Acme Logistics".into())
        );
        populate(&path, TARGET, &job, |_| {}).unwrap();
        assert!(matches!(
            *job.lock().unwrap(),
            Status::Ready {
                rows: TARGET,
                added: 0
            }
        ));
    }
}
