import type {JumpDirection} from './utils/gridNavigation';
import type {RangeSummary} from './components/SelectionSummary';
import type {FindRequest, FindResponse} from './components/FindPanel';
import type {
  ReviewPack, CellCoord, DependencyNavigation, QuestionTransitionInput,
  ConfigureCloseInput, PostAdjustmentInput, AdjustmentImpact, CellRange, CellStyle, EvidenceUpload, DependencyTarget, ReviewComparison, SourceDrilldown, ClosePeriod, CloseStatus,
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
  QueryPreviewPage,
  StagedPatch,
  TableSchema,
  ViewportResponse,
  WorksheetTable,
  WritebackReviewItem,
} from './types';

const BASE_URL = '';
const wireRange = (range: CellRange) => ({ start_row: range.start.row, start_col: range.start.col, end_row: range.end.row, end_col: range.end.col });
type WireRange = ReturnType<typeof wireRange>;
type WireDependencyTarget = Omit<DependencyTarget, 'range'> & { range: WireRange | null };
type WireDependencyNavigation = Omit<DependencyNavigation, 'upstream' | 'downstream'> & {
  upstream: WireDependencyTarget[]; downstream: WireDependencyTarget[];
};
const dependencyTarget = (target: WireDependencyTarget): DependencyTarget => ({
  ...target,
  range: target.range ? {
    start: {row:target.range.start_row,col:target.range.start_col},
    end: {row:target.range.end_row,col:target.range.end_col},
  } : null,
});

let pendingMutations=0;
const mutationListeners=new Set<()=>void>();
export const getPendingMutations=()=>pendingMutations;
export const subscribeMutations=(listener:()=>void)=>{mutationListeners.add(listener);return()=>{mutationListeners.delete(listener);};};
const readPostPaths=new Set(['/api/ranges/sort-filter','/api/ledger/page','/api/pivots/refresh','/api/ranges/summary','/api/ranges/capture','/api/ranges/read','/api/cells/find','/api/cells/explain','/api/cells/dependencies','/api/cells/viewport','/api/cells/viewport-panes','/api/query/analyze','/api/query/preview','/api/writeback/review','/api/benchmark/run']);
async function fetchJson<T>(url: string, options?: RequestInit): Promise<T> {
  const changing=!!options?.method && options.method!=='GET' && !readPostPaths.has(url);
  if(changing){pendingMutations++;mutationListeners.forEach(listener=>listener());}
  try {
    const res=await fetch(`${BASE_URL}${url}`,{...options,headers:{'Content-Type':'application/json',...options?.headers}});
    if(!res.ok)throw new Error(await res.text()||`HTTP error ${res.status}`);
    return await res.json();
  } finally {if(changing){pendingMutations--;mutationListeners.forEach(listener=>listener());}}
}

export const api = {
  jumpCell: (sheet_name:string, row:number, col:number, direction:JumpDirection, signal:AbortSignal) =>
    fetchJson<CellCoord>(`/api/cells/jump?${new URLSearchParams({sheet_name,row:String(row),col:String(col),direction})}`, {signal,cache:'no-store'}),
  setShowFormulas:(sheet_name:string,show_formulas:boolean)=>fetchJson('/api/sheets/show-formulas',{method:'POST',body:JSON.stringify({sheet_name,show_formulas})}),
  rangeView:(sheet_name:string,range:CellRange,options:RangeViewOptions,signal:AbortSignal)=>fetchJson<RangeView>('/api/ranges/sort-filter',{method:'POST',body:JSON.stringify({sheet_name,range:wireRange(range),...options}),signal}),
  functionCatalog:()=>fetchJson<{functions:string[]}>('/api/functions'),
  rangeSummary: (sheet_name:string,range:CellRange) => fetchJson<RangeSummary>('/api/ranges/summary',{method:'POST',body:JSON.stringify({sheet_name,...wireRange(range)})}),
  setFreeze: (sheet_name:string,frozen_rows:number,frozen_columns:number) => fetchJson('/api/sheets/freeze',{method:'POST',body:JSON.stringify({sheet_name,frozen_rows,frozen_columns})}),
  setColumnWidth: (sheet_name:string,column:number,width:number|null) => fetchJson('/api/sheets/column-width',{method:'POST',body:JSON.stringify({sheet_name,column,width})}),
  findCells: (request: FindRequest) => fetchJson<FindResponse>('/api/cells/find', {method: 'POST', body: JSON.stringify(request)}),
  addSheet: (name: string) => fetchJson<{status: string}>('/api/sheets', { method: 'POST', body: JSON.stringify({name}) }),
  insertRows: (sheet_name: string, before_row: number, count: number) =>
    fetchJson<{status: string}>('/api/sheets/insert-rows', {method:'POST', body:JSON.stringify({sheet_name,before_row,count})}),
  selectSheet: (name: string) => fetchJson<{status: string}>('/api/sheets/select', { method: 'POST', body: JSON.stringify({name}) }),
  renameSheet: (old_name: string, new_name: string) => fetchJson<{status: string}>('/api/sheets/rename', { method: 'POST', body: JSON.stringify({old_name,new_name}) }),
  deleteSheet: (name: string) => fetchJson<{active_sheet: string}>('/api/sheets/delete', { method: 'POST', body: JSON.stringify({name}) }),
  editRange: (sheet_name: string, row: number, col: number, values: string[][]) => fetchJson<{status: string}>('/api/ranges/edit', { method: 'POST', body: JSON.stringify({sheet_name,row,col,values}) }),
  captureRange: (sheet_name:string, range:CellRange) => fetchJson<{values:string[][];snapshot:string}>('/api/ranges/capture',{method:'POST',body:JSON.stringify({sheet_name,range:wireRange(range)})}),
  readRange: (sheet_name: string, range: CellRange) => fetchJson<string[][]>('/api/ranges/read', { method: 'POST', body: JSON.stringify({sheet_name,range:wireRange(range)}) }),
  formatRangeAttribute: (sheet_name: string, range: CellRange, attribute: string, value: string) => fetchJson<{status: string}>('/api/ranges/format-attribute', {method:'POST', body:JSON.stringify({sheet_name,range:wireRange(range),attribute,value})}),
  formatRange: (sheet_name: string, range: CellRange, style: CellStyle) => fetchJson<{status: string}>('/api/ranges/format', { method: 'POST', body: JSON.stringify({sheet_name,range:wireRange(range),style}) }),
  pasteSpecial: (sheet_name: string, range: CellRange, target_sheet: string, target: CellCoord, expected_snapshot: string, mode: 'values' | 'formulas') => fetchJson<{status: string}>('/api/ranges/copy', { method: 'POST', body: JSON.stringify({sheet_name,range:wireRange(range),target_sheet,target,expected_snapshot,mode}) }),
  copyRange: (sheet_name: string, range: CellRange, target_sheet: string, target: CellCoord, expected_values?: string[][], expected_snapshot?: string) => fetchJson<{status: string}>('/api/ranges/copy', { method: 'POST', body: JSON.stringify({sheet_name,range:wireRange(range),target_sheet,target,expected_values,expected_snapshot}) }),
  fillRange: (sheet_name: string, source: CellRange, target: CellRange) => fetchJson<{status: string}>('/api/ranges/fill', { method: 'POST', body: JSON.stringify({sheet_name,source:wireRange(source),target:wireRange(target)}) }),
  undo: () => fetchJson<{status: string}>('/api/history/undo', {method:'POST'}),
  redo: () => fetchJson<{status: string}>('/api/history/redo', {method:'POST'}),
  listClosePeriods: () => fetchJson<ClosePeriod[]>('/api/close/periods'),
  importTrialBalance: (request: {period:string,csv:string,mappings?:Record<string,string>}) => fetchJson<ClosePeriod>('/api/close/import', {method:'POST',body:JSON.stringify(request)}),
  mapCloseAccounts: (request: {period_id:string,mappings:Record<string,string>}) => fetchJson<ClosePeriod>('/api/close/map', {method:'POST',body:JSON.stringify(request)}),
  configureClose: (request: ConfigureCloseInput) => fetchJson<ClosePeriod>('/api/close/configure', {method:'POST',body:JSON.stringify(request)}),
  postCloseAdjustment: (request: PostAdjustmentInput) => fetchJson<AdjustmentImpact>('/api/close/adjustments', {method:'POST',body:JSON.stringify(request)}),
  getCloseStatus: (id: string) => fetchJson<CloseStatus>(`/api/close/${encodeURIComponent(id)}/status`),
  rollforwardClose: (request: {source_period_id:string,period:string,csv:string}) => fetchJson<ClosePeriod>('/api/close/rollforward', {method:'POST',body:JSON.stringify(request)}),
  exportReviewPack: (id: string) => fetchJson<ReviewPack>(`/api/close/${encodeURIComponent(id)}/pack`),
  recordAttachedReview: (request: {review:ReviewInput,attachments:EvidenceUpload[]}) => fetchJson<ReviewRecord>('/api/reviews/attached', {method:'POST',body:JSON.stringify(request)}),
  getReviewComparison: (id: string, after?: string) => fetchJson<ReviewComparison>(`/api/reviews/${encodeURIComponent(id)}/compare${after ? `?after=${encodeURIComponent(after)}` : ''}`),
  getSourceDrilldown: (name: string, offset = 0, limit = 50) => fetchJson<SourceDrilldown>(`/api/sources/${encodeURIComponent(name)}/drilldown?offset=${offset}&limit=${limit}`),
  explainCell: (sheet_name: string, row: number, col: number) =>
    fetchJson<CellExplanation>('/api/cells/explain', { method: 'POST', body: JSON.stringify({ sheet_name, row, col }) }),
  navigateDependencies: async (sheet_name: string, row: number, col: number): Promise<DependencyNavigation> => {
    const result = await fetchJson<WireDependencyNavigation>('/api/cells/dependencies', { method: 'POST', body: JSON.stringify({ sheet_name, row, col }) });
    return {...result, upstream:result.upstream.map(dependencyTarget), downstream:result.downstream.map(dependencyTarget)};
  },
  getOpenQuestions: () => fetchJson<ReviewRecord[]>('/api/questions'),
  transitionQuestion: (id: string, request: QuestionTransitionInput) => fetchJson<ReviewRecord>(`/api/questions/${encodeURIComponent(id)}/transition`, {method:'POST',body:JSON.stringify(request)}),
  getReviews: () => fetchJson<ReviewRecord[]>('/api/reviews'),
  saveReview: (data: ReviewInput) =>
    fetchJson<ReviewRecord>('/api/reviews', { method: 'POST', body: JSON.stringify(data) }),
  listRecovery: () => fetchJson<{id:string; modified:string}[]>('/api/workbook/recovery'),
  restoreRecovery: (id:string) => fetchJson<{status:string}>(`/api/workbook/recovery/${encodeURIComponent(id)}`, {method:'POST'}),
  getWorkbook: () => fetchJson<WorkbookSummary>('/api/workbook'),
  newWorkbook: () => fetchJson<{ status: string }>('/api/workbook/new', { method: 'POST' }),
  openWorkbook: (path: string) => fetchJson<any>('/api/workbook/open', { method: 'POST', body: JSON.stringify({ path }) }),
  saveWorkbook: (path: string) => fetchJson<any>('/api/workbook/save', { method: 'POST', body: JSON.stringify({ path }) }),

  getLedgerTrace: (target: LedgerTarget, signal?: AbortSignal) => fetchJson<LedgerTrace>("/api/ledger/trace?" + new URLSearchParams({sheet_name:target.sheet_name,row:String(target.row),col:String(target.col)}),{signal}),
  setLedgerTrace: (target: LedgerTarget, session: string|null, signal?: AbortSignal) => fetchJson<LedgerTrace>("/api/ledger/trace",{method:"POST",body:JSON.stringify({target,session}),signal}),
  ledgerPage: (input: LedgerRequest, signal?: AbortSignal) => fetchJson<LedgerPage>("/api/ledger/page", {method:"POST", headers:{"Content-Type":"application/json"}, body:JSON.stringify(input), signal}),
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
  previewQuery: (connection_name: string, sql: string, offset: number, limit: number, signal: AbortSignal) =>
    fetchJson<QueryPreviewPage>('/api/query/preview', {
      method: 'POST',
      body: JSON.stringify({ connection_name, sql, offset, limit }),
      signal,
    }),
  analyzeQuery: (connection_name: string, sql: string, signal?: AbortSignal) =>
    fetchJson<LineageReport>('/api/query/analyze', {
      method: 'POST',
      body: JSON.stringify({ connection_name, sql }),
      signal,
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

  insertLiveTable: (sheet_name: string, table_name: string, connection_name: string, sql: string, row: number, col: number) =>
    fetchJson<WorksheetTable>('/api/tables/insert-live', {
      method: 'POST',
      body: JSON.stringify({ sheet_name, table_name, connection_name, sql, row, col }),
    }),

  refreshLiveTable: (sheet_name: string, table_name: string) =>
    fetchJson<WorksheetTable>('/api/tables/refresh-live', {
      method: 'POST',
      body: JSON.stringify({ sheet_name, table_name }),
    }),

  setCell: (sheet_name: string, row: number, col: number, raw_input: string) =>
    fetchJson<{ status: string }>('/api/cells/set', {
      method: 'POST',
      body: JSON.stringify({ sheet_name, row, col, raw_input }),
    }),
  getViewportPanes: (sheet_name:string,start_row:number,start_col:number,end_row:number,end_col:number,visible_frozen_rows:number,visible_frozen_columns:number,overscan=5,signal?:AbortSignal) =>
    fetchJson<ViewportResponse>('/api/cells/viewport-panes',{method:'POST',body:JSON.stringify({sheet_name,start_row,start_col,end_row,end_col,overscan,visible_frozen_rows,visible_frozen_columns}),signal}),
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
  } & ({raw_input: string;proposed_value?:never}|{raw_input?:never;proposed_value:CellValue})) => fetchJson<{ status: string; id: string }>('/api/staged', { method: 'POST', body: JSON.stringify(data) }),
  discardStaged: (id: string) => fetchJson<{ status: string }>(`/api/staged/${encodeURIComponent(id)}`, { method: 'DELETE' }),
  discardAllStaged: () => fetchJson<{ status: string }>('/api/staged', { method: 'DELETE' }),

  reviewWriteback: () => fetchJson<WritebackReviewItem[]>('/api/writeback/review'),
  applyWriteback: (connection_name: string) =>
    fetchJson<AuditRecord>('/api/writeback/apply', {
      method: 'POST',
      body: JSON.stringify({ connection_name }),
    }),
  getAuditLog: () => fetchJson<AuditRecord[]>('/api/audit'),

  getPivots: (signal?: AbortSignal) => fetchJson<PivotDefinition[]>('/api/pivots', {signal}),
  savePivot: (def: PivotDefinition) =>
    fetchJson<{status: string; name: string; result: PivotResult}>('/api/pivots', { method: 'POST', body: JSON.stringify(def) }),
  getPivotData: (name: string, signal?: AbortSignal) => fetchJson<PivotResult>(`/api/pivots/${encodeURIComponent(name)}/data`, {signal}),
  refreshPivot: (name: string, signal?: AbortSignal) =>
    fetchJson<PivotResult>('/api/pivots/refresh', {method: 'POST', body: JSON.stringify({name}), signal}),

  getCharts: () => fetchJson<ChartDefinition[]>('/api/charts'),
  saveChart: (def: ChartDefinition) =>
    fetchJson<any>('/api/charts', { method: 'POST', body: JSON.stringify(def) }),
  getChartData: (id: string) => fetchJson<ChartData>(`/api/charts/${encodeURIComponent(id)}/data`),

  runBenchmarks: () => fetchJson<BenchmarkSuiteReport>('/api/benchmark/run', { method: 'POST' }),
};

export type LedgerSpec = {connection:string;table:string;date:string;cents:string;from:string|null;through:string|null;account?:{column:string;value:string}|null};
export type LedgerRequest = {spec:LedgerSpec;session:string|null;offset:number;limit:number};
export type LedgerPage = {session:string;columns:string[];rows:(string|null)[][];count:number;total_cents:string};

export type LedgerTarget = {sheet_name:string;row:number;col:number};
export type LedgerTrace = {link:LedgerSpec|null;source_changed:boolean;balance_cents:string|null};

export type RangeViewFilter = 'contains' | 'equals' | 'empty' | 'non_empty' | 'at_least' | 'at_most';
export type RangeViewOptions = {first_row_header:boolean;sort:{col:number;descending:boolean}|null;filter:{col:number;mode:RangeViewFilter;text:string}|null};
export type RangeView = {generation:number;headers:string[];total_rows:number;rows:{row:number;values:string[]}[]};
