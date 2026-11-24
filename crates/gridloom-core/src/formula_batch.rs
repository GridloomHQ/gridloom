//! Apply a worksheet's scalar formula template to rows in a linked SQLite database.
//! Each row gets a fresh evaluation context. The worksheet formulas are the only
//! calculation definition; no SQL expression or precomputed value can replace them.
use crate::ast::{BinaryOp, Expr, StructuredRef};
use crate::db::EngineKind;
use crate::evaluator::evaluate_expr;
use crate::functions::EvaluationContext;
use crate::parser::parse_formula;
use crate::{CellCoord, CellError, CellRange, Value, Workbook};
use rusqlite::{params_from_iter, Connection, OpenFlags, TransactionBehavior};
use serde::Serialize;
use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{Duration, Instant};

const MARKER: &str = "TAYRIVA FORMULA BATCH 1";
const HEADER: u32 = 4;
const TEMPLATE: u32 = 5;
const MAX_COLS: u32 = 64;

#[derive(Debug, Serialize)]
pub struct BatchPlan {
    pub connection: String,
    pub source: String,
    pub destination: String,
    pub key: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub revision: String,
}

#[derive(Debug, Serialize)]
pub struct BatchReport {
    pub rows: u64,
    pub elapsed_seconds: f64,
    pub read_seconds: f64,
    pub calculate_seconds: f64,
    pub write_seconds: f64,
}

pub struct FormulaBatch {
    pub plan: BatchPlan,
    path: String,
    operations: Vec<(usize, Expr)>,
    output_slots: Vec<usize>,
    slots: usize,
}

fn err(message: impl Into<String>) -> CellError {
    CellError::Sql(message.into())
}
fn db(error: rusqlite::Error) -> CellError {
    err(error.to_string())
}
fn identifier(s: &str) -> Result<String, CellError> {
    if s.len() > 64
        || s.is_empty()
        || s.to_ascii_lowercase().starts_with("sqlite_")
        || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(err(format!("Invalid batch database identifier: {s}")));
    }
    Ok(format!("\"{s}\""))
}

pub fn marked(sheet: &crate::workbook::Sheet) -> bool {
    sheet
        .get_cell(&CellCoord::new(0, 0))
        .is_some_and(|c| c.raw_input == MARKER)
}

impl FormulaBatch {
    pub fn compile(workbook: &Workbook, sheet_name: &str) -> Result<Self, CellError> {
        let sheet = workbook.sheets.get(sheet_name).ok_or(CellError::Ref)?;
        if !marked(sheet) {
            return Err(err("This sheet has no formula batch template."));
        }
        let cell = |r, c| {
            sheet
                .get_cell(&CellCoord::new(r, c))
                .map(|v| v.raw_input.trim())
                .unwrap_or("")
        };
        let connection = cell(1, 0).to_owned();
        let source = cell(1, 1).to_owned();
        let destination = cell(1, 2).to_owned();
        let key = cell(1, 3).to_owned();
        let config = workbook
            .connection_configs
            .get(&connection)
            .ok_or_else(|| err("Link the worksheet's SQLite connection first."))?;
        if config.kind != EngineKind::Sqlite || config.path.is_empty() || config.path == ":memory:"
        {
            return Err(err("A linked SQLite database file is required."));
        }
        if workbook
            .staged
            .patches
            .values()
            .any(|p| p.connection_name == connection)
        {
            return Err(err(
                "Commit or discard staged edits for this connection before running the batch.",
            ));
        }
        for name in [&source, &destination, &key] {
            identifier(name)?;
        }
        if source.eq_ignore_ascii_case(&destination) {
            return Err(err(
                "Use a separate results table; the source is read-only.",
            ));
        }
        let mut hasher = DefaultHasher::new();
        (&connection, &config.path, &source, &destination, &key).hash(&mut hasher);
        let mut inputs = vec![];
        let mut outputs = vec![];
        let mut output_slots = vec![];
        let mut operations = vec![];
        let mut slots = 0;
        let mut started_calc = false;
        let mut seen_columns = HashSet::new();
        for col in 0..MAX_COLS {
            let header = cell(HEADER, col);
            if header.is_empty() {
                break;
            }
            let (tag, name) = header
                .split_once(':')
                .ok_or_else(|| err("Template headers must use in:, calc:, or out:."))?;
            identifier(name)?;
            if name.eq_ignore_ascii_case(&key) || !seen_columns.insert(name.to_ascii_lowercase()) {
                return Err(err(format!("Repeated batch column: {name}")));
            }
            let raw = cell(TEMPLATE, col);
            (header, raw).hash(&mut hasher);
            let idx = col as usize;
            match tag {
                "in" if !started_calc => inputs.push(name.to_owned()),
                "calc" | "out" => {
                    started_calc = true;
                    if !raw.starts_with('=') || raw.len() > 512 {
                        return Err(err(format!(
                            "Expected a worksheet formula (at most 512 bytes) for {name}."
                        )));
                    }
                    let expr = parse_formula(raw)?;
                    validate(&expr, col, 0)?;
                    operations.push((idx, expr));
                    if tag == "out" {
                        outputs.push(name.to_owned());
                        output_slots.push(idx);
                    }
                }
                _ => return Err(err("Input columns must precede all formulas.")),
            }
            slots = idx + 1;
        }
        if inputs.is_empty()
            || outputs.is_empty()
            || slots >= MAX_COLS as usize
            || (slots as u32..MAX_COLS).any(|col| !cell(HEADER, col).is_empty())
        {
            return Err(err(
                "Use one contiguous template of 1–63 columns, including inputs and outputs.",
            ));
        }
        Ok(Self {
            plan: BatchPlan {
                connection,
                source,
                destination,
                key,
                inputs,
                outputs,
                revision: format!("{:016x}", hasher.finish()),
            },
            path: config.path.clone(),
            operations,
            output_slots,
            slots,
        })
    }

    pub fn run(&self) -> Result<BatchReport, CellError> {
        let start = Instant::now();
        let mut conn = Connection::open_with_flags(&self.path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(db)?;
        conn.busy_timeout(Duration::ZERO).map_err(db)?;
        conn.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA trusted_schema=OFF;",
        )
        .map_err(db)?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db)?;
        let p = &self.plan;
        for table in [&p.source, &p.destination] {
            // Require ordinary tables and a single, declared INTEGER key. Views and
            // virtual tables can execute unrelated code while reading or writing.
            let safe: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_list WHERE schema='main' AND type='table' AND name=?1 COLLATE NOCASE)
                     AND EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name=?2 COLLATE NOCASE AND upper(type)='INTEGER' AND pk=1)
                     AND (SELECT count(*) FROM pragma_table_info(?1) WHERE pk>0)=1",
                [table,&p.key], |r| r.get(0)).map_err(db)?;
            if !safe {
                return Err(err(format!(
                    "{table} must be a regular SQLite table with an INTEGER primary key named {}.",
                    p.key
                )));
            }
        }
        let inputs = p
            .inputs
            .iter()
            .map(|c| identifier(c))
            .collect::<Result<Vec<_>, _>>()?;
        let outputs = p
            .outputs
            .iter()
            .map(|c| identifier(c))
            .collect::<Result<Vec<_>, _>>()?;
        let key = identifier(&p.key)?;
        let source = identifier(&p.source)?;
        let destination = identifier(&p.destination)?;
        let placeholders = vec!["?"; outputs.len() + 1].join(",");
        let select = format!(
            "SELECT s.{key},{} FROM main.{source} AS s ORDER BY s.{key}",
            inputs
                .iter()
                .map(|c| format!("s.{c}"))
                .collect::<Vec<_>>()
                .join(",")
        );
        let insert = format!(
            "INSERT INTO main.{destination}({key},{}) VALUES ({placeholders})",
            outputs.join(",")
        );
        // Replacing the complete snapshot would fire any destination triggers.
        // Disallow them and refuse inbound references (including cascading FKs).
        let side_effect: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE type='trigger' AND tbl_name=?1 COLLATE NOCASE)
                 OR EXISTS(SELECT 1 FROM pragma_table_list AS t, pragma_foreign_key_list(t.name) AS f
                    WHERE t.schema='main' AND t.type='table' AND f.[table]=?1 COLLATE NOCASE)",
            [&p.destination], |r| r.get(0)).map_err(db)?;
        if side_effect {
            return Err(err(
                "The results table must not have triggers or inbound foreign keys.",
            ));
        }
        let mut read = Duration::ZERO;
        let mut calculate = Duration::ZERO;
        let mut write = Duration::ZERO;
        let mut count = 0;
        {
            let mut rows_stmt = tx.prepare(&select).map_err(db)?;
            // Do not allow SQLite to turn an unknown quoted identifier into a literal.
            let available = |table: &str, col: &str| {
                tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name=?2 COLLATE NOCASE AND upper(type)='INTEGER')",
                [table,col], |r| r.get::<_,bool>(0)).map_err(db)
            };
            for col in &p.inputs {
                if !available(&p.source, col)? {
                    return Err(err(format!("Input {col} must be an INTEGER column.")));
                }
            }
            for col in &p.outputs {
                if !available(&p.destination, col)? {
                    return Err(err(format!("Output {col} must be an INTEGER column.")));
                }
            }
            let mut writer = tx.prepare(&insert).map_err(db)?;
            let tick = Instant::now();
            tx.execute(&format!("DELETE FROM main.{destination}"), [])
                .map_err(db)?;
            write += tick.elapsed();
            let mut rows = rows_stmt.query([]).map_err(db)?;
            let mut context = RowContext(vec![Value::Blank; self.slots]);
            let mut results = vec![0_i64; self.output_slots.len() + 1];
            loop {
                let tick = Instant::now();
                let Some(row) = rows.next().map_err(db)? else {
                    read += tick.elapsed();
                    break;
                };
                let id: i64 = row.get(0).map_err(db)?;
                results[0] = id;
                for index in 0..self.plan.inputs.len() {
                    context.0[index] = Value::Integer(row.get(index + 1).map_err(|e| {
                        err(format!("Row key {id}, input {}: {e}", p.inputs[index]))
                    })?);
                }
                read += tick.elapsed();
                let tick = Instant::now();
                for (idx, expr) in &self.operations {
                    context.0[*idx] = evaluate_expr(expr, &context).map_err(|e| {
                        err(format!("Row key {id}, template column {}: {e}", idx + 1))
                    })?;
                }
                for (i, slot) in self.output_slots.iter().enumerate() {
                    let v = &context.0[*slot];
                    use rust_decimal::prelude::ToPrimitive;
                    results[i + 1] = match v {
                        Value::Integer(i) => Some(*i),
                        Value::Decimal(d) if d.fract().is_zero() => d.to_i64(),
                        _ => None,
                    }
                    .ok_or_else(|| {
                        err(format!(
                            "Row key {id}, {}: expected an exact SQLite integer, found {}",
                            p.outputs[i],
                            v.to_string_repr()
                        ))
                    })?;
                }
                calculate += tick.elapsed();
                let tick = Instant::now();
                writer
                    .execute(params_from_iter(results.iter()))
                    .map_err(db)?;
                write += tick.elapsed();
                count += 1;
            }
        }
        let tick = Instant::now();
        tx.commit().map_err(db)?;
        write += tick.elapsed();
        Ok(BatchReport {
            rows: count,
            elapsed_seconds: start.elapsed().as_secs_f64(),
            read_seconds: read.as_secs_f64(),
            calculate_seconds: calculate.as_secs_f64(),
            write_seconds: write.as_secs_f64(),
        })
    }
}

fn validate(expr: &Expr, column: u32, depth: u32) -> Result<(), CellError> {
    if depth > 64 {
        return Err(err("Batch formula is too deeply nested."));
    }
    match expr {
        Expr::Literal(Value::Integer(_) | Value::Decimal(_)) => Ok(()),
        Expr::CellRef(c) if c.row == TEMPLATE && c.col < column => Ok(()),
        Expr::Unary { expr, .. } => validate(expr,column,depth+1),
        Expr::Binary {op: BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div,left,right} => {
            validate(left,column,depth+1)?; validate(right,column,depth+1)
        }
        Expr::FunctionCall {name,args} if name.eq_ignore_ascii_case("ROUND") && args.len()==2 => {
            for a in args { validate(a,column,depth+1)?; } Ok(())
        }
        _ => Err(err("Batch formulas support exact arithmetic, ROUND, and earlier cells in the template row.")),
    }
}

struct RowContext(Vec<Value>);
impl EvaluationContext for RowContext {
    fn get_cell_value(&self, coord: CellCoord) -> Result<Value, CellError> {
        self.0
            .get(coord.col as usize)
            .cloned()
            .ok_or(CellError::Ref)
    }
    fn get_range_values(&self, _range: CellRange) -> Result<Value, CellError> {
        Err(CellError::Ref)
    }
    fn get_table_column(&self, _: &StructuredRef) -> Result<Value, CellError> {
        Err(CellError::Ref)
    }
    fn execute_sql_value(&self, _: &str, _: &str, _: &[Value]) -> Result<Value, CellError> {
        Err(CellError::Ref)
    }
    fn execute_sql_table(&self, _: &str, _: &str, _: &[Value]) -> Result<Value, CellError> {
        Err(CellError::Ref)
    }
}
