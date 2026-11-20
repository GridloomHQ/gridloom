use rusqlite::Connection as RusqliteConn;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const INVOICE_SEEDS: [(i64, &str, &str, f64, &str); 10] = [
    (1, "Acme Logistics", "2026-09-01", 14500.00, "Paid"),
    (2, "Stark Industries", "2026-09-05", 89400.50, "Approved"),
    (3, "Wayne Enterprises", "2026-09-12", 45200.00, "Pending"),
    (4, "Cyberdyne Systems", "2026-09-18", 12300.75, "Pending"),
    (5, "Umbrella Corp", "2026-09-22", 67800.00, "Paid"),
    (6, "Initech", "2026-09-28", 3200.00, "Paid"),
    (7, "Hooli Corp", "2026-10-02", 98500.20, "Approved"),
    (8, "Pied Piper", "2026-10-08", 24100.00, "Pending"),
    (9, "Massive Dynamic", "2026-10-14", 51200.00, "Paid"),
    (10, "Tyrell Corp", "2026-10-19", 112000.00, "Pending"),
];
pub(crate) const SALES_FIXTURE_ID: i32 = 0x474C494E; // GLIN

pub fn ensure_sample_databases() -> anyhow::Result<(PathBuf, PathBuf)> {
    let fixtures_dir = Path::new("fixtures");
    fs::create_dir_all(fixtures_dir)?;

    let sqlite_path = fixtures_dir.join("sample_sales.sqlite");
    match fs::symlink_metadata(&sqlite_path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            create_sample_sqlite(&sqlite_path)?
        }
        Err(error) => return Err(error.into()),
    }

    let duckdb_path = fixtures_dir.join("sample_analytics.duckdb");
    if !duckdb_path.exists() {
        create_sample_duckdb(&duckdb_path)?;
    }

    Ok((sqlite_path, duckdb_path))
}

pub(crate) fn create_sample_sqlite(path: &Path) -> anyhow::Result<()> {
    // Claim the path without following or replacing an existing file. A competing startup
    // may create it first; only the creator is allowed to stamp fixture ownership.
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
        Err(error) => return Err(error.into()),
    }
    let flags =
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW;
    let mut conn = RusqliteConn::open_with_flags(path, flags)?;
    let tx = conn.transaction()?;
    tx.pragma_update(None, "application_id", SALES_FIXTURE_ID)?;
    tx.execute(
        "CREATE TABLE IF NOT EXISTS invoices (
            id INTEGER PRIMARY KEY,
            customer_name TEXT NOT NULL,
            issued_on TEXT NOT NULL,
            amount REAL NOT NULL,
            status TEXT NOT NULL
        )",
        [],
    )?;

    let count: i64 = tx.query_row("SELECT count(*) FROM invoices", [], |r| r.get(0))?;
    if count == 0 {
        let mut stmt = tx.prepare(
            "INSERT INTO invoices (id, customer_name, issued_on, amount, status) VALUES (?, ?, ?, ?, ?)",
        )?;

        for r in INVOICE_SEEDS {
            stmt.execute(rusqlite::params![r.0, r.1, r.2, r.3, r.4])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn create_sample_duckdb(path: &Path) -> anyhow::Result<()> {
    let conn = duckdb::Connection::open(path)?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS server_metrics (
            id BIGINT PRIMARY KEY,
            region VARCHAR NOT NULL,
            recorded_at VARCHAR NOT NULL,
            requests_per_sec DOUBLE NOT NULL,
            p99_latency_ms DOUBLE NOT NULL,
            error_rate DOUBLE NOT NULL
        )",
        [],
    )?;

    let count: i64 = conn.query_row("SELECT count(*) FROM server_metrics", [], |r| r.get(0))?;
    if count == 0 {
        let mut stmt = conn.prepare("INSERT INTO server_metrics VALUES (?, ?, ?, ?, ?, ?)")?;
        let sample_records = [
            (101, "us-east-1", "2026-10-19 18:00:00", 12500.0, 4.2, 0.001),
            (102, "us-west-2", "2026-10-19 18:00:00", 8900.0, 3.8, 0.0005),
            (
                103,
                "eu-central-1",
                "2026-10-19 18:00:00",
                14200.0,
                5.1,
                0.002,
            ),
            (
                104,
                "ap-northeast-1",
                "2026-10-19 18:00:00",
                6300.0,
                6.4,
                0.0015,
            ),
            (105, "sa-east-1", "2026-10-19 18:00:00", 4100.0, 7.2, 0.003),
            (
                106,
                "us-east-1",
                "2026-10-19 19:00:00",
                13100.0,
                4.5,
                0.0012,
            ),
            (107, "us-west-2", "2026-10-19 19:00:00", 9400.0, 3.9, 0.0004),
            (
                108,
                "eu-central-1",
                "2026-10-19 19:00:00",
                15800.0,
                5.3,
                0.0018,
            ),
            (
                109,
                "ap-northeast-1",
                "2026-10-19 19:00:00",
                6700.0,
                6.1,
                0.0011,
            ),
            (110, "sa-east-1", "2026-10-19 19:00:00", 4400.0, 7.0, 0.0028),
        ];

        for r in sample_records {
            stmt.execute(duckdb::params![r.0, r.1, r.2, r.3, r.4, r.5])?;
        }
    }
    Ok(())
}

/// A deliberately small, inspectable cash-reserve model for the sample workbook.
pub fn seed_sample_financial_model(workbook: &mut gridloom_core::Workbook) -> anyhow::Result<()> {
    use gridloom_core::CellCoord;
    for (row, label, input) in [
        (
            1,
            "Cash collected",
            "=SUMIF(Invoices[status],\"Paid\",Invoices[amount])",
        ),
        (2, "Invoice total", "=SUM(Invoices[amount])"),
        (3, "Collection rate", "=IFERROR(J2/J3,0)"),
        (5, "Monthly fixed costs", "25000"),
        (6, "Reserve months", "3"),
        (7, "Cash after reserve", "=J2-J6*J7"),
    ] {
        workbook.set_cell("Sheet1", CellCoord::new(row, 8), label.into())?;
        workbook.set_cell("Sheet1", CellCoord::new(row, 9), input.into())?;
    }
    workbook.set_cell(
        "Sheet1",
        CellCoord::new(9, 8),
        "Select J8, then Explain & Review".into(),
    )?;
    workbook.set_cell(
        "Sheet1",
        CellCoord::new(10, 8),
        "Illustrative assumptions, not accounting advice".into(),
    )?;
    Ok(())
}
