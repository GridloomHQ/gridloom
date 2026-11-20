use crate::error::CellError;
use crate::value::Value;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotAggregation {
    Count,
    Sum,
    Average,
    Min,
    Max,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PivotDefinition {
    pub name: String,
    pub source_table: String,
    pub row_fields: Vec<String>,
    pub col_fields: Vec<String>,
    pub value_field: String,
    pub aggregation: PivotAggregation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PivotResult {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

pub fn compute_pivot(
    def: &PivotDefinition,
    column_names: &[String],
    source_rows: &[Vec<Value>],
) -> Result<PivotResult, CellError> {
    let mut field_indices = HashMap::new();
    for (i, name) in column_names.iter().enumerate() {
        field_indices.insert(name.to_lowercase(), i);
    }

    let val_idx = *field_indices
        .get(&def.value_field.to_lowercase())
        .ok_or(CellError::Value)?;

    let resolve_fields = |fields: &[String]| -> Result<Vec<usize>, CellError> {
        fields
            .iter()
            .map(|f| {
                field_indices
                    .get(&f.to_lowercase())
                    .copied()
                    .ok_or(CellError::Value)
            })
            .collect()
    };
    let row_indices = resolve_fields(&def.row_fields)?;
    let col_indices = resolve_fields(&def.col_fields)?;
    let key_for = |row: &[Value], indices: &[usize]| {
        indices
            .iter()
            .map(|&i| row.get(i).unwrap_or(&Value::Blank).to_string_repr())
            .collect::<Vec<_>>()
    };

    let mut groups: BTreeMap<Vec<String>, BTreeMap<Vec<String>, Vec<&Value>>> = BTreeMap::new();
    let mut column_keys = BTreeSet::new();
    if col_indices.is_empty() {
        column_keys.insert(Vec::new());
    }
    for row in source_rows {
        let key = key_for(row, &col_indices);
        column_keys.insert(key.clone());
        groups
            .entry(key_for(row, &row_indices))
            .or_default()
            .entry(key)
            .or_default()
            .push(row.get(val_idx).unwrap_or(&Value::Blank));
    }
    // Diagonal/sparse groups can still produce an oversized dense result.
    let result_width = def.row_fields.len().saturating_add(column_keys.len());
    if groups.len().saturating_mul(result_width) > 1_000_000 {
        return Err(CellError::Num);
    }

    let caption = format!("{:?}({})", def.aggregation, def.value_field);
    let mut headers = def.row_fields.clone();
    headers.extend(column_keys.iter().map(|key| {
        if key.is_empty() {
            caption.clone()
        } else {
            def.col_fields
                .iter()
                .zip(key)
                .map(|(field, value)| format!("{field}={value}"))
                .collect::<Vec<_>>()
                .join(" · ")
        }
    }));

    let mut rows = Vec::with_capacity(groups.len());
    for (key, mut columns) in groups {
        let mut row: Vec<Value> = key.into_iter().map(Value::Text).collect();
        for column in &column_keys {
            row.push(match columns.remove(column) {
                Some(values) => aggregate(def.aggregation, &values)?,
                None => Value::Blank,
            });
        }
        rows.push(row);
    }
    Ok(PivotResult { headers, rows })
}

fn aggregate(aggregation: PivotAggregation, vals: &[&Value]) -> Result<Value, CellError> {
    Ok(match aggregation {
        PivotAggregation::Count => {
            let count = vals.iter().filter(|v| !v.is_blank()).count();
            Value::Integer(count as i64)
        }
        PivotAggregation::Sum => {
            let mut sum = Decimal::ZERO;
            for v in vals {
                if let Ok(d) = v.to_decimal() {
                    sum = sum.checked_add(d).ok_or(CellError::Num)?;
                }
            }
            Value::Decimal(sum)
        }
        PivotAggregation::Average => {
            let mut sum = Decimal::ZERO;
            let mut count = 0;
            for v in vals {
                if let Ok(d) = v.to_decimal() {
                    sum = sum.checked_add(d).ok_or(CellError::Num)?;
                    count += 1;
                }
            }
            if count > 0 {
                Value::Decimal(sum / Decimal::from(count))
            } else {
                Value::Error(CellError::Div0)
            }
        }
        PivotAggregation::Min => {
            let mut min_val: Option<Decimal> = None;
            for v in vals {
                if let Ok(d) = v.to_decimal() {
                    min_val = Some(min_val.map_or(d, |m| m.min(d)));
                }
            }
            min_val.map(Value::Decimal).unwrap_or(Value::Blank)
        }
        PivotAggregation::Max => {
            let mut max_val: Option<Decimal> = None;
            for v in vals {
                if let Ok(d) = v.to_decimal() {
                    max_val = Some(max_val.map_or(d, |m| m.max(d)));
                }
            }
            max_val.map(Value::Decimal).unwrap_or(Value::Blank)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_by_columns_and_preserves_missing_intersections() {
        let def = PivotDefinition {
            name: "Test".into(),
            source_table: "Ledger".into(),
            row_fields: vec!["Region".into()],
            col_fields: vec!["Year".into(), "Quarter".into()],
            value_field: "Amount".into(),
            aggregation: PivotAggregation::Sum,
        };
        let columns = ["Region", "Year", "Quarter", "Amount"].map(String::from);
        let row = |region: &str, q: i64, amount: i64| {
            vec![
                Value::Text(region.into()),
                Value::Integer(2026),
                Value::Integer(q),
                Value::Integer(amount),
            ]
        };
        let result = compute_pivot(
            &def,
            &columns,
            &[row("North", 1, 12), row("North", 1, -2), row("South", 2, 0)],
        )
        .unwrap();
        assert_eq!(
            result.headers,
            vec![
                String::from("Region"),
                String::from("Year=2026 · Quarter=1"),
                String::from("Year=2026 · Quarter=2"),
            ]
        );
        assert!(matches!(result.rows[0][1], Value::Decimal(value) if value == Decimal::from(10)));
        assert!(matches!(result.rows[0][2], Value::Blank));
        assert!(matches!(result.rows[1][1], Value::Blank));
        assert!(matches!(result.rows[1][2], Value::Decimal(value) if value == Decimal::ZERO));
    }

    #[test]
    fn checked_aggregation_reports_overflow_without_panic() {
        let def = PivotDefinition {
            name: "Test".into(),
            source_table: "Ledger".into(),
            row_fields: vec![],
            col_fields: vec![],
            value_field: "Amount".into(),
            aggregation: PivotAggregation::Sum,
        };
        let rows = [vec![Value::Decimal(Decimal::MAX)], vec![Value::Decimal(Decimal::ONE)]];
        assert!(matches!(compute_pivot(&def, &["Amount".into()], &rows), Err(CellError::Num)));
    }
}
