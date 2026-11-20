use crate::ast::StructuredRef;
use crate::chart::{ChartData, ChartDataSeries, ChartDefinition};
use crate::db::{DbConnection, EngineKind};
use crate::depgraph::{DepNode, DependencyGraph};
use crate::error::CellError;
use crate::evaluator::evaluate_expr;
use crate::functions::EvaluationContext;
use crate::lineage::analyze_lineage;
use crate::parser::parse_formula;
use crate::pivot::{compute_pivot, PivotDefinition, PivotResult};
use crate::review::ReviewRecord;
use crate::spill::SpillManager;
use crate::table::{NamedSource, WorksheetTable};
use crate::value::{CellCoord, CellRange, Value};
use crate::virtual_view::{CellRenderData, ViewportRequest, ViewportResponse};
use crate::writeback::StagedRepository;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

/// Owns only the unique temporary path created by this save attempt.
struct SaveTemp {
    path: PathBuf,
    committed: bool,
}
impl Drop for SaveTemp {
    fn drop(&mut self) {
        if !self.committed {
            if let Err(error) = fs::remove_file(&self.path) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(path = %self.path.display(), %error, "Unable to remove failed-save temporary file");
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellData {
    pub coord: CellCoord,
    pub raw_input: String,
    pub formula: Option<String>,
    pub evaluated_value: Value,
    pub number_format: Option<String>,
    pub is_dirty: bool,
    #[serde(default)]
    pub style: crate::spreadsheet::CellStyle,
}

impl CellData {
    pub fn new(coord: CellCoord, raw_input: String) -> Self {
        let is_formula = raw_input.trim().starts_with('=');
        let formula = if is_formula {
            Some(raw_input.trim().to_string())
        } else {
            None
        };
        let evaluated_value = if !is_formula {
            parse_literal_or_text(&raw_input)
        } else {
            Value::Blank
        };
        Self {
            coord,
            raw_input,
            formula,
            evaluated_value,
            number_format: None,
            is_dirty: is_formula,
            style: Default::default(),
        }
    }
}

fn parse_literal_or_text(input: &str) -> Value {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Value::Blank;
    }
    if trimmed.eq_ignore_ascii_case("TRUE") {
        return Value::Boolean(true);
    }
    if trimmed.eq_ignore_ascii_case("FALSE") {
        return Value::Boolean(false);
    }
    if trimmed.eq_ignore_ascii_case("NULL") {
        return Value::Null;
    }
    if let Ok(i) = trimmed.parse::<i64>() {
        return Value::Integer(i);
    }
    if let Ok(d) = rust_decimal::Decimal::from_str_exact(trimmed) {
        return Value::Decimal(d);
    }
    if let Ok(f) = trimmed.parse::<f64>() {
        return Value::Float(f);
    }
    Value::Text(input.to_string())
}

/// Exact account filter for the read-only SQLite journal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerAccount {
    pub column: String,
    pub value: String,
}

/// Reproducible definition of a local journal query, not a cached result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerSpec {
    pub connection: String,
    pub table: String,
    pub date: String,
    pub cents: String,
    pub from: Option<String>,
    pub through: Option<String>,
    pub account: Option<LedgerAccount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerLink {
    pub spec: LedgerSpec,
    pub source_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sheet {
    pub name: String,
    pub cells: HashMap<String, CellData>,
    pub tables: HashMap<String, WorksheetTable>,
    pub spills: SpillManager,
    pub max_rows: u32,
    pub max_cols: u32,
    /// Sparse, presentation-only column widths in CSS pixels.
    #[serde(default)]
    pub column_widths: HashMap<u32, u16>,
    #[serde(default)]
    pub freeze_top_row: bool,
    #[serde(default)]
    pub freeze_first_column: bool,
    /// Optional counts distinguish a saved zero from an old workbook with only flags.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frozen_rows: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frozen_columns: Option<u32>,
    #[serde(default)]
    pub show_formulas: bool,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub ledger_links: HashMap<String, LedgerLink>,
}

impl Sheet {
    pub fn new(name: String) -> Self {
        Self {
            name,
            cells: HashMap::new(),
            tables: HashMap::new(),
            spills: SpillManager::new(),
            max_rows: 1000,
            max_cols: 100,
            column_widths: HashMap::new(),
            freeze_top_row: false,
            freeze_first_column: false,
            frozen_rows: None,
            frozen_columns: None,
            show_formulas: false,
            ledger_links: HashMap::new(),
        }
    }

    pub fn frozen_row_count(&self) -> u32 {
        self.frozen_rows
            .unwrap_or(u32::from(self.freeze_top_row))
            .min(self.max_rows.saturating_sub(1))
    }

    pub fn frozen_column_count(&self) -> u32 {
        self.frozen_columns
            .unwrap_or(u32::from(self.freeze_first_column))
            .min(self.max_cols.saturating_sub(1))
    }

    pub fn get_cell(&self, coord: &CellCoord) -> Option<&CellData> {
        self.cells.get(&coord.to_a1())
    }

    pub fn get_cell_mut(&mut self, coord: &CellCoord) -> Option<&mut CellData> {
        self.cells.get_mut(&coord.to_a1())
    }

    pub fn is_cell_occupied(&self, coord: CellCoord) -> bool {
        if let Some(cell) = self.cells.get(&coord.to_a1()) {
            if !cell.evaluated_value.is_blank() {
                return true;
            }
        }
        for table in self.tables.values() {
            if table.range.contains(coord) {
                return true;
            }
        }
        false
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub name: String,
    pub kind: EngineKind,
    pub path: String,
}

pub struct Workbook {
    pub sheets: HashMap<String, Sheet>,
    pub active_sheet: String,
    pub connections: HashMap<String, DbConnection>,
    pub connection_configs: HashMap<String, ConnectionConfig>,
    pub named_sources: HashMap<String, NamedSource>,
    pub staged: StagedRepository,
    pub pivots: HashMap<String, PivotDefinition>,
    pub charts: HashMap<String, ChartDefinition>,
    pub dep_graph: DependencyGraph,
    pub reviews: Vec<ReviewRecord>,
    pub close_periods: Vec<crate::close::ClosePeriod>,
    pub stale_sources: BTreeSet<String>,
    pub stale_calculation_sheets: BTreeSet<String>,
    pub file_path: Option<PathBuf>,
    pub edit_history: crate::spreadsheet::EditHistory,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SerializedWorkbook {
    #[serde(default)]
    pub reviews: Vec<ReviewRecord>,
    #[serde(default)]
    pub close_periods: Vec<crate::close::ClosePeriod>,
    #[serde(default)]
    pub stale_sources: BTreeSet<String>,
    #[serde(default)]
    pub stale_calculation_sheets: BTreeSet<String>,
    pub sheets: HashMap<String, Sheet>,
    pub active_sheet: String,
    pub connection_configs: HashMap<String, ConnectionConfig>,
    pub named_sources: HashMap<String, NamedSource>,
    pub staged: StagedRepository,
    pub pivots: HashMap<String, PivotDefinition>,
    pub charts: HashMap<String, ChartDefinition>,
}

impl Workbook {
    pub fn new() -> Self {
        let mut sheets = HashMap::new();
        let default_sheet = "Sheet1".to_string();
        sheets.insert(default_sheet.clone(), Sheet::new(default_sheet.clone()));

        Self {
            sheets,
            active_sheet: default_sheet,
            connections: HashMap::new(),
            connection_configs: HashMap::new(),
            named_sources: HashMap::new(),
            staged: StagedRepository::new(),
            pivots: HashMap::new(),
            charts: HashMap::new(),
            dep_graph: DependencyGraph::new(),
            reviews: Vec::new(),
            close_periods: Vec::new(),
            stale_sources: BTreeSet::new(),
            stale_calculation_sheets: BTreeSet::new(),
            file_path: None,
            edit_history: Default::default(),
        }
    }

    fn check_live_connection_rebind(&self, name: &str, kind: EngineKind, path: &Path) -> Result<(), CellError> {
        // Open/reconnect can restore the exact configuration, but must not
        // redirect a live table or outstanding patch to another database.
        let used = self.sheets.values().any(|sheet| sheet.tables.values().any(|table|
                table.live.as_ref().is_some_and(|live| live.connection_name == name)))
            || self.staged.patches.values().any(|patch| patch.connection_name == name);
        if used && self.connection_configs.get(name).is_none_or(|existing|
            existing.kind != kind || Path::new(&existing.path) != path) {
            return Err(CellError::Sql("This connection is used by a live table or staged edit. Reconnect the same database or use a different connection name.".into()));
        }
        Ok(())
    }

    pub fn add_sqlite_connection<P: AsRef<Path>>(
        &mut self,
        name: &str,
        path: P,
    ) -> Result<(), CellError> {
        self.check_live_connection_rebind(name, EngineKind::Sqlite, path.as_ref())?;
        let p_str = path.as_ref().to_string_lossy().to_string();
        let conn = DbConnection::open_sqlite(&path)?;
        self.connections.insert(name.to_string(), conn);
        self.connection_configs.insert(
            name.to_string(),
            ConnectionConfig {
                name: name.to_string(),
                kind: EngineKind::Sqlite,
                path: p_str,
            },
        );
        Ok(())
    }

    pub fn add_sqlite_memory(&mut self, name: &str) -> Result<(), CellError> {
        self.check_live_connection_rebind(name, EngineKind::Sqlite, Path::new(":memory:"))?;
        let conn = DbConnection::open_sqlite_memory()?;
        self.connections.insert(name.to_string(), conn);
        self.connection_configs.insert(
            name.to_string(),
            ConnectionConfig {
                name: name.to_string(),
                kind: EngineKind::Sqlite,
                path: ":memory:".to_string(),
            },
        );
        Ok(())
    }

    pub fn add_duckdb_connection<P: AsRef<Path>>(
        &mut self,
        name: &str,
        path: P,
    ) -> Result<(), CellError> {
        self.check_live_connection_rebind(name, EngineKind::DuckDb, path.as_ref())?;
        let p_str = path.as_ref().to_string_lossy().to_string();
        let conn = DbConnection::open_duckdb(&path)?;
        self.connections.insert(name.to_string(), conn);
        self.connection_configs.insert(
            name.to_string(),
            ConnectionConfig {
                name: name.to_string(),
                kind: EngineKind::DuckDb,
                path: p_str,
            },
        );
        Ok(())
    }

    pub fn add_duckdb_memory(&mut self, name: &str) -> Result<(), CellError> {
        self.check_live_connection_rebind(name, EngineKind::DuckDb, Path::new(":memory:"))?;
        let conn = DbConnection::open_duckdb_memory()?;
        self.connections.insert(name.to_string(), conn);
        self.connection_configs.insert(
            name.to_string(),
            ConnectionConfig {
                name: name.to_string(),
                kind: EngineKind::DuckDb,
                path: ":memory:".to_string(),
            },
        );
        Ok(())
    }

    pub fn create_named_source(
        &mut self,
        name: &str,
        conn_name: &str,
        sql: &str,
    ) -> Result<NamedSource, CellError> {
        self.clear_edit_history();
        // Replacing a source would require atomic schema/range/collision migration for every table.
        // Keep source creation distinct from refresh so changed column order cannot silently relabel data.
        if self.named_sources.contains_key(name) {
            return Err(CellError::Sql(format!(
                "Data source '{name}' already exists. Refresh it or choose a different name."
            )));
        }
        let conn = self
            .connections
            .get(conn_name)
            .ok_or_else(|| CellError::Sql(format!("Connection '{}' not found", conn_name)))?;

        // 1. Check lineage
        let dialect = match conn.kind() {
            EngineKind::Sqlite => "sqlite",
            EngineKind::DuckDb => "duckdb",
        };

        let schema_lookup = |tbl: &str| conn.get_table_schema(tbl).ok();
        let lineage = analyze_lineage(sql, dialect, schema_lookup);

        // 2. Execute read query
        let query_result = conn.execute_read_query(sql, &[])?;

        let source = NamedSource {
            name: name.to_string(),
            connection_name: conn_name.to_string(),
            sql: sql.to_string(),
            parameters: Vec::new(),
            last_result: Some(query_result),
            lineage: Some(lineage),
        };

        self.named_sources.insert(name.to_string(), source.clone());
        self.stale_sources.remove(name);
        let sheets: Vec<_> = self.sheets.keys().cloned().collect();
        for sheet in sheets {
            self.recalculate(&sheet)?;
        }
        self.invalidate_changed_reviews();
        Ok(source)
    }

    pub fn refresh_source(&mut self, name: &str) -> Result<(), CellError> {
        self.edit_history = Default::default();
        let source = self.named_sources.get(name).ok_or(CellError::Name)?;
        let conn = self
            .connections
            .get(&source.connection_name)
            .ok_or(CellError::Ref)?;
        let result = conn.execute_read_query(&source.sql, &[])?;
        let columns: Vec<_> = result.columns.iter().map(|c| c.name.clone()).collect();
        let mut sheets = self.sheets.clone();
        for sheet in sheets.values_mut() {
            for table in sheet.tables.values_mut().filter(|t| t.live.is_none() && t.source_name == name) {
                let resized = WorksheetTable::new(
                    table.name.clone(),
                    name.to_string(),
                    table.anchor,
                    columns.clone(),
                    result.rows.len() as u32,
                );
                table.columns = resized.columns;
                table.range = resized.range;
            }
            for table in sheet.tables.values().filter(|t| t.live.is_none() && t.source_name == name) {
                if sheet
                    .tables
                    .values()
                    .any(|other| other.name != table.name && other.range.intersects(&table.range))
                    || sheet
                        .cells
                        .values()
                        .any(|c| table.range.contains(c.coord) && (!c.raw_input.is_empty()))
                    || sheet
                        .spills
                        .active_spills
                        .values()
                        .any(|s| s.range.intersects(&table.range))
                {
                    return Err(CellError::Spill);
                }
                sheet.max_rows = sheet.max_rows.max(table.range.end_row + 1);
                sheet.max_cols = sheet.max_cols.max(table.range.end_col + 1);
            }
        }
        self.sheets = sheets;
        self.named_sources.get_mut(name).unwrap().last_result = Some(result);
        self.stale_sources.remove(name);
        let sheet_names: Vec<_> = self.sheets.keys().cloned().collect();
        for sheet_name in sheet_names {
            self.recalculate(&sheet_name)?;
        }
        Ok(())
    }

    pub fn insert_table(
        &mut self,
        sheet_name: &str,
        table_name: &str,
        source_name: &str,
        anchor: CellCoord,
    ) -> Result<WorksheetTable, CellError> {
        let source = self
            .named_sources
            .get(source_name)
            .ok_or_else(|| CellError::Sql(format!("Data source '{}' not found", source_name)))?;
        let query_res = source
            .last_result
            .as_ref()
            .ok_or_else(|| CellError::Sql("Data source has no query result".to_string()))?;
        let columns = query_res.columns.iter().map(|c| c.name.clone()).collect();
        let row_count = u32::try_from(query_res.rows.len()).map_err(|_| CellError::Ref)?;
        self.attach_table_extent(
            sheet_name,
            table_name,
            source_name,
            anchor,
            columns,
            row_count,
            None,
            false,
        )
    }

    pub fn insert_live_table(
        &mut self, sheet_name:&str, table_name:&str, connection_name:&str, sql:&str, anchor:CellCoord,
    ) -> Result<WorksheetTable,CellError> {
        if table_name.trim().is_empty() || table_name.len()>128 {return Err(CellError::Sql("Choose a worksheet table name (1–128 bytes).".into()));}
        let Some(DbConnection::Sqlite(engine)) = self.connections.get(connection_name) else {
            return Err(CellError::Sql("Live worksheets currently require a connected SQLite database.".into()));
        };
        if self.connection_configs.get(connection_name).is_some_and(|c| c.path==":memory:") {
            return Err(CellError::Sql("Live worksheets require a persistent SQLite file.".into()));
        }
        let (source,columns)=crate::live_table::LiveTable::attach(engine,connection_name,sql)?;
        let count=source.row_count;
        self.attach_table_extent(sheet_name,table_name,"",anchor,columns.into_iter().map(|c|c.name).collect(),count,Some(source),false)
    }

    pub fn refresh_live_table(&mut self,sheet_name:&str,table_name:&str)->Result<WorksheetTable,CellError> {
        let existing=self.sheets.get(sheet_name).and_then(|s|s.tables.get(table_name))
            .ok_or_else(||CellError::Sql("Live worksheet table not found.".into()))?;
        let live=existing.live.as_ref().ok_or_else(||CellError::Sql("This table is not a live worksheet table.".into()))?;
        let Some(DbConnection::Sqlite(engine))=self.connections.get(&live.connection_name) else {
            return Err(CellError::Sql("Live worksheet database is not connected.".into()));
        };
        let (refreshed,cols)=crate::live_table::LiveTable::attach(engine,&live.connection_name,&live.sql)?;
        let cols:Vec<_>=cols.into_iter().map(|c|c.name).collect();
        if cols!=existing.columns || refreshed.primary_key!=live.primary_key {
            return Err(CellError::Sql("Live worksheet source schema changed; reattach its table.".into()));
        }
        self.attach_table_extent(sheet_name,table_name,"",existing.anchor,cols,refreshed.row_count,Some(refreshed),true)
    }

    /// Attach dimensions using the same checks for materialized and future paged sources.
    /// No work scales with the incoming table area.
    fn attach_table_extent(
        &mut self,
        sheet_name: &str,
        table_name: &str,
        source_name: &str,
        anchor: CellCoord,
        columns: Vec<String>,
        row_count: u32,
        live: Option<crate::live_table::LiveTable>,
        replacing: bool,
    ) -> Result<WorksheetTable, CellError> {
        let column_count = u32::try_from(columns.len()).map_err(|_| CellError::Ref)?;
        if column_count == 0 {
            return Err(CellError::Value);
        }
        let rows = anchor
            .row
            .checked_add(row_count.max(1))
            .and_then(|n| n.checked_add(1))
            .ok_or(CellError::Ref)?;
        let cols = anchor.col.checked_add(column_count).ok_or(CellError::Ref)?;
        let mut table = WorksheetTable::new(
            table_name.into(),
            source_name.into(),
            anchor,
            columns,
            row_count,
        );
        table.live = live;
        let sheet = self.sheets.get(sheet_name).ok_or(CellError::Ref)?;
        if replacing {
            table.formats=sheet.tables.get(table_name).ok_or(CellError::Ref)?.formats.clone();
        }
        if (!replacing && sheet.tables.keys().any(|name|name.eq_ignore_ascii_case(table_name)))
            || sheet
                .tables
                .values()
                .any(|existing| (!replacing || existing.name!=table_name) && existing.range.intersects(&table.range))
            || sheet.cells.values().any(|cell| {
                table.range.contains(cell.coord)
                    && (cell.formula.is_some()
                        || !cell.raw_input.is_empty()
                        || !cell.evaluated_value.is_blank())
            })
            || sheet
                .spills
                .active_spills
                .values()
                .any(|spill| spill.range.intersects(&table.range))
        {
            return Err(CellError::Spill);
        }
        let original_sheets = self.sheets.clone();
        let original_graph = self.dep_graph.clone();
        let sheet = self
            .sheets
            .get_mut(sheet_name)
            .expect("worksheet checked above");
        sheet.max_rows = sheet.max_rows.max(rows);
        sheet.max_cols = sheet.max_cols.max(cols);
        sheet.tables.insert(table_name.into(), table.clone());
        if let Err(error) = self.recalculate(sheet_name) {
            self.sheets = original_sheets;
            self.dep_graph = original_graph;
            return Err(error);
        }
        self.clear_edit_history();
        self.invalidate_changed_reviews();
        Ok(table)
    }

    pub fn set_cell(
        &mut self,
        sheet_name: &str,
        coord: CellCoord,
        raw_input: String,
    ) -> Result<(), CellError> {
        self.edit_range(sheet_name, coord, vec![vec![raw_input]])
    }

    pub fn recalculate(&mut self, sheet_name: &str) -> Result<(), CellError> {
        if !self.sheets.contains_key(sheet_name) {
            return Err(CellError::Ref);
        }
        let formulas: Vec<_> = self
            .sheets
            .iter()
            .flat_map(|(name, sheet)| {
                sheet.cells.values().filter_map(|c| {
                    c.formula
                        .as_ref()
                        .map(|f| (name.clone(), c.coord, f.clone()))
                })
            })
            .collect();
        for _ in 0..=formulas.len() {
            let layout = |sheets: &HashMap<String, Sheet>| -> HashMap<_, _> {
                sheets
                    .iter()
                    .flat_map(|(name, s)| {
                        s.spills
                            .active_spills
                            .iter()
                            .map(move |(a, s)| ((name.clone(), *a), s.range))
                    })
                    .collect()
            };
            let before = layout(&self.sheets);
            let generation = self.dep_graph.generation;
            self.dep_graph = DependencyGraph::new();
            self.dep_graph.generation = generation;
            for (name, coord, formula) in &formulas {
                if let Ok(ast) = parse_formula(formula) {
                    self.extract_ast_dependencies(
                        name,
                        &DepNode::SheetCell(name.clone(), *coord),
                        &ast,
                    )?;
                }
            }
            let nodes: Vec<_> = formulas
                .iter()
                .map(|(n, c, _)| DepNode::SheetCell(n.clone(), *c))
                .collect();
            for node in self.dep_graph.get_recalculation_order(&nodes)? {
                if let DepNode::SheetCell(name, coord) = node {
                    self.evaluate_single_cell(&name, coord)?;
                }
            }
            if before == layout(&self.sheets) {
                self.dep_graph.generation += 1;
                self.stale_calculation_sheets.clear();
                self.invalidate_changed_reviews();
                return Ok(());
            }
        }
        Err(CellError::Cycle)
    }

    fn evaluate_single_cell(
        &mut self,
        sheet_name: &str,
        coord: CellCoord,
    ) -> Result<(), CellError> {
        let formula_opt = {
            let sheet = self.sheets.get(sheet_name).ok_or(CellError::Ref)?;
            sheet.get_cell(&coord).and_then(|c| c.formula.clone())
        };

        if let Some(formula) = formula_opt {
            let ast_result = parse_formula(&formula);
            let eval_value = match ast_result {
                Ok(ast) => {
                    let ctx = WorkbookEvalContext {
                        workbook: self,
                        sheet_name,
                        current_coord: coord,
                    };
                    evaluate_expr(&ast, &ctx).unwrap_or_else(Value::Error)
                }
                Err(e) => Value::Error(e),
            };

            // Check if returned an array -> dynamic spill!
            let sheet = self.sheets.get_mut(sheet_name).ok_or(CellError::Ref)?;
            if let Value::Array(ref rows) = eval_value {
                let r_count = rows.len();
                let c_count = rows.first().map_or(0, |r| r.len());

                let can_spill = sheet
                    .spills
                    .can_spill(coord, r_count, c_count, |c| sheet.is_cell_occupied(c));

                if can_spill {
                    if r_count > 0 && c_count > 0 {
                        sheet.spills.register_spill(coord, r_count, c_count);
                    } else {
                        sheet.spills.clear_anchor(&coord);
                    }
                    if let Some(c) = sheet.get_cell_mut(&coord) {
                        c.evaluated_value = eval_value;
                        c.is_dirty = false;
                    }
                } else {
                    sheet.spills.clear_anchor(&coord);
                    if let Some(c) = sheet.get_cell_mut(&coord) {
                        c.evaluated_value = Value::Error(CellError::Spill);
                        c.is_dirty = false;
                    }
                }
            } else {
                sheet.spills.clear_anchor(&coord);
                if let Some(c) = sheet.get_cell_mut(&coord) {
                    c.evaluated_value = eval_value;
                    c.is_dirty = false;
                }
            }
        }
        Ok(())
    }

    fn extract_ast_dependencies(
        &mut self,
        sheet_name: &str,
        node: &DepNode,
        expr: &crate::ast::Expr,
    ) -> Result<(), CellError> {
        use crate::ast::Expr;
        match expr {
            Expr::SheetRef { sheet, expr } => self.extract_ast_dependencies(sheet, node, expr)?,
            Expr::CellRef(coord) => {
                self.add_cell_dependency(sheet_name, node, *coord)?;
            }
            Expr::RangeRef(range) => {
                if (range.end_row as u64 - range.start_row as u64 + 1)
                    * (range.end_col as u64 - range.start_col as u64 + 1)
                    > 100_000
                {
                    return Err(CellError::Ref);
                }
                for r in range.start_row..=range.end_row {
                    for c in range.start_col..=range.end_col {
                        self.add_cell_dependency(sheet_name, node, CellCoord::new(r, c))?;
                    }
                }
            }
            Expr::SpillRef(coord) => {
                self.dep_graph
                    .add_dependency(node.clone(), DepNode::SheetCell(sheet_name.into(), *coord))?;
            }
            Expr::StructuredRef(sref) => {
                self.dep_graph
                    .add_dependency(node.clone(), DepNode::Table(sref.table_name.clone()))?;
            }
            Expr::Unary { expr, .. } => self.extract_ast_dependencies(sheet_name, node, expr)?,
            Expr::Binary { left, right, .. } => {
                self.extract_ast_dependencies(sheet_name, node, left)?;
                self.extract_ast_dependencies(sheet_name, node, right)?;
            }
            Expr::FunctionCall { args, .. } => {
                for arg in args {
                    self.extract_ast_dependencies(sheet_name, node, arg)?;
                }
            }
            Expr::ArrayLiteral(rows) => {
                for row in rows {
                    for cell in row {
                        self.extract_ast_dependencies(sheet_name, node, cell)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn add_cell_dependency(
        &mut self,
        sheet_name: &str,
        node: &DepNode,
        coord: CellCoord,
    ) -> Result<(), CellError> {
        let anchor = self
            .sheets
            .get(sheet_name)
            .ok_or(CellError::Ref)?
            .spills
            .get_spill_at(&coord)
            .map(|s| s.anchor)
            .unwrap_or(coord);
        self.dep_graph
            .add_dependency(node.clone(), DepNode::SheetCell(sheet_name.into(), anchor))
    }

    pub fn get_viewport(&self, req: &ViewportRequest) -> Result<ViewportResponse, CellError> {
        let sheet = self.sheets.get(&req.sheet_name).ok_or(CellError::Ref)?;

        if sheet.max_rows==0 || sheet.max_cols==0 || req.start_row>req.end_row || req.start_col>req.end_col
            || req.start_row>=sheet.max_rows || req.start_col>=sheet.max_cols || req.overscan>100 { return Err(CellError::Ref); }
        let start_row = req.start_row.saturating_sub(req.overscan);
        let end_row = req.end_row.saturating_add(req.overscan).min(sheet.max_rows-1);
        let start_col = req.start_col.saturating_sub(req.overscan);
        let end_col = req.end_col.saturating_add(req.overscan).min(sheet.max_cols-1);
        let area=u64::from(end_row-start_row+1)*u64::from(end_col-start_col+1);
        let window_range=CellRange::new(start_row,start_col,end_row,end_col);
        if area>20_000 && sheet.tables.values().any(|table|table.live.is_some()&&table.range.intersects(&window_range)) {
            return Err(CellError::Sql("Select a smaller range to read live worksheet data.".into()));
        }
        let live_windows: HashMap<_,_> = sheet.tables.values().filter_map(|table|{
            let spec=table.live.as_ref()?;
            let first=start_row.max(table.anchor.row+1);
            let last=end_row.min(table.range.end_row);
            if first>last || table.range.start_col>end_col || table.range.end_col<start_col {return None;}
            let values = if let Some(DbConnection::Sqlite(engine))=self.connections.get(&spec.connection_name) {
                spec.window(engine,first-table.anchor.row-1,last-first+1,&table.columns)
            } else { Err(CellError::Sql("Live worksheet database is not connected.".into())) };
            Some((table.name.as_str(),(first,values)))
        }).collect();
        let mut cells_data = HashMap::new();

        for r in start_row..=end_row {
            for c in start_col..=end_col {
                let coord = CellCoord::new(r, c);
                let a1 = coord.to_a1();

                // 1. Check if cell is in a worksheet table
                let mut table_hit = None;
                for table in sheet.tables.values() {
                    if table.range.contains(coord) {
                        let is_header = coord.row == table.anchor.row;
                        table_hit = Some((table, is_header));
                        break;
                    }
                }

                if let Some((table, is_header)) = table_hit {
                    let style = table.cell_style(coord);
                    let number_format = style.number_format.clone();
                    let col_offset = (coord.col - table.anchor.col) as usize;
                    if is_header {
                        let col_name = table.columns.get(col_offset).cloned().unwrap_or_default();
                        cells_data.insert(
                            a1.clone(),
                            CellRenderData {
                                raw_input: None,
                                style,
                                coord,
                                display_text: col_name.clone(),
                                raw_value: Value::Text(col_name),
                                formula: None,
                                number_format,
                                is_staged: false,
                                is_spill: false,
                                is_error: false,
                                table_name: Some(table.name.clone()),
                                is_table_header: true,
                                live_edit: None,
                            },
                        );
                        continue;
                    } else if let Some(live)=&table.live {
                        let key_index=table.columns.iter().position(|c|c==&live.primary_key);
                        let fetched=live_windows.get(table.name.as_str()).ok_or(CellError::Ref)?;
                        let (value,live_edit,is_staged)=match &fetched.1 {
                            Err(err)=>(Value::Error(err.clone()),None,false),
                            Ok(rows)=>if let Some(row)=rows.get((r-fetched.0) as usize) {
                                let original=row.get(col_offset).ok_or(CellError::Ref)?;
                                let key=key_index.and_then(|idx|row.get(idx)).ok_or(CellError::Ref)?;
                                let column=&table.columns[col_offset];
                                let edit=live.edit(column,key,original);
                                let overlay=self.staged.patches.values().find(|patch|
                                    patch.connection_name==live.connection_name && patch.table_name.eq_ignore_ascii_case(&live.base_table)
                                        && matches!(patch.pk_columns.as_slice(),[pk] if pk.eq_ignore_ascii_case(&live.primary_key))
                                        && matches!(patch.pk_values.as_slice(),[pk] if pk.is_not_distinct_from(key))
                                        && patch.column_name.eq_ignore_ascii_case(column));
                                (overlay.map_or_else(||original.clone(),|patch|patch.proposed_value.clone()),edit,overlay.is_some())
                            } else {(Value::Blank,None,false)},
                        };
                        let is_error=value.is_error();
                        cells_data.insert(a1,CellRenderData {
                            raw_input:None, style, coord,
                            display_text:crate::spreadsheet::format_value(&value,number_format.as_deref()),
                            raw_value:value, formula:None, number_format, is_staged, is_spill:false, is_error,
                            table_name:Some(table.name.clone()),is_table_header:false,live_edit,
                        });
                        continue;
                    } else {
                        // Data row from source
                        let row_offset = (coord.row - table.anchor.row - 1) as usize;
                        if let Some(source) = self.named_sources.get(&table.source_name) {
                            if let Some(res) = &source.last_result {
                                if let Some(row_data) = res.rows.get(row_offset) {
                                    let mut val =
                                        row_data.get(col_offset).cloned().unwrap_or(Value::Blank);
                                    let col_name =
                                        table.columns.get(col_offset).cloned().unwrap_or_default();

                                    // Check staged overlay
                                    let mut is_staged = false;
                                    for patch in self.staged.patches.values() {
                                        let matches_row =
                                            source.lineage.as_ref().is_some_and(|lineage| {
                                                patch.connection_name == source.connection_name
                                                    && lineage.base_table.as_deref()
                                                        == Some(&patch.table_name)
                                                    && !patch.pk_columns.is_empty()
                                                    && patch.pk_columns.len()
                                                        == patch.pk_values.len()
                                                    && patch
                                                        .pk_columns
                                                        .iter()
                                                        .zip(&patch.pk_values)
                                                        .all(|(key, value)| {
                                                            res.columns
                                                                .iter()
                                                                .position(|c| c.name == *key)
                                                                .and_then(|idx| row_data.get(idx))
                                                                .is_some_and(|actual| {
                                                                    actual
                                                                        .is_not_distinct_from(value)
                                                                })
                                                        })
                                            });
                                        if matches_row && patch.column_name == col_name {
                                            val = patch.proposed_value.clone();
                                            is_staged = true;
                                            break;
                                        }
                                    }

                                    cells_data.insert(
                                        a1.clone(),
                                        CellRenderData {
                                            raw_input: None,
                                            style,
                                            coord,
                                            display_text: crate::spreadsheet::format_value(
                                                &val,
                                                number_format.as_deref(),
                                            ),
                                            raw_value: val,
                                            formula: None,
                                            number_format,
                                            is_staged,
                                            is_spill: false,
                                            is_error: false,
                                            table_name: Some(table.name.clone()),
                                            is_table_header: false,
                                live_edit: None,
                                        },
                                    );
                                    continue;
                                }
                            }
                        }

                        // An empty result still reserves a body row; its formatting is live
                        // even before the next refresh supplies rows.
                        cells_data.insert(
                            a1.clone(),
                            CellRenderData {
                                raw_input: None,
                                style,
                                coord,
                                display_text: String::new(),
                                raw_value: Value::Blank,
                                formula: None,
                                number_format,
                                is_staged: false,
                                is_spill: false,
                                is_error: false,
                                table_name: Some(table.name.clone()),
                                is_table_header: false,
                                live_edit: None,
                            },
                        );
                        continue;
                    }
                }

                // 2. Check dynamic spill cell
                if let Some(spill) = sheet.spills.get_spill_at(&coord) {
                    if coord != spill.anchor {
                        // Spilled cell value
                        let r_offset = (coord.row - spill.anchor.row) as usize;
                        let c_offset = (coord.col - spill.anchor.col) as usize;
                        if let Some(anchor_cell) = sheet.get_cell(&spill.anchor) {
                            if let Value::Array(ref rows) = anchor_cell.evaluated_value {
                                let val = rows
                                    .get(r_offset)
                                    .and_then(|row| row.get(c_offset))
                                    .cloned()
                                    .unwrap_or(Value::Blank);

                                cells_data.insert(
                                    a1.clone(),
                                    CellRenderData {
                                        raw_input: None,
                                        style: Default::default(),
                                        coord,
                                        display_text: val.to_string_repr(),
                                        raw_value: val,
                                        formula: None,
                                        number_format: None,
                                        is_staged: false,
                                        is_spill: true,
                                        is_error: false,
                                        table_name: None,
                                        is_table_header: false,
                                live_edit: None,
                                    },
                                );
                                continue;
                            }
                        }
                    }
                }

                // 3. Regular cell
                if let Some(cell) = sheet.get_cell(&coord) {
                    let visible_value = match &cell.evaluated_value {
                        Value::Array(rows) => rows
                            .first()
                            .and_then(|r| r.first())
                            .cloned()
                            .unwrap_or(Value::Blank),
                        value => value.clone(),
                    };
                    let is_err = visible_value.is_error();
                    cells_data.insert(
                        a1,
                        CellRenderData {
                            raw_input: Some(cell.raw_input.clone()),
                            style: cell.style.clone(),
                            coord,
                            display_text: crate::spreadsheet::format_value(
                                &visible_value,
                                cell.number_format.as_deref(),
                            ),
                            raw_value: visible_value,
                            formula: cell.formula.clone(),
                            number_format: cell.number_format.clone(),
                            is_staged: false,
                            is_spill: false,
                            is_error: is_err,
                            table_name: None,
                            is_table_header: false,
                                live_edit: None,
                        },
                    );
                }
            }
        }

        Ok(ViewportResponse {
            sheet_name: req.sheet_name.clone(),
            start_row,
            start_col,
            end_row,
            end_col,
            cells: cells_data,
            total_rows: sheet.max_rows,
            total_cols: sheet.max_cols,
            generation: self.dep_graph.generation,
        })
    }

    pub fn compute_pivot_result(&self, name: &str) -> Result<PivotResult, CellError> {
        self.preview_pivot(self.pivots.get(name).ok_or(CellError::Name)?)
    }

    pub fn preview_pivot(&self, definition: &PivotDefinition) -> Result<PivotResult, CellError> {
        let source = self
            .named_sources
            .get(&definition.source_table)
            .ok_or(CellError::Name)?;
        let result = source
            .last_result
            .as_ref()
            .ok_or_else(|| CellError::Sql("No source snapshot. Use Refresh from source.".into()))?;
        let columns = result
            .columns
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<_>>();
        compute_pivot(definition, &columns, &result.rows)
    }

    /// Recompute a saved layout from its live read query, without overwriting its
    /// workbook snapshot, resizing inserted tables, or applying staged changes.
    pub fn refresh_pivot_result(&self, name: &str) -> Result<PivotResult, CellError> {
        let definition = self.pivots.get(name).ok_or(CellError::Name)?;
        let source = self
            .named_sources
            .get(&definition.source_table)
            .ok_or(CellError::Name)?;
        let connection = self
            .connections
            .get(&source.connection_name)
            .ok_or(CellError::Ref)?;
        if !source.parameters.is_empty() {
            return Err(CellError::Sql(
                "Parameterized pivot sources are not supported".into(),
            ));
        }
        let result = connection.execute_read_query(&source.sql, &[])?;
        let columns = result
            .columns
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<_>>();
        compute_pivot(definition, &columns, &result.rows)
    }

    pub fn compute_chart_data(&self, id: &str) -> Result<ChartData, CellError> {
        let chart_def = self.charts.get(id).ok_or_else(|| CellError::Name)?;

        let source = self
            .named_sources
            .get(&chart_def.source_table)
            .ok_or_else(|| CellError::Name)?;

        let res = source
            .last_result
            .as_ref()
            .ok_or_else(|| CellError::Sql("No source query result".to_string()))?;

        let cat_col_idx = res
            .columns
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(&chart_def.category_column))
            .ok_or(CellError::Value)?;

        let mut categories = Vec::new();
        for r in &res.rows {
            categories.push(
                r.get(cat_col_idx)
                    .map_or(String::new(), |v| v.to_string_repr()),
            );
        }

        let mut series_data = Vec::new();
        for s in &chart_def.series {
            let val_col_idx = res
                .columns
                .iter()
                .position(|c| c.name.eq_ignore_ascii_case(&s.value_column))
                .ok_or(CellError::Value)?;

            let mut values = Vec::new();
            for r in &res.rows {
                let cell = r.get(val_col_idx).unwrap_or(&Value::Blank);
                values.push(cell.to_f64().unwrap_or(0.0));
            }
            series_data.push(ChartDataSeries {
                name: s.name.clone(),
                values,
            });
        }

        Ok(ChartData {
            title: chart_def.title.clone(),
            chart_type: chart_def.chart_type,
            categories,
            series: series_data,
        })
    }

    pub fn apply_writeback(
        &mut self,
        connection_name: &str,
    ) -> Result<crate::writeback::AuditRecord, CellError> {
        self.edit_history = Default::default();
        let conn = self
            .connections
            .get(connection_name)
            .ok_or_else(|| CellError::Sql(format!("Connection '{}' not found", connection_name)))?;
        let record = self.staged.apply_writeback(conn, connection_name)?;
        self.stale_sources.extend(
            self.named_sources
                .values()
                .filter(|source| source.connection_name == connection_name)
                .map(|source| source.name.clone()),
        );
        self.stale_calculation_sheets
            .extend(self.sheets.keys().cloned());
        // Cached source results have not necessarily been refreshed after the database transaction.
        // Conservatively invalidate all local sign-offs, including direct SQL calculations.
        for review in &mut self.reviews {
            review.invalidated = true;
            review.invalidation_reason = Some(
                "Database writeback was applied. Refresh sources, recalculate and review again."
                    .into(),
            );
        }
        // Direct SQL formulas can be recalculated without replacing cached named-source results.
        // Keep a persistent guard on any sheet that cannot be recalculated after the commit.
        let sheets: Vec<_> = self.sheets.keys().cloned().collect();
        for sheet in sheets {
            let _ = self.recalculate(&sheet);
        }
        Ok(record)
    }

    /// Atomic file package save
    pub fn save_to_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), CellError> {
        self.write_workbook_file(path, true)
    }

    /// Write a recovery copy without changing the path used by explicit Save.
    pub fn save_recovery_copy<P: AsRef<Path>>(&mut self, path: P) -> Result<(), CellError> {
        self.write_workbook_file(path, false)
    }

    fn write_workbook_file<P: AsRef<Path>>(
        &mut self,
        path: P,
        set_path: bool,
    ) -> Result<(), CellError> {
        self.invalidate_changed_reviews();
        let p = path.as_ref().to_path_buf();
        let parent = p
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        p.file_name()
            .ok_or_else(|| CellError::Sql("Save path must name a file".into()))?;
        // Bounded name length also permits destinations near a filesystem's filename limit.
        let tmp_path = parent.join(format!(".gridloom-save-{}.tmp", uuid::Uuid::new_v4()));

        #[derive(Serialize)]
        struct Snapshot<'a> {
            reviews: &'a Vec<ReviewRecord>,
            close_periods: &'a Vec<crate::close::ClosePeriod>,
            stale_sources: &'a BTreeSet<String>,
            stale_calculation_sheets: &'a BTreeSet<String>,
            sheets: &'a HashMap<String, Sheet>,
            active_sheet: &'a String,
            connection_configs: &'a HashMap<String, ConnectionConfig>,
            named_sources: &'a HashMap<String, NamedSource>,
            staged: &'a StagedRepository,
            pivots: &'a HashMap<String, PivotDefinition>,
            charts: &'a HashMap<String, ChartDefinition>,
        }
        let serialized = Snapshot {
            reviews: &self.reviews,
            close_periods: &self.close_periods,
            stale_sources: &self.stale_sources,
            stale_calculation_sheets: &self.stale_calculation_sheets,
            sheets: &self.sheets,
            active_sheet: &self.active_sheet,
            connection_configs: &self.connection_configs,
            named_sources: &self.named_sources,
            staged: &self.staged,
            pivots: &self.pivots,
            charts: &self.charts,
        };

        // create_new rejects pre-existing files and symlinks instead of truncating them.
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&tmp_path)
            .map_err(|e| CellError::Sql(format!("Failed to create save temporary file: {}", e)))?;
        let mut temporary = SaveTemp {
            path: tmp_path.clone(),
            committed: false,
        };
        #[cfg(unix)]
        {
            // Preserve existing regular-file permission bits; new workbooks remain private.
            // Do not follow a destination symlink or copy its target's permissions.
            match fs::symlink_metadata(&p) {
                Ok(metadata) if metadata.is_file() => {
                    file.set_permissions(metadata.permissions()).map_err(|e| {
                        CellError::Sql(format!("Failed to preserve workbook permissions: {}", e))
                    })?
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(CellError::Sql(format!(
                        "Failed to inspect workbook permissions: {}",
                        error
                    )))
                }
            }
        }
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, &serialized)
            .map_err(|e| CellError::Sql(format!("Failed to serialize workbook: {}", e)))?;

        writer
            .flush()
            .map_err(|e| CellError::Sql(format!("Failed to flush workbook: {}", e)))?;
        writer.get_ref().sync_all().map_err(|e| {
            CellError::Sql(format!(
                "Failed to sync save file before replacement: {}",
                e
            ))
        })?;
        drop(writer);
        // Same-directory rename is the commit point. Failures before this preserve the target.
        fs::rename(&tmp_path, &p)
            .map_err(|e| CellError::Sql(format!("Failed to commit atomic save: {}", e)))?;
        temporary.committed = true;
        // The replacement is already committed. A directory-sync failure must not be
        // reported as a failed save (which would falsely imply the old file remains).
        // Best effort on Unix; this is not a full power-loss/crash recovery guarantee.
        #[cfg(unix)]
        if let Err(error) = File::open(parent).and_then(|directory| directory.sync_all()) {
            tracing::warn!(path = %parent.display(), %error, "Workbook replaced; directory durability could not be confirmed");
        }
        if set_path {
            self.file_path = Some(p);
        }
        Ok(())
    }

    /// Open and deserialize workbook
    pub fn open_from_file<P: AsRef<Path>>(path: P) -> Result<Self, CellError> {
        let p = path.as_ref().to_path_buf();
        let file = File::open(&p)
            .map_err(|e| CellError::Sql(format!("Failed to open workbook file: {}", e)))?;
        let reader = BufReader::new(file);
        let serialized: SerializedWorkbook = serde_json::from_reader(reader)
            .map_err(|e| CellError::Sql(format!("Corrupt workbook format: {}", e)))?;

        let mut wb = Workbook {
            sheets: serialized.sheets,
            active_sheet: serialized.active_sheet,
            connections: HashMap::new(),
            connection_configs: serialized.connection_configs.clone(),
            named_sources: serialized.named_sources,
            staged: serialized.staged,
            pivots: serialized.pivots,
            charts: serialized.charts,
            reviews: serialized.reviews,
            close_periods: serialized.close_periods,
            stale_sources: serialized.stale_sources,
            stale_calculation_sheets: serialized.stale_calculation_sheets,
            dep_graph: DependencyGraph::new(),
            file_path: Some(p),
            edit_history: Default::default(),
        };

        // Reconnect active database connections
        for config in serialized.connection_configs.values() {
            match config.kind {
                EngineKind::Sqlite => {
                    if config.path == ":memory:" {
                        let _ = wb.add_sqlite_memory(&config.name);
                    } else if Path::new(&config.path).exists() {
                        let _ = wb.add_sqlite_connection(&config.name, &config.path);
                    }
                }
                EngineKind::DuckDb => {
                    if config.path == ":memory:" {
                        let _ = wb.add_duckdb_memory(&config.name);
                    } else if Path::new(&config.path).exists() {
                        let _ = wb.add_duckdb_connection(&config.name, &config.path);
                    }
                }
            }
        }

        // Rebuild calculated values and dependencies rather than trusting persisted caches.
        let sheets: Vec<_> = wb.sheets.keys().cloned().collect();
        for sheet in sheets {
            wb.recalculate(&sheet)?;
        }
        wb.invalidate_changed_reviews();
        Ok(wb)
    }
}

pub struct WorkbookEvalContext<'a> {
    pub workbook: &'a Workbook,
    pub sheet_name: &'a str,
    pub current_coord: CellCoord,
}

impl<'a> EvaluationContext for WorkbookEvalContext<'a> {
    fn evaluate_on_sheet(&self, sheet: &str, expr: &crate::ast::Expr) -> Result<Value, CellError> {
        if !self.workbook.sheets.contains_key(sheet) {
            return Err(CellError::Ref);
        }
        let ctx = WorkbookEvalContext {
            workbook: self.workbook,
            sheet_name: sheet,
            current_coord: self.current_coord,
        };
        evaluate_expr(expr, &ctx)
    }

    fn get_cell_value(&self, coord: CellCoord) -> Result<Value, CellError> {
        let sheet = self
            .workbook
            .sheets
            .get(self.sheet_name)
            .ok_or(CellError::Ref)?;

        // 1. Check if cell is in a table
        for table in sheet.tables.values() {
            if table.range.contains(coord) {
                if table.live.is_some() {
                    return Err(CellError::Sql("Formulas and review cannot read a live worksheet table yet. Use a bounded snapshot.".into()));
                }
                if coord.row == table.anchor.row {
                    let col_offset = (coord.col - table.anchor.col) as usize;
                    let name = table.columns.get(col_offset).cloned().unwrap_or_default();
                    return Ok(Value::Text(name));
                } else {
                    let row_offset = (coord.row - table.anchor.row - 1) as usize;
                    let col_offset = (coord.col - table.anchor.col) as usize;
                    if let Some(source) = self.workbook.named_sources.get(&table.source_name) {
                        if let Some(res) = &source.last_result {
                            if let Some(r) = res.rows.get(row_offset) {
                                return Ok(r.get(col_offset).cloned().unwrap_or(Value::Blank));
                            }
                        }
                    }
                }
            }
        }

        if let Some(spill) = sheet.spills.get_spill_at(&coord) {
            {
                if let Some(cell) = sheet.get_cell(&spill.anchor) {
                    if let Value::Array(rows) = &cell.evaluated_value {
                        return Ok(rows
                            .get((coord.row - spill.anchor.row) as usize)
                            .and_then(|r| r.get((coord.col - spill.anchor.col) as usize))
                            .cloned()
                            .unwrap_or(Value::Blank));
                    }
                }
            }
        }
        // 2. Regular cell
        if let Some(cell) = sheet.get_cell(&coord) {
            Ok(cell.evaluated_value.clone())
        } else {
            Ok(Value::Blank)
        }
    }

    fn get_spill_values(&self, coord: CellCoord) -> Result<Value, CellError> {
        self.workbook
            .sheets
            .get(self.sheet_name)
            .and_then(|s| s.get_cell(&coord))
            .map(|c| c.evaluated_value.clone())
            .ok_or(CellError::Ref)
    }

    fn get_range_values(&self, range: CellRange) -> Result<Value, CellError> {
        if (range.end_row as u64 - range.start_row as u64 + 1)
            * (range.end_col as u64 - range.start_col as u64 + 1)
            > 100_000
        {
            return Err(CellError::Ref);
        }
        let mut rows = Vec::new();
        for r in range.start_row..=range.end_row {
            let mut row = Vec::new();
            for c in range.start_col..=range.end_col {
                row.push(self.get_cell_value(CellCoord::new(r, c))?);
            }
            rows.push(row);
        }
        Ok(Value::Array(rows))
    }

    fn get_table_column(&self, sref: &StructuredRef) -> Result<Value, CellError> {
        let sheet = self
            .workbook
            .sheets
            .get(self.sheet_name)
            .ok_or(CellError::Ref)?;

        let table = sheet
            .tables
            .values()
            .find(|t| t.name.eq_ignore_ascii_case(&sref.table_name))
            .ok_or(CellError::Ref)?;

        if table.live.is_some() {
            return Err(CellError::Sql("Formulas cannot use structured references to a live worksheet table yet. Use a bounded snapshot.".into()));
        }
        let range = table.resolve_structured_ref(sref, Some(self.current_coord))?;
        self.get_range_values(range)
    }

    fn execute_sql_value(
        &self,
        connection_name: &str,
        sql: &str,
        params: &[Value],
    ) -> Result<Value, CellError> {
        let conn = self
            .workbook
            .connections
            .get(connection_name)
            .ok_or_else(|| CellError::Sql(format!("Connection '{}' not found", connection_name)))?;

        let res = conn.execute_read_query(sql, params)?;

        if res.rows.is_empty() {
            // Explicit NULL on 0 rows as specified
            Ok(Value::Null)
        } else if res.rows.len() == 1 && res.columns.len() == 1 {
            Ok(res.rows[0][0].clone())
        } else {
            // Shape error on multiple rows or columns as specified
            Err(CellError::Value)
        }
    }

    fn execute_sql_table(
        &self,
        connection_name: &str,
        sql: &str,
        params: &[Value],
    ) -> Result<Value, CellError> {
        let conn = self
            .workbook
            .connections
            .get(connection_name)
            .ok_or_else(|| CellError::Sql(format!("Connection '{}' not found", connection_name)))?;

        let res = conn.execute_read_query(sql, params)?;
        Ok(Value::Array(res.rows))
    }
}

#[cfg(test)]
mod paged_table_extent_tests {
    use super::*;

    #[test]
    fn ten_million_rows_attach_by_metadata_without_allocating_cells() {
        let mut workbook = Workbook::new();
        let anchor = CellCoord::new(0, 0);
        let table = workbook
            .attach_table_extent(
                "Sheet1",
                "Large",
                "future_paged_source",
                anchor,
                vec!["ID".into(), "Amount".into()],
                10_000_000,
                None,
                false,
            )
            .unwrap();
        let sheet = &workbook.sheets["Sheet1"];
        assert_eq!(table.range.end_row, 10_000_000);
        assert_eq!(sheet.max_rows, 10_000_001);
        assert!(sheet.cells.is_empty());
        assert!(matches!(
            workbook.attach_table_extent(
                "Sheet1",
                "AtTail",
                "other",
                CellCoord::new(10_000_000, 1),
                vec!["ID".into()],
                2,
                None,
                false,
            ),
            Err(CellError::Spill)
        ));
        assert!(!workbook.sheets["Sheet1"].tables.contains_key("AtTail"));
    }

    #[test]
    fn reject_invalid_extent_and_keep_blank_formula_and_undo_history() {
        let mut workbook = Workbook::new();
        workbook
            .set_cell(
                "Sheet1",
                CellCoord::new(0, 0),
                "=IF(TRUE,\"\",\"x\")".into(),
            )
            .unwrap();
        let attach = |workbook: &mut Workbook, row, len| {
            workbook.attach_table_extent(
                "Sheet1",
                "T",
                "future_paged_source",
                CellCoord::new(row, 0),
                vec!["ID".into()],
                len,
                None,
                false,
            )
        };
        assert!(matches!(
            attach(&mut workbook, u32::MAX - 2, 10_000_000),
            Err(CellError::Ref)
        ));
        assert!(matches!(
            attach(&mut workbook, 0, 10_000_000),
            Err(CellError::Spill)
        ));
        assert!(workbook.sheets["Sheet1"].tables.is_empty());
        workbook.undo_sheet_edit().unwrap();
        assert!(workbook.sheets["Sheet1"].cells.is_empty());
    }
}
