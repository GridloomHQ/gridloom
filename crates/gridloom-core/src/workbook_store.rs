//! Versioned SQLite container for manual workbook saves. It is not an
//! external connection and can never be a staged writeback target.
use crate::error::CellError;
use crate::workbook::{SerializedWorkbook, Sheet};
use rusqlite::{params, Connection, OpenFlags};
use serde::{de::DeserializeOwned, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

// SQLite application_id, ASCII GLWB.
const APP_ID: i64 = 0x474c5742;
const FORMAT_VERSION: i64 = 1;

fn storage(error: impl std::fmt::Display) -> CellError {
    CellError::Sql(format!("Workbook database: {error}"))
}

fn encode(value: &impl Serialize) -> Result<String, CellError> {
    serde_json::to_string(value).map_err(storage)
}

fn decode<T: DeserializeOwned>(value: &str) -> Result<T, CellError> {
    serde_json::from_str(value).map_err(storage)
}

/// Called only for a fresh, private same-directory temporary file.
pub(crate) fn write(
    path: &Path,
    metadata: &impl Serialize,
    sheets: &HashMap<String, Sheet>,
) -> Result<(), CellError> {
    let mut db =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE).map_err(storage)?;
    db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON")
        .map_err(storage)?;
    let tx = db.transaction().map_err(storage)?;
    tx.pragma_update(None, "application_id", APP_ID)
        .map_err(storage)?;
    tx.pragma_update(None, "user_version", FORMAT_VERSION)
        .map_err(storage)?;
    tx.execute_batch(
        "CREATE TABLE workbook (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), json TEXT NOT NULL) STRICT;
         CREATE TABLE worksheet (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE) STRICT;
         CREATE TABLE worksheet_cell (
           sheet_id TEXT NOT NULL REFERENCES worksheet(id), coordinate TEXT NOT NULL, json TEXT NOT NULL,
           PRIMARY KEY (sheet_id, coordinate)
         ) STRICT, WITHOUT ROWID;
         CREATE TABLE worksheet_table (
           sheet_id TEXT NOT NULL REFERENCES worksheet(id), name TEXT NOT NULL, json TEXT NOT NULL,
           PRIMARY KEY (sheet_id, name)
         ) STRICT, WITHOUT ROWID;",
    ).map_err(storage)?;
    tx.execute("INSERT INTO workbook VALUES (1, ?1)", [encode(metadata)?])
        .map_err(storage)?;
    {
        let mut sheet_stmt = tx
            .prepare_cached("INSERT INTO worksheet VALUES (?1, ?2)")
            .map_err(storage)?;
        let mut cell_stmt = tx
            .prepare_cached("INSERT INTO worksheet_cell VALUES (?1, ?2, ?3)")
            .map_err(storage)?;
        let mut table_stmt = tx.prepare_cached("INSERT INTO worksheet_table VALUES (?1, ?2, ?3)").map_err(storage)?;
        for (name, sheet) in sheets {
            let key = sheet.storage_id.to_string();
            sheet_stmt.execute(params![key, name]).map_err(storage)?;
            for (name, table) in &sheet.tables {
                table_stmt.execute(params![key, name, encode(table)?]).map_err(storage)?;
            }
            for (coordinate, cell) in &sheet.cells {
                cell_stmt.execute(params![key, coordinate, encode(cell)?]).map_err(storage)?;
            }
        }
    }
    tx.commit().map_err(storage)?;
    db.close().map_err(|(_, error)| storage(error))
}

/// Reject other databases before staging a manual save.
pub(crate) fn protect_existing(path: &Path) -> Result<(), CellError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(storage(e)),
    };
    if !metadata.is_file() {
        return Err(storage("save destination is not a regular file"));
    }
    let mut file = File::open(path).map_err(storage)?;
    let mut header = [0; 16];
    match file.read_exact(&mut header) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
        Err(e) => return Err(storage(e)),
    }
    if &header[8..12] == b"DUCK" {
        return Err(storage("save destination is a DuckDB database"));
    }
    if &header == b"SQLite format 3\0" {
        let db =
            Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(storage)?;
        let id: i64 = db
            .pragma_query_value(None, "application_id", |r| r.get(0))
            .map_err(storage)?;
        let version: i64 = db
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(storage)?;
        if id != APP_ID || version != FORMAT_VERSION {
            return Err(storage(
                "save destination is not a supported Gridloom workbook",
            ));
        }
    }
    Ok(())
}

pub(crate) fn read(path: &Path) -> Result<SerializedWorkbook, CellError> {
    let db =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(storage)?;
    db.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA query_only=ON")
        .map_err(storage)?;
    let app_id: i64 = db
        .pragma_query_value(None, "application_id", |r| r.get(0))
        .map_err(storage)?;
    if app_id != APP_ID {
        return Err(storage("this SQLite file is not a Gridloom workbook"));
    }
    let version: i64 = db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(storage)?;
    if version != FORMAT_VERSION {
        return Err(storage(format!("unsupported format version {version}")));
    }
    let payload: String = db
        .query_row("SELECT json FROM workbook WHERE singleton = 1", [], |r| {
            r.get(0)
        })
        .map_err(storage)?;
    let mut snapshot: SerializedWorkbook = decode(&payload)?;
    let mut seen = HashSet::with_capacity(snapshot.sheets.len());
    let mut registry = db
        .prepare("SELECT id, name FROM worksheet")
        .map_err(storage)?;
    let mut ids = registry.query([]).map_err(storage)?;
    while let Some(row) = ids.next().map_err(storage)? {
        let id: String = row.get(0).map_err(storage)?;
        let name: String = row.get(1).map_err(storage)?;
        let sheet = snapshot
            .sheets
            .get_mut(&name)
            .ok_or_else(|| storage("unexpected sheet registration"))?;
        if sheet.storage_id.to_string() != id
            || !seen.insert(name)
            || !sheet.cells.is_empty()
            || !sheet.tables.is_empty()
        {
            return Err(storage("invalid sheet registration"));
        }
        read_rows(
            &db,
            "SELECT coordinate, json FROM worksheet_cell WHERE sheet_id = ?1",
            &id,
            &mut sheet.cells,
        )?;
        read_rows(
            &db,
            "SELECT name, json FROM worksheet_table WHERE sheet_id = ?1",
            &id,
            &mut sheet.tables,
        )?;
    }
    if seen.len() != snapshot.sheets.len() || !snapshot.sheets.contains_key(&snapshot.active_sheet)
    {
        return Err(storage("missing registered or active sheet"));
    }
    Ok(snapshot)
}

fn read_rows<T: DeserializeOwned>(
    db: &Connection,
    sql: &str,
    id: &str,
    target: &mut HashMap<String, T>,
) -> Result<(), CellError> {
    let mut statement = db.prepare_cached(sql).map_err(storage)?;
    let mut rows = statement.query([id]).map_err(storage)?;
    while let Some(row) = rows.next().map_err(storage)? {
        let key: String = row.get(0).map_err(storage)?;
        let value: String = row.get(1).map_err(storage)?;
        if target.insert(key, decode(&value)?).is_some() {
            return Err(storage("duplicate worksheet entry"));
        }
    }
    Ok(())
}
