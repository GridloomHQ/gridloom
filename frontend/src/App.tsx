import { AboutDialog } from './components/AboutDialog';
import { SortFilterPanel } from './components/SortFilterPanel';
import { LedgerPanel } from './components/LedgerPanel';
import {SelectionSummary} from './components/SelectionSummary';
import {wholeAxisRange, type SelectionAxis} from './utils/gridNavigation';
import {isCellLoaded} from './utils/frozenViewport';
import {ViewportCache, viewportBudget, viewportCovers, requestCovers, type ViewBounds} from './utils/viewportCache';
import { FindPanel, FindMatch } from './components/FindPanel';
import React, { useState, useEffect, useCallback, useRef, lazy, Suspense } from 'react';
import { normalizedRange, encodeTsv, decodeTsv, boundedRangeDimensions, ordinaryFormatRangeIsBounded, GridRange } from './utils/selection';
import { displayValue } from './utils/values';
import { ExplainReviewPanel } from './components/ExplainReviewPanel';
import { OpenQuestions } from './components/OpenQuestions';
import { Toolbar, worksheetCommandClass } from './components/Toolbar';
import { SheetTabs } from './components/SheetTabs';
import { FormulaBar } from './components/FormulaBar';
import { Grid } from './components/Grid';
import { KeyboardShortcutsDialog, PasteSpecialDialog, type PasteMode } from './components/PasteControls';
import { SchemaBrowserModal } from './components/SchemaBrowserModal';
import { QueryEditorModal } from './components/QueryEditorModal';
import { InsertTableModal } from './components/InsertTableModal';
import { StagedReviewModal } from './components/StagedReviewModal';
import { PivotBuilderModal } from './components/PivotBuilderModal';
import { ChartModal } from './components/ChartModal';
import { AuditLogModal } from './components/AuditLogModal';
import { BenchmarkModal } from './components/BenchmarkModal';
import { api,getPendingMutations,subscribeMutations } from './api';
import type {LedgerTarget} from './api';
import {saveBlocker} from './utils/saveGuard';
import { ViewportResponse, WorksheetTable, ReviewDraft, DependencyHighlight } from './types';
import { colIndexToName, coordToA1 } from './utils/coords';

const MonthlyClosePanel = lazy(() => import('./components/MonthlyClosePanel').then(module => ({ default: module.MonthlyClosePanel })));
const CLOSE_ADDON = 'gridloom:addon:monthly-close';

export const App: React.FC = () => {
  const [isFindOpen,setIsFindOpen]=useState(false);
  const [sortFilterRange,setSortFilterRange]=useState<GridRange|null>(null);
  const [isLedgerOpen,setIsLedgerOpen]=useState(false);
  const [ledgerTarget,setLedgerTarget]=useState<LedgerTarget|undefined>();
  const [isCloseOpen, setIsCloseOpen] = useState(false);
  const [closeAddon, setCloseAddon] = useState(() => {
    try { return localStorage.getItem(CLOSE_ADDON) === 'enabled'; } catch { return false; }
  });
  const toggleCloseAddon = (enabled: boolean) => {
    try { localStorage.setItem(CLOSE_ADDON, enabled ? 'enabled' : 'disabled'); } catch { /* Session only. */ }
    setCloseAddon(enabled);
    if (!enabled) setIsCloseOpen(false);
  };
  const [sheets, setSheets] = useState<string[]>(['Sheet1']);
  const [anchor, setAnchor] = useState({row: 0, col: 0});
  const [selectionAxis, setSelectionAxis] = useState<SelectionAxis | null>(null);
  const [busy, setBusy] = useState(false);
  const [gridDraft,setGridDraft]=useState(false),[formulaDraft,setFormulaDraft]=useState(false);
  const [formulaDraftText,setFormulaDraftText]=useState<string|null>(null);
  const [functionNames,setFunctionNames]=useState<string[]>([]);
  const [functionCatalogStatus,setFunctionCatalogStatus]=useState<'loading'|'ready'|'error'>('loading');
  const reloadFunctionCatalog=useCallback(()=>{
    setFunctionCatalogStatus('loading');
    void api.functionCatalog().then(({functions})=>{setFunctionNames(functions);setFunctionCatalogStatus('ready');},()=>setFunctionCatalogStatus('error'));
  },[]);
  useEffect(reloadFunctionCatalog,[reloadFunctionCatalog]);
  const [filePath,setFilePath]=useState<string|null>(null);
  const [replacement,setReplacement]=useState<'new'|'open'|'recover'|null>(null);
  const [recoveryCopies,setRecoveryCopies]=useState<{id:string;modified:string}[]>([]);
  const [recoveryId,setRecoveryId]=useState('');
  const [apiPending,setApiPending]=useState(getPendingMutations());
  const mutationLock = useRef(false);
  const metadataSequence = useRef(0);
  const clipboard = useRef<{text:string;values:string[][];snapshot:string;sheet:string;range:GridRange} | null>(null);
  const [pasteSpecialOpen, setPasteSpecialOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [copyData, setCopyData] = useState<{text:string;values:string[][];snapshot:string;key:string} | null>(null);
  const reviewDrafts = useRef(new Map<string, ReviewDraft>());
  const [isExplainOpen, setIsExplainOpen] = useState(false);
  const [isQuestionsOpen, setIsQuestionsOpen] = useState(false);
  const [dependencyPath, setDependencyPath] = useState<{sheetName: string;row: number;col: number}[]>([]);
  const [dependencyHighlight, setDependencyHighlight] = useState<DependencyHighlight | null>(null);
  const clearDependencyTrace = () => { setDependencyPath([]); setDependencyHighlight(null); };
  const [reviewRevision, setReviewRevision] = useState(0);
  const [workbookEpoch, setWorkbookEpoch] = useState(0);
  const [sheetName, setSheetName] = useState('Sheet1');
  const [selectedRow, setSelectedRow] = useState(0);
  const [selectedCol, setSelectedCol] = useState(0);
  const [storedViewport, setViewport] = useState<ViewportResponse | null>(null);
  const viewport = storedViewport?.sheet_name === sheetName ? storedViewport : null;
  const [stagedCount, setStagedCount] = useState(0);
  const [sheetWidths,setSheetWidths]=useState<Record<string,Record<number,number>>>({});
  const [sheetShowFormulas,setSheetShowFormulas]=useState<Record<string,boolean>>({});
  const showFormulas=sheetShowFormulas[sheetName]??false;
  const [sheetFreeze,setSheetFreeze]=useState<Record<string,{rows:number;columns:number}>>({});
  const frozenRows=sheetFreeze[sheetName]?.rows??0,frozenColumns=sheetFreeze[sheetName]?.columns??0;
  const viewportConfig=JSON.stringify([workbookEpoch,sheetName,frozenRows,frozenColumns]);
  const viewportConfigRef=useRef(viewportConfig);
  viewportConfigRef.current=viewportConfig;
  const [viewportCache]=useState(()=>new ViewportCache(viewportBudget(
    (navigator as Navigator & {deviceMemory?:number}).deviceMemory)));
  const viewportScope=useRef('');
  const displayedWindow=useRef<ViewportResponse|null>(null);
  const viewportPending=useRef<{bounds:ViewBounds; controller:AbortController; task:Promise<void>}|null>(null);
  const priorBounds=useRef<ViewBounds|undefined>(undefined);
  const [viewportRevision,setViewportRevision]=useState(0);
  const [widthDraft,setWidthDraft]=useState('110');
  const columnWidths=sheetWidths[sheetName]??{};
  useEffect(()=>setWidthDraft(String(columnWidths[selectedCol]??110)),[sheetName,selectedCol,sheetWidths]);
  const [tableMetadata, setTableMetadata] = useState<{sheet: string; tables: WorksheetTable[]} | null>(null);
  const tables = tableMetadata?.sheet === sheetName ? tableMetadata.tables : [];

  // Visible bounds
  const [visibleBounds, setVisibleBounds] = useState<ViewBounds>({
    startRow: 0,
    startCol: 0,
    endRow: 40,
    endCol: 20,
    pinnedRows: 0,
    pinnedColumns: 0,
  });
  const latestBounds=useRef(visibleBounds);

  // Modal open states
  const [isSchemaBrowserOpen, setIsSchemaBrowserOpen] = useState(false);
  const [isQueryEditorOpen, setIsQueryEditorOpen] = useState(false);
  const [queryInitialConn, setQueryInitialConn] = useState('');
  const [queryInitialSql, setQueryInitialSql] = useState('');
  const [isInsertTableOpen, setIsInsertTableOpen] = useState(false);
  const [isStagedReviewOpen, setIsStagedReviewOpen] = useState(false);
  const [isPivotsOpen, setIsPivotsOpen] = useState(false);
  const [isChartsOpen, setIsChartsOpen] = useState(false);
  const [isAuditLogOpen, setIsAuditLogOpen] = useState(false);
  const [isBenchmarksOpen, setIsBenchmarksOpen] = useState(false);
  const [isAboutOpen, setIsAboutOpen] = useState(false);

  // Status message
  const [statusMsg, setStatusMsg] = useState('Ready');

  // Load workbook tables & staged patches count
  const refreshMetadata = useCallback(async () => {
    const request=++metadataSequence.current;
    try {
      const [wb, staged] = await Promise.all([
        api.getWorkbook().catch(() => null),
        api.getStaged().catch(() => []),
      ]);
      if(request!==metadataSequence.current)return;
      if (wb) {
        setFilePath(wb.file_path);
        setSheetName(wb.active_sheet);
        setSheets(wb.sheets.map(s => s.name));
        setSheetShowFormulas(Object.fromEntries(wb.sheets.map(s=>[s.name,s.show_formulas??false])));
        setSheetFreeze(Object.fromEntries(wb.sheets.map(s=>[s.name,{rows:s.frozen_rows??Number(!!s.freeze_top_row),columns:s.frozen_columns??Number(!!s.freeze_first_column)}])));
        setSheetWidths(Object.fromEntries(wb.sheets.map(s=>[s.name,s.column_widths??{}])));
        setTableMetadata({sheet: wb.active_sheet, tables: wb.sheets.find(s => s.name === wb.active_sheet)?.tables ?? []});
      }
      setStagedCount(staged.length);
      setReviewRevision(n => n + 1);
    } catch {
      // ignore
    }
  }, []);

  const invalidateViewport = useCallback(() => {
    viewportPending.current?.controller.abort();
    viewportPending.current=null;
    viewportCache.clear();
    displayedWindow.current=null;
    priorBounds.current=undefined;
  }, [viewportCache]);

  useEffect(() => {
    let wasPending=getPendingMutations()>0;
    return subscribeMutations(() => {
      const count=getPendingMutations();
      setApiPending(count);
      if (!wasPending && count>0) invalidateViewport();
      // Explicit App mutations already await a fresh viewport. This covers
      // writes from nested panels that do not request their own refresh.
      if (wasPending && count===0 && !mutationLock.current) setViewportRevision(n=>n+1);
      wasPending=count>0;
    });
  }, [invalidateViewport]);
  useEffect(() => invalidateViewport, [invalidateViewport]);

  const requestViewport = useCallback(async (current:ViewBounds) => {
    if(viewportConfig!==viewportConfigRef.current || getPendingMutations()>0)return;
    if(viewportScope.current!==viewportConfig){
      invalidateViewport();
      viewportScope.current=viewportConfig;
    }
    const withVisiblePanes = (b:ViewBounds):ViewBounds => ({
      ...b,pinnedRows:Math.min(frozenRows,b.pinnedRows),pinnedColumns:Math.min(frozenColumns,b.pinnedColumns),
    });
    const bounds=withVisiblePanes(current);
    const previous=priorBounds.current;
    priorBounds.current=bounds;
    const shown=displayedWindow.current;
    const cached=shown&&viewportCovers(shown,bounds) ? shown : viewportCache.find(bounds);
    if(cached){
      viewportPending.current?.controller.abort();
      viewportPending.current=null;
      displayedWindow.current=cached;
      setViewport(cached);
      return;
    }
    const pending=viewportPending.current;
    if(pending&&requestCovers(pending.bounds,bounds))return pending.task;
    pending?.controller.abort();
    const window=viewportCache.plan(bounds,previous);
    const controller=new AbortController();
    const task=(async()=>{
      try{
        const view=await api.getViewportPanes(sheetName,window.startRow,window.startCol,
          window.endRow,window.endCol,window.pinnedRows,window.pinnedColumns,0,controller.signal);
        if(controller.signal.aborted||viewportConfig!==viewportConfigRef.current||view.sheet_name!==sheetName)return;
        viewportCache.put(view);
        if(viewportCovers(view,withVisiblePanes(latestBounds.current))){
          displayedWindow.current=view;
          setViewport(view);
        }
      }catch(error){
        if(!controller.signal.aborted)console.error('Failed to fetch viewport:',error);
      }finally{
        if(viewportPending.current?.controller===controller)viewportPending.current=null;
      }
    })();
    viewportPending.current={controller,bounds:window,task};
    await task;
  }, [sheetName,viewportConfig,frozenRows,frozenColumns,viewportCache,invalidateViewport]);

  const fetchViewportData = useCallback(async () => {
    if(viewportConfig!==viewportConfigRef.current)return;
    invalidateViewport();
    await requestViewport(latestBounds.current);
  }, [viewportConfig,invalidateViewport,requestViewport]);

  useEffect(() => { void refreshMetadata(); }, [refreshMetadata]);
  useEffect(() => {
    // Coalesce visual scroll events within a frame; a distant keyboard jump
    // requests the destination window, never a corridor of intermediate rows.
    const frame=requestAnimationFrame(()=>{void requestViewport(latestBounds.current);});
    return ()=>cancelAnimationFrame(frame);
  }, [requestViewport, visibleBounds, viewportRevision]);

  const handleViewportChange = useCallback((startRow: number, startCol: number, endRow: number, endCol: number, pinnedRows: number, pinnedColumns: number) => {
    const next={startRow,startCol,endRow,endCol,pinnedRows,pinnedColumns};
    latestBounds.current=next;
    setVisibleBounds(prev => prev.startRow === startRow && prev.startCol === startCol && prev.endRow === endRow && prev.endCol === endCol
      && prev.pinnedRows === pinnedRows && prev.pinnedColumns === pinnedColumns ? prev : next);
  }, []);

  const handleCommitCell = async (row: number, col: number, rawInput: string, asSqlNull=false): Promise<boolean> => {
    if(mutationLock.current)return false;mutationLock.current=true;setBusy(true);clipboard.current=null;
    try {
      setStatusMsg(`Setting ${coordToA1(row, col)}...`);
      const key = coordToA1(row, col);
      const existing = viewport?.cells[key];

      // A live row carries only its own stable key and original value. Never
      // ask for an entire source result to stage an edit to a visible cell.
      const live = existing?.live_edit;
      if (live) {
        const tbl = tables.find((table) => table.name === existing?.table_name);
        const columnName = live.column_name;
        if (!tbl?.live || tbl.columns[col - tbl.anchor.col] !== columnName)
          throw new Error('Live table metadata is not ready. Try the edit again.');
        if ([live.pk_value, live.original_value].some(value => value?.type === 'Integer' && !Number.isSafeInteger(value.value))) {
          throw new Error('This source row has an integer that cannot be addressed exactly in the browser. No edit was staged.');
        }
        await api.stageEdit({
          connection_name: live.connection_name,
          table_name: live.table_name,
          pk_columns: [live.pk_column],
          pk_values: [live.pk_value],
          column_name: columnName,
          original_value: live.original_value,
          ...(asSqlNull ? {proposed_value: {type: 'Null'} as const} : {raw_input: rawInput}),
        });
        setStatusMsg(`Staged edit for ${tbl.name}[${columnName}]`);
        await refreshMetadata();
        await fetchViewportData();
        return true;
      }
      if (tables.some(({live,range}) => live && row>=range.start_row && row<=range.end_row && col>=range.start_col && col<=range.end_col)) {
        throw new Error(existing ? 'This live table cell is read-only' : 'Wait for this live row to load before editing');
      }

      // Check if this cell belongs to an editable table
      if (existing?.table_name && !existing.is_table_header) {
        // Find table and its source
        const tbl = tables.find((t) => t.name === existing.table_name);
        if (tbl) {
          const sources = await api.getSources();
          const src = sources.find((s) => s.name === tbl.source_name);
          if (src?.lineage?.is_editable && src.last_result) {
            const colIdxInTable = col - tbl.anchor.col;
            const rowIdxInTable = row - tbl.anchor.row - 1; // subtract header
            const targetColName = tbl.columns[colIdxInTable];

            if (targetColName && src.lineage.editable_columns.includes(targetColName)) {
              // Locate primary key column indices in the source result
              const pkCols = src.lineage.primary_keys;
              const pkValues = pkCols.map((pk) => {
                const pkIdx = src.last_result?.columns.findIndex((c) => c.name === pk);
                const value = pkIdx !== undefined && pkIdx >= 0 ? src.last_result?.rows[rowIdxInTable]?.[pkIdx] : undefined;
                if (!value || value.type === 'Null') throw new Error('Missing primary key value');
                return value;
              });

              const origVal = src.last_result.rows[rowIdxInTable]?.[colIdxInTable];
              if (!origVal) throw new Error('Source row is no longer available; refresh the table');
              if ([origVal,...pkValues].some(value=>value.type==='Integer'&&!Number.isSafeInteger(value.value))) {
                throw new Error('This source row has an integer that cannot be addressed exactly in the browser. No edit was staged.');
              }
              const names=src.last_result.columns.map(column=>column.name.toLowerCase());
              if([targetColName,...pkCols].some(name=>names.filter(projected=>projected===name.toLowerCase()).length!==1)){
                throw new Error('The query does not expose unique target and primary key column names.');
              }

              // The server parses raw text using the declared SQL type. It never uses the previous cell's type.
              await api.stageEdit({
                connection_name: src.connection_name,
                table_name: src.lineage.base_table || tbl.source_name,
                pk_columns: pkCols,
                pk_values: pkValues,
                column_name: targetColName,
                original_value: origVal,
                ...(asSqlNull?{proposed_value:{type:'Null'} as const}:{raw_input:rawInput}),
              });

              setStatusMsg(`Staged edit for ${tbl.name}[${targetColName}]`);
              await refreshMetadata();
              await fetchViewportData();
              return true;
            }
          }
        }
        throw new Error('This table column is read-only');
      }
      if (existing?.is_table_header) throw new Error('Table headers are read-only');
      if(asSqlNull)throw new Error('Select a SQL source data cell to set NULL.');

      // Normal spreadsheet cell edit
      await api.setCell(sheetName, row, col, rawInput);
      setStatusMsg(`Updated ${coordToA1(row, col)}`);
      await refreshMetadata();
      await fetchViewportData();
    } catch (err: any) {
      setStatusMsg(`Error: ${err.message}`);
      return false;
    } finally {mutationLock.current=false;setBusy(false);}
    return true;
  };

  const activePosition = {row:selectedRow, col:selectedCol};
  const selection = selectionAxis
    ? wholeAxisRange(selectionAxis, activePosition, viewport?.total_rows ?? 1000, viewport?.total_cols ?? 100)
    : normalizedRange(anchor, activePosition);
  const rangeKey = `${sheetName}:${selection.start.row}:${selection.start.col}:${selection.end.row}:${selection.end.col}:${reviewRevision}`;
  useEffect(() => {
    let cancelled = false;
    setCopyData(null);
    const count=(selection.end.row-selection.start.row+1)*(selection.end.col-selection.start.col+1);
    if(count>10000)return;
    api.captureRange(sheetName,selection).then(({values:rows,snapshot})=>{
      if(cancelled)return;
      setCopyData({text:encodeTsv(rows),values:rows,snapshot,key:rangeKey});
    }).catch(()=>{});
    return ()=>{cancelled=true;};
  },[rangeKey]);
  const selectCell = (r:number,c:number,extend=false) => {
    setSelectedRow(r);setSelectedCol(c);setSelectionAxis(null);
    if(!extend)setAnchor({row:r,col:c});
    else if(selectionAxis)setAnchor({row:selectedRow,col:selectedCol});
  };
  const mutate = async (label:string, operation:()=>Promise<unknown>, preserveClipboard=false) => {
    if(mutationLock.current)return;
    mutationLock.current=true;setBusy(true);
    try {
      await operation(); if(!preserveClipboard)clipboard.current=null;
      await refreshMetadata();
      await fetchViewportData();
      setStatusMsg(label);
    } catch(error:any){setStatusMsg(`Error: ${error.message}`);}
    finally{mutationLock.current=false;setBusy(false);}
  };
  const insertSelectedRows = () => {
    if (busy || apiPending || mutationLock.current || gridDraft || formulaDraft) return;
    for (const draft of reviewDrafts.current.values()) {
      if (draft.reviewer.trim() || draft.note.trim() || draft.evidence.length || draft.attachments?.length) {
        setStatusMsg('Finish or discard your review drafts before inserting rows.');return;
      }
    }
    const before=selection.start.row;
    const count=selection.end.row-before+1;
    const label=count===1?'one row':`${count} rows`;
    if(count>1000){setStatusMsg('Select at most 1,000 rows to insert.');return;}
    if(!window.confirm(`Insert ${label} above row ${before+1} in ${sheetName}? Existing rows and references will move. This clears Undo/Redo history and cannot be undone. Use Save As beforehand if you want a separate copy.`))return;
    void mutate(`Inserted ${label} above row ${before+1}`, async()=>{
      await api.insertRows(sheetName,before,count);
      clearDependencyTrace();
      reviewDrafts.current.clear();
      setIsExplainOpen(false);
      setIsQuestionsOpen(false);
      selectCell(before,selectedCol);
    });
  };
  const resizeColumn = (column:number,width:number|null) => {
    if(width!==null&&(!Number.isInteger(width)||width<48||width>600)){setStatusMsg('Column width must be a whole number from 48 to 600 pixels.');return;}
    void mutate(`Resized column ${colIndexToName(column)}`,()=>api.setColumnWidth(sheetName,column,width),true);
  };
  const boundedSelection = () => {
    try { return boundedRangeDimensions(selection); }
    catch(error) { setStatusMsg(error instanceof Error ? error.message : 'Selection is too large'); return null; }
  };
  const handleClear = () => {
    const dimensions=boundedSelection(); if(!dimensions)return;
    void mutate('Cleared selection',()=>api.editRange(sheetName,selection.start.row,selection.start.col,
      Array.from({length:dimensions.rows},()=>Array(dimensions.columns).fill(''))));
  };
  const handleApplyFormat = (format:string) => {
    const tableRanges = tables.map(({range}) => ({
      start: {row: range.start_row, col: range.start_col},
      end: {row: range.end_row, col: range.end_col},
    }));
    if (!ordinaryFormatRangeIsBounded(selection, tableRanges)) {
      boundedSelection();
      return;
    }
    const style=viewport?.cells[coordToA1(selectedRow,selectedCol)]?.style ?? {bold:false,italic:false,fill:null,number_format:null};
    let attribute: string, value: string;
    if(format==='Bold'){attribute='bold';value=String(!style.bold);}
    else if(format==='Italic'){attribute='italic';value=String(!style.italic);}
    else if(format.startsWith('Fill:')){attribute='fill';value=format.slice(5);}
    else {attribute='number_format';value=format.toLowerCase();}
    void mutate('Formatted selection',()=>api.formatRangeAttribute(sheetName,selection,attribute,value));
  };
  const handleCopy = (event:React.ClipboardEvent) => {
    event.preventDefault();
    if(copyData===null || copyData.key !== rangeKey){setStatusMsg('Selection is loading or exceeds 10,000 cells. Try copying again.');return;}
    event.clipboardData.setData('text/plain',copyData.text);
    clipboard.current={...copyData,sheet:sheetName,range:selection};
    setStatusMsg('Copied selection. Paste into a destination cell.');
  };
  const pasteText = (text:string) => {
    const copied=clipboard.current;
    void mutate('Pasted selection',()=>copied?.text===text
      ?api.copyRange(copied.sheet,copied.range,sheetName,{row:selectedRow,col:selectedCol},copied.values,copied.snapshot)
      :api.editRange(sheetName,selectedRow,selectedCol,decodeTsv(text)),true);
  };
  const handlePaste = (event:React.ClipboardEvent) => {event.preventDefault();pasteText(event.clipboardData.getData('text/plain'));};
  const handlePasteSpecial = (mode: PasteMode) => {
    if (gridDraft || formulaDraft || mutationLock.current) return;
    const copied = clipboard.current;
    if (!copied) { setStatusMsg('Copy a worksheet selection before using Paste Special.'); return; }
    void mutate(mode === 'values' ? 'Pasted values only' : 'Pasted formulas only',
      () => api.pasteSpecial(copied.sheet, copied.range, sheetName, {row:selectedRow,col:selectedCol}, copied.snapshot, mode), true);
  };
  const handleFillDown = () => {if(!boundedSelection())return;void mutate('Filled selection down',()=>api.fillRange(sheetName,{start:selection.start,end:{row:selection.start.row,col:selection.end.col}},selection));};
  const handleFillRight = () => {if(!boundedSelection())return;void mutate('Filled selection right',()=>api.fillRange(sheetName,{start:selection.start,end:{row:selection.end.row,col:selection.start.col}},selection));};
  const changeSheet = (name:string) => void mutate(`Selected ${name}`,async()=>{await api.selectSheet(name);setViewport(null);selectCell(0,0);clearDependencyTrace();},true);
  const moveToExplanation = (row: number, col: number, targetSheet: string, after: () => void) => {
    if (mutationLock.current) return;
    const apply = () => { selectCell(row, col); setIsExplainOpen(true); after(); };
    if (targetSheet === sheetName) apply();
    else void mutate(`Selected ${targetSheet}!${coordToA1(row, col)}`, async () => {
      await api.selectSheet(targetSheet); setSheetName(targetSheet); apply();
    }, true);
  };
  const followDependency = (row: number, col: number, targetSheet = sheetName) => {
    if (row === selectedRow && col === selectedCol && targetSheet === sheetName) return;
    moveToExplanation(row, col, targetSheet, () => {
      setDependencyPath(previous => [...previous, { sheetName, row: selectedRow, col: selectedCol }].slice(-20));
    });
  };
  const goBackDependency = () => {
    const previous = dependencyPath[dependencyPath.length - 1];
    if (!previous) return;
    moveToExplanation(previous.row, previous.col, previous.sheetName, () => {
      setDependencyHighlight(null); setDependencyPath(path => path.slice(0, -1));
    });
  };
  const addSheet = () => {const name=prompt('New worksheet name:',`Sheet${sheets.length+1}`);if(name)void mutate(`Added ${name}`,async()=>{await api.addSheet(name);await api.selectSheet(name);selectCell(0,0);clearDependencyTrace();});};
  const canChangeSheetStructure = () => {
    if (busy || apiPending || mutationLock.current || gridDraft || formulaDraft) {
      setStatusMsg('Finish the current edit before changing worksheets.');
      return false;
    }
    for (const draft of reviewDrafts.current.values()) {
      if (draft.reviewer.trim() || draft.note.trim() || draft.evidence.length || draft.attachments?.length) {
        setStatusMsg('Finish or discard your review drafts before changing worksheets.');
        return false;
      }
    }
    return true;
  };
  const closeSheetReview = () => {
    clearDependencyTrace();
    setIsExplainOpen(false);
    setIsQuestionsOpen(false);
    reviewDrafts.current.clear();
  };
  const renameSheet = (current: string) => {
    if (!canChangeSheetStructure()) return;
    const name = prompt('Rename worksheet:', current)?.trim();
    if (name && name !== current) void mutate(`Renamed ${current} to ${name}`, async () => {
      await api.renameSheet(current, name);
      closeSheetReview();
    });
  };
  const deleteSheet = (name: string) => {
    if (sheets.length <= 1 || !canChangeSheetStructure()) return;
    if (!window.confirm(`Delete worksheet "${name}"? This removes its cells and tables from the workbook, but does not change the database. It cannot be undone and clears worksheet Undo/Redo history.`)) return;
    void mutate(`Deleted ${name}`, async () => {
      const { active_sheet } = await api.deleteSheet(name);
      closeSheetReview();
      if (active_sheet !== sheetName) {
        setViewport(null);
        setSheetName(active_sheet);
        selectCell(0, 0);
      }
    });
  };

  const draftProblem=()=>saveBlocker(gridDraft,formulaDraft,
    isCloseOpen||isExplainOpen||isQuestionsOpen||isSchemaBrowserOpen||isQueryEditorOpen||isInsertTableOpen||isStagedReviewOpen||isPivotsOpen||isChartsOpen||isAuditLogOpen||isBenchmarksOpen,
    reviewDrafts.current.values());
  const readyForFileAction=()=>{
    const problem=draftProblem();if(problem){setStatusMsg(problem);return false;}
    if(mutationLock.current||getPendingMutations()>0){setStatusMsg('Wait for the current operation to finish before saving or replacing the workbook.');return false;}
    return true;
  };
  const handleSaveWorkbook=async(saveAs=false):Promise<boolean>=>{
    if(!readyForFileAction())return false;
    const path=saveAs||!filePath?prompt('Save committed workbook content to:',filePath??'my_workbook.gridloom'):filePath;
    if(!path?.trim())return false;
    if(!readyForFileAction())return false;
    mutationLock.current=true;setBusy(true);
    try {await api.saveWorkbook(path);metadataSequence.current++;setFilePath(path);setStatusMsg(`Saved workbook to ${path}`);return true;}
    catch(err:any){setStatusMsg(`Failed to save: ${err.message}`);return false;}
    finally{mutationLock.current=false;setBusy(false);}
  };
  const requestReplacement=(kind:'new'|'open')=>{if(readyForFileAction())setReplacement(kind);};
  const continueReplacement=async(save:boolean)=>{
    if(!replacement||!readyForFileAction())return;
    const kind=replacement;
    if(kind==='recover'&&!recoveryId)return;
    if(save&&!await handleSaveWorkbook())return;
    const path=kind==='open'?prompt('Enter path of .gridloom file to open:',filePath??'my_workbook.gridloom'):null;
    if(kind==='open'&&!path?.trim())return;
    if(!readyForFileAction())return;
    setReplacement(null);
    await mutate(kind==='new'?'Created new workbook':kind==='recover'?'Recovered workbook. Use Save As to keep a named copy.':`Opened workbook from ${path}`,async()=>{
      if(kind==='new')await api.newWorkbook();else if(kind==='recover')await api.restoreRecovery(recoveryId);else await api.openWorkbook(path!);
      invalidateViewport();setViewport(null);selectCell(0,0);
      setWorkbookEpoch(e=>e+1);reviewDrafts.current.clear();clearDependencyTrace();
    });
  };
  const handleNewWorkbook=()=>requestReplacement('new');
  const handleOpenWorkbook=()=>requestReplacement('open');
  const handleRecoverWorkbook=async()=>{
    if(!readyForFileAction())return;
    try {
      const copies=await api.listRecovery();
      if(!readyForFileAction())return;
      if(!copies.length){setStatusMsg('No recovery copies from earlier workbooks or sessions.');return;}
      setRecoveryCopies(copies);setRecoveryId(copies[0].id);setReplacement('recover');
    } catch(err:any){setStatusMsg(`Cannot list recovery copies: ${err.message}`);}
  };

  const handleRefreshAll = async () => {
    await mutate('All sources refreshed successfully', async () => {
      const workbook = await api.getWorkbook();
      for (const sheet of workbook.sheets)
        for (const table of sheet.tables)
          if (table.live) await api.refreshLiveTable(sheet.name, table.name);
      for (const source of workbook.sources) await api.refreshSource(source);
    });
  };

  const openQueryWithTable = (connName: string, tableName: string) => {
    setQueryInitialConn(connName);
    setQueryInitialSql(`SELECT * FROM "${tableName.replace(/"/g, '""')}"`);
    setIsQueryEditorOpen(true);
  };

  const activeKey = coordToA1(selectedRow, selectedCol);
  const activeCellData = viewport?.cells[activeKey];
  const selectedCellLoaded = isCellLoaded(viewport,selectedRow,selectedCol);

  const findNavigate = (match:FindMatch) => {
    if(mutationLock.current)return;
    const apply=()=>{selectCell(match.coord.row,match.coord.col);clearDependencyTrace();};
    if(match.sheet_name===sheetName)apply();
    else void mutate(`Selected ${match.sheet_name}!${match.address}`,async()=>{
      await api.selectSheet(match.sheet_name);setSheetName(match.sheet_name);setViewport(null);apply();
    },true);
  };
  const freezeDisabled = busy || apiPending>0 || gridDraft || formulaDraft;
  const updateFreeze = (rows:number, columns:number) => {
    if (freezeDisabled || mutationLock.current || getPendingMutations()>0) return;
    void mutate('Updated frozen panes',()=>api.setFreeze(sheetName,rows,columns),true);
  };

  return (
    <div onKeyDown={e=>{if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='s'&&!e.altKey){e.preventDefault();if(!replacement)void handleSaveWorkbook(e.shiftKey);return;}if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='f' && (e.target as HTMLElement).closest('[role="grid"]')){e.preventDefault();setIsFindOpen(true);}}} className="flex flex-col h-screen w-screen bg-slate-100 overflow-hidden font-sans">
      {replacement&&<div className="fixed inset-0 z-[100] bg-black/30 flex items-center justify-center" role="dialog" aria-modal="true" aria-label="Replace current workbook?" onKeyDown={e=>{e.stopPropagation();if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='s')e.preventDefault();if(e.key==='Escape'&&!busy)setReplacement(null);
          if(e.key==='Tab'){const buttons=Array.from(e.currentTarget.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'));const first=buttons[0],last=buttons[buttons.length-1];if(e.shiftKey&&document.activeElement===first){e.preventDefault();last?.focus();}else if(!e.shiftKey&&document.activeElement===last){e.preventDefault();first?.focus();}}}}>
        <div className="bg-white rounded shadow-xl p-5 max-w-lg space-y-3">
          <h2 className="font-semibold">Replace current workbook?</h2>
          <p>This always asks before replacing the workbook. Choose whether to save its current committed content first. Unsubmitted cell, review and dialog drafts are not saved. Saving does not apply staged database edits.</p>
          {replacement==='recover'&&<label className="block">Recovery copy <select value={recoveryId} onChange={e=>setRecoveryId(e.target.value)}>{recoveryCopies.map(copy=><option key={copy.id} value={copy.id}>{new Date(copy.modified).toLocaleString()} ({copy.id.slice(0,8)})</option>)}</select></label>}
          <div className="flex gap-2"><button disabled={busy||apiPending>0} onClick={()=>void continueReplacement(true)}>Save and continue</button><button disabled={busy||apiPending>0} onClick={()=>void continueReplacement(false)}>Discard and continue</button><button autoFocus disabled={busy} onClick={()=>setReplacement(null)}>Cancel</button></div>
        </div>
      </div>}
      <div className="contents" inert={replacement?true:undefined}>
      {/* Top Application Toolbar */}
      <Toolbar
        busy={busy}
        closeAddon={closeAddon}
        onToggleCloseAddon={toggleCloseAddon}
        onOpenClose={() => setIsCloseOpen(true)}
        onUndo={() => void mutate('Undid worksheet edit',api.undo)}
        onRedo={() => void mutate('Redid worksheet edit',api.redo)}
        onFillDown={handleFillDown}
        onFillRight={handleFillRight}
        onOpenExplain={() => setIsExplainOpen(true)}
        onOpenQuestions={() => setIsQuestionsOpen(true)}
        onOpenSchemaBrowser={() => setIsSchemaBrowserOpen(true)}
        onOpenQueryEditor={() => setIsQueryEditorOpen(true)}
        onOpenInsertTable={() => setIsInsertTableOpen(true)}
        onOpenStagedReview={() => setIsStagedReviewOpen(true)}
        onOpenPivots={() => setIsPivotsOpen(true)}
        onOpenCharts={() => setIsChartsOpen(true)}
        onOpenAuditLog={() => setIsAuditLogOpen(true)}
        onOpenBenchmarks={() => setIsBenchmarksOpen(true)}
        onOpenAbout={() => setIsAboutOpen(true)}
        onNewWorkbook={handleNewWorkbook}
        onSaveWorkbook={()=>void handleSaveWorkbook()}
        onSaveAsWorkbook={()=>void handleSaveWorkbook(true)}
        onOpenWorkbook={handleOpenWorkbook}
        onRefreshAll={handleRefreshAll}
        onApplyFormat={handleApplyFormat}
        fileExtras={<>
        <button disabled={busy} onClick={()=>void handleRecoverWorkbook()} className={worksheetCommandClass}>Recover autosave</button>
        </>}
        homeExtras={<>
        <button disabled={busy} onClick={()=>setIsFindOpen(true)} className={worksheetCommandClass}>Find (Ctrl+F)</button>
        <button type="button" className={worksheetCommandClass} disabled={busy||gridDraft||formulaDraft}
          aria-haspopup="dialog" aria-keyshortcuts="Control+Alt+V Meta+Alt+V"
          onClick={()=>setPasteSpecialOpen(true)} title="Ctrl/Command + Alt/Option + V">Paste Special</button>
        <button type="button" className={worksheetCommandClass} aria-haspopup="dialog"
          onClick={()=>setShortcutsOpen(true)}>Keyboard shortcuts</button>
        </>}
        insertExtras={<>
<button disabled={busy||apiPending>0||gridDraft||formulaDraft||!selectedCellLoaded} onClick={insertSelectedRows} className={worksheetCommandClass} title="Insert one blank row per selected row. Clears Undo/Redo.">Insert rows above</button>
        </>}
        dataExtras={<>
<button disabled={busy||apiPending>0||gridDraft||formulaDraft} onMouseDown={e=>{if(gridDraft||formulaDraft)e.preventDefault();}} onClick={()=>setSortFilterRange(selection)} className={worksheetCommandClass}>Sort &amp; filter</button>
        <button disabled={busy} onClick={()=>{setLedgerTarget(undefined);setIsLedgerOpen(true);}} className={worksheetCommandClass}>SQLite ledger</button>
        <button disabled={busy||!selectedCellLoaded||gridDraft||formulaDraft} onClick={()=>{setLedgerTarget({sheet_name:sheetName,row:selectedRow,col:selectedCol});setIsLedgerOpen(true);}} className={worksheetCommandClass}>Trace balance</button>
        {activeCellData?.table_name&&!activeCellData.is_table_header&&<button disabled={busy||gridDraft||formulaDraft} onClick={()=>void handleCommitCell(selectedRow,selectedCol,'',true)} className={worksheetCommandClass} title="Stage SQL NULL; use a normal edit to enter empty or literal text">Set SQL NULL</button>}
        </>}
        viewExtras={<>
        <form className="flex items-center gap-1" onSubmit={e=>{e.preventDefault();resizeColumn(selectedCol,Number(widthDraft));}}>
          <label htmlFor="column-width">Column {colIndexToName(selectedCol)} width</label><input id="column-width" aria-label="Column width in pixels" type="number" min="48" max="600" step="1" disabled={busy} value={widthDraft} onChange={e=>setWidthDraft(e.target.value)} className="w-16 border rounded p-1"/>
          <span>px</span><button disabled={busy} className={worksheetCommandClass}>Set width</button><button type="button" disabled={busy} onClick={()=>resizeColumn(selectedCol,null)} className={worksheetCommandClass}>Reset width</button>
        </form>
        <label className="flex items-center gap-1 rounded border border-slate-300 px-2 py-1" onMouseDown={e=>e.preventDefault()} title={gridDraft||formulaDraft?'Finish or cancel the current cell edit before changing formula view.':'Show formula text without changing values, clipboard, Find or exports.'}>
          <input type="checkbox" checked={showFormulas} disabled={busy||apiPending>0||gridDraft||formulaDraft}
            onChange={e=>{if(gridDraft||formulaDraft||mutationLock.current||getPendingMutations()>0)return;void mutate('Updated formula view',()=>api.setShowFormulas(sheetName,e.target.checked),true);}}/>Show formulas
        </label>
        <label className="flex items-center gap-1 rounded border border-slate-300 px-2 py-1"><input type="checkbox" checked={frozenRows===1} disabled={freezeDisabled} onChange={e=>updateFreeze(e.target.checked?1:0,frozenColumns)}/>Freeze top row</label>
        <label className="flex items-center gap-1 rounded border border-slate-300 px-2 py-1"><input type="checkbox" checked={frozenColumns===1} disabled={freezeDisabled} onChange={e=>updateFreeze(frozenRows,e.target.checked?1:0)}/>Freeze first column</label>
        <button type="button" className={worksheetCommandClass} disabled={freezeDisabled||selectedRow===0} onClick={()=>updateFreeze(selectedRow,frozenColumns)}>Freeze rows above</button>
        <button type="button" className={worksheetCommandClass} disabled={freezeDisabled||selectedCol===0} onClick={()=>updateFreeze(frozenRows,selectedCol)}>Freeze columns to the left</button>
        <button type="button" className={worksheetCommandClass} disabled={freezeDisabled||(selectedRow===0&&selectedCol===0)} onClick={()=>updateFreeze(selectedRow,selectedCol)}>Freeze at selection</button>
        {frozenRows>1&&<button type="button" className={worksheetCommandClass} disabled={freezeDisabled} onClick={()=>updateFreeze(0,frozenColumns)}>Unfreeze {frozenRows} rows</button>}
        {frozenColumns>1&&<button type="button" className={worksheetCommandClass} disabled={freezeDisabled} onClick={()=>updateFreeze(frozenRows,0)}>Unfreeze {frozenColumns} columns</button>}
        </>}
        stagedCount={stagedCount}
      />

      {pasteSpecialOpen && <PasteSpecialDialog onClose={()=>setPasteSpecialOpen(false)}
        onPaste={handlePasteSpecial} busy={busy||gridDraft||formulaDraft}
        copySource={clipboard.current ? `${clipboard.current.sheet}!${coordToA1(clipboard.current.range.start.row,clipboard.current.range.start.col)}:${coordToA1(clipboard.current.range.end.row,clipboard.current.range.end.col)}` : null}/>}
      {shortcutsOpen && <KeyboardShortcutsDialog onClose={()=>setShortcutsOpen(false)}/>}

      {/* Formula Bar */}
      <FormulaBar sheetName={sheetName} onDraftChange={setFormulaDraft} onDraftText={setFormulaDraftText}
        functionNames={functionNames} catalogStatus={functionCatalogStatus} onReloadCatalog={reloadFunctionCatalog} key={`${workbookEpoch}:${sheetName}`}
        disabled={busy || !selectedCellLoaded}
        selectedRow={selectedRow}
        selectedCol={selectedCol}
        activeCellData={activeCellData}
        maxRows={viewport?.total_rows??1000} maxCols={viewport?.total_cols??100}
        onGoTo={(row,col)=>{clearDependencyTrace();selectCell(row,col);}}
        onCommit={(val) => handleCommitCell(selectedRow, selectedCol, val)}
      />

      {/* Virtualized Worksheet Grid Canvas */}
      <div className="flex flex-1 min-h-0 overflow-hidden">
      <Grid onDraftChange={setGridDraft} functionNames={functionNames} externalDraft={formulaDraftText} key={workbookEpoch}
        sheetName={sheetName}
        showFormulas={showFormulas}
        frozenRows={frozenRows} frozenColumns={frozenColumns}
        columnWidths={columnWidths} onColumnResize={resizeColumn}
        viewport={viewport}
        selectedRow={selectedRow}
        selectedCol={selectedCol}
        selection={selection} busy={busy}
        traceHighlight={dependencyHighlight}
        onSelectCell={(row, col, extend) => { clearDependencyTrace(); selectCell(row, col, extend); }}
        onSelectAxis={axis => { clearDependencyTrace(); setSelectionAxis(axis); }}
        onNavigationError={() => setStatusMsg("Could not navigate the worksheet. Try again.")}
        onCopy={handleCopy} onPaste={handlePaste} onPasteSpecial={()=>setPasteSpecialOpen(true)}
        onUndo={() => void mutate('Undid worksheet edit',api.undo)}
        onRedo={() => void mutate('Redid worksheet edit',api.redo)}
        onFillDown={handleFillDown} onFillRight={handleFillRight}
        onClear={handleClear}
        onCommitCell={handleCommitCell}
        onViewportChange={handleViewportChange}
      />

      {isLedgerOpen && <LedgerPanel key={workbookEpoch} target={ledgerTarget} onClose={()=>setIsLedgerOpen(false)}/>}
      {isFindOpen && <FindPanel key={workbookEpoch} sheetName={sheetName} revision={`${workbookEpoch}:${reviewRevision}`} busy={busy} onNavigate={findNavigate} onClose={()=>setIsFindOpen(false)}/> }
      {isExplainOpen && <ExplainReviewPanel
        key={`${workbookEpoch}:${sheetName}:${selectedRow}:${selectedCol}`}
        sheetName={sheetName} row={selectedRow} col={selectedCol}
        revision={`${viewport?.generation ?? 0}:${reviewRevision}`}
        drafts={reviewDrafts.current}
        onClose={() => { setIsExplainOpen(false); clearDependencyTrace(); }}
        onNavigate={followDependency}
        onBack={goBackDependency}
        path={dependencyPath}
        onHighlight={(target, direction = 'upstream') => setDependencyHighlight(target ? {target, direction} : null)}
      />}
      </div>

      <SelectionSummary key={workbookEpoch} sheetName={sheetName} range={selection} revision={`${workbookEpoch}:${reviewRevision}`} busy={busy}/>
      {/* Bottom Sheet Tabs & Status Bar */}
      <div className="flex items-center justify-between border-t border-slate-300 bg-white px-3 py-1 text-xs select-none shadow-inner">
        <SheetTabs sheets={sheets} active={sheetName} busy={busy}
          onSelect={changeSheet} onAdd={addSheet} onRename={renameSheet} onDelete={deleteSheet}/>

        {/* Calculation & System Status */}
        <div className="flex items-center gap-4 text-slate-500 text-[11px]">
          <span role="status" className="truncate max-w-sm" title={statusMsg}>{busy ? 'Working…' : statusMsg}</span>
          <span className="font-mono text-slate-400">Gen #{viewport?.generation ?? 0}</span>
          {stagedCount > 0 && (
            <button
              onClick={() => setIsStagedReviewOpen(true)}
              className="text-amber-700 font-bold underline hover:text-amber-800"
            >
              {stagedCount} staged {stagedCount === 1 ? 'edit' : 'edits'} pending writeback
            </button>
          )}
          <span className="font-mono text-slate-400">{coordToA1(selection.start.row,selection.start.col)}:{coordToA1(selection.end.row,selection.end.col)}</span>
        </div>
      </div>

      {isAboutOpen && <AboutDialog onClose={()=>setIsAboutOpen(false)}/>}
      {closeAddon && isCloseOpen && <Suspense fallback={<p role="status" className="fixed bottom-4 right-4 bg-white border p-2">Loading Monthly close…</p>}><MonthlyClosePanel onClose={()=>setIsCloseOpen(false)} onChanged={()=>{clipboard.current=null;void refreshMetadata();void fetchViewportData();}} onSelectCell={(sheet,row,col)=>moveToExplanation(row,col,sheet,()=>{clearDependencyTrace();setIsCloseOpen(false);})}/></Suspense>}
      {isQuestionsOpen && <OpenQuestions onClose={() => setIsQuestionsOpen(false)} onNavigate={question => {
        moveToExplanation(question.coord.row, question.coord.col, question.sheet_name, () => {
          clearDependencyTrace(); setIsQuestionsOpen(false);
        });
      }} />}
      {/* Modals */}
      <SchemaBrowserModal
        isOpen={isSchemaBrowserOpen}
        onClose={() => setIsSchemaBrowserOpen(false)}
        onOpenQueryWithTable={openQueryWithTable}
      />

      <QueryEditorModal
        isOpen={isQueryEditorOpen}
        onClose={() => setIsQueryEditorOpen(false)}
        initialConnection={queryInitialConn}
        initialSql={queryInitialSql}
        sheets={sheets}
        selection={{sheetName, row: selectedRow, col: selectedCol}}
        onInsertTableFromSource={async placement => {
          const {sourceName, connectionName, sql, sheetName: targetSheet, row, col, mode} = placement;
          if (mode === 'live') {
            await api.insertLiveTable(targetSheet, sourceName, connectionName, sql, row, col);
          } else {
            await api.createSource(sourceName, connectionName, sql);
            try {
              await api.insertTable(targetSheet, sourceName, sourceName, row, col);
            } catch (err) {
              await refreshMetadata();
              throw new Error(`Source "${sourceName}" was saved, but the snapshot was not inserted. Use Insert → Insert Table to choose another location. ${err instanceof Error ? err.message : ''}`);
            }
          }
          await refreshMetadata();
          await fetchViewportData();
        }}
      />

      <InsertTableModal
        isOpen={isInsertTableOpen}
        onClose={() => setIsInsertTableOpen(false)}
        sheets={sheets}
        selection={{sheetName, row: selectedRow, col: selectedCol}}
        onTableInserted={() => {
          refreshMetadata();
          fetchViewportData();
        }}
      />

      <StagedReviewModal
        isOpen={isStagedReviewOpen}
        onClose={() => setIsStagedReviewOpen(false)}
        onAppliedSuccessfully={() => {
          refreshMetadata();
          fetchViewportData();
        }}
      />

      <PivotBuilderModal
        isOpen={isPivotsOpen}
        onClose={() => setIsPivotsOpen(false)}
        tables={tables}
      />

      <ChartModal
        isOpen={isChartsOpen}
        onClose={() => setIsChartsOpen(false)}
        tables={tables}
      />

      <AuditLogModal
        isOpen={isAuditLogOpen}
        onClose={() => setIsAuditLogOpen(false)}
      />

      {sortFilterRange&&<SortFilterPanel sheetName={sheetName} range={sortFilterRange} onClose={()=>setSortFilterRange(null)}
        onGoTo={(row,col)=>{clearDependencyTrace();selectCell(row,col);setSortFilterRange(null);}}/>}

      <BenchmarkModal
        isOpen={isBenchmarksOpen}
        onClose={() => setIsBenchmarksOpen(false)}
      />
      </div>
    </div>
  );
};
