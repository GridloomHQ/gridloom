//! Validate staged edits against the declared SQL write target, not the last displayed value.
use super::{ColumnMeta, DbConnection, EngineKind};
use crate::{CellError, Value, Workbook};
use chrono::NaiveDate;
use rust_decimal::{prelude::*, Decimal};
use uuid::Uuid;

pub enum SqlEditInput {
    Raw(String),
    Typed(Value),
}

enum Kind {
    Text,
    Integer(i128, i128),
    Decimal(Option<(u32, u32)>),
    Real { single: bool },
    Boolean,
    Date,
}

fn spec(engine: EngineKind, ty: &str) -> Option<Kind> {
    let ty = ty.trim().to_ascii_uppercase();
    let (name, dimensions) = if let Some((name, tail)) = ty.split_once('(') {
        (name.trim(), Some(tail.strip_suffix(')')?))
    } else {
        (ty.as_str(), None)
    };
    let integer = |min, max| Some(Kind::Integer(min, max));
    let duck = engine == EngineKind::DuckDb;
    match name {
        "TEXT" | "VARCHAR" | "CHAR" | "CHARACTER" | "CHARACTER VARYING" | "NCHAR" | "NVARCHAR"
        | "NATIVE CHARACTER" | "CLOB" | "STRING" => Some(Kind::Text),
        "TINYINT" if duck => integer(i8::MIN as i128, i8::MAX as i128),
        "SMALLINT" if duck => integer(i16::MIN as i128, i16::MAX as i128),
        "INTEGER" | "INT" if duck => integer(i32::MIN as i128, i32::MAX as i128),
        "UTINYINT" if duck => integer(0, u8::MAX as i128),
        "USMALLINT" if duck => integer(0, u16::MAX as i128),
        "UINTEGER" if duck => integer(0, u32::MAX as i128),
        "UBIGINT" if duck => integer(0, i64::MAX as i128),
        "INT" | "INTEGER" | "TINYINT" | "SMALLINT" | "MEDIUMINT" | "BIGINT" | "INT2" | "INT8"
        | "UNSIGNED BIG INT" => integer(i64::MIN as i128, i64::MAX as i128),
        "DECIMAL" | "NUMERIC" => {
            let dimensions = dimensions.map(|part| {
                if let Some((p, s)) = part.split_once(',') {
                    Some((p.trim().parse::<u32>().ok()?, s.trim().parse::<u32>().ok()?))
                } else {
                    Some((part.trim().parse::<u32>().ok()?, 0))
                }
            });
            let dimensions = match dimensions {
                None => None,
                Some(item) => Some(item?),
            };
            if dimensions.is_some_and(|(p, s)| p == 0 || p > 38 || s > p) {
                return None;
            }
            Some(Kind::Decimal(dimensions))
        }
        "FLOAT" | "REAL" => Some(Kind::Real { single: duck }),
        "DOUBLE" | "DOUBLE PRECISION" => Some(Kind::Real { single: false }),
        "BOOLEAN" | "BOOL" => Some(Kind::Boolean),
        "DATE" => Some(Kind::Date),
        _ => None,
    }
}

fn error(column: &ColumnMeta, detail: &str) -> CellError {
    CellError::Sql(format!("{} ({}): {detail}", column.name, column.data_type))
}

fn raw_integer(raw: &str) -> Option<i128> {
    if raw.trim().is_empty() {
        return None;
    }
    raw.trim().parse().ok()
}

fn exactly_typed_integer(value: Value) -> Option<i128> {
    match value {
        Value::Integer(value) => Some(value as i128),
        Value::Decimal(value) if value.fract().is_zero() => value.to_i128(),
        _ => None,
    }
}

fn exact_decimal(raw: &str) -> Option<Decimal> {
    let raw = raw.trim();
    let digits = raw.strip_prefix(['+', '-']).unwrap_or(raw);
    if !digits
        .bytes()
        .all(|byte| byte.is_ascii_digit() || byte == b'.')
    {
        return None;
    }
    Decimal::from_str_exact(raw).ok()
}

fn decimal_fits(value: Decimal, precision: u32, scale: u32) -> bool {
    let value = value.normalize();
    let integer_digits = value
        .mantissa()
        .unsigned_abs()
        .checked_ilog10()
        .map_or(0, |n| (n + 1).saturating_sub(value.scale()));
    value.scale() <= scale && integer_digits <= precision - scale
}

fn sql_value(
    engine: EngineKind,
    column: &ColumnMeta,
    input: SqlEditInput,
) -> Result<Value, CellError> {
    use SqlEditInput::{Raw, Typed};
    let kind = spec(engine, &column.data_type)
        .ok_or_else(|| error(column, "This SQL type is read-only in Gridloom."))?;
    if matches!(&input, Typed(Value::Null))
        || (!matches!(&kind, Kind::Text)
            && matches!(&input, Raw(s) if s.trim().eq_ignore_ascii_case("NULL")))
    {
        return if column.is_nullable {
            Ok(Value::Null)
        } else {
            Err(error(column, "NULL is not allowed."))
        };
    }
    let parsed = match kind {
        Kind::Text => match input {
            Raw(s) | Typed(Value::Text(s)) => return Ok(Value::Text(s)),
            Typed(Value::EmptyText) => return Ok(Value::Text(String::new())),
            _ => {
                return Err(error(
                    column,
                    "Enter literal text. Use SQL NULL explicitly to clear a nullable column.",
                ))
            }
        },
        Kind::Integer(min, max) => {
            let value = match input {
                Raw(s) => raw_integer(&s),
                Typed(v) => exactly_typed_integer(v),
            };
            value
                .filter(|value| (*value >= min) && (*value <= max))
                .and_then(|value| i64::try_from(value).ok())
                .map(Value::Integer)
                .ok_or_else(|| {
                    error(
                        column,
                        &format!("Enter a whole number from {min} to {max}."),
                    )
                })
        }
        Kind::Decimal(dimensions) => {
            let value = match input {
                Raw(s) => exact_decimal(&s),
                Typed(Value::Decimal(d)) => Some(d),
                Typed(Value::Integer(i)) => Some(Decimal::from(i)),
                _ => None,
            }
            .ok_or_else(|| {
                error(
                    column,
                    "Enter an exact fixed-point number without separators or an exponent.",
                )
            })?;
            if let Some((precision, scale)) = dimensions {
                if !decimal_fits(value, precision, scale) {
                    return Err(error(column, &format!("The exact value does not fit precision {precision}, scale {scale}; rounding is not applied.")));
                }
            }
            if engine == EngineKind::Sqlite {
                if value.fract().is_zero() {
                    return value.to_i64().map(Value::Integer).ok_or_else(|| error(column, "SQLite can preserve whole numbers only within its signed 64-bit integer range."));
                }
                // SQLite NUMERIC stores fractions as REAL. Match the actual canonical Gridloom
                // decode, and reject magnitudes/scales for which the staged decimal would change.
                if value.to_f64().and_then(Decimal::from_f64) != Some(value) {
                    return Err(error(column, "SQLite would change this numeric value on readback. Use a scaled INTEGER account value or a DuckDB DECIMAL column to preserve it."));
                }
            }
            Ok(Value::Decimal(value))
        }
        Kind::Real { single } => {
            let raw = match input {
                Raw(s) => s.trim().parse::<f64>().ok(),
                Typed(Value::Float(f)) => Some(f),
                Typed(Value::Integer(i)) if i.unsigned_abs() <= 9_007_199_254_740_992 => Some(i as f64),
                Typed(Value::Decimal(d)) => d.to_f64(),
                _ => None,
            }.filter(|n| n.is_finite())
             .ok_or_else(|| error(column, "Enter a finite number. This SQL column stores approximate floating-point values."))?;
            let stored = if single { (raw as f32) as f64 } else { raw };
            if !stored.is_finite() || (stored == 0.0 && raw != 0.0) {
                return Err(error(
                    column,
                    "This number exceeds the representable range of the column.",
                ));
            }
            Ok(Value::Float(stored))
        }
        Kind::Boolean => {
            let value = match input {
                Raw(s) if s.trim().eq_ignore_ascii_case("true") || s.trim() == "1" => Some(true),
                Raw(s) if s.trim().eq_ignore_ascii_case("false") || s.trim() == "0" => Some(false),
                Typed(Value::Boolean(b)) => Some(b),
                _ => None,
            };
            value
                .map(Value::Boolean)
                .ok_or_else(|| error(column, "Enter TRUE or FALSE (1 or 0)."))
        }
        Kind::Date => {
            let date = match input {
                Raw(s) => {
                    let raw = s.trim();
                    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
                        .ok()
                        .filter(|date| date.format("%Y-%m-%d").to_string() == raw)
                }
                Typed(Value::Date(date)) => Some(date),
                _ => None,
            };
            date.map(Value::Date)
                .ok_or_else(|| error(column, "Enter a valid calendar date as YYYY-MM-DD."))
        }
    };
    parsed
}

impl DbConnection {
    /// Verify declared primary key/target and parse the proposal without executing any data write.
    pub fn prepare_edit(
        &self,
        table: &str,
        column_name: &str,
        pk_columns: &[String],
        pk_values: Vec<Value>,
        input: SqlEditInput,
    ) -> Result<(String, Vec<String>, Vec<Value>, Value), CellError> {
        let schema = self.get_table_schema(table)?;
        if schema.primary_keys.is_empty()
            || schema.primary_keys.len() != pk_columns.len()
            || pk_columns.len() != pk_values.len()
        {
            return Err(CellError::Sql(
                "A complete non-NULL declared primary key is required.".into(),
            ));
        }
        let column = schema
            .columns
            .iter()
            .find(|col| col.name.eq_ignore_ascii_case(column_name))
            .ok_or_else(|| {
                CellError::Sql(
                    "The column is not an editable base-table column. Refresh the source.".into(),
                )
            })?;
        if column.is_primary_key {
            return Err(error(column, "Primary key columns are read-only."));
        }
        match self {
            DbConnection::Sqlite(engine) => engine.check_column_writable(table, &column.name)?,
            DbConnection::DuckDb(engine) => engine.check_column_writable(table, &column.name)?,
        }
        let mut keys = Vec::new();
        for name in &schema.primary_keys {
            let input_index = pk_columns
                .iter()
                .position(|key| key.eq_ignore_ascii_case(name))
                .ok_or_else(|| {
                    CellError::Sql(
                        "The supplied columns do not match the declared primary key.".into(),
                    )
                })?;
            let key_column = schema
                .columns
                .iter()
                .find(|col| col.name == *name)
                .expect("schema annotated primary key");
            let value = pk_values[input_index].clone();
            if matches!(
                value,
                Value::Null | Value::Blank | Value::Array(_) | Value::Error(_)
            ) {
                return Err(CellError::Sql(
                    "Primary key values must be non-NULL scalar values.".into(),
                ));
            }
            // Existing SQL key binders use binary64 for decimal values. Don't use a
            // rounded identity for an UPDATE, even when the edit itself is valid.
            let value = match spec(self.kind(), &key_column.data_type) {
                Some(Kind::Decimal(_)) => {
                    let exact = sql_value(self.kind(), key_column, SqlEditInput::Typed(value))?;
                    let Some(integer) =
                        exactly_typed_integer(exact).and_then(|n| i64::try_from(n).ok())
                    else {
                        return Err(error(key_column, "Fractional decimal keys are read-only because the current writeback cannot bind them exactly."));
                    };
                    Value::Integer(integer)
                }
                Some(Kind::Real { .. }) => {
                    return Err(error(
                        key_column,
                        "Approximate floating-point keys are read-only.",
                    ))
                }
                Some(Kind::Boolean)
                    if self.kind() == EngineKind::Sqlite
                        && matches!(value, Value::Integer(0 | 1)) =>
                {
                    value
                }
                Some(Kind::Text) if matches!(value, Value::Date(_) | Value::Timestamp(_)) => {
                    return Err(error(key_column, "This text key was read as a date or time; its original spelling cannot be verified."));
                }
                _ => sql_value(self.kind(), key_column, SqlEditInput::Typed(value))?,
            };
            keys.push(value);
        }
        let proposal = sql_value(self.kind(), column, input)?;
        Ok((column.name.clone(), schema.primary_keys, keys, proposal))
    }
}

impl Workbook {
    #[allow(clippy::too_many_arguments)]
    pub fn stage_sql_edit(
        &mut self,
        connection: String,
        table: String,
        pk_columns: Vec<String>,
        pk_values: Vec<Value>,
        column: String,
        original: Value,
        input: SqlEditInput,
    ) -> Result<Uuid, CellError> {
        if matches!(original, Value::Blank | Value::Array(_) | Value::Error(_)) {
            return Err(CellError::Sql(
                "The original source value must be a scalar SQL value.".into(),
            ));
        }
        let source = self.connections.get(&connection).ok_or_else(|| {
            CellError::Sql(
                "SQL connection is not available. Reconnect the source before editing.".into(),
            )
        })?;
        let (column, pk_columns, pk_values, proposal) =
            source.prepare_edit(&table, &column, &pk_columns, pk_values, input)?;
        let id = self.staged.stage_edit(
            connection, table, pk_columns, pk_values, column, original, proposal,
        );
        self.invalidate_changed_reviews();
        Ok(id)
    }
}
