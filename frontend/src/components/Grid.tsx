import React, { useState, useRef, useEffect } from 'react';
import { ViewportResponse, DependencyHighlight } from '../types';
import { editableCellInput } from '../utils/cellInput';
import { colIndexToName, coordToA1 } from '../utils/coords';
import { containsCell, GridRange } from '../utils/selection';
import { gridNavigation, NavigationKey } from '../utils/gridNavigation';

interface GridProps {
  sheetName: string; viewport: ViewportResponse | null; selectedRow: number; selectedCol: number;
  selection: GridRange; busy: boolean; traceHighlight?: DependencyHighlight | null;
  onSelectCell: (row: number, col: number, extend?: boolean) => void;
  onCommitCell: (row: number, col: number, value: string) => Promise<boolean>;
  onViewportChange: (startRow: number, startCol: number, endRow: number, endCol: number) => void;
  onCopy: (event: React.ClipboardEvent) => void;
  onPaste: (event: React.ClipboardEvent) => void;
  onUndo: () => void; onRedo: () => void; onFillDown: () => void;
  onClear: () => void;
}
const W = 110, H = 26, LEFT = 48;
export const Grid: React.FC<GridProps> = (props) => {
  const { sheetName, viewport, selectedRow, selectedCol, selection, busy, onSelectCell, onCommitCell, onViewportChange } = props;
  const container = useRef<HTMLDivElement>(null);
  const [scroll, setScroll] = useState({top:0,left:0});
  const [size, setSize] = useState({width:1000,height:600});
  const [edit, setEdit] = useState<string | null>(null);
  const editor = useRef<HTMLInputElement>(null);
  const editing = useRef(false);
  const dragging = useRef(false);
  const committing = useRef(false);
  const editGeneration = useRef(0);
  const currentTarget = useRef({sheetName, selectedRow, selectedCol});
  currentTarget.current = {sheetName, selectedRow, selectedCol};
  const highlight = props.traceHighlight?.target.sheet_name === sheetName ? props.traceHighlight : null;
  const traceRange = highlight?.target.range ?? (highlight?.target.coord ? {start:highlight.target.coord, end:highlight.target.coord} : null);
  const totalRows = viewport?.total_rows ?? 1000, totalCols = viewport?.total_cols ?? 100;
  const startRow = Math.max(0, Math.floor(scroll.top / H) - 3), endRow = Math.min(totalRows - 1, startRow + Math.ceil(size.height / H) + 6);
  const startCol = Math.max(0, Math.floor(scroll.left / W) - 2), endCol = Math.min(totalCols - 1, startCol + Math.ceil(size.width / W) + 4);
  useEffect(() => { const el=container.current; if (!el) return; const ro=new ResizeObserver(([entry])=>setSize({width:entry.contentRect.width,height:entry.contentRect.height}));ro.observe(el);return ()=>ro.disconnect(); }, []);
  useEffect(() => { onViewportChange(startRow,startCol,endRow,endCol); }, [startRow,startCol,endRow,endCol,onViewportChange]);
  useEffect(() => { const stop=()=>{dragging.current=false;}; window.addEventListener('mouseup',stop); return ()=>window.removeEventListener('mouseup',stop); }, []);
  useEffect(() => { editGeneration.current++;editing.current=false;setEdit(null); },[sheetName,selectedRow,selectedCol]);
  useEffect(() => { if (edit !== null) editor.current?.focus(); }, [edit !== null]);
  useEffect(() => {
    const el=container.current;if(!el)return;
    const x=selectedCol*W,y=selectedRow*H;
    if(x<el.scrollLeft)el.scrollLeft=x;else if(x+W>el.scrollLeft+el.clientWidth-LEFT)el.scrollLeft=x+W-el.clientWidth+LEFT;
    if(y<el.scrollTop)el.scrollTop=y;else if(y+H>el.scrollTop+el.clientHeight-H)el.scrollTop=y+H-el.clientHeight+H;
  },[selectedRow,selectedCol,sheetName]);
  useEffect(() => {
    const el=container.current, target=highlight?.target;
    const first=target?.range?.start ?? target?.coord;
    if (!el || !first) return;
    // Reveal the first cell of an offscreen trace without moving the selection.
    const x=first.col*W, y=first.row*H;
    if(x<el.scrollLeft)el.scrollLeft=x;else if(x+W>el.scrollLeft+el.clientWidth-LEFT)el.scrollLeft=x+W-el.clientWidth+LEFT;
    if(y<el.scrollTop)el.scrollTop=y;else if(y+H>el.scrollTop+el.clientHeight-H)el.scrollTop=y+H-el.clientHeight+H;
  },[highlight]);
  const navigation = (event: NavigationKey) => gridNavigation(event,
    {row:selectedRow,col:selectedCol}, totalRows, totalCols, Math.floor((size.height-H)/H));
  const begin = (initial?: string) => {
    if(busy || committing.current)return;
    // Missing viewport data is not an empty cell: never initialize an edit from
    // stale/offscreen content while an address or sheet jump is still loading.
    if (!viewport || viewport.sheet_name !== sheetName || selectedRow < viewport.start_row || selectedRow > viewport.end_row || selectedCol < viewport.start_col || selectedCol > viewport.end_col) return;
    const cell=viewport?.cells[coordToA1(selectedRow,selectedCol)];
    editGeneration.current++;editing.current=true;setEdit(initial ?? editableCellInput(cell));
  };
  const finish = async (commit: boolean, move?: ReturnType<typeof navigation>) => {
    if(!editing.current || committing.current)return;
    if (!commit) {
      editGeneration.current++;editing.current=false;setEdit(null);container.current?.focus();return;
    }
    if (edit === null) return;
    const generation = editGeneration.current;
    const target = {sheetName, selectedRow, selectedCol};
    committing.current=true;
    let success = false;
    try { success = await onCommitCell(selectedRow,selectedCol,edit); }
    catch { /* Parent reports mutation failures. Keep the draft available for retry. */ }
    finally { committing.current=false; }
    const current = currentTarget.current;
    if (generation !== editGeneration.current || current.sheetName !== target.sheetName) return;
    if (current.selectedRow !== target.selectedRow || current.selectedCol !== target.selectedCol) {
      editing.current=false;setEdit(null);return;
    }
    // A blur may have opened a dialog or moved focus to another control. Do not
    // steal it when the network response arrives. Keyboard commits retain focus.
    const hasEditorFocus = document.activeElement === editor.current;
    if (!success) { if (move || hasEditorFocus) editor.current?.focus();return; }
    editing.current=false;setEdit(null);
    if (move) onSelectCell(move.row,move.col,move.extend);
    if (move || hasEditorFocus) container.current?.focus();
  };
  const onKeyDown = (e: React.KeyboardEvent) => {
    // Composition Enter confirms the IME candidate, not the spreadsheet edit.
    if (e.nativeEvent.isComposing) return;
    if (editing.current) {
      if(e.key==='Escape'){e.preventDefault();void finish(false);}
      else if(e.key==='Enter'||e.key==='Tab'){
        e.preventDefault();
        if (!e.ctrlKey && !e.metaKey && !e.altKey) void finish(true,navigation(e));
      }
      return;
    }
    if(busy || committing.current)return;
    const move = navigation(e);
    if (move) { e.preventDefault();onSelectCell(move.row,move.col,move.extend);return; }
    if(e.ctrlKey||e.metaKey){if(e.key.toLowerCase()==='z'){e.preventDefault();e.shiftKey?props.onRedo():props.onUndo();}else if(e.key.toLowerCase()==='y'){e.preventDefault();props.onRedo();}else if(e.key.toLowerCase()==='d'){e.preventDefault();props.onFillDown();}return;}
    if(e.key==='F2'){e.preventDefault();begin();}
    else if(e.key==='Delete'||e.key==='Backspace'){e.preventDefault();props.onClear();}
    else if(e.key.length===1&&!e.altKey){e.preventDefault();begin(e.key);}
  };
  const rows=Array.from({length:Math.max(0,endRow-startRow+1)},(_,i)=>i+startRow),cols=Array.from({length:Math.max(0,endCol-startCol+1)},(_,i)=>i+startCol);

  return <div ref={container} role="grid" aria-label={`${sheetName} worksheet`} aria-busy={busy} tabIndex={0}
    onScroll={e=>setScroll({top:e.currentTarget.scrollTop,left:e.currentTarget.scrollLeft})} onKeyDown={onKeyDown}
    onCopy={e=>{if(!editing.current)props.onCopy(e);}} onPaste={e=>{if(!editing.current&&!busy)props.onPaste(e);}}
    className="relative flex-1 overflow-auto outline-none bg-white select-none">
    <div style={{width:totalCols*W+LEFT,height:totalRows*H+H,position:'relative'}}>
      {rows.map(r=>cols.map(c=>{
        const key=coordToA1(r,c),cell=viewport?.cells[key],active=r===selectedRow&&c===selectedCol,selected=containsCell(selection,r,c);
        const style=cell?.style;
        const traced = traceRange && containsCell(traceRange, r, c) ? highlight?.direction : undefined;
        return <div key={key} role="gridcell" aria-label={key} aria-selected={selected} data-dependency-highlight={traced}
          onMouseDown={e=>{if(e.button!==0||busy||committing.current)return;e.preventDefault();if(editing.current){void finish(true,{row:r,col:c,extend:e.shiftKey});return;}dragging.current=true;onSelectCell(r,c,e.shiftKey);container.current?.focus();}}
          onMouseEnter={()=>{if(dragging.current)onSelectCell(r,c,true);}}
          onDoubleClick={()=>{if(active)begin();}}
          style={{position:'absolute',left:c*W+LEFT,top:r*H+H,width:W,height:H,backgroundColor:traced==='upstream'?'#ccfbf1':traced==='downstream'?'#ede9fe':cell?.is_staged?'#fffbeb':selected?'#eff6ff':style?.fill??undefined,fontWeight:style?.bold||cell?.is_table_header?'bold':undefined,fontStyle:style?.italic?'italic':undefined}}
          className={`border-r border-b border-slate-200 px-1.5 flex items-center text-xs overflow-hidden cursor-cell ${active?'ring-2 ring-inset ring-blue-500':''} ${cell?.is_error?'text-rose-700':''} ${cell?.is_table_header?'bg-slate-100':''}`}
          title={traced ? (traced === 'upstream' ? 'Upstream dependency' : 'Downstream dependent') + (cell?.display_text ? ': ' + cell.display_text : '') : cell?.display_text}>{cell?.is_staged&&<span className="text-amber-600 mr-1">●</span>}<span className="truncate">{cell?.display_text??''}</span></div>;
      }))}
      {cols.map(c=><div key={`c${c}`} aria-hidden style={{position:'absolute',top:scroll.top,left:c*W+LEFT,width:W,height:H,zIndex:30}} className="bg-slate-100 border-r border-b border-slate-300 text-center text-xs py-1">{colIndexToName(c)}</div>)}
      {rows.map(r=><div key={`r${r}`} aria-hidden style={{position:'absolute',left:scroll.left,top:r*H+H,width:LEFT,height:H,zIndex:30}} className="bg-slate-100 border-r border-b border-slate-300 text-center text-xs py-1">{r+1}</div>)}
      <div aria-hidden style={{position:'absolute',top:scroll.top,left:scroll.left,width:LEFT,height:H,zIndex:40}} className="bg-slate-200 border-r border-b border-slate-300"/>
      {edit!==null&&<input ref={editor} aria-label={`Edit ${coordToA1(selectedRow,selectedCol)}`} value={edit} onChange={e=>setEdit(e.target.value)} onBlur={()=>{void finish(true);}} readOnly={busy} style={{position:'absolute',left:selectedCol*W+LEFT,top:selectedRow*H+H,width:Math.max(W,160),height:H,zIndex:50}} className="px-1 text-xs border-2 border-blue-600 bg-white outline-none"/>}
    </div>
  </div>;
};
