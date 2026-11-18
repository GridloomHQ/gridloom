use crate::error::CellError;
use crate::value::Value;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;

/// Helper to flatten nested values and arrays into a flat list of values
pub fn flatten_values(args: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();
    for val in args {
        match val {
            Value::Array(rows) => {
                for row in rows {
                    for cell in row {
                        out.push(cell.clone());
                    }
                }
            }
            other => out.push(other.clone()),
        }
    }
    out
}

pub fn sum(args: &[Value]) -> Result<Value, CellError> {
    let mut total = Decimal::ZERO;
    let flat = flatten_values(args);
    for val in flat {
        match val {
            Value::Integer(i) => {
                total = total.checked_add(Decimal::from(i)).ok_or(CellError::Num)?
            }
            Value::Decimal(d) => total = total.checked_add(d).ok_or(CellError::Num)?,
            Value::Float(f) => {
                if let Some(d) = Decimal::from_f64(f) {
                    total = total.checked_add(d).ok_or(CellError::Num)?;
                }
            }
            Value::Error(e) => return Err(e),
            // Text and blank cells in ranges are ignored in Excel SUM
            _ => {}
        }
    }
    Ok(Value::Decimal(total))
}

pub fn product(args: &[Value]) -> Result<Value, CellError> {
    let flat = flatten_values(args);
    let mut prod = Decimal::ONE;
    let mut count = 0;
    for val in flat {
        match val {
            Value::Integer(i) => {
                prod = prod.checked_mul(Decimal::from(i)).ok_or(CellError::Num)?;
                count += 1;
            }
            Value::Decimal(d) => {
                prod = prod.checked_mul(d).ok_or(CellError::Num)?;
                count += 1;
            }
            Value::Float(f) => {
                let d = Decimal::from_f64(f).ok_or(CellError::Num)?;
                prod = prod.checked_mul(d).ok_or(CellError::Num)?;
                count += 1;
            }
            Value::Error(e) => return Err(e),
            _ => {}
        }
    }
    if count == 0 {
        Ok(Value::Decimal(Decimal::ZERO))
    } else {
        Ok(Value::Decimal(prod))
    }
}

pub fn abs(args: &[Value]) -> Result<Value, CellError> {
    if args.is_empty() {
        return Err(CellError::Value);
    }
    let d = args[0].to_decimal()?;
    Ok(Value::Decimal(d.abs()))
}

pub fn sign(args: &[Value]) -> Result<Value, CellError> {
    if args.is_empty() {
        return Err(CellError::Value);
    }
    let d = args[0].to_decimal()?;
    if d > Decimal::ZERO {
        Ok(Value::Integer(1))
    } else if d < Decimal::ZERO {
        Ok(Value::Integer(-1))
    } else {
        Ok(Value::Integer(0))
    }
}

pub fn mod_fn(args: &[Value]) -> Result<Value, CellError> {
    if args.len() < 2 {
        return Err(CellError::Value);
    }
    let n = args[0].to_decimal()?;
    let d = args[1].to_decimal()?;
    if d.is_zero() {
        return Err(CellError::Div0);
    }
    // Truncating remainder avoids an overflowing or precision-losing quotient.
    let remainder = n.checked_rem(d).ok_or(CellError::Num)?;
    if remainder.is_zero() || remainder.is_sign_negative() == d.is_sign_negative() {
        return Ok(Value::Decimal(remainder));
    }
    // MOD follows the divisor's sign. Decimal checked_add can silently round this
    // correction; align exact coefficients instead and reject an unrepresentable sum.
    let remainder = remainder.normalize();
    let divisor = d.normalize();
    let scale = remainder.scale().max(divisor.scale());
    let align = |value: Decimal| {
        value
            .mantissa()
            .checked_mul(10i128.pow(scale - value.scale()))
            .ok_or(CellError::Num)
    };
    let mut coefficient = align(remainder)?
        .checked_add(align(divisor)?)
        .ok_or(CellError::Num)?;
    let mut scale = scale;
    while scale > 0 && coefficient % 10 == 0 {
        coefficient /= 10;
        scale -= 1;
    }
    Decimal::try_from_i128_with_scale(coefficient, scale)
        .map(Value::Decimal)
        .map_err(|_| CellError::Num)
}

pub fn power(args: &[Value]) -> Result<Value, CellError> {
    if args.len() < 2 {
        return Err(CellError::Value);
    }
    let base = args[0].to_f64()?;
    let exp = args[1].to_f64()?;
    let res = base.powf(exp);
    if res.is_nan() || res.is_infinite() {
        return Err(CellError::Num);
    }
    if let Some(d) = Decimal::from_f64(res) {
        Ok(Value::Decimal(d))
    } else {
        Ok(Value::Float(res))
    }
}

pub fn sqrt(args: &[Value]) -> Result<Value, CellError> {
    if args.is_empty() {
        return Err(CellError::Value);
    }
    let d = args[0].to_decimal()?;
    if d < Decimal::ZERO {
        return Err(CellError::Num);
    }
    let f = d.to_f64().ok_or(CellError::Num)?;
    let res = f.sqrt();
    if let Some(dec) = Decimal::from_f64(res) {
        Ok(Value::Decimal(dec))
    } else {
        Ok(Value::Float(res))
    }
}

#[derive(Clone, Copy)]
enum DecimalRounding {
    HalfEven,
    AwayFromZero,
    TowardZero,
}

/// Quantize the exact decimal coefficient, never a scaled/rounded Decimal quotient.
/// All powers are bounded before conversion, including for i64::MIN digits.
fn round_decimal(args: &[Value], mode: DecimalRounding) -> Result<Value, CellError> {
    let n = args.first().ok_or(CellError::Value)?.to_decimal()?;
    // Preserve existing coercion: numeric fractional digits truncate toward zero.
    let digits = if args.len() > 1 { args[1].to_i64()? } else { 0 };
    if digits >= i64::from(n.scale()) {
        return Ok(Value::Decimal(n));
    }
    let coefficient = n.mantissa().abs(); // Decimal coefficients are at most 96 bits.
    let drop = i128::from(n.scale()) - i128::from(digits);
    let rounded = if drop > 29 {
        // The coefficient is <10^29, hence strictly below half of 10^drop.
        match mode {
            DecimalRounding::AwayFromZero if coefficient != 0 => 1,
            _ => 0,
        }
    } else {
        let divisor = 10i128.pow(drop as u32);
        let quotient = coefficient / divisor;
        let remainder = coefficient % divisor;
        let increment = match mode {
            DecimalRounding::HalfEven => {
                remainder > divisor / 2 || (remainder == divisor / 2 && quotient % 2 != 0)
            }
            DecimalRounding::AwayFromZero => remainder != 0,
            DecimalRounding::TowardZero => false,
        };
        quotient + i128::from(increment)
    };
    let (coefficient, scale) = if digits >= 0 {
        (rounded, digits as u32) // digits < original scale <=28
    } else if rounded == 0 {
        (0, 0)
    } else {
        let power = -i128::from(digits);
        if power > 29 {
            return Err(CellError::Num);
        }
        (
            rounded
                .checked_mul(10i128.pow(power as u32))
                .ok_or(CellError::Num)?,
            0,
        )
    };
    let signed = if n.is_sign_negative() {
        -coefficient
    } else {
        coefficient
    };
    Decimal::try_from_i128_with_scale(signed, scale)
        .map(Value::Decimal)
        .map_err(|_| CellError::Num)
}

pub fn round(args: &[Value]) -> Result<Value, CellError> {
    round_decimal(args, DecimalRounding::HalfEven)
}

pub fn roundup(args: &[Value]) -> Result<Value, CellError> {
    round_decimal(args, DecimalRounding::AwayFromZero)
}

pub fn rounddown(args: &[Value]) -> Result<Value, CellError> {
    round_decimal(args, DecimalRounding::TowardZero)
}

pub fn int_fn(args: &[Value]) -> Result<Value, CellError> {
    if args.is_empty() {
        return Err(CellError::Value);
    }
    let n = args[0].to_decimal()?;
    Ok(Value::Decimal(n.floor()))
}

pub fn trunc(args: &[Value]) -> Result<Value, CellError> {
    round_decimal(args, DecimalRounding::TowardZero)
}

pub fn ceiling_math(args: &[Value]) -> Result<Value, CellError> {
    if args.is_empty() {
        return Err(CellError::Value);
    }
    let n = args[0].to_decimal()?;
    let sig = if args.len() > 1 && !args[1].is_blank() {
        args[1].to_decimal()?.abs()
    } else {
        Decimal::ONE
    };
    if sig.is_zero() {
        return Ok(Value::Decimal(Decimal::ZERO));
    }
    let mode = if args.len() > 2 { args[2].to_i64()? } else { 0 };
    let div = n / sig;
    let mult = if n < Decimal::ZERO && mode != 0 {
        div.floor() // towards zero
    } else {
        div.ceil()
    };
    Ok(Value::Decimal(mult * sig))
}

pub fn floor_math(args: &[Value]) -> Result<Value, CellError> {
    if args.is_empty() {
        return Err(CellError::Value);
    }
    let n = args[0].to_decimal()?;
    let sig = if args.len() > 1 && !args[1].is_blank() {
        args[1].to_decimal()?.abs()
    } else {
        Decimal::ONE
    };
    if sig.is_zero() {
        return Ok(Value::Decimal(Decimal::ZERO));
    }
    let mode = if args.len() > 2 { args[2].to_i64()? } else { 0 };
    let div = n / sig;
    let mult = if n < Decimal::ZERO && mode != 0 {
        div.ceil() // towards zero
    } else {
        div.floor()
    };
    Ok(Value::Decimal(mult * sig))
}

pub fn sumproduct(args: &[Value]) -> Result<Value, CellError> {
    if args.is_empty() {
        return Err(CellError::Value);
    }
    let mut flat_arrays: Vec<Vec<Decimal>> = Vec::new();
    for arg in args {
        let mut list = Vec::new();
        match arg {
            Value::Array(rows) => {
                for r in rows {
                    for cell in r {
                        list.push(cell.to_decimal().unwrap_or(Decimal::ZERO));
                    }
                }
            }
            other => {
                list.push(other.to_decimal().unwrap_or(Decimal::ZERO));
            }
        }
        flat_arrays.push(list);
    }

    let len = flat_arrays[0].len();
    for arr in &flat_arrays {
        if arr.len() != len {
            return Err(CellError::Value);
        }
    }

    let mut sum = Decimal::ZERO;
    for i in 0..len {
        let mut prod = Decimal::ONE;
        for arr in &flat_arrays {
            prod *= arr[i];
        }
        sum += prod;
    }
    Ok(Value::Decimal(sum))
}

pub fn matches_criteria(cell: &Value, criteria: &Value) -> bool {
    let crit_str = criteria.to_string_repr().trim().to_string();
    if crit_str.starts_with(">=") {
        let target = Decimal::from_str_exact(crit_str[2..].trim()).unwrap_or(Decimal::ZERO);
        cell.to_decimal().map_or(false, |val| val >= target)
    } else if crit_str.starts_with("<=") {
        let target = Decimal::from_str_exact(crit_str[2..].trim()).unwrap_or(Decimal::ZERO);
        cell.to_decimal().map_or(false, |val| val <= target)
    } else if crit_str.starts_with("<>") {
        let target = &crit_str[2..].trim();
        cell.to_string_repr() != *target
    } else if crit_str.starts_with('>') {
        let target = Decimal::from_str_exact(crit_str[1..].trim()).unwrap_or(Decimal::ZERO);
        cell.to_decimal().map_or(false, |val| val > target)
    } else if crit_str.starts_with('<') {
        let target = Decimal::from_str_exact(crit_str[1..].trim()).unwrap_or(Decimal::ZERO);
        cell.to_decimal().map_or(false, |val| val < target)
    } else if crit_str.starts_with('=') {
        let target = &crit_str[1..].trim();
        cell.to_string_repr().eq_ignore_ascii_case(target)
    } else {
        if let Ok(target_num) = Decimal::from_str_exact(&crit_str) {
            cell.to_decimal().map_or(false, |v| v == target_num)
        } else {
            cell.to_string_repr().eq_ignore_ascii_case(&crit_str)
        }
    }
}

pub fn sumif(args: &[Value]) -> Result<Value, CellError> {
    if args.len() < 2 {
        return Err(CellError::Value);
    }
    let range = match &args[0] {
        Value::Array(rows) => rows,
        _ => return Err(CellError::Value),
    };
    let criteria = &args[1];
    let sum_range = if args.len() > 2 {
        match &args[2] {
            Value::Array(rows) => Some(rows),
            _ => return Err(CellError::Value),
        }
    } else {
        None
    };

    let mut sum = Decimal::ZERO;
    for (r_idx, row) in range.iter().enumerate() {
        for (c_idx, cell) in row.iter().enumerate() {
            if matches_criteria(cell, criteria) {
                let add_val = if let Some(sr) = sum_range {
                    sr.get(r_idx)
                        .and_then(|r| r.get(c_idx))
                        .unwrap_or(&Value::Blank)
                } else {
                    cell
                };
                if let Ok(d) = add_val.to_decimal() {
                    sum = sum.checked_add(d).ok_or(CellError::Num)?;
                }
            }
        }
    }
    Ok(Value::Decimal(sum))
}

pub fn sumifs(args: &[Value]) -> Result<Value, CellError> {
    if args.len() < 3 || args.len() % 2 == 0 {
        return Err(CellError::Value);
    }
    let sum_range = match &args[0] {
        Value::Array(rows) => rows,
        _ => return Err(CellError::Value),
    };

    let mut criteria_pairs = Vec::new();
    let mut idx = 1;
    while idx < args.len() {
        let range = match &args[idx] {
            Value::Array(rows) => rows,
            _ => return Err(CellError::Value),
        };
        let crit = &args[idx + 1];
        criteria_pairs.push((range, crit));
        idx += 2;
    }

    let mut total = Decimal::ZERO;
    for (r_idx, row) in sum_range.iter().enumerate() {
        for (c_idx, sum_cell) in row.iter().enumerate() {
            let mut all_match = true;
            for (crange, crit) in &criteria_pairs {
                let cell = crange
                    .get(r_idx)
                    .and_then(|r| r.get(c_idx))
                    .unwrap_or(&Value::Blank);
                if !matches_criteria(cell, crit) {
                    all_match = false;
                    break;
                }
            }
            if all_match {
                if let Ok(d) = sum_cell.to_decimal() {
                    total = total.checked_add(d).ok_or(CellError::Num)?;
                }
            }
        }
    }
    Ok(Value::Decimal(total))
}
