//! Bounded read windows for live SQLite base tables, never a cached QueryResult.
use serde::{Deserialize, Serialize};
use sqlparser::ast::{
    BinaryOperator, Expr, OrderByKind, OrderBySort, SetExpr, Statement, TableFactor,
};
use sqlparser::dialect::SQLiteDialect;
use sqlparser::parser::Parser;

use crate::db::sqlite::{ColumnMeta, SqliteEngine};
use crate::value::Value;
use crate::CellError;

const MAX_LIVE_CELLS: usize = 20_000;
const MAX_LIVE_COLUMNS: usize = 256;
const MAX_SQL_BYTES: usize = 16_384;
const JS_EXACT_INT: i64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveTable {
    pub connection_name: String,
    pub sql: String,
    pub base_table: String,
    pub primary_key: String,
    pub row_count: u32,
    // An indexed rank lookup is valid only for unfiltered contiguous PK order.
    pub(crate) dense_key_bounds: Option<(i64, i64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveEdit {
    pub connection_name: String,
    pub table_name: String,
    pub pk_column: String,
    pub pk_value: Value,
    pub column_name: String,
    pub original_value: Value,
}

struct LiveQuery {
    base_table: String,
    where_clause: String,
    order: Vec<(String, bool)>,
}

pub(crate) fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn qualified(name: &str) -> String {
    format!("gridloom_live.{}", quote_identifier(name))
}

fn unsupported() -> CellError {
    CellError::Sql("Live worksheets require SELECT * FROM one SQLite table, optionally with simple WHERE comparisons and ORDER BY columns. Joins, computed projections, LIMIT and OFFSET are not supported.".into())
}

fn valid_literal(expr: &Expr) -> bool {
    use sqlparser::ast::{UnaryOperator, Value as Literal};
    match expr {
        Expr::Value(v) => matches!(
            v.value,
            Literal::Number(..)
                | Literal::SingleQuotedString(_)
                | Literal::Boolean(_)
                | Literal::Null
        ),
        Expr::UnaryOp { op, expr } if matches!(op, UnaryOperator::Plus | UnaryOperator::Minus) => {
            matches!(&**expr, Expr::Value(v) if matches!(v.value, Literal::Number(..)))
        }
        _ => false,
    }
}

fn predicate(expr: &Expr) -> Result<String, CellError> {
    match expr {
        Expr::Nested(expr) => Ok(format!("({})", predicate(expr)?)),
        Expr::BinaryOp { left, op, right }
            if matches!(op, BinaryOperator::And | BinaryOperator::Or) =>
        {
            Ok(format!(
                "({} {} {})",
                predicate(left)?,
                op,
                predicate(right)?
            ))
        }
        Expr::BinaryOp { left, op, right }
            if matches!(
                op,
                BinaryOperator::Eq
                    | BinaryOperator::NotEq
                    | BinaryOperator::Lt
                    | BinaryOperator::LtEq
                    | BinaryOperator::Gt
                    | BinaryOperator::GtEq
            ) =>
        {
            match (&**left, &**right) {
                (Expr::Identifier(id), literal) if valid_literal(literal) => {
                    Ok(format!("{} {} {}", qualified(&id.value), op, literal))
                }
                (literal, Expr::Identifier(id)) if valid_literal(literal) => {
                    Ok(format!("{} {} {}", literal, op, qualified(&id.value)))
                }
                _ => Err(unsupported()),
            }
        }
        Expr::IsNull(inner) | Expr::IsNotNull(inner) => match &**inner {
            Expr::Identifier(id) => Ok(format!(
                "{} IS {}NULL",
                qualified(&id.value),
                if matches!(expr, Expr::IsNull(_)) {
                    ""
                } else {
                    "NOT "
                }
            )),
            _ => Err(unsupported()),
        },
        _ => Err(unsupported()),
    }
}

impl LiveQuery {
    fn parse(sql: &str) -> Result<Self, CellError> {
        if sql.len() > MAX_SQL_BYTES {
            return Err(unsupported());
        }
        crate::db::security::validate_read_only_sql(sql, "sqlite")?;
        let ast = Parser::parse_sql(&SQLiteDialect {}, sql).map_err(|_| unsupported())?;
        let [Statement::Query(query)] = ast.as_slice() else {
            return Err(unsupported());
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            return Err(unsupported());
        };
        let [from] = select.from.as_slice() else {
            return Err(unsupported());
        };
        let TableFactor::Table { name, .. } = &from.relation else {
            return Err(unsupported());
        };
        let Some(table) = name.0.last().and_then(|name| name.as_ident()) else {
            return Err(unsupported());
        };
        if name.0.len() > 2
            || (name.0.len() == 2
                && !name.0[0]
                    .as_ident()
                    .is_some_and(|name| name.value.eq_ignore_ascii_case("main")))
        {
            return Err(unsupported());
        }
        // Compare canonical AST text with reconstructed accepted grammar. This
        // rejects any modifiers/joins/window clauses that we do not preserve.
        let mut expected = format!("SELECT * FROM {name}");
        let where_clause = if let Some(expr) = &select.selection {
            expected.push_str(&format!(" WHERE {expr}"));
            format!(" WHERE {}", predicate(expr)?)
        } else {
            String::new()
        };
        let mut order = Vec::new();
        if let Some(by) = &query.order_by {
            let OrderByKind::Expressions(expressions) = &by.kind else {
                return Err(unsupported());
            };
            if by.interpolate.is_some() || expressions.len() > 8 {
                return Err(unsupported());
            }
            for item in expressions {
                let Expr::Identifier(id) = &item.expr else {
                    return Err(unsupported());
                };
                if item.options.nulls_first.is_some()
                    || item.with_fill.is_some()
                    || matches!(item.options.sort, Some(OrderBySort::Using(_)))
                {
                    return Err(unsupported());
                }
                order.push((
                    id.value.clone(),
                    matches!(item.options.sort, Some(OrderBySort::Desc)),
                ));
            }
            expected.push_str(&format!(" {by}"));
        }
        if query.to_string() != expected {
            return Err(unsupported());
        }
        Ok(Self {
            base_table: table.value.clone(),
            where_clause,
            order,
        })
    }

    fn order_by(&self, key: &str, reverse: bool) -> String {
        let key_part = (key.to_owned(), false);
        let list = self.order.iter().chain(
            (!self
                .order
                .iter()
                .any(|(col, _)| col.eq_ignore_ascii_case(key)))
            .then_some(&key_part),
        );
        list.map(|(column, descending)| {
            format!(
                "{} {}",
                qualified(column),
                if *descending ^ reverse { "DESC" } else { "ASC" }
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
    }

    fn order_is_key_only(&self, key: &str) -> bool {
        self.order.is_empty()
            || (self.order.len() == 1 && self.order[0].0.eq_ignore_ascii_case(key))
    }
}

impl LiveTable {
    pub(crate) fn attach(
        engine: &SqliteEngine,
        connection_name: &str,
        sql: &str,
    ) -> Result<(Self, Vec<ColumnMeta>), CellError> {
        let query = LiveQuery::parse(sql)?;
        let schema = engine.live_schema(&query.base_table)?;
        if schema.columns.len() > MAX_LIVE_COLUMNS {
            return Err(CellError::Sql(
                "Live worksheets support at most 256 source columns.".into(),
            ));
        }
        let keys = &schema.primary_keys;
        if keys.len() != 1
            || !schema
                .columns
                .iter()
                .any(|c| c.name == keys[0] && c.data_type.eq_ignore_ascii_case("INTEGER"))
        {
            return Err(CellError::Sql(
                "Live worksheets require one declared INTEGER primary key.".into(),
            ));
        }
        let pk = keys[0].clone();
        if query.order.iter().any(|(name, _)| {
            !schema
                .columns
                .iter()
                .any(|c| c.name.eq_ignore_ascii_case(name))
        }) {
            return Err(unsupported());
        }
        let (count, key_bounds) = engine.live_count(&query.base_table, &pk, &query.where_clause)?;
        let row_count = u32::try_from(count).map_err(|_| CellError::Ref)?;
        let dense_key_bounds = key_bounds.filter(|(lo, hi)| {
            query.where_clause.is_empty()
                && query.order_is_key_only(&pk)
                && i128::from(*hi) - i128::from(*lo) + 1 == i128::from(count)
        });
        Ok((
            Self {
                connection_name: connection_name.into(),
                sql: sql.into(),
                base_table: query.base_table,
                primary_key: pk,
                row_count,
                dense_key_bounds,
            },
            schema.columns,
        ))
    }

    pub(crate) fn window(
        &self,
        engine: &SqliteEngine,
        offset: u32,
        limit: u32,
        expected_columns: &[String],
    ) -> Result<Vec<Vec<Value>>, CellError> {
        let count = limit.min(self.row_count.saturating_sub(offset));
        if (count as usize).saturating_mul(expected_columns.len()) > MAX_LIVE_CELLS {
            return Err(CellError::Sql(
                "Select a smaller range to read live worksheet data.".into(),
            ));
        }
        if count == 0 {
            return Ok(vec![]);
        }
        // Reparse persisted SQL before executing. Serialized metadata is not an
        // authority to run arbitrary statements or change the underlying table.
        let query = LiveQuery::parse(&self.sql)?;
        if query.base_table != self.base_table {
            return Err(CellError::Ref);
        }
        let base = format!(
            "main.{} AS gridloom_live",
            quote_identifier(&self.base_table)
        );
        let key = &self.primary_key;
        let key_sql = qualified(key);
        let (sql, reverse) = if let Some((lo, hi)) = self.dense_key_bounds.filter(|(lo, hi)| {
            query.where_clause.is_empty()
                && query.order_is_key_only(key)
                && i128::from(*hi) - i128::from(*lo) + 1 == i128::from(self.row_count)
        }) {
            let descending = query.order.first().is_some_and(|(_, desc)| *desc);
            let row = if descending {
                hi - i64::from(offset)
            } else {
                lo + i64::from(offset)
            };
            let reach = i64::from(count - 1);
            let (lower, upper) = if descending {
                (row - reach, row)
            } else {
                (row, row + reach)
            };
            (format!("SELECT * FROM {base} WHERE {key_sql} BETWEEN {lower} AND {upper} ORDER BY {} LIMIT {count}",query.order_by(key,false)),false)
        } else {
            let tail = self.row_count - offset - count;
            let reverse = tail < offset;
            (
                format!(
                    "SELECT * FROM {base}{} ORDER BY {} LIMIT {count} OFFSET {}",
                    query.where_clause,
                    query.order_by(key, reverse),
                    if reverse { tail } else { offset }
                ),
                reverse,
            )
        };
        let result = engine.execute_read_query(&sql, &[])?;
        if !result
            .columns
            .iter()
            .map(|c| &c.name)
            .eq(expected_columns.iter())
        {
            return Err(CellError::Sql(
                "Live worksheet source columns changed; reattach the table.".into(),
            ));
        }
        let mut rows = result.rows;
        if rows.len() != count as usize {
            return Err(CellError::Sql(
                "Live worksheet source rows changed; refresh the live table to update its layout."
                    .into(),
            ));
        }
        if reverse {
            rows.reverse();
        }
        Ok(rows)
    }

    pub(crate) fn edit(&self, column: &str, key: &Value, original: &Value) -> Option<LiveEdit> {
        // JS's JSON integers aren't exact outside its safe range. Keep those
        // rows/values displayable but do not issue an unsafe optimistic edit.
        let safe = |value: &Value| !matches!(value,Value::Integer(n) if !(-JS_EXACT_INT..=JS_EXACT_INT).contains(n));
        if column.eq_ignore_ascii_case(&self.primary_key)
            || !matches!(key, Value::Integer(_))
            || !safe(key)
            || !safe(original)
        {
            return None;
        }
        Some(LiveEdit {
            connection_name: self.connection_name.clone(),
            table_name: self.base_table.clone(),
            pk_column: self.primary_key.clone(),
            pk_value: key.clone(),
            column_name: column.into(),
            original_value: original.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::CellCoord;
    use crate::virtual_view::ViewportRequest;
    use crate::Workbook;

    fn file_setup() -> (tempfile::TempDir, std::path::PathBuf, rusqlite::Connection) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("live.sqlite");
        let external = rusqlite::Connection::open(&path).unwrap();
        external
            .execute_batch(
                "CREATE TABLE invoices(id INTEGER PRIMARY KEY, customer TEXT, amount INTEGER);
            INSERT INTO invoices VALUES (1,'same',10),(2,'second',20),(3,'same',30);",
            )
            .unwrap();
        (dir, path, external)
    }

    #[test]
    fn live_schema_and_query_subset_scoped_sort_and_filter() {
        let (_dir, path, external) = file_setup();
        external.execute_batch("CREATE VIEW v AS SELECT * FROM invoices; CREATE TABLE composite(x INTEGER,y INTEGER,PRIMARY KEY(x,y));").unwrap();
        let engine = SqliteEngine::open(path).unwrap();
        for sql in [
            "SELECT id FROM invoices",
            "SELECT * FROM invoices LIMIT 5",
            "SELECT * FROM invoices i",
            "SELECT * FROM invoices WHERE random() > 4",
            "SELECT * FROM invoices WHERE bad = 4",
            "SELECT * FROM v",
            "SELECT * FROM composite",
            "SELECT * FROM invoices ORDER BY bad",
            "SELECT * FROM invoices WHERE id IN (SELECT id FROM invoices)",
        ] {
            assert!(LiveTable::attach(&engine, "Live", sql).is_err(), "{sql}");
        }
        let (source, columns) = LiveTable::attach(
            &engine,
            "Live",
            "select * from invoices where customer = 'same' ORDER BY amount DESC;",
        )
        .unwrap();
        let cols = columns.into_iter().map(|c| c.name).collect::<Vec<_>>();
        assert_eq!(source.row_count, 2);
        assert!(matches!(
            source.window(&engine, 0, 1, &cols).unwrap()[0][0],
            Value::Integer(3)
        ));
        assert!(matches!(
            source.window(&engine, 1, 1, &cols).unwrap()[0][0],
            Value::Integer(1)
        ));
        let (source, _) = LiveTable::attach(
            &engine,
            "Live",
            "SELECT * FROM invoices WHERE customer = 'x'' OR 1=1 --'",
        )
        .unwrap();
        assert_eq!(source.row_count, 0);
    }

    #[test]
    fn live_refresh_collisions_roundtrip_formula_guard_and_edit_overlay() {
        use crate::db::input::SqlEditInput;
        let (dir, path, external) = file_setup();
        let mut book = Workbook::new();
        book.add_sqlite_connection("Live", &path).unwrap();
        let origin = CellCoord::new(1, 1);
        book.insert_live_table(
            "Sheet1",
            "LiveInvoices",
            "Live",
            "SELECT * FROM invoices",
            origin,
        )
        .unwrap();
        let view = |book: &Workbook| {
            book.get_viewport(&ViewportRequest {
                sheet_name: "Sheet1".into(),
                start_row: 1,
                start_col: 1,
                end_row: 8,
                end_col: 4,
                overscan: 0,
            })
            .unwrap()
        };
        let edit = view(&book).cells["D3"].live_edit.clone().unwrap();
        assert!(view(&book).cells["B3"].live_edit.is_none());
        assert!(matches!(edit.pk_value, Value::Integer(1)));
        book.stage_sql_edit(
            edit.connection_name,
            edit.table_name,
            vec![edit.pk_column],
            vec![edit.pk_value],
            edit.column_name,
            edit.original_value,
            SqlEditInput::Raw("11".into()),
        )
        .unwrap();
        let view_data = view(&book);
        assert_eq!(view_data.cells["D3"].display_text, "11");
        assert!(matches!(
            view_data.cells["D3"]
                .live_edit
                .as_ref()
                .unwrap()
                .original_value,
            Value::Integer(10)
        ));
        book.apply_writeback("Live").unwrap();
        assert_eq!(
            external
                .query_row("SELECT amount FROM invoices WHERE id=1", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            11
        );
        book.set_cell("Sheet1", CellCoord::new(5, 1), "obstacle".into())
            .unwrap();
        external
            .execute("INSERT INTO invoices VALUES(4,'new',40)", [])
            .unwrap();
        assert!(matches!(
            book.refresh_live_table("Sheet1", "LiveInvoices"),
            Err(CellError::Spill)
        ));
        assert_eq!(
            book.sheets["Sheet1"].tables["LiveInvoices"].range.end_row,
            4
        );
        book.set_cell("Sheet1", CellCoord::new(5, 1), "".into())
            .unwrap();
        assert_eq!(
            book.refresh_live_table("Sheet1", "LiveInvoices")
                .unwrap()
                .range
                .end_row,
            5
        );
        assert_eq!(view(&book).cells["D6"].display_text, "40");
        book.set_cell("Sheet1", CellCoord::new(0, 0), "=D6".into())
            .unwrap();
        assert_eq!(
            book.sheets["Sheet1"].cells["A1"]
                .evaluated_value
                .to_string_repr(),
            "#SQL!"
        );
        let saved = dir.path().join("test.tay");
        book.save_to_file(&saved).unwrap();
        let loaded = Workbook::open_from_file(saved).unwrap();
        assert!(loaded.named_sources.is_empty());
        assert_eq!(view(&loaded).cells["C6"].display_text, "new");
    }

    #[test]
    #[ignore = "10M SQLite integration fixture; exercises indexed dense-table tail paging"]
    fn ten_million_actual_sql_rows() {
        let (_dir, path, external) = file_setup();
        external.execute_batch("WITH RECURSIVE ids(i) AS (VALUES(4) UNION ALL SELECT i+1 FROM ids WHERE i<10000000)
            INSERT INTO invoices SELECT i,'bulk',i*10 FROM ids").unwrap();
        let engine = SqliteEngine::open(path).unwrap();
        let (table, cols) = LiveTable::attach(&engine, "Live", "SELECT * FROM invoices").unwrap();
        assert_eq!(table.row_count, 10_000_000);
        assert_eq!(table.dense_key_bounds, Some((1, 10_000_000)));
        let cols = cols.into_iter().map(|c| c.name).collect::<Vec<_>>();
        let tail = table.window(&engine, 9_999_998, 2, &cols).unwrap();
        assert!(matches!(tail[0][0], Value::Integer(9_999_999)));
        assert!(matches!(tail[1][0], Value::Integer(10_000_000)));
    }
}
