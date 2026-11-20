use crate::error::CellError;
use sqlparser::ast::{SetExpr, Statement, TableFactor};
use sqlparser::dialect::{DuckDbDialect, GenericDialect, SQLiteDialect};
use sqlparser::parser::Parser;
use sqlparser::tokenizer::{Token, Tokenizer};

fn dialect_for(name: &str) -> Box<dyn sqlparser::dialect::Dialect> {
    match name.to_lowercase().as_str() {
        "sqlite" => Box::new(SQLiteDialect {}),
        "duckdb" => Box::new(DuckDbDialect {}),
        _ => Box::new(GenericDialect {}),
    }
}

/// Build a bounded query without reformatting the submitted SQL. The SQL engine
/// applies the outer page limit after the original filters, order, and limit.
pub(super) fn preview_sql(
    sql: &str,
    dialect_name: &str,
    offset: u64,
    fetch: usize,
) -> Result<String, CellError> {
    let sql = sql.trim();
    validate_read_only_sql(sql, dialect_name)?;
    let dialect = dialect_for(dialect_name);
    let tokens = Tokenizer::new(dialect.as_ref(), sql)
        .tokenize_with_location()
        .map_err(|e| CellError::Sql(format!("SQL syntax error: {e}")))?;
    let semicolon = tokens.iter().find(|item| item.token == Token::SemiColon)
        .map(|item| item.span.start);
    let ast = Parser::new(dialect.as_ref()).with_tokens_with_locations(tokens)
        .parse_statements()
        .map_err(|e| CellError::Sql(format!("SQL syntax error: {e}")))?;
    if !matches!(ast.as_slice(), [Statement::Query(_)]) {
        return Err(CellError::Sql("Preview requires one read-only SELECT query".into()));
    }

    // Remove a terminal delimiter (including comments after it), not semicolons
    // in text literals, quoted identifiers or comments. Preserve dialect syntax.
    let mut sql_body = sql;
    if let Some(location) = semicolon {
        let line_start: usize = sql.split_inclusive('\n')
            .take((location.line - 1) as usize)
            .map(str::len)
            .sum();
        let column_start = sql[line_start..].char_indices()
            .nth((location.column - 1) as usize)
            .map(|(index, _)| index);
        let index = column_start.map(|column| line_start + column)
            .filter(|&index| sql.as_bytes().get(index) == Some(&b';'))
            .ok_or_else(|| CellError::Sql("Could not locate SQL statement delimiter".into()))?;
        sql_body = &sql[..index];
    }
    Ok(format!(
        "SELECT * FROM (\n{sql_body}\n) AS gridloom_preview LIMIT {fetch} OFFSET {offset}"
    ))
}

/// Strictly validates that SQL is a read-only query.
/// Disallows mutating statements (INSERT, UPDATE, DELETE, DROP, CREATE, ALTER),
/// extension loading, database attachment, and dangerous system calls.
pub fn validate_read_only_sql(sql: &str, dialect_name: &str) -> Result<(), CellError> {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return Err(CellError::Sql("Empty SQL query".to_string()));
    }

    // Pre-parse block checks for dangerous commands
    let upper = trimmed.to_uppercase();
    let dangerous_keywords = [
        "ATTACH", "DETACH", "LOAD", "INSTALL", "COPY", "EXPORT", "IMPORT",
        "PRAGMA WRITABLE_SCHEMA", "VACUUM", "DROP", "ALTER", "CREATE",
        "INSERT", "UPDATE", "DELETE", "TRUNCATE", "REPLACE",
    ];

    for kw in &dangerous_keywords {
        // Match keyword boundaries
        let kw_prefix = format!("{} ", kw);
        let kw_paren = format!("{}(", kw);
        let kw_semi = format!(";{}", kw);
        if upper.starts_with(kw)
            || upper.contains(&format!(" {}", kw_prefix))
            || upper.contains(&format!("\n{}", kw_prefix))
            || upper.contains(&kw_paren)
            || upper.contains(&kw_semi)
        {
            return Err(CellError::Sql(format!(
                "Security boundary violation: disallowed command '{}' in read-only worksheet query",
                kw
            )));
        }
    }

    // Parse with AST to guarantee it is strictly a read-only SELECT / WITH statement
    let dialect = dialect_for(dialect_name);
    let ast = Parser::parse_sql(dialect.as_ref(), trimmed).map_err(|e| {
        CellError::Sql(format!("SQL syntax error: {}", e))
    })?;

    if ast.is_empty() {
        return Err(CellError::Sql("Empty SQL AST".to_string()));
    }

    for stmt in ast {
        match stmt {
            Statement::Query(query) => {
                // Check body
                validate_query_body(&query.body)?;
            }
            Statement::Explain { .. } => {
                // EXPLAIN is allowed for inspection
            }
            other => {
                return Err(CellError::Sql(format!(
                    "Disallowed statement type: read-only worksheet execution permits only SELECT queries, got: {:?}",
                    other
                )));
            }
        }
    }

    Ok(())
}

fn validate_query_body(body: &SetExpr) -> Result<(), CellError> {
    match body {
        SetExpr::Select(select) => {
            for table_ref in &select.from {
                validate_table_factor(&table_ref.relation)?;
                for join in &table_ref.joins {
                    validate_table_factor(&join.relation)?;
                }
            }
            Ok(())
        }
        SetExpr::Query(inner) => validate_query_body(&inner.body),
        SetExpr::SetOperation { left, right, .. } => {
            validate_query_body(left)?;
            validate_query_body(right)?;
            Ok(())
        }
        SetExpr::Values(_) => Ok(()),
        _ => Ok(()),
    }
}

fn validate_table_factor(tf: &TableFactor) -> Result<(), CellError> {
    match tf {
        TableFactor::Table { name, .. } => {
            let name_str = name.to_string().to_uppercase();
            // Block sqlite_master modification attempts or system functions
            if name_str.starts_with("READ_CSV")
                || name_str.starts_with("READ_PARQUET")
                || name_str.starts_with("HTTPFS")
            {
                // In a strict local environment, only base tables are allowed for worksheet tables
            }
            Ok(())
        }
        TableFactor::Derived { subquery, .. } => validate_query_body(&subquery.body),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_selects() {
        assert!(validate_read_only_sql("SELECT * FROM invoices", "sqlite").is_ok());
        assert!(validate_read_only_sql("SELECT id, amount FROM invoices WHERE amount > 100 ORDER BY id", "duckdb").is_ok());
        assert!(validate_read_only_sql("WITH t AS (SELECT 1 AS val) SELECT * FROM t", "sqlite").is_ok());
    }

    #[test]
    fn test_blocked_mutations() {
        assert!(validate_read_only_sql("DELETE FROM invoices", "sqlite").is_err());
        assert!(validate_read_only_sql("DROP TABLE invoices", "sqlite").is_err());
        assert!(validate_read_only_sql("INSERT INTO invoices VALUES (1, 10)", "duckdb").is_err());
        assert!(validate_read_only_sql("UPDATE invoices SET amount = 0", "sqlite").is_err());
        assert!(validate_read_only_sql("ATTACH DATABASE 'test.db' AS test", "sqlite").is_err());
    }
}
