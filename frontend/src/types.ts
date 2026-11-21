export type CellValue =
  | { type: 'Blank' | 'EmptyText' | 'Null' }
  | { type: 'Text' | 'Date' | 'Timestamp' | 'Decimal'; value: string }
  | { type: 'Integer' | 'Float'; value: number }
  | { type: 'Boolean'; value: boolean }
  | { type: 'Array'; value: CellValue[][] }
  | { type: 'Error'; value: unknown };

export interface WorkbookSummary {
  active_sheet: string;
  sheets: { name: string; tables: WorksheetTable[]; column_widths?: Record<number,number>; freeze_top_row?:boolean; freeze_first_column?:boolean; frozen_rows?:number; frozen_columns?:number; show_formulas?:boolean }[];
  connections: string[];
  sources: string[];
  staged_patches_count: number;
  file_path: string | null;
}

export interface CellCoord {
  row: number;
  col: number;
}

export interface CellRenderData {
  coord: CellCoord;
  display_text: string;
  raw_value: CellValue;
  formula?: string;
  number_format?: string;
  raw_input?: string;
  style?: CellStyle;
  is_staged: boolean;
  is_spill: boolean;
  is_error: boolean;
  table_name?: string;
  is_table_header: boolean;
  live_edit?: {
    connection_name: string;
    table_name: string;
    pk_column: string;
    column_name: string;
    pk_value: CellValue;
    original_value: CellValue;
  };
}

export interface ViewportResponse {
  loaded_ranges?: {start_row:number;start_col:number;end_row:number;end_col:number}[];
  sheet_name: string;
  start_row: number;
  start_col: number;
  end_row: number;
  end_col: number;
  cells: Record<string, CellRenderData>;
  total_rows: number;
  total_cols: number;
  generation: number;
}

export interface ColumnMeta {
  name: string;
  data_type: string;
  is_primary_key: boolean;
  is_nullable: boolean;
}

export interface TableSchema {
  table_name: string;
  columns: ColumnMeta[];
  primary_keys: string[];
}

export interface QueryResult {
  columns: ColumnMeta[];
  rows: CellValue[][];
  total_rows: number;
}

export interface QueryPreviewPage {
  columns: ColumnMeta[];
  rows: CellValue[][];
  offset: number;
  has_more: boolean;
}

export interface LineageReport {
  is_editable: boolean;
  base_table?: string;
  primary_keys: string[];
  editable_columns: string[];
  read_only_columns: string[];
  read_only_reasons: string[];
}

export interface NamedSource {
  name: string;
  connection_name: string;
  sql: string;
  last_result?: QueryResult;
  lineage?: LineageReport;
}

export type LocalColumnType = 'Any' | 'Text' | 'Integer' | 'Decimal' | 'Boolean' | 'Date';
export interface LocalColumn {name: string; kind: LocalColumnType}

export interface WorksheetTable {
  name: string;
  source_name: string;
  anchor: CellCoord;
  range: { start_row: number; start_col: number; end_row: number; end_col: number };
  columns: string[];
  local?: { id: string; column_types: LocalColumnType[] };
  live?: { connection_name: string; sql: string; base_table: string; primary_key: string };
}

export interface StagedPatch {
  id: string;
  connection_name: string;
  table_name: string;
  pk_columns: string[];
  pk_values: CellValue[];
  column_name: string;
  original_value: CellValue;
  proposed_value: CellValue;
  staged_at: string;
}

export interface WritebackReviewItem {
  patch: StagedPatch;
  conflict_status: 'NoConflict' | 'RowDeletedExternally' | { CheckFailed: { message: string } } | { RowModifiedExternally: { current_value: any } };
  can_apply: boolean;
}

export interface AuditRecord {
  id: string;
  connection_name: string;
  table_name: string;
  applied_at: string;
  patch_count: number;
  status: 'AppliedSuccessfully' | 'RolledBackConflict' | 'RolledBackError';
  error_message?: string;
  patches: StagedPatch[];
}

export type PivotAggregation = 'Count' | 'Sum' | 'Average' | 'Min' | 'Max';

export interface PivotDefinition {
  name: string;
  source_table: string;
  row_fields: string[];
  col_fields: string[];
  value_field: string;
  aggregation: PivotAggregation;
}

export interface PivotResult {
  headers: string[];
  rows: CellValue[][];
}

export type ChartType = 'Line' | 'Bar' | 'Column' | 'Scatter' | 'Pie';

export interface ChartSeriesBinding {
  name: string;
  value_column: string;
}

export interface ChartDefinition {
  id: string;
  title: string;
  chart_type: ChartType;
  source_table: string;
  category_column: string;
  series: ChartSeriesBinding[];
}

export interface ChartDataSeries {
  name: string;
  values: number[];
}

export interface ChartData {
  title: string;
  chart_type: ChartType;
  categories: string[];
  series: ChartDataSeries[];
}

export interface BenchmarkResult {
  workload_name: string;
  operations: number;
  elapsed_ms: number;
  throughput_ops_per_sec: number;
  memory_peak_rss_kb: number | null;
  status: string;
  notes: string;
}

export interface BenchmarkSuiteReport {
  total_workloads: number;
  total_elapsed_ms: number;
  results: BenchmarkResult[];
  passed_correctness_gate: boolean;
  passed_resource_gate: boolean | null;
}


export interface EvidenceReference { label: string; reference: string }
export interface ReviewRecord {
  id: string;
  sheet_name: string;
  coord: CellCoord;
  decision: 'Reviewed' | 'Question';
  reviewer: string;
  note: string;
  evidence: EvidenceReference[];
  attachments?: EvidenceAttachment[];
  input_snapshot?: ReviewInputSnapshot | null;
  created_at: string;
  snapshot: string;
  invalidated: boolean;
  invalidation_reason: string | null;
  question_events?: QuestionEvent[];
}
export type QuestionAction = 'Resolved' | 'Reopened';
export interface QuestionEvent {
  id: string; action: QuestionAction; actor: string; note: string; created_at: string;
  snapshot: string; input_snapshot: ReviewInputSnapshot;
}
export interface QuestionTransitionInput {
  action: QuestionAction; actor: string; note: string; expected_snapshot: string; expected_last_event_id?: string | null;
}
export type DependencyDirection = 'upstream' | 'downstream';
export interface DependencyTarget {
  id: string; kind: 'cell' | 'range' | 'table' | 'source' | 'spill'; label: string;
  sheet_name: string | null; coord: CellCoord | null; range: CellRange | null;
  formula: string | null; source_name: string | null;
}
export interface DependencyNavigation {
  sheet_name: string; coord: CellCoord; upstream: DependencyTarget[]; downstream: DependencyTarget[];
  warnings: string[]; truncated: boolean;
}
export interface DependencyHighlight { target: DependencyTarget; direction: DependencyDirection }
export interface ExplanationNode {
  id: string;
  kind: 'cell' | 'table' | 'source' | 'sql';
  label: string;
  formula: string | null;
  value: CellValue | null;
  depends_on: string[];
  detail: string | null;
  coord: CellCoord | null;
  sheet_name?: string | null;
  source_name?: string | null;
}
export interface CellExplanation {
  sheet_name: string;
  coord: CellCoord;
  value: CellValue;
  formula: string | null;
  nodes: ExplanationNode[];
  warnings: string[];
  snapshot: string;
  can_review: boolean;
  review: ReviewRecord | null;
  input_snapshot?: ReviewInputSnapshot;
}
export interface ReviewInput {
  sheet_name: string;
  row: number;
  col: number;
  expected_snapshot: string;
  decision: 'Reviewed' | 'Question';
  reviewer: string;
  note: string;
  evidence: EvidenceReference[];
}

export interface ReviewDraft { reviewer: string; decision: 'Reviewed' | 'Question'; note: string; evidence: EvidenceReference[]; attachments?: EvidenceUpload[] }

export interface CellRange { start: CellCoord; end: CellCoord }
export interface CellStyle { bold: boolean; italic: boolean; fill: string | null; number_format: string | null }
export interface ReviewInputSnapshot {
  version: number; sheet_name: string; coord: CellCoord; value: CellValue;
  formula: string | null; inputs: Record<string, unknown>; nodes: ExplanationNode[];
  warnings: string[]; can_review: boolean;
}
export interface EvidenceUpload { filename: string; media_type: string; bytes: number[]; replaces?: string | null }
export interface EvidenceAttachment { id: string; version: number; previous_version_id: string | null; filename: string; media_type: string; sha256: string; bytes: number[]; created_at: string }
export interface SnapshotChange { path: string; before: unknown | null; after: unknown | null }
export interface ChangeSummary { category: string; label: string; sheet_name?: string | null; coord?: CellCoord | null; before?: unknown; after?: unknown }
export interface AffectedCell { sheet_name: string; coord: CellCoord; label: string }
export interface ReviewComparison {
  before_review_id: string; after_review_id: string | null; before_snapshot: string; after_snapshot: string;
  changes: SnapshotChange[]; summaries?: ChangeSummary[]; affected_cells?: AffectedCell[];
}
export interface SourceDrilldownRow { cached_row_index: number; primary_key: Record<string, CellValue>; values: CellValue[] }
export interface SourceDrilldown { source_name: string; connection_name: string; base_table: string; sql: string; columns: string[]; rows: SourceDrilldownRow[]; total_rows: number; offset: number; warning: string }
export type CloseMapping = 'Revenue' | 'Cost of sales' | 'Operating expenses' | 'Other income' | 'Other expense' | 'Balance sheet';
export interface CloseAccount { account: string; name: string; debit: string; credit: string; mapping: string }
export interface ReportLine { id: string; label: string; parent_id: string | null; normal_balance: 'Debit' | 'Credit' }
export interface CloseDimensions { entity: string; currency: string; units: string }
export interface ConfigureCloseInput extends CloseDimensions { period_id: string; report_lines: ReportLine[]; mappings: Record<string,string> }
export interface AdjustmentLine { account: string; debit: string; credit: string }
export interface PostAdjustmentInput { period_id: string; author: string; rationale: string; lines: AdjustmentLine[]; evidence: EvidenceUpload[] }
export interface AdjustmentImpact {
  adjustment: CloseAdjustment;
  affected_lines: { label: string; coord: CellCoord; before: CellValue; after: CellValue; before_status: string; after_status: string }[];
  invalidated_reviews: { id: string; sheet_name: string; coord: CellCoord; reviewer: string }[];
}
export interface CloseAdjustment { id: string; author: string; rationale: string; lines: AdjustmentLine[]; evidence: EvidenceAttachment[]; created_at: string }
export interface ClosePeriod { id: string; period: string; version: number; sheet_name: string; created_at: string; source_csv: string; source_sha256: string; accounts: CloseAccount[]; prior_period_id: string | null; entity?: string; currency?: string; units?: string; reconciliation_version?: number; report_lines?: ReportLine[]; adjustments?: CloseAdjustment[] }
export interface CloseQueueItem { label: string; coord: CellCoord; value: CellValue; status: string; review_id: string | null }
export interface LiveCloseAccount { row: number; imported_account: string; account: string; name: string; mapping: string; debit: string | null; credit: string | null; identity_matches_import: boolean; imported_balance?: string | null; adjustment_balance?: string | null; reported_balance?: string | null }
export interface CloseStatus { period: ClosePeriod; live_accounts: LiveCloseAccount[]; debits: string; credits: string; difference: string; balanced: boolean; unmapped_accounts: string[]; queue: CloseQueueItem[]; ready_for_close: boolean; warnings: string[] }
export interface ReviewPack {
  format: 'gridloom-review-pack'; version: number; exported_at: string;
  status: CloseStatus; worksheet: unknown; reviews: ReviewRecord[];
  explanations: CellExplanation[]; limitations: string[];
}
