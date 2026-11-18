//! Workbook-contained monthly-close working papers, not an external ledger or audit assurance.
use crate::functions::EvaluationContext;
use crate::review::{EvidenceAttachment, EvidenceUpload, ReviewDecision};
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
    #[serde(default)]
    pub entity: String,
    #[serde(default)]
    pub currency: String,
    #[serde(default = "default_units")]
    pub units: String,
    #[serde(default)]
    pub report_lines: Vec<ReportLine>,
    #[serde(default)]
    pub adjustments: Vec<CloseAdjustment>,
    /// Legacy files retain their original formula layout until explicitly configured.
    #[serde(default)]
    pub reconciliation_version: u32,
    #[serde(default)]
    pub configuration_history: Vec<CloseConfiguration>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloseConfiguration {
    pub created_at: DateTime<Utc>,
    pub entity: String,
    pub currency: String,
    pub units: String,
    pub report_lines: Vec<ReportLine>,
    pub mappings: BTreeMap<String, String>,
}
fn default_units() -> String {
    "units".into()
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NormalBalance {
    Debit,
    Credit,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportLine {
    pub id: String,
    pub label: String,
    pub parent_id: Option<String>,
    pub normal_balance: NormalBalance,
}
#[derive(Debug, Deserialize)]
pub struct ConfigureCloseRequest {
    pub period_id: Uuid,
    pub entity: String,
    pub currency: String,
    pub units: String,
    pub report_lines: Vec<ReportLine>,
    pub mappings: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdjustmentLine {
    pub account: String,
    pub debit: Decimal,
    pub credit: Decimal,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloseAdjustment {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub author: String,
    pub rationale: String,
    pub lines: Vec<AdjustmentLine>,
    pub evidence: Vec<EvidenceAttachment>,
}
#[derive(Debug, Deserialize)]
pub struct PostAdjustmentRequest {
    pub period_id: Uuid,
    pub author: String,
    pub rationale: String,
    pub lines: Vec<AdjustmentLine>,
    pub evidence: Vec<EvidenceUpload>,
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
    pub imported_balance: Decimal,
    pub adjustment_balance: Decimal,
    pub reported_balance: Option<Decimal>,
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
    /// Non-recursive integrity checks for review guards. Never calls explain_cell/close_status.
    pub fn close_integrity_issues(&self, id: Uuid) -> Vec<String> {
        let Some(period) = self.close_periods.iter().find(|p| p.id == id) else {
            return vec!["Close period not found".into()];
        };
        let Some(sheet) = self.sheets.get(&period.sheet_name) else {
            return vec!["Close worksheet is missing".into()];
        };
        if period.reconciliation_version == 0 {
            return Vec::new();
        }
        let mut issues = Vec::new();
        let expected = build_sheet(period);
        for cell in expected.cells.values() {
            let c = cell.coord;
            if c.col == 0 || c.col == 3 || c.col == 4 || c.col == 5 || c.col == 8 || c.col >= 10 {
                if sheet.get_cell(&c).map(|v| &v.raw_input) != Some(&cell.raw_input) {
                    issues.push(format!(
                        "Working-paper import, report or journal differs at {}",
                        c.to_a1()
                    ));
                }
            }
        }
        for (i, _) in period.accounts.iter().enumerate() {
            if !sheet
                .get_cell(&CellCoord::new(i as u32 + 1, 2))
                .map(|c| valid_mapping(period, &c.evaluated_value.to_string_repr()))
                .unwrap_or(false)
            {
                issues.push(format!("Unmapped account at row {}", i + 2));
            }
        }
        if sheet.tables.values().any(|t| {
            t.range.start_col <= 20
                && t.range.start_row
                    <= period
                        .accounts
                        .len()
                        .max(period.report_lines.len())
                        .max(
                            period
                                .adjustments
                                .iter()
                                .map(|j| j.lines.len())
                                .sum::<usize>(),
                        )
                        .max(6) as u32
        }) {
            issues.push("A source table overlaps the working-paper region".into());
        }
        let total = period.accounts.iter().try_fold(Decimal::ZERO, |sum, a| {
            sum.checked_add(a.debit)?.checked_sub(a.credit)
        });
        if total != Some(Decimal::ZERO) {
            issues.push("Imported trial balance is not balanced".into());
        }
        issues.sort();
        issues
    }

    /// Change report presentation and dimensions without replacing imported facts or journals.
    pub fn configure_close(
        &mut self,
        request: ConfigureCloseRequest,
    ) -> Result<ClosePeriod, String> {
        let index = self
            .close_periods
            .iter()
            .position(|p| p.id == request.period_id)
            .ok_or("Close period not found")?;
        let mut period = self.close_periods[index].clone();
        for (label, value, limit) in [
            ("Entity", request.entity.as_str(), 200),
            ("Currency", request.currency.as_str(), 12),
            ("Units", request.units.as_str(), 80),
        ] {
            if value.trim().is_empty() || value.len() > limit || value.chars().any(char::is_control)
            {
                return Err(format!(
                    "{label} is required and must be bounded plain text."
                ));
            }
        }
        if !request.currency.bytes().all(|c| c.is_ascii_uppercase()) || request.currency.len() != 3
        {
            return Err("Currency must be a three-letter uppercase code. No currency conversion or consolidation is implied.".into());
        }
        validate_report(&request.report_lines)?;
        if !period.adjustments.is_empty()
            && ((!period.entity.is_empty() && period.entity != request.entity.trim())
                || (!period.currency.is_empty() && period.currency != request.currency)
                || period.units != request.units.trim())
        {
            return Err("Dimensions of a working paper with posted adjustments cannot be reinterpreted. Import a new version for a different entity, currency or units.".into());
        }
        period.entity = request.entity.trim().into();
        period.currency = request.currency;
        period.units = request.units.trim().into();
        period.report_lines = request.report_lines;
        period.reconciliation_version = 1;
        validate_period_mappings(&period, &request.mappings)?;
        let original = self
            .sheets
            .get(&period.sheet_name)
            .ok_or("Close worksheet is missing")?
            .clone();
        let mut mappings = BTreeMap::new();
        for (i, a) in period.accounts.iter().enumerate() {
            let coord = CellCoord::new(i as u32 + 1, 2);
            let current = original
                .get_cell(&coord)
                .map(|c| c.evaluated_value.to_string_repr())
                .unwrap_or_default();
            let mapping = request.mappings.get(&a.account).cloned().unwrap_or(current);
            if !mapping.is_empty() && !valid_mapping(&period, &mapping) {
                return Err(format!(
                    "Account {} requires a mapping to a leaf of the new report.",
                    a.account
                ));
            }
            mappings.insert(a.account.clone(), mapping);
        }
        period.configuration_history.push(CloseConfiguration {
            created_at: Utc::now(),
            entity: period.entity.clone(),
            currency: period.currency.clone(),
            units: period.units.clone(),
            report_lines: period.report_lines.clone(),
            mappings: mappings.clone(),
        });
        self.replace_close_regions(index, period.clone(), &mappings)?;
        Ok(period)
    }

    /// Append a balanced supported journal. Existing journal entries and evidence are immutable.
    /// Reverse a mistake with a new supported compensating journal, not by editing the old entry.
    pub fn post_close_adjustment(
        &mut self,
        request: PostAdjustmentRequest,
    ) -> Result<CloseAdjustment, String> {
        let index = self
            .close_periods
            .iter()
            .position(|p| p.id == request.period_id)
            .ok_or("Close period not found")?;
        let mut period = self.close_periods[index].clone();
        if request.author.trim().is_empty()
            || request.author.len() > 200
            || request.rationale.trim().is_empty()
            || request.rationale.len() > 4000
        {
            return Err("An author and a bounded, non-empty rationale are required.".into());
        }
        if request.lines.len() < 2
            || request.lines.len() > 100
            || period
                .adjustments
                .iter()
                .map(|j| j.lines.len())
                .sum::<usize>()
                + request.lines.len()
                > 2000
        {
            return Err("A journal needs 2–100 lines; a working paper supports at most 2,000 adjustment lines.".into());
        }
        let mut debit = Decimal::ZERO;
        let mut credit = Decimal::ZERO;
        for line in &request.lines {
            if !period.accounts.iter().any(|a| a.account == line.account) {
                return Err(format!("Unknown account {}", line.account));
            }
            if line.debit < Decimal::ZERO
                || line.credit < Decimal::ZERO
                || (line.debit > Decimal::ZERO) == (line.credit > Decimal::ZERO)
            {
                return Err("Each journal line needs exactly one positive debit or credit.".into());
            }
            debit = debit
                .checked_add(line.debit)
                .ok_or("Journal total exceeds decimal precision")?;
            credit = credit
                .checked_add(line.credit)
                .ok_or("Journal total exceeds decimal precision")?;
        }
        if debit != credit {
            return Err("Adjustment debits must equal credits exactly.".into());
        }
        if period
            .adjustments
            .iter()
            .flat_map(|j| &j.evidence)
            .map(|e| e.bytes.len())
            .sum::<usize>()
            + request
                .evidence
                .iter()
                .map(|e| e.bytes.len())
                .sum::<usize>()
            > 32 * 1024 * 1024
        {
            return Err(
                "A working paper supports at most 32 MiB of captured adjustment evidence.".into(),
            );
        }
        if request.evidence.is_empty()
            || request.evidence.len() > 20
            || request
                .evidence
                .iter()
                .map(|e| e.bytes.len())
                .sum::<usize>()
                > 8 * 1024 * 1024
        {
            return Err("Attach 1–20 supporting files totaling at most 8 MiB.".into());
        }
        let mut evidence = Vec::new();
        for upload in request.evidence {
            if upload.filename.trim().is_empty()
                || upload.filename.len() > 255
                || upload.filename.contains(['/', '\\'])
                || upload.filename.chars().any(char::is_control)
                || upload.media_type.trim().is_empty()
                || upload.media_type.len() > 120
                || upload.media_type.chars().any(char::is_control)
                || upload.bytes.is_empty()
                || upload.bytes.len() > 2 * 1024 * 1024
                || upload.replaces.is_some()
            {
                return Err("Adjustment evidence needs a safe filename, media type, non-empty bytes up to 2 MiB, and cannot replace prior evidence.".into());
            }
            evidence.push(EvidenceAttachment {
                id: Uuid::new_v4(),
                version: 1,
                previous_version_id: None,
                filename: upload.filename,
                media_type: upload.media_type,
                sha256: format!("{:x}", Sha256::digest(&upload.bytes)),
                bytes: upload.bytes,
                created_at: Utc::now(),
            });
        }
        let journal = CloseAdjustment {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            author: request.author.trim().into(),
            rationale: request.rationale.trim().into(),
            lines: request.lines,
            evidence,
        };
        // Never use a journal post to silently repair unauthorized grid edits.
        let status = self.close_status(period.id)?;
        if status
            .live_accounts
            .iter()
            .zip(&period.accounts)
            .any(|(live, imported)| {
                live.debit != Some(imported.debit) || live.credit != Some(imported.credit)
            })
        {
            return Err("Working debit/credit values differ from the retained import. Restore them or import a corrected version before posting adjustments.".into());
        }
        if status.warnings.len() > 1 {
            return Err(
                "Restore changed or invalid working-paper cells before posting an adjustment."
                    .into(),
            );
        }
        period.adjustments.push(journal.clone());
        period.reconciliation_version = 1;
        let mut positive = Decimal::ZERO;
        let mut negative = Decimal::ZERO;
        for account in &period.accounts {
            let adjustment = adjustment_total(&period, &account.account)?;
            let reported = (account.debit - account.credit)
                .checked_add(adjustment)
                .ok_or("Reported balance exceeds decimal precision")?;
            if reported > Decimal::ZERO {
                positive = positive
                    .checked_add(reported)
                    .ok_or("Reported totals exceed decimal precision")?;
            } else {
                negative = negative
                    .checked_add(reported)
                    .ok_or("Reported totals exceed decimal precision")?;
            }
        }
        self.replace_close_regions(index, period, &BTreeMap::new())?;
        Ok(journal)
    }

    fn replace_close_regions(
        &mut self,
        index: usize,
        period: ClosePeriod,
        mappings: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        let original = self
            .sheets
            .get(&period.sheet_name)
            .ok_or("Close worksheet is missing")?
            .clone();
        if original.tables.values().any(|t| t.range.start_col <= 20) {
            return Err("Move source tables outside columns A:U before configuring or posting to a working paper.".into());
        }
        let mut replacement = build_sheet(&period);
        let old_report_rows = self.close_periods[index].report_lines.len().max(6) as u32;
        let new_report_rows = period.report_lines.len().max(6) as u32;
        let old_journal_rows = self.close_periods[index]
            .adjustments
            .iter()
            .map(|j| j.lines.len())
            .sum::<usize>() as u32;
        let new_journal_rows = period
            .adjustments
            .iter()
            .map(|j| j.lines.len())
            .sum::<usize>() as u32;
        // Only the generated output/schedule regions are rewritten. Keep user notes and editable display columns.
        let mut merged = original.clone();
        merged.cells.retain(|_, c| {
            !((7..=8).contains(&c.coord.col) && c.coord.row <= old_report_rows.max(new_report_rows)
                || (10..=12).contains(&c.coord.col) && c.coord.row <= period.accounts.len() as u32
                || (14..=20).contains(&c.coord.col)
                    && c.coord.row <= old_journal_rows.max(new_journal_rows))
        });
        for (address, cell) in replacement.cells.drain() {
            if cell.coord.col >= 7 {
                merged.cells.insert(address, cell);
            }
        }
        for (i, a) in period.accounts.iter().enumerate() {
            if let Some(mapping) = mappings.get(&a.account) {
                put_text(&mut merged, i as u32 + 1, 2, mapping);
            }
        }
        merged.max_rows = merged.max_rows.max(replacement.max_rows);
        self.sheets.insert(period.sheet_name.clone(), merged);
        let previous = self.close_periods[index].clone();
        self.close_periods[index] = period.clone();
        if let Err(error) = self.recalculate(&period.sheet_name) {
            self.sheets.insert(period.sheet_name, original);
            self.close_periods[index] = previous;
            return Err(error.to_string());
        }
        self.edit_history = Default::default();
        self.invalidate_changed_reviews();
        Ok(())
    }

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
            entity: String::new(),
            currency: String::new(),
            units: default_units(),
            report_lines: Vec::new(),
            adjustments: Vec::new(),
            reconciliation_version: 1,
            configuration_history: Vec::new(),
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
        validate_period_mappings(&period, &request.mappings)?;
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
        let mut live_mappings = BTreeMap::new();
        for (i, a) in period.accounts.iter().enumerate() {
            live_mappings.insert(
                a.account.clone(),
                self.sheets[&period.sheet_name]
                    .get_cell(&CellCoord::new(i as u32 + 1, 2))
                    .map(|c| c.evaluated_value.to_string_repr())
                    .unwrap_or_default(),
            );
        }
        self.close_periods[index]
            .configuration_history
            .push(CloseConfiguration {
                created_at: Utc::now(),
                entity: period.entity.clone(),
                currency: period.currency.clone(),
                units: period.units.clone(),
                report_lines: period.report_lines.clone(),
                mappings: live_mappings,
            });
        self.edit_history = Default::default();
        self.invalidate_changed_reviews();
        Ok(self.close_periods[index].clone())
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
            if !matches!(current(2), Some(Value::Text(v)) if valid_mapping(&period, v)) {
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
                imported_balance: account.debit - account.credit,
                adjustment_balance: adjustment_total(&period, &account.account)?,
                reported_balance: if period.reconciliation_version == 0 {
                    debit.zip(credit).map(|(d, c)| d - c)
                } else {
                    exact_amount(
                        context
                            .get_cell_value(CellCoord::new(row, 12))
                            .ok()
                            .as_ref(),
                    )
                },
            });
            if period.reconciliation_version > 0
                && (debit != Some(account.debit) || credit != Some(account.credit))
            {
                warnings.push(format!("Imported debit/credit changed at row {}. Post a supported adjustment instead; reported balances retain the import baseline.", row+1));
            }
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
                || (table.range.start_row <= period.report_lines.len().max(6) as u32
                    && table.range.start_col <= 8
                    && table.range.end_col >= 7)
                || (period.reconciliation_version > 0
                    && table.range.start_row <= period.accounts.len() as u32
                    && table.range.start_col <= 12
                    && table.range.end_col >= 10)
                || (period.reconciliation_version > 0
                    && table.range.start_row
                        <= period
                            .adjustments
                            .iter()
                            .map(|j| j.lines.len())
                            .sum::<usize>() as u32
                    && table.range.start_col <= 20
                    && table.range.end_col >= 14)
        }) {
            warnings.push("A source table overlaps the close working paper. Move it outside the trial-balance and P&L regions before closing.".into());
        }
        let expected = build_sheet(&period);
        let mut queue = Vec::new();
        let labels = report_labels(&period);
        for (i, label) in labels.iter().enumerate() {
            let coord = CellCoord::new(i as u32 + 1, 8);
            if sheet.get_cell(&coord).and_then(|c| c.formula.as_ref())
                != expected.get_cell(&coord).and_then(|c| c.formula.as_ref())
            {
                warnings.push(format!("P&L formula changed at {}.", coord.to_a1()));
            }
            let explanation = self
                .explain_cell(&period.sheet_name, coord)
                .map_err(|e| e.to_string())?;
            let open_question = self.reviews.iter().rev().find(|r| {
                r.sheet_name == period.sheet_name && r.coord == coord && r.question_is_open()
            });
            let status = if open_question.is_some() {
                "Question"
            } else if !explanation.can_review {
                "Blocked"
            } else {
                match explanation.review.as_ref() {
                    Some(r) if r.decision == ReviewDecision::Question => "Unreviewed",
                    Some(r) if r.invalidated => "Needs re-review",
                    Some(_) => "Reviewed",
                    None => "Unreviewed",
                }
            };
            queue.push(CloseQueueItem {
                label: label.clone(),
                coord,
                value: explanation.value,
                status: status.into(),
                review_id: open_question
                    .map(|r| r.id)
                    .or_else(|| explanation.review.map(|r| r.id)),
            });
        }
        if period.reconciliation_version > 0 {
            for (address, expected_cell) in &expected.cells {
                let c = expected_cell.coord;
                if c.col >= 10
                    && sheet.get_cell(&c).map(|v| &v.raw_input) != Some(&expected_cell.raw_input)
                {
                    warnings.push(format!("Reconciliation or journal cell changed at {address}. Restore the working paper before closing."));
                }
            }
        }
        let difference = debits - credits;
        let balanced = difference == Decimal::ZERO
            && !warnings
                .iter()
                .any(|w| w.starts_with("Invalid debit/credit"));
        let ready_for_close = !self
            .reviews
            .iter()
            .any(|r| r.sheet_name == period.sheet_name && r.question_is_open())
            && balanced
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
                    if valid_mapping(&previous, mapping) {
                        mappings.insert(a.account.clone(), mapping.clone());
                    }
                }
            }
        }
        let custom = !previous.report_lines.is_empty();
        let mut period = self.import_trial_balance(ImportTrialBalanceRequest {
            period: request.period,
            csv: request.csv,
            mappings: if custom {
                BTreeMap::new()
            } else {
                mappings.clone()
            },
        })?;
        if custom || !previous.entity.is_empty() || !previous.currency.is_empty() {
            period = self.configure_close(ConfigureCloseRequest {
                period_id: period.id,
                entity: previous.entity.clone(),
                currency: previous.currency.clone(),
                units: previous.units.clone(),
                report_lines: previous.report_lines.clone(),
                mappings,
            })?;
        }
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
            serde_json::json!({"format":"gridloom-review-pack", "version":2, "exported_at":Utc::now(), "status":status, "worksheet":sheet, "reviews":reviews, "explanations":explanations, "limitations":["This JSON contains local workbook evidence and reproducible cached inputs, not authenticated signatures or proof of external ledger completeness.", "Imported source CSV is retained unchanged; worksheet edits are represented separately. Review records retain their own input snapshots."]}),
        )
    }
}

fn exact_amount(value: Option<&Value>) -> Option<Decimal> {
    match value {
        Some(Value::Decimal(v)) => Some(*v),
        Some(Value::Integer(v)) => Some(Decimal::from(*v)),
        _ => None,
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
            format!(
                "={sign}SUMIF(C2:C{end},\"{mapping}\",{net}2:{net}{end})",
                net = if period.reconciliation_version > 0 {
                    "M"
                } else {
                    "F"
                }
            ),
        );
    }
    put_text(&mut sheet, 6, 7, "Net profit");
    put(&mut sheet, 6, 8, "=I2-I3-I4+I5-I6".into());
    if !period.report_lines.is_empty() {
        sheet
            .cells
            .retain(|_, c| !(c.coord.col == 7 || c.coord.col == 8));
        put_text(&mut sheet, 0, 7, &format!("Report {}", period.period));
        put_text(&mut sheet, 0, 8, "Amount");
        for (i, line) in period.report_lines.iter().enumerate() {
            put_text(&mut sheet, i as u32 + 1, 7, &line.label);
            let children: Vec<_> = period
                .report_lines
                .iter()
                .enumerate()
                .filter(|(_, l)| l.parent_id.as_deref() == Some(line.id.as_str()))
                .collect();
            let sign = if line.normal_balance == NormalBalance::Credit {
                "-"
            } else {
                ""
            };
            let formula = if children.is_empty() {
                format!("={sign}SUMIF(C2:C{end},\"{}\",M2:M{end})", line.id)
            } else {
                let terms: Vec<_> = children
                    .iter()
                    .map(|(j, l)| {
                        format!(
                            "{}I{}",
                            if l.normal_balance == NormalBalance::Credit {
                                "-"
                            } else {
                                "+"
                            },
                            j + 2
                        )
                    })
                    .collect();
                format!("={sign}(0{})", terms.join(""))
            };
            put(&mut sheet, i as u32 + 1, 8, formula);
        }
    }
    if period.reconciliation_version > 0 {
        for (col, label) in [
            (10, "Imported balance"),
            (11, "Documented adjustments"),
            (12, "Reported balance"),
            (14, "Adjustment account"),
            (15, "Journal ID"),
            (16, "Adjustment debit"),
            (17, "Adjustment credit"),
            (18, "Rationale"),
            (19, "Author"),
            (20, "Evidence SHA-256"),
        ] {
            put_text(&mut sheet, 0, col, label);
        }
        let mut journal_row = 1;
        let mut refs: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for journal in &period.adjustments {
            for line in &journal.lines {
                put_text(&mut sheet, journal_row, 14, &line.account);
                put_text(&mut sheet, journal_row, 15, &journal.id.to_string());
                put(&mut sheet, journal_row, 16, line.debit.to_string());
                put(&mut sheet, journal_row, 17, line.credit.to_string());
                put_text(&mut sheet, journal_row, 18, &journal.rationale);
                put_text(&mut sheet, journal_row, 19, &journal.author);
                put_text(
                    &mut sheet,
                    journal_row,
                    20,
                    &journal
                        .evidence
                        .iter()
                        .map(|e| e.sha256.clone())
                        .collect::<Vec<_>>()
                        .join(";"),
                );
                refs.entry(line.account.clone()).or_default().push(format!(
                    "+Q{}-R{}",
                    journal_row + 1,
                    journal_row + 1
                ));
                journal_row += 1;
            }
        }
        for (i, a) in period.accounts.iter().enumerate() {
            let row = i as u32 + 1;
            put(&mut sheet, row, 10, (a.debit - a.credit).to_string());
            put(
                &mut sheet,
                row,
                11,
                format!(
                    "=0{}",
                    refs.get(&a.account).map(|r| r.join("")).unwrap_or_default()
                ),
            );
            put(&mut sheet, row, 12, format!("=K{}+L{}", row + 1, row + 1));
        }
        sheet.max_rows = sheet.max_rows.max(journal_row + 10);
    }
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

fn report_labels(period: &ClosePeriod) -> Vec<String> {
    if period.report_lines.is_empty() {
        [
            "Revenue",
            "Cost of sales",
            "Operating expenses",
            "Other income",
            "Other expense",
            "Net profit",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    } else {
        period
            .report_lines
            .iter()
            .map(|l| l.label.clone())
            .collect()
    }
}
fn valid_mapping(period: &ClosePeriod, mapping: &str) -> bool {
    if period.report_lines.is_empty() {
        MAPPINGS.contains(&mapping)
    } else {
        period.report_lines.iter().any(|line| line.id == mapping)
            && !period
                .report_lines
                .iter()
                .any(|line| line.parent_id.as_deref() == Some(mapping))
    }
}
fn validate_period_mappings(
    period: &ClosePeriod,
    mappings: &BTreeMap<String, String>,
) -> Result<(), String> {
    for (account, mapping) in mappings {
        if !period.accounts.iter().any(|a| &a.account == account) {
            return Err(format!("Mapping refers to unknown account {account}."));
        }
        if !mapping.is_empty() && !valid_mapping(period, mapping) {
            return Err(format!("Unknown or non-leaf report mapping {mapping}."));
        }
    }
    Ok(())
}
fn adjustment_total(period: &ClosePeriod, account: &str) -> Result<Decimal, String> {
    period
        .adjustments
        .iter()
        .flat_map(|j| &j.lines)
        .filter(|l| l.account == account)
        .try_fold(Decimal::ZERO, |sum, line| {
            sum.checked_add(line.debit)?.checked_sub(line.credit)
        })
        .ok_or("Adjustment balance exceeds decimal precision".into())
}
fn validate_report(lines: &[ReportLine]) -> Result<(), String> {
    if lines.len() > 100 {
        return Err("A report supports at most 100 lines.".into());
    }
    let mut ids = BTreeSet::new();
    for line in lines {
        if line.id.is_empty()
            || line.id.len() > 80
            || !line
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
            || !ids.insert(line.id.clone())
            || line.label.trim().is_empty()
            || line.label.len() > 200
            || line.label.chars().any(char::is_control)
        {
            return Err("Report lines need unique alphanumeric IDs (underscore/hyphen allowed) and bounded plain-text labels.".into());
        }
    }
    for line in lines {
        let mut seen = BTreeSet::from([line.id.clone()]);
        let mut parent = line.parent_id.as_ref();
        while let Some(id) = parent {
            if !seen.insert(id.clone()) {
                return Err("Report hierarchy contains a cycle.".into());
            }
            let p = lines
                .iter()
                .find(|l| &l.id == id)
                .ok_or("Report parent does not exist")?;
            parent = p.parent_id.as_ref();
            if seen.len() > 20 {
                return Err("Report hierarchy supports at most 20 levels.".into());
            }
        }
    }
    Ok(())
}
