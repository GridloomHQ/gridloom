//! Read-only browsing of file-backed SQLite ledgers without materializing the result.
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use gridloom_core::{
    db::EngineKind,
    virtual_view::ViewportRequest,
    workbook::{LedgerLink, LedgerSpec},
    CellCoord, Value, Workbook,
};
use rusqlite::types::Value as SqlValue;
use rusqlite::{types::ValueRef, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct Request {
    spec: LedgerSpec,
    session: Option<Uuid>,
    offset: u64,
    limit: u32,
}
#[derive(Serialize)]
pub struct Page {
    session: Uuid,
    rows: Vec<Vec<Option<String>>>,
    columns: Vec<String>,
    count: u64,
    total_cents: String,
}
struct Ledger {
    spec: LedgerSpec,
    path: PathBuf,
    identity: std::fs::Metadata,
    db: Connection,
    version: i64,
    count: u64,
    total: i64,
    ordering: String,
    key: String,
    last: (u64, Vec<(String, i64)>),
    touched: Instant,
}
#[derive(Default)]
pub struct Ledgers(Mutex<HashMap<Uuid, Ledger>>);
type Failure = (StatusCode, String);
fn bad(error: impl std::fmt::Display) -> Failure {
    (StatusCode::BAD_REQUEST, error.to_string())
}
fn stale() -> Failure {
    (
        StatusCode::CONFLICT,
        "Ledger changed or session expired. Reload to start from a fresh view.".into(),
    )
}
fn ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}
fn data_version(c: &Connection) -> rusqlite::Result<i64> {
    c.query_row("PRAGMA main.data_version", [], |r| r.get(0))
}
fn same_file(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if a.dev() != b.dev() || a.ino() != b.ino() {
            return false;
        }
    }
    a.len() == b.len() && a.modified().ok() == b.modified().ok()
}
fn account_clause(s: &LedgerSpec, parameter: usize) -> String {
    s.account
        .as_ref()
        .map(|a| format!(" AND {} COLLATE BINARY = ?{parameter}", ident(&a.column)))
        .unwrap_or_default()
}
fn predicate(s: &LedgerSpec, account_parameter: usize) -> String {
    let d = ident(&s.date);
    let mut filters = vec![];
    if s.from.is_some() {
        filters.push(format!("{d} COLLATE BINARY >= ?1"));
    }
    if s.through.is_some() {
        filters.push(format!("{d} COLLATE BINARY <= ?2"));
    }
    let date_filters: String = if filters.is_empty() {
        "?1 IS NULL AND ?2 IS NULL".into()
    } else if s.through.is_none() {
        format!("{} AND ?2 IS NULL", filters[0])
    } else {
        if s.from.is_none() {
            filters.push("?1 IS NULL".into());
        }
        filters.join(" AND ")
    };
    date_filters + &account_clause(s, account_parameter)
}
fn filter_values(s: &LedgerSpec) -> Vec<SqlValue> {
    vec![s.from.clone().into(), s.through.clone().into()]
}
impl Ledger {
    fn open(spec: LedgerSpec, path: PathBuf) -> Result<Self, Failure> {
        let identity = std::fs::metadata(&path).map_err(bad)?;
        if !identity.is_file() {
            return Err(bad("Ledger source must be a SQLite file"));
        }
        let db =
            Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(bad)?;
        db.busy_timeout(Duration::from_secs(2)).map_err(bad)?;
        db.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA query_only=ON; PRAGMA cache_size=-2048; PRAGMA temp_store=FILE;").map_err(bad)?;
        let version = data_version(&db).map_err(bad)?;
        let kind: String = db
            .query_row(
                "SELECT type FROM pragma_table_list WHERE schema='main' AND name=?1",
                [&spec.table],
                |r| r.get(0),
            )
            .map_err(bad)?;
        if kind != "table" {
            return Err(bad("Select a physical SQLite table"));
        }
        let mut meta = db
            .prepare("SELECT name,type,pk FROM pragma_table_info(?1,'main')")
            .map_err(bad)?;
        let fields = meta
            .query_map([&spec.table], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i32>(2)?,
                ))
            })
            .map_err(bad)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(bad)?;
        drop(meta);
        let keys: Vec<_> = fields.iter().filter(|f| f.2 != 0).collect();
        if keys.len() != 1 || !keys[0].1.eq_ignore_ascii_case("INTEGER") {
            return Err(bad(
                "Ledger needs one INTEGER PRIMARY KEY for stable ordering",
            ));
        }
        if !fields.iter().any(|f| f.0 == spec.date) || !fields.iter().any(|f| f.0 == spec.cents) {
            return Err(bad("Select existing posting date and amount columns"));
        }
        if spec
            .account
            .as_ref()
            .is_some_and(|a| !fields.iter().any(|f| f.0 == a.column))
        {
            return Err(bad("Select an existing account column"));
        }
        let table = ident(&spec.table);
        let key = ident(&keys[0].0);
        let date = ident(&spec.date);
        let amount = ident(&spec.cents);
        let where_sql = predicate(&spec, 3);
        let mut filters = filter_values(&spec);
        if let Some(a) = &spec.account {
            filters.push(a.value.clone().into());
        }
        let sql=format!("SELECT count(*), coalesce(sum(CASE WHEN typeof({amount})='integer' THEN {amount} END),0), count(CASE WHEN typeof({amount})!='integer' OR typeof({date})!='text' OR {date} NOT GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]' OR typeof({key})!='integer' THEN 1 END) FROM main.{table} WHERE {where_sql}");
        let (count, total, invalid): (i64, i64, i64) = db
            .query_row(&sql, rusqlite::params_from_iter(filters), |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(bad)?;
        if invalid != 0 {
            return Err(bad(
                "Matching rows must have text dates, integer cents and integer keys",
            ));
        }
        let ordering = format!("{date} COLLATE BINARY, {key}");
        let ledger = Self {
            spec,
            path,
            identity,
            db,
            version,
            count: count as u64,
            total,
            ordering,
            key: keys[0].0.clone(),
            last: (0, vec![]),
            touched: Instant::now(),
        };
        ledger.check()?;
        Ok(ledger)
    }
    fn check(&self) -> Result<(), Failure> {
        if data_version(&self.db).map_err(bad)? != self.version
            || !std::fs::metadata(&self.path).is_ok_and(|now| same_file(&self.identity, &now))
        {
            return Err(stale());
        }
        Ok(())
    }
    fn page(&mut self, id: Uuid, offset: u64, limit: u32) -> Result<Page, Failure> {
        self.check()?;
        let table = ident(&self.spec.table);
        let where_sql = predicate(&self.spec, 5);
        let anchor = offset
            .checked_sub(self.last.0 + 1)
            .and_then(|i| self.last.1.get(i as usize));
        let sql = if anchor.is_some() {
            let date = ident(&self.spec.date);
            let key = ident(&self.key);
            let order = &self.ordering;
            // The verified anchor already satisfies the lower bound. Presenting two
            // competing lower bounds may make SQLite choose the weaker index seek.
            let upper = if self.spec.through.is_some() {
                format!("AND {date} COLLATE BINARY <= ?2")
            } else {
                "AND ?2 IS NULL".to_string()
            };
            let account = account_clause(&self.spec, 7);
            // Separate indexed ranges let SQLite seek on both keys inside a large tie group.
            format!("SELECT * FROM (
                SELECT * FROM (SELECT * FROM main.{table} WHERE ?1 IS ?1 AND ?4=0 AND {date} COLLATE BINARY = ?5 AND {key} > ?6 {account} ORDER BY {order} LIMIT ?3)
                UNION ALL
                SELECT * FROM (SELECT * FROM main.{table} WHERE {date} COLLATE BINARY > ?5 {upper} {account} ORDER BY {order} LIMIT ?3)
            ) ORDER BY {order} LIMIT ?3")
        } else {
            format!(
                "SELECT * FROM main.{table} WHERE {where_sql} ORDER BY {} LIMIT ?3 OFFSET ?4",
                self.ordering
            )
        };
        let mut stmt = self.db.prepare(&sql).map_err(bad)?;
        let columns = stmt
            .column_names()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        if columns.len() > 64 {
            return Err(bad("Ledger preview supports at most 64 columns"));
        }
        let mut q: Vec<rusqlite::types::Value> = vec![
            self.spec.from.clone().into(),
            self.spec.through.clone().into(),
            i64::from(limit).into(),
            (if anchor.is_some() { 0 } else { offset as i64 }).into(),
        ];
        if let Some((d, k)) = anchor {
            q.push(d.clone().into());
            q.push((*k).into());
        }
        if let Some(a) = &self.spec.account {
            q.push(a.value.clone().into());
        }
        let vals = stmt
            .query_map(rusqlite::params_from_iter(q), |row| {
                let cells = (0..columns.len())
                    .map(|i| {
                        Ok(match row.get_ref(i)? {
                            ValueRef::Null => None,
                            ValueRef::Integer(v) => Some(v.to_string()),
                            ValueRef::Real(v) => Some(v.to_string()),
                            ValueRef::Text(v) => {
                                Some(String::from_utf8_lossy(&v[..v.len().min(4096)]).into_owned())
                            }
                            ValueRef::Blob(v) => Some(format!("[{} bytes]", v.len())),
                        })
                    })
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok((
                    cells,
                    (
                        row.get::<_, String>(self.spec.date.as_str())?,
                        row.get::<_, i64>(self.key.as_str())?,
                    ),
                ))
            })
            .map_err(bad)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(bad)?;
        drop(stmt);
        self.check()?;
        let (vals, anchors) = vals.into_iter().unzip();
        self.last = (offset, anchors);
        self.touched = Instant::now();
        Ok(Page {
            session: id,
            rows: vals,
            columns,
            count: self.count,
            total_cents: self.total.to_string(),
        })
    }
}

pub async fn browse(
    State(state): State<AppState>,
    Json(input): Json<Request>,
) -> Result<Json<Page>, Failure> {
    if !(1..=200).contains(&input.limit) || input.offset > i64::MAX as u64 {
        return Err(bad("Invalid page size or offset"));
    }
    if input
        .spec
        .account
        .as_ref()
        .is_some_and(|a| a.column.is_empty() || a.value.is_empty() || a.value.len() > 1024)
    {
        return Err(bad(
            "Use an account code of 1 to 1024 bytes and an account column",
        ));
    }
    for date in [&input.spec.from, &input.spec.through]
        .into_iter()
        .flatten()
    {
        let parsed = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(bad)?;
        if parsed.format("%Y-%m-%d").to_string() != *date {
            return Err(bad("Expected YYYY-MM-DD"));
        }
    }
    if matches!((&input.spec.from,&input.spec.through),(Some(a),Some(b)) if a>b) {
        return Err(bad("Start date must not follow end date"));
    }
    let path = {
        let wb = state.workbook.read().await;
        let config = wb
            .connection_configs
            .get(&input.spec.connection)
            .ok_or_else(|| bad("Unknown connection"))?;
        if !matches!(config.kind, EngineKind::Sqlite) || config.path == ":memory:" {
            return Err(bad("Use a file-backed SQLite connection"));
        }
        PathBuf::from(&config.path)
    };
    tokio::task::spawn_blocking(move || {
        // Bound concurrent work and retained SQLite page caches. Requests are local.
        let mut sessions = state.ledgers.0.lock().map_err(bad)?;
        sessions.retain(|_, s| s.touched.elapsed() < Duration::from_secs(600));
        if let Some(id) = input.session {
            let ledger = sessions
                .get_mut(&id)
                .filter(|s| s.spec == input.spec && s.path == path)
                .ok_or_else(stale)?;
            return ledger.page(id, input.offset, input.limit).map(Json);
        }
        let mut ledger = Ledger::open(input.spec, path)?;
        let id = Uuid::new_v4();
        let page = ledger.page(id, input.offset, input.limit)?;
        if sessions.len() >= 4 {
            if let Some(id) = sessions
                .iter()
                .min_by_key(|(_, s)| s.touched)
                .map(|(id, _)| *id)
            {
                sessions.remove(&id);
            }
        }
        sessions.insert(id, ledger);
        Ok(Json(page))
    })
    .await
    .map_err(bad)?
}

#[derive(Deserialize)]
pub struct Target {
    sheet_name: String,
    row: u32,
    col: u32,
}
#[derive(Deserialize)]
pub struct UpdateLink {
    target: Target,
    session: Option<Uuid>,
}
#[derive(Serialize)]
pub struct Trace {
    link: Option<LedgerSpec>,
    source_changed: bool,
    balance_cents: Option<String>,
}
fn exact_cents(value: Value) -> Option<i128> {
    match value {
        Value::Integer(n) => Some(i128::from(n) * 100),
        Value::Decimal(n) => {
            let divisor = 10_i128.pow(n.scale());
            let cents = n.mantissa() * 100;
            (cents % divisor == 0).then_some(cents / divisor)
        }
        _ => None,
    }
}
fn trace(wb: &Workbook, target: &Target) -> Result<Trace, Failure> {
    let sheet = wb
        .sheets
        .get(&target.sheet_name)
        .ok_or_else(|| bad("Unknown sheet"))?;
    if target.row >= sheet.max_rows || target.col >= sheet.max_cols {
        return Err(bad("Cell is outside the worksheet"));
    }
    let a1 = CellCoord::new(target.row, target.col).to_a1();
    let link = sheet.ledger_links.get(&a1);
    let source_changed = link.is_some_and(|l| {
        !wb.connection_configs
            .get(&l.spec.connection)
            .is_some_and(|cfg| matches!(cfg.kind, EngineKind::Sqlite) && cfg.path == l.source_path)
    });
    let balance_cents = if wb.stale_calculation_sheets.contains(&target.sheet_name) {
        None
    } else {
        let view = wb
            .get_viewport(&ViewportRequest {
                sheet_name: target.sheet_name.clone(),
                start_row: target.row,
                end_row: target.row,
                start_col: target.col,
                end_col: target.col,
                overscan: 0,
            })
            .map_err(bad)?;
        view.cells
            .into_iter()
            .find_map(|(key, cell)| {
                (key == a1 && !cell.is_staged)
                    .then(|| exact_cents(cell.raw_value))
                    .flatten()
            })
            .map(|v| v.to_string())
    };
    Ok(Trace {
        link: link.map(|l| l.spec.clone()),
        source_changed,
        balance_cents,
    })
}

pub async fn get_trace(
    State(state): State<AppState>,
    Query(target): Query<Target>,
) -> Result<Json<Trace>, Failure> {
    let wb = state.workbook.read().await;
    trace(&wb, &target).map(Json)
}

pub async fn update_trace(
    State(state): State<AppState>,
    Json(input): Json<UpdateLink>,
) -> Result<Json<Trace>, Failure> {
    let link = if let Some(id) = input.session {
        let ledgers = state.ledgers.clone();
        let link = tokio::task::spawn_blocking(move || {
            let sessions = ledgers.0.lock().map_err(bad)?;
            let ledger = sessions
                .get(&id)
                .filter(|l| l.touched.elapsed() < Duration::from_secs(600))
                .ok_or_else(stale)?;
            ledger.check()?;
            if ledger.spec.account.is_none()
                || ledger.spec.from.is_none()
                || ledger.spec.through.is_none()
            {
                return Err(bad(
                    "Choose an account and both dates to save a ledger link",
                ));
            }
            Ok(LedgerLink {
                spec: ledger.spec.clone(),
                source_path: ledger.path.to_string_lossy().into_owned(),
            })
        })
        .await
        .map_err(bad)??;
        Some(link)
    } else {
        None
    };
    let mut wb = state.workbook.write().await;
    let before = trace(&wb, &input.target)?;
    if let Some(l) = &link {
        if before.balance_cents.is_none() {
            return Err(bad(
                "Select a committed exact numeric cell in currency units with no fractional cents",
            ));
        }
        if !wb
            .connection_configs
            .get(&l.spec.connection)
            .is_some_and(|c| matches!(c.kind, EngineKind::Sqlite) && c.path == l.source_path)
        {
            return Err(stale());
        }
    }
    let sheet = wb
        .sheets
        .get_mut(&input.target.sheet_name)
        .ok_or_else(|| bad("Unknown sheet"))?;
    let key = CellCoord::new(input.target.row, input.target.col).to_a1();
    if let Some(l) = link {
        sheet.ledger_links.insert(key, l);
    } else {
        sheet.ledger_links.remove(&key);
    }
    trace(&wb, &input.target).map(Json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gridloom_core::db::sqlite::SqliteEngine;

    fn fixture(path: &std::path::Path, n: i64) -> Connection {
        let db = Connection::open(path).unwrap();
        db.execute_batch("PRAGMA journal_mode=WAL;
            CREATE TABLE entries(id INTEGER PRIMARY KEY, posted TEXT NOT NULL, cents INTEGER NOT NULL);
            CREATE INDEX entries_order ON entries(posted COLLATE BINARY,id);").unwrap();
        db.execute("WITH RECURSIVE seq(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM seq WHERE i<?1)
            INSERT INTO entries SELECT i,printf('2026-11-%02d',(i-1)/100000+1), CASE WHEN i%2=0 THEN 103 ELSE -100 END FROM seq",[n]).unwrap();
        db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .unwrap();
        db
    }
    fn spec() -> LedgerSpec {
        LedgerSpec {
            connection: "fixture".into(),
            table: "entries".into(),
            date: "posted".into(),
            cents: "cents".into(),
            from: None,
            through: None,
            account: None,
        }
    }
    #[test]
    fn paging_ties_exact_totals_and_revision_guards() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("fixture.sqlite");
        let writer = fixture(&path, 1000);
        let mut l = Ledger::open(spec(), path.clone()).unwrap();
        let id = Uuid::new_v4();
        let mut ids = vec![];
        for off in (0..1000).step_by(150) {
            let p = l.page(id, off, 150).unwrap();
            assert_eq!(p.count, 1000);
            assert_eq!(p.total_cents, "1500");
            ids.extend(
                p.rows
                    .into_iter()
                    .map(|r| r[0].as_ref().unwrap().parse::<i64>().unwrap()),
            );
        }
        assert!(ids.into_iter().eq(1..=1000));
        writer
            .execute("UPDATE entries SET cents=999 WHERE id=1", [])
            .unwrap();
        assert_eq!(l.page(id, 0, 150).err().unwrap().0, StatusCode::CONFLICT);
        let mut s = spec();
        s.from = Some("2026-11-02".into());
        let mut empty = Ledger::open(s, path).unwrap();
        let p = empty.page(id, 0, 150).unwrap();
        assert_eq!((p.count, p.total_cents, p.rows.len()), (0, "0".into(), 0));
    }
    #[test]
    #[ignore = "two-million-row performance fixture; set GRIDLOOM_LEDGER_BENCH_MODE to fixture, eager or paged"]
    fn ledger_benchmark() {
        let path = PathBuf::from(std::env::var("GRIDLOOM_LEDGER_BENCH_PATH").unwrap());
        let mode = std::env::var("GRIDLOOM_LEDGER_BENCH_MODE").unwrap();
        let start = Instant::now();
        match mode.as_str() {
            "fixture" => {
                fixture(&path, 2_000_000);
            }
            "eager" => {
                let db = SqliteEngine::open(path).unwrap();
                let eager = db
                    .execute_read_query(
                        "SELECT * FROM entries ORDER BY posted COLLATE BINARY,id",
                        &[],
                    )
                    .unwrap();
                assert_eq!(eager.rows.len(), 2_000_000);
                println!("eager load: {:?}", start.elapsed());
                std::hint::black_box(eager);
            }
            "paged" => {
                let mut l = Ledger::open(spec(), path.clone()).unwrap();
                let id = Uuid::new_v4();
                let p = l.page(id, 0, 150).unwrap();
                assert_eq!(p.count, 2_000_000);
                assert_eq!(p.total_cents, "3000000");
                println!("first page including totals: {:?}", start.elapsed());
                let deep = Instant::now();
                let p = l.page(id, 1_999_850, 150).unwrap();
                assert_eq!(p.rows.last().unwrap()[0].as_deref(), Some("2000000"));
                l.page(id, 1_994_900, 150).unwrap();
                println!("deep page: {:?}", deep.elapsed());
                let scroll = Instant::now();
                for off in (1_995_000..1_999_000).step_by(100) {
                    let p = l.page(id, off, 150).unwrap();
                    assert_eq!(
                        p.rows[0][0].as_ref().unwrap().parse::<u64>().unwrap(),
                        off + 1
                    );
                }
                println!("40 deep scroll pages: {:?}", scroll.elapsed());
                let traversal = Instant::now();
                for off in (0..2_000_000).step_by(150) {
                    let p = l.page(id, off, 150).unwrap();
                    assert_eq!(p.rows.len(), 150.min(2_000_000 - off as usize));
                    for (idx, r) in p.rows.iter().enumerate() {
                        assert_eq!(
                            r[0].as_ref().unwrap().parse::<u64>().unwrap(),
                            1 + off + idx as u64
                        );
                    }
                }
                println!("every unfiltered row verified: {:?}", traversal.elapsed());
                let mut filtered = spec();
                filtered.from = Some("2026-11-10".into());
                filtered.through = Some("2026-11-12".into());
                let mut f = Ledger::open(filtered, path).unwrap();
                assert_eq!((f.count, f.total), (300000, 450000));
                for off in (0..300_000).step_by(150) {
                    let p = f.page(id, off, 150).unwrap();
                    for (idx, r) in p.rows.iter().enumerate() {
                        assert_eq!(
                            r[0].as_ref().unwrap().parse::<u64>().unwrap(),
                            900_001 + off + idx as u64
                        );
                    }
                }
                println!("every filtered row verified");
            }
            _ => panic!("invalid benchmark mode"),
        }
    }
}
