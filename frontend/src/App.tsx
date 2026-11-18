import React, { useState, useEffect, useCallback, useRef } from 'react';
import { MonthlyClosePanel } from './components/MonthlyClosePanel';
import { normalizedRange, encodeTsv, decodeTsv, GridRange } from './utils/selection';
import { displayValue } from './utils/values';
import { ExplainReviewPanel } from './components/ExplainReviewPanel';
import { OpenQuestions } from './components/OpenQuestions';
import { Toolbar } from './components/Toolbar';
import { FormulaBar } from './components/FormulaBar';
import { Grid } from './components/Grid';
import { SchemaBrowserModal } from './components/SchemaBrowserModal';
import { QueryEditorModal } from './components/QueryEditorModal';
import { InsertTableModal } from './components/InsertTableModal';
import { StagedReviewModal } from './components/StagedReviewModal';
import { PivotBuilderModal } from './components/PivotBuilderModal';
import { ChartModal } from './components/ChartModal';
import { AuditLogModal } from './components/AuditLogModal';
import { BenchmarkModal } from './components/BenchmarkModal';
import { api } from './api';
import { ViewportResponse, WorksheetTable, ReviewDraft, DependencyHighlight } from './types';
import { parseEdit } from './utils/values';
import { coordToA1 } from './utils/coords';

export const App: React.FC = () => {
  const [isCloseOpen, setIsCloseOpen] = useState(false);
  const [sheets, setSheets] = useState<string[]>(['Sheet1']);
  const [anchor, setAnchor] = useState({row: 0, col: 0});
  const [busy, setBusy] = useState(false);
  const mutationLock = useRef(false);
  const viewportSequence = useRef(0);
  const metadataSequence = useRef(0);
  const clipboard = useRef<{text:string;values:string[][];snapshot:string;sheet:string;range:GridRange} | null>(null);
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
  const activeSheetRef = useRef(sheetName);
  activeSheetRef.current = sheetName;
  const [selectedRow, setSelectedRow] = useState(0);
  const [selectedCol, setSelectedCol] = useState(0);
  const [storedViewport, setViewport] = useState<ViewportResponse | null>(null);
  const viewport = storedViewport?.sheet_name === sheetName ? storedViewport : null;
  const [stagedCount, setStagedCount] = useState(0);
  const [tables, setTables] = useState<WorksheetTable[]>([]);

  // Visible bounds
  const [visibleBounds, setVisibleBounds] = useState({
    startRow: 0,
    startCol: 0,
    endRow: 40,
    endCol: 20,
  });

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
        setSheetName(wb.active_sheet);
        setSheets(wb.sheets.map(s => s.name));
        setTables(wb.sheets.find(s => s.name === wb.active_sheet)?.tables ?? []);
      }
      setStagedCount(staged.length);
      setReviewRevision(n => n + 1);
    } catch {
      // ignore
    }
  }, []);

  // Fetch viewport cells
  const fetchViewportData = useCallback(async (startR: number, startC: number, endR: number, endC: number) => {
    try {
      const request = ++viewportSequence.current;
      const data = await api.getViewport(sheetName, startR, startC, endR, endC, 5);
      if (request === viewportSequence.current && data.sheet_name === activeSheetRef.current) setViewport(data);
    } catch (err: any) {
      console.error('Failed to fetch viewport:', err);
    }
  }, [sheetName]);

  useEffect(() => { void refreshMetadata(); }, [refreshMetadata]);
  useEffect(() => {
    void fetchViewportData(visibleBounds.startRow, visibleBounds.startCol, visibleBounds.endRow, visibleBounds.endCol);
  }, [fetchViewportData, visibleBounds]);

  const handleViewportChange = useCallback((startRow: number, startCol: number, endRow: number, endCol: number) => {
    setVisibleBounds(prev => prev.startRow === startRow && prev.startCol === startCol && prev.endRow === endRow && prev.endCol === endCol
      ? prev : { startRow, startCol, endRow, endCol });
  }, []);

  const handleCommitCell = async (row: number, col: number, rawInput: string) => {
    if(mutationLock.current)return;mutationLock.current=true;setBusy(true);clipboard.current=null;
    try {
      setStatusMsg(`Setting ${coordToA1(row, col)}...`);
      const key = coordToA1(row, col);
      const existing = viewport?.cells[key];

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
              const proposedVal = parseEdit(rawInput, origVal);

              // Stage edit!
              await api.stageEdit({
                connection_name: src.connection_name,
                table_name: src.lineage.base_table || tbl.source_name,
                pk_columns: pkCols,
                pk_values: pkValues,
                column_name: targetColName,
                original_value: origVal,
                proposed_value: proposedVal,
              });

              setStatusMsg(`Staged edit for ${tbl.name}[${targetColName}]`);
              await refreshMetadata();
              fetchViewportData(visibleBounds.startRow, visibleBounds.startCol, visibleBounds.endRow, visibleBounds.endCol);
              return;
            }
          }
        }
        throw new Error('This table column is read-only');
      }
      if (existing?.is_table_header) throw new Error('Table headers are read-only');

      // Normal spreadsheet cell edit
      await api.setCell(sheetName, row, col, rawInput);
      setStatusMsg(`Updated ${coordToA1(row, col)}`);
      await refreshMetadata();
      fetchViewportData(visibleBounds.startRow, visibleBounds.startCol, visibleBounds.endRow, visibleBounds.endCol);
    } catch (err: any) {
      setStatusMsg(`Error: ${err.message}`);
    } finally {mutationLock.current=false;setBusy(false);}
  };

  const selection = normalizedRange(anchor, {row:selectedRow, col:selectedCol});
  const rangeKey = `${sheetName}:${anchor.row}:${anchor.col}:${selectedRow}:${selectedCol}:${reviewRevision}`;
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
    setSelectedRow(r);setSelectedCol(c);if(!extend)setAnchor({row:r,col:c});
  };
  const mutate = async (label:string, operation:()=>Promise<unknown>, preserveClipboard=false) => {
    if(mutationLock.current)return;
    mutationLock.current=true;setBusy(true);
    try {
      await operation(); if(!preserveClipboard)clipboard.current=null;
      await refreshMetadata();
      await fetchViewportData(visibleBounds.startRow,visibleBounds.startCol,visibleBounds.endRow,visibleBounds.endCol);
      setStatusMsg(label);
    } catch(error:any){setStatusMsg(`Error: ${error.message}`);}
    finally{mutationLock.current=false;setBusy(false);}
  };
  const handleApplyFormat = (format:string) => {
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
  const handleFillDown = () => void mutate('Filled selection down',()=>api.fillRange(sheetName,{start:selection.start,end:{row:selection.start.row,col:selection.end.col}},selection));
  const handleFillRight = () => void mutate('Filled selection right',()=>api.fillRange(sheetName,{start:selection.start,end:{row:selection.end.row,col:selection.start.col}},selection));
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
  const renameSheet = () => {const name=prompt('Rename worksheet:',sheetName);if(name&&name!==sheetName)void mutate(`Renamed to ${name}`,()=>api.renameSheet(sheetName,name));};

  const handleNewWorkbook = async () => {
    if (mutationLock.current || !confirm('Create a new workbook? Unsaved changes will be discarded.')) return;
    await mutate('Created new workbook', async () => {
      await api.newWorkbook();
      viewportSequence.current++; setViewport(null); selectCell(0, 0);
      setWorkbookEpoch(e => e + 1); reviewDrafts.current.clear(); clearDependencyTrace();
    });
  };

  const handleSaveWorkbook = async () => {
    const path = prompt('Enter save path:', 'my_workbook.gridloom');
    if (path) {
      try {
        await api.saveWorkbook(path);
        setStatusMsg(`Saved workbook to ${path}`);
      } catch (err: any) {
        alert(`Failed to save: ${err.message}`);
      }
    }
  };

  const handleOpenWorkbook = async () => {
    if (mutationLock.current) return;
    const path = prompt('Enter path of .gridloom file to open. Unsaved changes will be discarded:', 'my_workbook.gridloom');
    if (!path) return;
    await mutate(`Opened workbook from ${path}`, async () => {
      await api.openWorkbook(path);
      viewportSequence.current++; setViewport(null); selectCell(0, 0);
      setWorkbookEpoch(e => e + 1); reviewDrafts.current.clear(); clearDependencyTrace();
    });
  };

  const handleRefreshAll = async () => {
    await mutate('All sources refreshed successfully', async () => {
      const sources = await api.getSources();
      for (const source of sources) await api.refreshSource(source.name);
    });
  };

  const openQueryWithTable = (connName: string, tableName: string) => {
    setQueryInitialConn(connName);
    setQueryInitialSql(`SELECT * FROM ${tableName} LIMIT 50`);
    setIsQueryEditorOpen(true);
  };

  const activeKey = coordToA1(selectedRow, selectedCol);
  const activeCellData = viewport?.cells[activeKey];

  return (
    <div className="flex flex-col h-screen w-screen bg-slate-100 overflow-hidden font-sans">
      {/* Top Application Toolbar */}
      <Toolbar
        busy={busy}
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
        onNewWorkbook={handleNewWorkbook}
        onSaveWorkbook={handleSaveWorkbook}
        onOpenWorkbook={handleOpenWorkbook}
        onRefreshAll={handleRefreshAll}
        onApplyFormat={handleApplyFormat}
        stagedCount={stagedCount}
      />

      {/* Formula Bar */}
      <FormulaBar key={`${workbookEpoch}:${sheetName}`}
        disabled={busy}
        selectedRow={selectedRow}
        selectedCol={selectedCol}
        activeCellData={activeCellData}
        onCommit={(val) => handleCommitCell(selectedRow, selectedCol, val)}
      />

      {/* Virtualized Worksheet Grid Canvas */}
      <div className="flex flex-1 min-h-0 overflow-hidden">
      <Grid
        sheetName={sheetName}
        viewport={viewport}
        selectedRow={selectedRow}
        selectedCol={selectedCol}
        selection={selection} busy={busy}
        traceHighlight={dependencyHighlight}
        onSelectCell={(row, col, extend) => { clearDependencyTrace(); selectCell(row, col, extend); }}
        onCopy={handleCopy} onPaste={handlePaste}
        onUndo={() => void mutate('Undid worksheet edit',api.undo)}
        onRedo={() => void mutate('Redid worksheet edit',api.redo)}
        onFillDown={handleFillDown}
        onClear={() => void mutate('Cleared selection',()=>api.editRange(sheetName,selection.start.row,selection.start.col,Array.from({length:selection.end.row-selection.start.row+1},()=>Array(selection.end.col-selection.start.col+1).fill(''))))}
        onCommitCell={handleCommitCell}
        onViewportChange={handleViewportChange}
      />

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

      {/* Bottom Sheet Tabs & Status Bar */}
      <div className="flex items-center justify-between border-t border-slate-300 bg-white px-3 py-1 text-xs select-none shadow-inner">
        {/* Sheet Tabs */}
        <div className="flex items-center gap-1">
          {sheets.map(name=><button key={name} disabled={busy} onClick={()=>changeSheet(name)} onDoubleClick={()=>{if(name===sheetName)renameSheet();}} aria-current={name===sheetName?'page':undefined} className={`px-3 py-1 rounded-t ${name===sheetName?'bg-emerald-50 text-emerald-800 font-bold border-b-2 border-emerald-600':'hover:bg-slate-100'}`}>{name}</button>)}
          <button disabled={busy} onClick={addSheet} aria-label="Add worksheet" className="px-2 py-1 font-bold">+</button>
          <button disabled={busy} onClick={renameSheet} className="px-2 py-1 text-slate-500">Rename</button>
        </div>

        {/* Calculation & System Status */}
        <div className="flex items-center gap-4 text-slate-500 text-[11px]">
          <span role="status" className="truncate max-w-sm">{busy ? 'Working…' : statusMsg}</span>
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

      {isCloseOpen && <MonthlyClosePanel onClose={()=>setIsCloseOpen(false)} onChanged={()=>{clipboard.current=null;void refreshMetadata();void fetchViewportData(visibleBounds.startRow,visibleBounds.startCol,visibleBounds.endRow,visibleBounds.endCol);}} onSelectCell={(sheet,row,col)=>moveToExplanation(row,col,sheet,()=>{clearDependencyTrace();setIsCloseOpen(false);})}/>}
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
        onInsertTableFromSource={(srcName) => {
          api.insertTable(sheetName, srcName, srcName, selectedRow, selectedCol)
            .then(() => {
              refreshMetadata();
              fetchViewportData(visibleBounds.startRow, visibleBounds.startCol, visibleBounds.endRow, visibleBounds.endCol);
            })
            .catch((err) => alert(err.message));
        }}
      />

      <InsertTableModal
        isOpen={isInsertTableOpen}
        onClose={() => setIsInsertTableOpen(false)}
        activeRow={selectedRow}
        activeCol={selectedCol}
        onTableInserted={() => {
          refreshMetadata();
          fetchViewportData(visibleBounds.startRow, visibleBounds.startCol, visibleBounds.endRow, visibleBounds.endCol);
        }}
      />

      <StagedReviewModal
        isOpen={isStagedReviewOpen}
        onClose={() => setIsStagedReviewOpen(false)}
        onAppliedSuccessfully={() => {
          refreshMetadata();
          fetchViewportData(visibleBounds.startRow, visibleBounds.startCol, visibleBounds.endRow, visibleBounds.endCol);
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

      <BenchmarkModal
        isOpen={isBenchmarksOpen}
        onClose={() => setIsBenchmarksOpen(false)}
      />
    </div>
  );
};
