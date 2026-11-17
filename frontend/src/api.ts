import {
  CellCoord, CellRange, CellStyle, EvidenceUpload, ReviewComparison, SourceDrilldown, ClosePeriod, CloseStatus,
  AuditRecord,
  CellExplanation,
  ReviewRecord,
  ReviewInput,
  CellValue,
  WorkbookSummary,
  BenchmarkSuiteReport,
  ChartData,
  ChartDefinition,
  ColumnMeta,
  LineageReport,
  NamedSource,
  PivotDefinition,
  PivotResult,
  QueryResult,
  StagedPatch,
  TableSchema,
  ViewportResponse,
  WorksheetTable,
  WritebackReviewItem,
} from './types';

const BASE_URL = '';
const wireRange = (range: CellRange) => ({ start_row: range.start.row, start_col: range.start.col, end_row: range.end.row, end_col: range.end.col });

async function fetchJson<T>(url: string, options?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE_URL}${url}`, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      ...options?.headers,
    },
  });
  if (!res.ok) {
    const errText = await res.text();
    throw new Error(errText || `HTTP error ${res.status}`);
  }
  return res.json();
}

export const api = {
  addSheet: (name: string) => fetchJson<{status: string}>('/api/sheets', { method: 'POST', body: JSON.stringify({name}) }),
  selectSheet: (name: string) => fetchJson<{status: string}>('/api/sheets/select', { method: 'POST', body: JSON.stringify({name}) }),
  renameSheet: (old_name: string, new_name: string) => fetchJson<{status: string}>('/api/sheets/rename', { method: 'POST', body: JSON.stringify({old_name,new_name}) }),
  editRange: (sheet_name: string, row: number, col: number, values: string[][]) => fetchJson<{status: string}>('/api/ranges/edit', { method: 'POST', body: JSON.stringify({sheet_name,row,col,values}) }),
  captureRange: (sheet_name:string, range:CellRange) => fetchJson<{values:string[][];snapshot:string}>('/api/ranges/capture',{method:'POST',body:JSON.stringify({sheet_name,range:wireRange(range)})}),
  readRange: (sheet_name: string, range: CellRange) => fetchJson<string[][]>('/api/ranges/read', { method: 'POST', body: JSON.stringify({sheet_name,range:wireRange(range)}) }),
  formatRangeAttribute: (sheet_name: string, range: CellRange, attribute: string, value: string) => fetchJson<{status: string}>('/api/ranges/format-attribute', {method:'POST', body:JSON.stringify({sheet_name,range:wireRange(range),attribute,value})}),
  formatRange: (sheet_name: string, range: CellRange, style: CellStyle) => fetchJson<{status: string}>('/api/ranges/format', { method: 'POST', body: JSON.stringify({sheet_name,range:wireRange(range),style}) }),
  copyRange: (sheet_name: string, range: CellRange, target_sheet: string, target: CellCoord, expected_values?: string[][], expected_snapshot?: string) => fetchJson<{status: string}>('/api/ranges/copy', { method: 'POST', body: JSON.stringify({sheet_name,range:wireRange(range),target_sheet,target,expected_values,expected_snapshot}) }),
  fillRange: (sheet_name: string, source: CellRange, target: CellRange) => fetchJson<{status: string}>('/api/ranges/fill', { method: 'POST', body: JSON.stringify({sheet_name,source:wireRange(source),target:wireRange(target)}) }),
  undo: () => fetchJson<{status: string}>('/api/history/undo', {method:'POST'}),
  redo: () => fetchJson<{status: string}>('/api/history/redo', {method:'POST'}),
  listClosePeriods: () => fetchJson<ClosePeriod[]>('/api/close/periods'),
  importTrialBalance: (request: {period:string,csv:string,mappings?:Record<string,string>}) => fetchJson<ClosePeriod>('/api/close/import', {method:'POST',body:JSON.stringify(request)}),
  mapCloseAccounts: (request: {period_id:string,mappings:Record<string,string>}) => fetchJson<ClosePeriod>('/api/close/map', {method:'POST',body:JSON.stringify(request)}),
  getCloseStatus: (id: string) => fetchJson<CloseStatus>(`/api/close/${encodeURIComponent(id)}/status`),
  rollforwardClose: (request: {source_period_id:string,period:string,csv:string}) => fetchJson<ClosePeriod>('/api/close/rollforward', {method:'POST',body:JSON.stringify(request)}),
  exportReviewPack: (id: string) => fetchJson<Record<string, unknown>>(`/api/close/${encodeURIComponent(id)}/pack`),
  recordAttachedReview: (request: {review:ReviewInput,attachments:EvidenceUpload[]}) => fetchJson<ReviewRecord>('/api/reviews/attached', {method:'POST',body:JSON.stringify(request)}),
  getReviewComparison: (id: string, after?: string) => fetchJson<ReviewComparison>(`/api/reviews/${encodeURIComponent(id)}/compare${after ? `?after=${encodeURIComponent(after)}` : ''}`),
  getSourceDrilldown: (name: string, offset = 0, limit = 50) => fetchJson<SourceDrilldown>(`/api/sources/${encodeURIComponent(name)}/drilldown?offset=${offset}&limit=${limit}`),
  explainCell: (sheet_name: string, row: number, col: number) =>
    fetchJson<CellExplanation>('/api/cells/explain', { method: 'POST', body: JSON.stringify({ sheet_name, row, col }) }),
  getReviews: () => fetchJson<ReviewRecord[]>('/api/reviews'),
  saveReview: (data: ReviewInput) =>
    fetchJson<ReviewRecord>('/api/reviews', { method: 'POST', body: JSON.stringify(data) }),
  getWorkbook: () => fetchJson<WorkbookSummary>('/api/workbook'),
  newWorkbook: () => fetchJson<{ status: string }>('/api/workbook/new', { method: 'POST' }),
  openWorkbook: (path: string) => fetchJson<any>('/api/workbook/open', { method: 'POST', body: JSON.stringify({ path }) }),
  saveWorkbook: (path: string) => fetchJson<any>('/api/workbook/save', { method: 'POST', body: JSON.stringify({ path }) }),

  getConnections: () => fetchJson<{ name: string; kind: string; path: string }[]>('/api/connections'),
  connectDb: (name: string, kind: string, path: string) =>
    fetchJson<any>('/api/connections', {
      method: 'POST',
      body: JSON.stringify({ name, kind, path }),
    }),
  getTables: (connName: string) => fetchJson<string[]>(`/api/connections/${encodeURIComponent(connName)}/tables`),
  getTableSchema: (connName: string, tableName: string) =>
    fetchJson<TableSchema>(`/api/connections/${encodeURIComponent(connName)}/tables/${encodeURIComponent(tableName)}/schema`),

  executeQuery: (connection_name: string, sql: string) =>
    fetchJson<QueryResult>('/api/query/execute', {
      method: 'POST',
      body: JSON.stringify({ connection_name, sql }),
    }),
  analyzeQuery: (connection_name: string, sql: string) =>
    fetchJson<LineageReport>('/api/query/analyze', {
      method: 'POST',
      body: JSON.stringify({ connection_name, sql }),
    }),

  getSources: () => fetchJson<NamedSource[]>('/api/sources'),
  createSource: (name: string, connection_name: string, sql: string) =>
    fetchJson<NamedSource>('/api/sources', {
      method: 'POST',
      body: JSON.stringify({ name, connection_name, sql }),
    }),
  refreshSource: (name: string) =>
    fetchJson<any>(`/api/sources/${encodeURIComponent(name)}/refresh`, { method: 'POST' }),

  insertTable: (sheet_name: string, table_name: string, source_name: string, row: number, col: number) =>
    fetchJson<WorksheetTable>('/api/tables/insert', {
      method: 'POST',
      body: JSON.stringify({ sheet_name, table_name, source_name, row, col }),
    }),

  setCell: (sheet_name: string, row: number, col: number, raw_input: string) =>
    fetchJson<{ status: string }>('/api/cells/set', {
      method: 'POST',
      body: JSON.stringify({ sheet_name, row, col, raw_input }),
    }),
  getViewport: (sheet_name: string, start_row: number, start_col: number, end_row: number, end_col: number, overscan = 5) =>
    fetchJson<ViewportResponse>('/api/cells/viewport', {
      method: 'POST',
      body: JSON.stringify({ sheet_name, start_row, start_col, end_row, end_col, overscan }),
    }),

  getStaged: () => fetchJson<StagedPatch[]>('/api/staged'),
  stageEdit: (data: {
    connection_name: string;
    table_name: string;
    pk_columns: string[];
    pk_values: CellValue[];
    column_name: string;
    original_value: CellValue;
    proposed_value: CellValue;
  }) => fetchJson<{ status: string; id: string }>('/api/staged', { method: 'POST', body: JSON.stringify(data) }),
  discardStaged: (id: string) => fetchJson<{ status: string }>(`/api/staged/${encodeURIComponent(id)}`, { method: 'DELETE' }),
  discardAllStaged: () => fetchJson<{ status: string }>('/api/staged', { method: 'DELETE' }),

  reviewWriteback: () => fetchJson<WritebackReviewItem[]>('/api/writeback/review'),
  applyWriteback: (connection_name: string) =>
    fetchJson<AuditRecord>('/api/writeback/apply', {
      method: 'POST',
      body: JSON.stringify({ connection_name }),
    }),
  getAuditLog: () => fetchJson<AuditRecord[]>('/api/audit'),

  getPivots: () => fetchJson<PivotDefinition[]>('/api/pivots'),
  savePivot: (def: PivotDefinition) =>
    fetchJson<any>('/api/pivots', { method: 'POST', body: JSON.stringify(def) }),
  getPivotData: (name: string) => fetchJson<PivotResult>(`/api/pivots/${encodeURIComponent(name)}/data`),

  getCharts: () => fetchJson<ChartDefinition[]>('/api/charts'),
  saveChart: (def: ChartDefinition) =>
    fetchJson<any>('/api/charts', { method: 'POST', body: JSON.stringify(def) }),
  getChartData: (id: string) => fetchJson<ChartData>(`/api/charts/${encodeURIComponent(id)}/data`),

  runBenchmarks: () => fetchJson<BenchmarkSuiteReport>('/api/benchmark/run', { method: 'POST' }),
};
