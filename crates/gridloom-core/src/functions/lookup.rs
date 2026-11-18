use crate::error::CellError;
use crate::value::Value;
use rust_decimal::Decimal;
use std::collections::HashSet;

fn to_matrix(val: &Value) -> Vec<Vec<Value>> {
    match val {
        Value::Array(rows) => rows.clone(),
        other => vec![vec![other.clone()]],
    }
}

pub fn index_fn(args: &[Value]) -> Result<Value, CellError> {
    if args.len() < 2 {
        return Err(CellError::Value);
    }
    let matrix = to_matrix(&args[0]);
    let row_num = args[1].to_i64()?; // 1-based in Excel
    let col_num = if args.len() > 2 {
        args[2].to_i64()?
    } else {
        1
    };

    if row_num < 0 || col_num < 0 {
        return Err(CellError::Ref);
    }

    if row_num == 0 && col_num == 0 {
        return Ok(Value::Array(matrix));
    }

    if row_num == 0 {
        // Return entire column
        let c_idx = (col_num - 1) as usize;
        let mut col_vals = Vec::new();
        for r in &matrix {
            let cell = r.get(c_idx).cloned().unwrap_or(Value::Blank);
            col_vals.push(vec![cell]);
        }
        return Ok(Value::Array(col_vals));
    }

    if col_num == 0 {
        // Return entire row
        let r_idx = (row_num - 1) as usize;
        if let Some(r) = matrix.get(r_idx) {
            return Ok(Value::Array(vec![r.clone()]));
        } else {
            return Err(CellError::Ref);
        }
    }

    let r_idx = (row_num - 1) as usize;
    let c_idx = (col_num - 1) as usize;

    if let Some(r) = matrix.get(r_idx) {
        if let Some(cell) = r.get(c_idx) {
            Ok(cell.clone())
        } else {
            Err(CellError::Ref)
        }
    } else {
        Err(CellError::Ref)
    }
}

pub fn match_fn(args: &[Value]) -> Result<Value, CellError> {
    if args.len() < 2 {
        return Err(CellError::Value);
    }
    let lookup_value = &args[0];
    let matrix = to_matrix(&args[1]);
    let match_type = if args.len() > 2 {
        args[2].to_i64()?
    } else {
        1
    };

    let flat: Vec<Value> = matrix.into_iter().flatten().collect();

    if match_type == 0 {
        // Exact match
        for (i, item) in flat.iter().enumerate() {
            if item == lookup_value {
                return Ok(Value::Integer((i + 1) as i64));
            }
        }
        Err(CellError::NA)
    } else if match_type > 0 {
        // Less than or equal to, assuming sorted ascending
        let mut last_idx = None;
        for (i, item) in flat.iter().enumerate() {
            if item <= lookup_value {
                last_idx = Some(i + 1);
            } else {
                break;
            }
        }
        last_idx.map(|idx| Value::Integer(idx as i64)).ok_or(CellError::NA)
    } else {
        // Greater than or equal to, assuming sorted descending
        let mut last_idx = None;
        for (i, item) in flat.iter().enumerate() {
            if item >= lookup_value {
                last_idx = Some(i + 1);
            } else {
                break;
            }
        }
        last_idx.map(|idx| Value::Integer(idx as i64)).ok_or(CellError::NA)
    }
}

pub fn vlookup(args: &[Value]) -> Result<Value, CellError> {
    if args.len() < 3 {
        return Err(CellError::Value);
    }
    let lookup_val = &args[0];
    let table = to_matrix(&args[1]);
    let col_index = args[2].to_i64()?; // 1-based
    let range_lookup = if args.len() > 3 {
        args[3].to_bool()?
    } else {
        true
    };

    if col_index < 1 {
        return Err(CellError::Value);
    }
    let c_idx = (col_index - 1) as usize;
    if table.first().is_some_and(|row| c_idx >= row.len()) {
        return Err(CellError::Ref);
    }

    if !range_lookup {
        // Exact match
        for row in &table {
            if let Some(first) = row.first() {
                if first == lookup_val {
                    return row.get(c_idx).cloned().ok_or(CellError::Ref);
                }
            }
        }
        Err(CellError::NA)
    } else {
        // Approximate match (largest value less than or equal)
        let mut best_row = None;
        for row in &table {
            if let Some(first) = row.first() {
                if first <= lookup_val {
                    best_row = Some(row);
                } else {
                    break;
                }
            }
        }
        if let Some(row) = best_row {
            row.get(c_idx).cloned().ok_or(CellError::Ref)
        } else {
            Err(CellError::NA)
        }
    }
}

// XLOOKUP never widens exact accounting values to f64 for comparison.
// A mixed Float comparison is permitted only when its exact binary rational
// has an exactly representable Decimal (not merely an f64 roundtrip).
fn exact_float_decimal(value: f64) -> Result<Decimal, CellError> {
    if !value.is_finite() {
        return Err(CellError::Num);
    }
    if value == 0.0 {
        return Ok(Decimal::ZERO);
    }
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let mut mantissa = (bits & ((1u64 << 52) - 1)) as i128;
    let mut power = if exponent == 0 {
        -1074
    } else {
        exponent - 1023 - 52
    };
    if exponent != 0 {
        mantissa += 1i128 << 52;
    }
    while power < 0 && mantissa % 2 == 0 {
        mantissa /= 2;
        power += 1;
    }
    let (mantissa, scale) = if power >= 0 {
        (
            mantissa
                .checked_mul(2i128.checked_pow(power as u32).ok_or(CellError::Num)?)
                .ok_or(CellError::Num)?,
            0,
        )
    } else {
        let scale = (-power) as u32;
        if scale > 28 {
            return Err(CellError::Num);
        }
        (
            mantissa
                .checked_mul(5i128.checked_pow(scale).ok_or(CellError::Num)?)
                .ok_or(CellError::Num)?,
            scale,
        )
    };
    Decimal::try_from_i128_with_scale(if bits >> 63 != 0 { -mantissa } else { mantissa }, scale)
        .map_err(|_| CellError::Num)
}
fn xlookup_compare(a: &Value, b: &Value) -> Result<Option<std::cmp::Ordering>, CellError> {
    use std::cmp::Ordering;
    for value in [a, b] {
        match value {
            Value::Error(e) => return Err(e.clone()),
            Value::Float(n) if !n.is_finite() => return Err(CellError::Num),
            Value::Array(_) => return Err(CellError::Value),
            _ => {}
        }
    }
    match (a, b) {
        (Value::Float(a), Value::Integer(b)) => {
            Ok(exact_float_decimal(*a)?.partial_cmp(&Decimal::from(*b)))
        }
        (Value::Integer(a), Value::Float(b)) => {
            Ok(Decimal::from(*a).partial_cmp(&exact_float_decimal(*b)?))
        }
        (Value::Float(a), Value::Decimal(b)) => Ok(exact_float_decimal(*a)?.partial_cmp(b)),
        (Value::Decimal(a), Value::Float(b)) => Ok(a.partial_cmp(&exact_float_decimal(*b)?)),
        (Value::Blank, Value::Blank)
        | (Value::Null, Value::Null)
        | (Value::EmptyText, Value::EmptyText) => Ok(Some(Ordering::Equal)),
        _ => Ok(a.partial_cmp(b)),
    }
}
fn xlookup_mode(value: &Value, allowed: &[i64]) -> Result<i64, CellError> {
    if let Value::Error(e) = value {
        return Err(e.clone());
    }
    // Compare against the small exact allowed integers rather than truncating decimals.
    for mode in allowed {
        let matched = match value {
            Value::Integer(n) => n == mode,
            Value::Decimal(n) => *n == Decimal::from(*mode),
            Value::Float(n) => n.is_finite() && *n == *mode as f64,
            _ => false,
        };
        if matched {
            return Ok(*mode);
        }
    }
    Err(CellError::Value)
}
pub fn xlookup(args: &[Value]) -> Result<Value, CellError> {
    use std::cmp::Ordering;
    if !(3..=6).contains(&args.len()) {
        return Err(CellError::Value);
    }
    let lookup_value = &args[0];
    xlookup_compare(lookup_value, lookup_value)?;
    for value in [&args[1], &args[2]] {
        if let Value::Error(e) = value {
            return Err(e.clone());
        }
    }
    let lookup = to_matrix(&args[1]);
    let returns = to_matrix(&args[2]);
    let lookup_cols = lookup.first().map_or(0, Vec::len);
    let return_cols = returns.first().map_or(0, Vec::len);
    if lookup.is_empty()
        || lookup_cols == 0
        || returns.is_empty()
        || return_cols == 0
        || lookup.iter().any(|r| r.len() != lookup_cols)
        || returns.iter().any(|r| r.len() != return_cols)
        || (lookup.len() > 1 && lookup_cols > 1)
    {
        return Err(CellError::Value);
    }
    // A scalar lookup can project either a one-row or one-column result.
    let horizontal = lookup_cols > 1 || (lookup.len() == 1 && returns.len() > 1);
    if (horizontal && return_cols != lookup_cols) || (!horizontal && returns.len() != lookup.len())
    {
        return Err(CellError::Value);
    }
    let values: Vec<&Value> = lookup.iter().flatten().collect();
    let match_mode = args
        .get(4)
        .map(|v| xlookup_mode(v, &[-1, 0, 1]))
        .transpose()?
        .unwrap_or(0);
    let search_mode = args
        .get(5)
        .map(|v| xlookup_mode(v, &[-1, 1]))
        .transpose()?
        .unwrap_or(1);
    let indices: Vec<usize> = if search_mode == -1 {
        (0..values.len()).rev().collect()
    } else {
        (0..values.len()).collect()
    };
    let mut found = None;
    for &index in &indices {
        if xlookup_compare(values[index], lookup_value)? == Some(Ordering::Equal) {
            found = Some(index);
            break;
        }
    }
    if found.is_none() && match_mode != 0 {
        for &index in &indices {
            let order = xlookup_compare(values[index], lookup_value)?;
            let eligible = if match_mode == -1 {
                order == Some(Ordering::Less)
            } else {
                order == Some(Ordering::Greater)
            };
            if eligible {
                let better = match found {
                    None => true,
                    Some(best) => {
                        xlookup_compare(values[index], values[best])?
                            == Some(if match_mode == -1 {
                                Ordering::Greater
                            } else {
                                Ordering::Less
                            })
                    }
                };
                if better {
                    found = Some(index);
                }
            }
        }
    }
    let Some(index) = found else {
        return args.get(3).cloned().ok_or(CellError::NA);
    };
    if horizontal {
        if returns.len() == 1 {
            Ok(returns[0][index].clone())
        } else {
            Ok(Value::Array(
                returns.iter().map(|row| vec![row[index].clone()]).collect(),
            ))
        }
    } else if return_cols == 1 {
        Ok(returns[index][0].clone())
    } else {
        Ok(Value::Array(vec![returns[index].clone()]))
    }
}

pub fn filter_fn(args: &[Value]) -> Result<Value, CellError> {
    if args.len() < 2 {
        return Err(CellError::Value);
    }
    let array = to_matrix(&args[0]);
    let include_matrix = to_matrix(&args[1]);
    let include_flat: Vec<bool> = include_matrix
        .into_iter()
        .flatten()
        .map(|v| v.to_bool().unwrap_or(false))
        .collect();

    if include_flat.len() != array.len() {
        return Err(CellError::Value);
    }

    let mut result_rows = Vec::new();
    for (i, row) in array.into_iter().enumerate() {
        if include_flat[i] {
            result_rows.push(row);
        }
    }

    if result_rows.is_empty() {
        if args.len() > 2 {
            Ok(args[2].clone())
        } else {
            Err(CellError::NA)
        }
    } else {
        Ok(Value::Array(result_rows))
    }
}

pub fn sort_fn(args: &[Value]) -> Result<Value, CellError> {
    if args.is_empty() {
        return Err(CellError::Value);
    }
    let mut matrix = to_matrix(&args[0]);
    let sort_index = if args.len() > 1 {
        args[1].to_i64()?
    } else {
        1
    };
    let sort_order = if args.len() > 2 {
        args[2].to_i64()?
    } else {
        1
    }; // 1 = asc, -1 = desc

    if sort_index < 1 {
        return Err(CellError::Value);
    }
    let col_idx = (sort_index - 1) as usize;

    matrix.sort_by(|a, b| {
        let val_a = a.get(col_idx).unwrap_or(&Value::Blank);
        let val_b = b.get(col_idx).unwrap_or(&Value::Blank);
        if sort_order >= 0 {
            val_a.partial_cmp(val_b).unwrap_or(std::cmp::Ordering::Equal)
        } else {
            val_b.partial_cmp(val_a).unwrap_or(std::cmp::Ordering::Equal)
        }
    });

    Ok(Value::Array(matrix))
}

pub fn unique_fn(args: &[Value]) -> Result<Value, CellError> {
    if args.is_empty() {
        return Err(CellError::Value);
    }
    let matrix = to_matrix(&args[0]);
    let mut seen = HashSet::new();
    let mut out_rows = Vec::new();

    for row in matrix {
        let key: Vec<String> = row.iter().map(|v| v.to_string_repr()).collect();
        if seen.insert(key) {
            out_rows.push(row);
        }
    }

    Ok(Value::Array(out_rows))
}

pub fn sequence_fn(args: &[Value]) -> Result<Value, CellError> {
    if args.is_empty() {
        return Err(CellError::Value);
    }
    let rows = args[0].to_i64()?;
    let cols = if args.len() > 1 {
        args[1].to_i64()?
    } else {
        1
    };
    let start = if args.len() > 2 {
        args[2].to_decimal()?
    } else {
        Decimal::ONE
    };
    let step = if args.len() > 3 {
        args[3].to_decimal()?
    } else {
        Decimal::ONE
    };

    if rows < 1 || cols < 1 {
        return Err(CellError::Value);
    }

    let mut current = start;
    let mut matrix = Vec::new();
    for _ in 0..rows {
        let mut row_vals = Vec::new();
        for _ in 0..cols {
            row_vals.push(Value::Decimal(current));
            current += step;
        }
        matrix.push(row_vals);
    }

    Ok(Value::Array(matrix))
}

pub fn transpose_fn(args: &[Value]) -> Result<Value, CellError> {
    if args.is_empty() {
        return Err(CellError::Value);
    }
    let matrix = to_matrix(&args[0]);
    if matrix.is_empty() {
        return Ok(Value::Array(vec![]));
    }
    let row_count = matrix.len();
    let col_count = matrix[0].len();

    let mut transposed = vec![vec![Value::Blank; row_count]; col_count];
    for r in 0..row_count {
        for c in 0..col_count {
            transposed[c][r] = matrix[r][c].clone();
        }
    }

    Ok(Value::Array(transposed))
}
