//! Workbook-contained monthly-close working papers, not an external ledger or audit assurance.
use crate::functions::EvaluationContext;
use crate::review::ReviewDecision;
use crate::value::{CellCoord, Value};
use crate::workbook::{CellData, Sheet, Workbook, WorkbookEvalContext};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

pub const MAPPINGS: [&str; 6] = [
    "Revenue",
    "Cost of sales",
    "Operating expenses",
    "Other income",
    "Other expense",
    "Balance sheet",
];
const MAX_ACCOUNTS: usize = 1000;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloseAccount {
    pub account: String,
    pub name: String,
    pub debit: Decimal,
    pub credit: Decimal,
    pub mapping: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClosePeriod {
    pub id: Uuid,
    pub period: String,
    pub version: u32,
    pub sheet_name: String,
    pub created_at: DateTime<Utc>,
    pub source_csv: String,
    pub source_sha256: String,
    pub accounts: Vec<CloseAccount>,
    pub prior_period_id: Option<Uuid>,
}
#[derive(Debug, Deserialize)]
pub struct ImportTrialBalanceRequest {
    pub period: String,
    pub csv: String,
    #[serde(default)]
    pub mappings: BTreeMap<String, String>,
}
#[derive(Debug, Deserialize)]
pub struct MapAccountsRequest {
    pub period_id: Uuid,
    pub mappings: BTreeMap<String, String>,
}
#[derive(Debug, Deserialize)]
pub struct RollforwardRequest {
    pub source_period_id: Uuid,
    pub period: String,
    pub csv: String,
}
#[derive(Debug, Serialize)]
pub struct CloseQueueItem {
    pub label: String,
    pub coord: CellCoord,
    pub value: Value,
    pub status: String,
    pub review_id: Option<Uuid>,
}
/// Current worksheet display data, separate from the retained import baseline.
#[derive(Debug, Serialize)]
pub struct LiveCloseAccount {
    pub row: u32,
    pub imported_account: String,
    pub account: String,
    pub name: String,
    pub mapping: String,
    /// Null means the current cell is not a valid, non-negative exact amount.
    pub debit: Option<Decimal>,
    pub credit: Option<Decimal>,
    pub identity_matches_import: bool,
}
#[derive(Debug, Serialize)]
pub struct CloseStatus {
    pub period: ClosePeriod,
    pub live_accounts: Vec<LiveCloseAccount>,
    pub debits: Decimal,
    pub credits: Decimal,
    pub difference: Decimal,
    pub balanced: bool,
    pub unmapped_accounts: Vec<String>,
    pub queue: Vec<CloseQueueItem>,
    pub ready_for_close: bool,
    pub warnings: Vec<String>,
}

impl Workbook {
    pub fn import_trial_balance(
        &mut self,
        request: ImportTrialBalanceRequest,
    ) -> Result<ClosePeriod, String> {
        validate_period(&request.period)?;
        let accounts = parse_trial_balance(&request.csv, &request.mappings)?;
        let version = self
            .close_periods
            .iter()
            .filter(|p| p.period == request.period)
            .map(|p| p.version)
            .max()
            .unwrap_or(0)
            + 1;
        let id = Uuid::new_v4();
        let mut sheet_name = format!("Close {} v{}", request.period, version);
        if self.sheets.contains_key(&sheet_name) {
            sheet_name = format!("{} {}", sheet_name, &id.to_string()[..8]);
        }
        let period = ClosePeriod {
            id,
            period: request.period,
            version,
            sheet_name: sheet_name.clone(),
            created_at: Utc::now(),
            source_sha256: format!("sha256:{:x}", Sha256::digest(request.csv.as_bytes())),
            source_csv: request.csv,
            accounts,
            prior_period_id: None,
        };
        let sheet = build_sheet(&period);
        self.sheets.insert(sheet_name.clone(), sheet);
        if let Err(e) = self.recalculate(&sheet_name) {
            self.sheets.remove(&sheet_name);
            return Err(e.to_string());
        }
        self.active_sheet = sheet_name;
        self.close_periods.push(period.clone());
        self.edit_history = Default::default();
        Ok(period)
    }

    pub fn map_close_accounts(
        &mut self,
        request: MapAccountsRequest,
    ) -> Result<ClosePeriod, String> {
        let index = self
            .close_periods
            .iter()
            .position(|p| p.id == request.period_id)
            .ok_or("Close period not found")?;
        let period = self.close_periods[index].clone();
        validate_mappings(
            &request.mappings,
            &period.accounts.iter().map(|a| a.account.clone()).collect(),
        )?;
        let original = self
            .sheets
            .get(&period.sheet_name)
            .ok_or("Close worksheet is missing")?
            .clone();
        if original.tables.values().any(|t| {
            t.range.start_row <= period.accounts.len() as u32
                && t.range.end_row >= 1
                && t.range.start_col <= 2
        }) {
            return Err("A source table overlaps account identities or mappings. Move it before mapping the close working paper.".into());
        }
        // Do not overwrite an account row whose identity was manually changed.
        for (i, account) in period.accounts.iter().enumerate() {
            let cell = original.get_cell(&CellCoord::new(i as u32 + 1, 0));
            if !matches!(cell.map(|c| &c.evaluated_value), Some(Value::Text(t)) if t == &account.account)
            {
                return Err("Account identities changed in the worksheet. Import a new version before mapping.".into());
            }
        }
        let sheet = self.sheets.get_mut(&period.sheet_name).unwrap();
        for (i, account) in period.accounts.iter().enumerate() {
            if let Some(mapping) = request.mappings.get(&account.account) {
                put_text(sheet, i as u32 + 1, 2, mapping);
            }
        }
        if let Err(e) = self.recalculate(&period.sheet_name) {
            self.sheets.insert(period.sheet_name.clone(), original);
            return Err(e.to_string());
        }
        self.edit_history = Default::default();
        self.invalidate_changed_reviews();
        Ok(period)
    }

    pub fn close_status(&self, id: Uuid) -> Result<CloseStatus, String> {
        let period = self
            .close_periods
            .iter()
            .find(|p| p.id == id)
            .ok_or("Close period not found")?
            .clone();
        let sheet = self
            .sheets
            .get(&period.sheet_name)
            .ok_or("Close worksheet is missing")?;
        let mut debits = Decimal::ZERO;
        let mut credits = Decimal::ZERO;
        let mut unmapped_accounts = Vec::new();
        let mut live_accounts = Vec::with_capacity(period.accounts.len());
        let mut warnings = vec!["Local review labels are self-reported, not authenticated approval. Reconciliation checks debit equals credit, not completeness or correctness of the ledger.".into()];
        for (i, account) in period.accounts.iter().enumerate() {
            let row = i as u32 + 1;
            let context = WorkbookEvalContext {
                workbook: self,
                sheet_name: &period.sheet_name,
                current_coord: CellCoord::new(row, 0),
            };
            let values: Vec<_> = (0..5)
                .map(|col| context.get_cell_value(CellCoord::new(row, col)))
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?;
            let current = |col: u32| values.get(col as usize);
            let debit = exact_nonnegative_amount(current(3));
            let credit = exact_nonnegative_amount(current(4));
            for (col, amount, total) in [(3, debit, &mut debits), (4, credit, &mut credits)] {
                if let Some(amount) = amount {
                    *total = total
                        .checked_add(amount)
                        .ok_or("Reconciliation exceeds decimal precision")?;
                } else {
                    warnings.push(format!(
                        "Invalid debit/credit at {}. Use non-negative exact amounts.",
                        CellCoord::new(row, col).to_a1()
                    ));
                }
            }
            let identity_matches_import =
                matches!(current(0), Some(Value::Text(v)) if v == &account.account);
            if !identity_matches_import {
                warnings.push(format!(
                    "Imported account identity changed at A{}.",
                    row + 1
                ));
            }
            if !matches!(current(2), Some(Value::Text(v)) if MAPPINGS.contains(&v.as_str())) {
                unmapped_accounts.push(account.account.clone());
            }
            live_accounts.push(LiveCloseAccount {
                row,
                imported_account: account.account.clone(),
                account: current(0).map(Value::to_string_repr).unwrap_or_default(),
                name: current(1).map(Value::to_string_repr).unwrap_or_default(),
                mapping: current(2).map(Value::to_string_repr).unwrap_or_default(),
                debit,
                credit,
                identity_matches_import,
            });
            let expected_net = format!("=D{}-E{}", row + 1, row + 1);
            if sheet
                .get_cell(&CellCoord::new(row, 5))
                .and_then(|c| c.formula.as_deref())
                != Some(expected_net.as_str())
            {
                warnings.push(format!("Net-balance formula changed at F{}.", row + 1));
            }
        }
        // Imported working-paper regions must not be silently overlaid by a source table.
        // Refresh can expand a previously separate source into this area.
        if sheet.tables.values().any(|table| {
            (table.range.start_row <= period.accounts.len() as u32 && table.range.start_col <= 5)
                || (table.range.start_row <= 6
                    && table.range.start_col <= 8
                    && table.range.end_col >= 7)
        }) {
            warnings.push("A source table overlaps the close working paper. Move it outside the trial-balance and P&L regions before closing.".into());
        }
        let expected = build_sheet(&period);
        let mut queue = Vec::new();
        for (i, label) in [
            "Revenue",
            "Cost of sales",
            "Operating expenses",
            "Other income",
            "Other expense",
            "Net profit",
        ]
        .iter()
        .enumerate()
        {
            let coord = CellCoord::new(i as u32 + 1, 8);
            if sheet.get_cell(&coord).and_then(|c| c.formula.as_ref())
                != expected.get_cell(&coord).and_then(|c| c.formula.as_ref())
            {
                warnings.push(format!("P&L formula changed at {}.", coord.to_a1()));
            }
            let explanation = self
                .explain_cell(&period.sheet_name, coord)
                .map_err(|e| e.to_string())?;
            let status = if !explanation.can_review {
                "Blocked"
            } else {
                match explanation.review.as_ref() {
                    Some(r) if r.invalidated => "Needs re-review",
                    Some(r) if r.decision == ReviewDecision::Question => "Question",
                    Some(_) => "Reviewed",
                    None => "Unreviewed",
                }
            };
            queue.push(CloseQueueItem {
                label: (*label).into(),
                coord,
                value: explanation.value,
                status: status.into(),
                review_id: explanation.review.map(|r| r.id),
            });
        }
        let difference = debits - credits;
        let balanced = difference == Decimal::ZERO
            && !warnings
                .iter()
                .any(|w| w.starts_with("Invalid debit/credit"));
        let ready_for_close = balanced
            && warnings.len() == 1
            && unmapped_accounts.is_empty()
            && queue.iter().all(|q| q.status == "Reviewed");
        Ok(CloseStatus {
            period,
            live_accounts,
            debits,
            credits,
            difference,
            balanced,
            unmapped_accounts,
            queue,
            ready_for_close,
            warnings,
        })
    }

    pub fn rollforward_close(
        &mut self,
        request: RollforwardRequest,
    ) -> Result<ClosePeriod, String> {
        let previous = self
            .close_periods
            .iter()
            .find(|p| p.id == request.source_period_id)
            .ok_or("Source period not found")?
            .clone();
        validate_period(&request.period)?;
        if request.period <= previous.period {
            return Err("Rollforward requires a later month and a new trial balance; prior balances and sign-offs are not carried.".into());
        }
        let sheet = self
            .sheets
            .get(&previous.sheet_name)
            .ok_or("Source worksheet is missing")?;
        let new_accounts = parse_trial_balance(&request.csv, &BTreeMap::new())?;
        let mut mappings = BTreeMap::new();
        for (i, a) in previous.accounts.iter().enumerate() {
            if !matches!(sheet.get_cell(&CellCoord::new(i as u32 + 1, 0)).map(|c| &c.evaluated_value), Some(Value::Text(v)) if v == &a.account)
            {
                return Err("Source account identities changed. Import a corrected version before rollforward.".into());
            }
            if new_accounts.iter().any(|n| n.account == a.account) {
                if let Some(Value::Text(mapping)) = sheet
                    .get_cell(&CellCoord::new(i as u32 + 1, 2))
                    .map(|c| &c.evaluated_value)
                {
                    if MAPPINGS.contains(&mapping.as_str()) {
                        mappings.insert(a.account.clone(), mapping.clone());
                    }
                }
            }
        }
        let mut period = self.import_trial_balance(ImportTrialBalanceRequest {
            period: request.period,
            csv: request.csv,
            mappings,
        })?;
        period.prior_period_id = Some(previous.id);
        *self.close_periods.last_mut().unwrap() = period.clone();
        Ok(period)
    }

    pub fn export_review_pack(&self, id: Uuid) -> Result<serde_json::Value, String> {
        let status = self.close_status(id)?;
        let sheet = &self.sheets[&status.period.sheet_name];
        let reviews: Vec<_> = self
            .review_history()
            .into_iter()
            .filter(|r| r.sheet_name == status.period.sheet_name)
            .collect();
        let explanations: Vec<_> = status
            .queue
            .iter()
            .map(|q| self.explain_cell(&status.period.sheet_name, q.coord))
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        Ok(
            serde_json::json!({"format":"gridloom-review-pack", "version":1, "exported_at":Utc::now(), "status":status, "worksheet":sheet, "reviews":reviews, "explanations":explanations, "limitations":["This JSON contains local workbook evidence and reproducible cached inputs, not authenticated signatures or proof of external ledger completeness.", "Imported source CSV is retained unchanged; worksheet edits are represented separately. Review records retain their own input snapshots."]}),
        )
    }
}

fn exact_nonnegative_amount(value: Option<&Value>) -> Option<Decimal> {
    match value {
        Some(Value::Decimal(v)) if *v >= Decimal::ZERO => Some(*v),
        Some(Value::Integer(v)) if *v >= 0 => Some(Decimal::from(*v)),
        _ => None,
    }
}

fn put_text(sheet: &mut Sheet, row: u32, col: u32, text: &str) {
    let coord = CellCoord::new(row, col);
    let mut cell = CellData::new(coord, text.into());
    cell.formula = None;
    cell.evaluated_value = Value::Text(text.into());
    cell.is_dirty = false;
    sheet.cells.insert(coord.to_a1(), cell);
}
fn put(sheet: &mut Sheet, row: u32, col: u32, raw: String) {
    let coord = CellCoord::new(row, col);
    sheet.cells.insert(coord.to_a1(), CellData::new(coord, raw));
}
fn build_sheet(period: &ClosePeriod) -> Sheet {
    let mut sheet = Sheet::new(period.sheet_name.clone());
    sheet.max_rows = 1000.max(period.accounts.len() as u32 + 10);
    for (i, h) in ["Account", "Name", "Mapping", "Debit", "Credit", "Net debit"]
        .iter()
        .enumerate()
    {
        put_text(&mut sheet, 0, i as u32, h);
    }
    for (i, a) in period.accounts.iter().enumerate() {
        let row = i as u32 + 1;
        put_text(&mut sheet, row, 0, &a.account);
        put_text(&mut sheet, row, 1, &a.name);
        put_text(&mut sheet, row, 2, &a.mapping);
        put(&mut sheet, row, 3, a.debit.to_string());
        put(&mut sheet, row, 4, a.credit.to_string());
        put(&mut sheet, row, 5, format!("=D{}-E{}", row + 1, row + 1));
    }
    put_text(&mut sheet, 0, 7, &format!("P&L {}", period.period));
    put_text(&mut sheet, 0, 8, "Amount");
    let end = period.accounts.len() + 1;
    for (i, mapping) in MAPPINGS[..5].iter().enumerate() {
        put_text(&mut sheet, i as u32 + 1, 7, mapping);
        let sign = if [0, 3].contains(&i) { "-" } else { "" };
        put(
            &mut sheet,
            i as u32 + 1,
            8,
            format!("={sign}SUMIF(C2:C{end},\"{mapping}\",F2:F{end})"),
        );
    }
    put_text(&mut sheet, 6, 7, "Net profit");
    put(&mut sheet, 6, 8, "=I2-I3-I4+I5-I6".into());
    sheet
}
fn validate_period(period: &str) -> Result<(), String> {
    if period.len() != 7
        || period.as_bytes()[4] != b'-'
        || !period[..4].bytes().all(|c| c.is_ascii_digit())
        || !period[5..].bytes().all(|c| c.is_ascii_digit())
        || !(1..=12).contains(&period[5..].parse::<u32>().unwrap_or(0))
    {
        return Err("Period must use YYYY-MM with a valid month.".into());
    }
    Ok(())
}
fn validate_mappings(
    mappings: &BTreeMap<String, String>,
    accounts: &BTreeSet<String>,
) -> Result<(), String> {
    for (account, mapping) in mappings {
        if !accounts.contains(account) {
            return Err(format!("Mapping refers to unknown account {account}."));
        }
        if !mapping.is_empty() && !MAPPINGS.contains(&mapping.as_str()) {
            return Err(format!("Unknown P&L mapping {mapping}."));
        }
    }
    Ok(())
}
fn parse_trial_balance(
    csv: &str,
    mappings: &BTreeMap<String, String>,
) -> Result<Vec<CloseAccount>, String> {
    if csv.len() > 1024 * 1024 {
        return Err("Trial balance exceeds the 1 MiB import limit.".into());
    }
    let records = parse_csv(csv)?;
    let header = records.first().ok_or("CSV is empty")?;
    let normalized: Vec<_> = header
        .iter()
        .map(|h| h.trim().trim_start_matches('\u{feff}').to_ascii_lowercase())
        .collect();
    if normalized.len() != 4 || normalized.iter().collect::<BTreeSet<_>>().len() != 4 {
        return Err("CSV must have exactly account,name,debit,credit columns (any order).".into());
    }
    let indices: Vec<_> = ["account", "name", "debit", "credit"]
        .iter()
        .map(|h| {
            normalized
                .iter()
                .position(|v| v == h)
                .ok_or("CSV requires account,name,debit,credit headers".to_string())
        })
        .collect::<Result<_, _>>()?;
    let mut accounts = Vec::new();
    let mut ids = BTreeSet::new();
    for (line, record) in records.iter().enumerate().skip(1) {
        if record.len() == 1 && record[0].trim().is_empty() {
            continue;
        }
        if record.len() != 4 {
            return Err(format!(
                "CSV record {} has the wrong column count.",
                line + 1
            ));
        }
        let account = record[indices[0]].trim().to_string();
        let name = record[indices[1]].trim().to_string();
        if account.is_empty()
            || account.len() > 120
            || name.len() > 500
            || !ids.insert(account.clone())
        {
            return Err(format!(
                "CSV record {} has an empty, duplicate, or oversized account/name.",
                line + 1
            ));
        }
        let amount = |index: usize| -> Result<Decimal, String> {
            let raw = record[index].trim();
            let v = if raw.is_empty() {
                Decimal::ZERO
            } else {
                Decimal::from_str_exact(raw).map_err(|_| format!("CSV record {} needs an exact decimal, without currency symbols or grouping separators.", line + 1))?
            };
            if v < Decimal::ZERO {
                return Err(format!(
                    "CSV record {} has a negative debit/credit.",
                    line + 1
                ));
            }
            Ok(v)
        };
        accounts.push(CloseAccount {
            mapping: mappings.get(&account).cloned().unwrap_or_default(),
            account,
            name,
            debit: amount(indices[2])?,
            credit: amount(indices[3])?,
        });
        if accounts.len() > MAX_ACCOUNTS {
            return Err(format!("Import supports at most {MAX_ACCOUNTS} accounts."));
        }
    }
    if accounts.is_empty() {
        return Err("Trial balance needs at least one account.".into());
    }
    validate_mappings(mappings, &ids)?;
    // Reject sums whose exact representation would overflow during reconciliation or formulas.
    for credit in [false, true] {
        accounts
            .iter()
            .try_fold(Decimal::ZERO, |sum, a| {
                sum.checked_add(if credit { a.credit } else { a.debit })
            })
            .ok_or("Trial-balance total exceeds decimal precision")?;
    }
    Ok(accounts)
}

// RFC 4180 quoting, including commas, escaped quotes, CRLF and embedded newlines.
fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, String> {
    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut chars = input.chars().peekable();
    let mut quoted = false;
    let mut closed = false;
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        if closed && ![',', '\r', '\n'].contains(&c) {
            return Err("Unexpected text after closing CSV quote.".into());
        }
        match c {
            '"' if field.is_empty() && !closed => quoted = true,
            '"' => return Err("Quote in unquoted CSV field.".into()),
            ',' => {
                row.push(std::mem::take(&mut field));
                closed = false;
            }
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut row));
                closed = false;
            }
            _ => field.push(c),
        }
    }
    if quoted {
        return Err("Unclosed CSV quoted field.".into());
    }
    if !field.is_empty() || !row.is_empty() || closed {
        row.push(field);
        records.push(row);
    }
    Ok(records)
}
