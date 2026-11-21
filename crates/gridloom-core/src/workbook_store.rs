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
const FORMAT_VERSION: i64 = 2;

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
           sheet_id TEXT NOT NULL REFERENCES worksheet(id), name TEXT NOT NULL, local_id TEXT UNIQUE, json TEXT NOT NULL,
           PRIMARY KEY (sheet_id, name)
         ) STRICT, WITHOUT ROWID;
         CREATE TABLE worksheet_table_cell (
           table_id TEXT NOT NULL REFERENCES worksheet_table(local_id), coordinate TEXT NOT NULL, json TEXT NOT NULL,
           PRIMARY KEY (table_id, coordinate)
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
        let mut table_stmt = tx
            .prepare_cached("INSERT INTO worksheet_table VALUES (?1, ?2, ?3, ?4)")
            .map_err(storage)?;
        let mut table_cell_stmt = tx
            .prepare_cached("INSERT INTO worksheet_table_cell VALUES (?1, ?2, ?3)")
            .map_err(storage)?;
        for (name, sheet) in sheets {
            let key = sheet.storage_id.to_string();
            sheet_stmt.execute(params![key, name]).map_err(storage)?;
            let local_tables: Vec<_> = sheet
                .tables
                .values()
                .filter_map(|table| {
                    table
                        .local
                        .as_ref()
                        .map(|local| (table.range, local.id.to_string()))
                })
                .collect();
            for (name, table) in &sheet.tables {
                let local_id = table.local.as_ref().map(|local| local.id.to_string());
                table_stmt
                    .execute(params![key, name, local_id, encode(table)?])
                    .map_err(storage)?;
            }
            for (coordinate, cell) in &sheet.cells {
                if let Some((_, table_id)) = local_tables.iter().find(|(range, _)| {
                    range.contains(cell.coord) && cell.coord.row > range.start_row
                }) {
                    table_cell_stmt
                        .execute(params![table_id, coordinate, encode(cell)?])
                        .map_err(storage)?;
                } else {
                    cell_stmt
                        .execute(params![key, coordinate, encode(cell)?])
                        .map_err(storage)?;
                }
            }
        }
    }
    tx.commit().map_err(storage)?;
    db.close().map_err(|(_, error)| storage(error))
}

/// Prevent the workbook storage schema from being treated as a linked source,
/// including when opened from another workbook or through a path alias.
pub(crate) fn reject_as_linked_source(path: &Path) -> Result<(), CellError> {
    if !fs::metadata(path).is_ok_and(|metadata| metadata.is_file()) {
        return Ok(());
    }
    let mut header = [0; 72];
    let Ok(mut file) = File::open(path) else {
        return Ok(()); // The database engine reports its normal access errors.
    };
    if file.read_exact(&mut header).is_ok()
        && &header[..16] == b"SQLite format 3\0"
        && header[68..72] == (APP_ID as u32).to_be_bytes()
    {
        return Err(storage(
            "Open this file as a Gridloom workbook, not as a linked source",
        ));
    }
    Ok(())
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
        if id != APP_ID || !(1..=FORMAT_VERSION).contains(&version) {
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
    if !(1..=FORMAT_VERSION).contains(&version) {
        return Err(storage(format!("unsupported format version {version}")));
    }
    let payload: String = db
        .query_row("SELECT json FROM workbook WHERE singleton = 1", [], |r| {
            r.get(0)
        })
        .map_err(storage)?;
    let mut snapshot: SerializedWorkbook = decode(&payload)?;
    let mut seen = HashSet::with_capacity(snapshot.sheets.len());
    let mut local_registrations = if version >= 2 {
        Some(
            db.prepare_cached("SELECT name, local_id FROM worksheet_table WHERE sheet_id = ?1")
                .map_err(storage)?,
        )
    } else {
        None
    };
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
        let Some(local_registrations) = &mut local_registrations else {
            continue;
        };
        let mut local_ids = local_registrations.query([&id]).map_err(storage)?;
        while let Some(record) = local_ids.next().map_err(storage)? {
            let name: String = record.get(0).map_err(storage)?;
            let stored: Option<String> = record.get(1).map_err(storage)?;
            let table = sheet
                .tables
                .get(&name)
                .ok_or_else(|| storage("unknown table record"))?;
            let expected = table.local.as_ref().map(|local| local.id.to_string());
            if expected != stored {
                return Err(storage("invalid workbook-local table registration"));
            }
            if let Some(local_id) = stored {
                let Some(local) = &table.local else {
                    unreachable!()
                };
                if local.column_types.len() != table.columns.len()
                    || table.live.is_some()
                    || !table.source_name.is_empty()
                {
                    return Err(storage("inconsistent workbook-local table schema"));
                }
                let mut cells: HashMap<String, crate::workbook::CellData> = HashMap::new();
                read_rows(
                    &db,
                    "SELECT coordinate, json FROM worksheet_table_cell WHERE table_id = ?1",
                    &local_id,
                    &mut cells,
                )?;
                for (coordinate, mut cell) in cells {
                    let (column, kind) = table
                        .local_column(cell.coord)
                        .ok_or_else(|| storage("cell outside its local table"))?;
                    kind.enforce(&mut cell, column)?;
                    if cell.coord.to_a1() != coordinate
                        || sheet.cells.insert(coordinate, cell).is_some()
                    {
                        return Err(storage("invalid workbook-local table cell"));
                    }
                }
            }
        }
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
