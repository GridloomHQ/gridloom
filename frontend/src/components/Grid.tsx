import React, { useState, useRef, useEffect, useMemo, useLayoutEffect } from 'react';
import { ViewportResponse, DependencyHighlight } from '../types';
import { editableCellInput } from '../utils/cellInput';
import { colIndexToName, coordToA1 } from '../utils/coords';
import { containsCell, GridRange } from '../utils/selection';
import { gridNavigation, NavigationKey } from '../utils/gridNavigation';
import { columnGeometry, clampColumnWidth } from '../utils/columnGeometry';
import { paneIndices, paneOffset, paneLayer, revealAxis } from '../utils/frozenPanes';
import { isCellLoaded } from '../utils/frozenViewport';
import {cycleReference} from '../utils/referenceCycle';
import {cellPresentation} from '../utils/cellPresentation';

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
  onDraftChange:(active:boolean)=>void;
  columnWidths: Record<number,number>;
  showFormulas:boolean;
  freezeTopRow:boolean; freezeFirstColumn:boolean;
  onColumnResize?: (column:number,width:number) => void;
}
const H = 26, LEFT = 48;
export const Grid: React.FC<GridProps> = (props) => {
  const { sheetName, viewport, selectedRow, selectedCol, selection, busy, onSelectCell, onCommitCell, onViewportChange } = props;
  const container = useRef<HTMLDivElement>(null);
  const [scroll, setScroll] = useState({top:0,left:0});
  const [size, setSize] = useState({width:1000,height:600});
  const [edit, setEdit] = useState<string | null>(null);
  useEffect(()=>{props.onDraftChange(edit!==null);},[edit!==null,props.onDraftChange]);
  useEffect(()=>()=>props.onDraftChange(false),[props.onDraftChange]);
  const editor = useRef<HTMLInputElement>(null);
  const referenceSelection=useRef<{start:number;end:number}|null>(null);
  useLayoutEffect(()=>{const selection=referenceSelection.current;referenceSelection.current=null;if(selection&&editor.current)editor.current.setSelectionRange(selection.start,selection.end);},[edit]);
  // Draft text is owned by the cell where editing began, never by selection.
  const draftTarget = useRef<{sheetName:string;selectedRow:number;selectedCol:number}|null>(null);
  const draftMatchesSelection = draftTarget.current?.sheetName===sheetName && draftTarget.current?.selectedRow===selectedRow && draftTarget.current?.selectedCol===selectedCol;
  const editing = useRef(false);
  const dragging = useRef(false);
  const committing = useRef(false);
  const failedInput = useRef<string | null>(null);
  const resize = useRef<{column:number;startX:number;original:number;width:number;sheet:string}|null>(null);
  const [resizePreview,setResizePreview] = useState<{column:number;width:number}|null>(null);
  const latestResizeProps = useRef(props);latestResizeProps.current=props;
  const editGeneration = useRef(0);
  const currentTarget = useRef({sheetName, selectedRow, selectedCol});
  currentTarget.current = {sheetName, selectedRow, selectedCol};
  const highlight = props.traceHighlight?.target.sheet_name === sheetName ? props.traceHighlight : null;
  const traceRange = highlight?.target.range ?? (highlight?.target.coord ? {start:highlight.target.coord, end:highlight.target.coord} : null);
  const totalRows = viewport?.total_rows ?? 1000, totalCols = viewport?.total_cols ?? 100;
  const geometry = useMemo(()=>columnGeometry(totalCols,props.columnWidths),[totalCols,props.columnWidths]);
  const frozenHeight=props.freezeTopRow?H:0, frozenWidth=props.freezeFirstColumn?geometry.width(0):0;
  const cellLeft=(c:number)=>paneOffset(c,geometry.offset(c),scroll.left,props.freezeFirstColumn)+LEFT;
  const cellTop=(r:number)=>paneOffset(r,r*H,scroll.top,props.freezeTopRow)+H;
  const selectedX=geometry.offset(selectedCol), selectedWidth=geometry.width(selectedCol);
  const traceFirst=highlight?.target.range?.start ?? highlight?.target.coord;
  const traceX=traceFirst?geometry.offset(traceFirst.col):null, traceWidth=traceFirst?geometry.width(traceFirst.col):null;
  const startRow = Math.max(0, Math.floor(scroll.top / H) - 3), endRow = Math.min(totalRows - 1, startRow + Math.ceil(size.height / H) + 6);
  const startCol = Math.max(0, geometry.atPixel(scroll.left) - 2), endCol = Math.min(totalCols - 1, geometry.atPixel(scroll.left + Math.max(0,size.width-LEFT)) + 2);
  useEffect(() => { const el=container.current; if (!el) return; const ro=new ResizeObserver(([entry])=>setSize({width:entry.contentRect.width,height:entry.contentRect.height}));ro.observe(el);return ()=>ro.disconnect(); }, []);
  useEffect(() => { onViewportChange(startRow,startCol,endRow,endCol); }, [startRow,startCol,endRow,endCol,onViewportChange]);
  useEffect(() => { const stop=()=>{dragging.current=false;}; window.addEventListener('mouseup',stop); return ()=>window.removeEventListener('mouseup',stop); }, []);
  useEffect(() => {
    const cancel=()=>{resize.current=null;setResizePreview(null);};
    const move=(e:MouseEvent)=>{
      const drag=resize.current;if(!drag)return;
      if(latestResizeProps.current.busy || latestResizeProps.current.sheetName!==drag.sheet){cancel();return;}
      drag.width=clampColumnWidth(drag.original+e.clientX-drag.startX);
      setResizePreview({column:drag.column,width:drag.width});
    };
    const up=()=>{
      const drag=resize.current;cancel();
      const current=latestResizeProps.current;
      if(drag && !current.busy && current.sheetName===drag.sheet && drag.width!==drag.original)
        current.onColumnResize?.(drag.column,drag.width);
    };
    const key=(e:KeyboardEvent)=>{if(e.key==='Escape'&&resize.current){e.preventDefault();e.stopPropagation();cancel();}};
    window.addEventListener('mousemove',move);window.addEventListener('mouseup',up);window.addEventListener('keydown',key,true);window.addEventListener('blur',cancel);
    return()=>{resize.current=null;window.removeEventListener('mousemove',move);window.removeEventListener('mouseup',up);window.removeEventListener('keydown',key,true);window.removeEventListener('blur',cancel);};
  },[]);
  useEffect(()=>{resize.current=null;setResizePreview(null);},[sheetName,busy]);
  useLayoutEffect(() => { editGeneration.current++;editing.current=false;draftTarget.current=null;failedInput.current=null;referenceSelection.current=null;setEdit(null); },[sheetName,selectedRow,selectedCol]);
  useEffect(() => { if (edit !== null) editor.current?.focus(); }, [edit !== null]);
  useEffect(() => {
    const el=container.current;if(!el)return;
    const x=selectedX,w=selectedWidth,y=selectedRow*H;
    el.scrollLeft=revealAxis(x,w,el.scrollLeft,el.clientWidth-LEFT,frozenWidth,props.freezeFirstColumn&&selectedCol===0);
    el.scrollTop=revealAxis(y,H,el.scrollTop,el.clientHeight-H,frozenHeight,props.freezeTopRow&&selectedRow===0);
  },[selectedRow,selectedCol,sheetName,selectedX,selectedWidth,frozenWidth,frozenHeight]);
  useEffect(() => {
    const el=container.current, target=highlight?.target;
    const first=target?.range?.start ?? target?.coord;
    if (!el || !first) return;
    // Reveal the first cell of an offscreen trace without moving the selection.
    const x=geometry.offset(first.col),w=geometry.width(first.col), y=first.row*H;
    el.scrollLeft=revealAxis(x,w,el.scrollLeft,el.clientWidth-LEFT,frozenWidth,props.freezeFirstColumn&&first.col===0);
    el.scrollTop=revealAxis(y,H,el.scrollTop,el.clientHeight-H,frozenHeight,props.freezeTopRow&&first.row===0);
  },[highlight,traceX,traceWidth,frozenWidth,frozenHeight]);
  const navigation = (event: NavigationKey) => gridNavigation(event,
    {row:selectedRow,col:selectedCol}, totalRows, totalCols, Math.floor((size.height-H-frozenHeight)/H));
  const begin = (initial?: string) => {
    if(busy || committing.current || resize.current)return;
    // Missing viewport data is not an empty cell: never initialize an edit from
    // stale/offscreen content while an address or sheet jump is still loading.
    if (!viewport || viewport.sheet_name !== sheetName || !isCellLoaded(viewport,selectedRow,selectedCol)) return;
    const cell=viewport?.cells[coordToA1(selectedRow,selectedCol)];
    editGeneration.current++;draftTarget.current={sheetName,selectedRow,selectedCol};failedInput.current=null;editing.current=true;setEdit(initial ?? editableCellInput(cell));
  };
  const finish = async (commit: boolean, move?: ReturnType<typeof navigation>) => {
    if(!editing.current || committing.current)return;
    const target=draftTarget.current,current=currentTarget.current;
    // An old blur/event callback must not submit a draft after programmatic
    // navigation, even before the selection-reset effect has run.
    if (!target || target.sheetName!==current.sheetName || target.selectedRow!==current.selectedRow || target.selectedCol!==current.selectedCol || target.sheetName!==sheetName || target.selectedRow!==selectedRow || target.selectedCol!==selectedCol) return;
    if (!commit) {
      editGeneration.current++;editing.current=false;draftTarget.current=null;setEdit(null);container.current?.focus();return;
    }
    if (edit === null) return;
    // A failed draft needs an explicit retry or a change, not another network
    // write every time the user focuses a navigation or error-recovery control.
    if (!move && failedInput.current === edit) return;
    const generation = editGeneration.current;
    committing.current=true;
    let success = false;
    try { success = await onCommitCell(target.selectedRow,target.selectedCol,edit); }
    catch { /* Parent reports mutation failures. Keep the draft available for retry. */ }
    finally { committing.current=false; }
    const after = currentTarget.current;
    if (generation !== editGeneration.current || after.sheetName !== target.sheetName) return;
    if (after.selectedRow !== target.selectedRow || after.selectedCol !== target.selectedCol) {
      editing.current=false;draftTarget.current=null;setEdit(null);return;
    }
    // A blur may have opened a dialog or moved focus to another control. Do not
    // steal it when the network response arrives. Keyboard commits retain focus.
    const hasEditorFocus = document.activeElement === editor.current;
    if (!success) { failedInput.current=edit; if (move || hasEditorFocus) editor.current?.focus();return; }
    editing.current=false;draftTarget.current=null;setEdit(null);
    if (move) onSelectCell(move.row,move.col,move.extend);
    if (move || hasEditorFocus) container.current?.focus();
  };
  const onKeyDown = (e: React.KeyboardEvent) => {
    // Composition Enter confirms the IME candidate, not the spreadsheet edit.
    if (e.nativeEvent.isComposing) return;
    if (editing.current) {
      if(e.key==='F4'&&!e.altKey&&!e.ctrlKey&&!e.metaKey&&!e.shiftKey){
        e.preventDefault();if(busy||committing.current||edit===null||!editor.current)return;
        const result=cycleReference(edit,editor.current.selectionStart??0,editor.current.selectionEnd??0);
        if(result){failedInput.current=null;referenceSelection.current={start:result.selectionStart,end:result.selectionEnd};setEdit(result.value);}return;
      }
      if(e.key==='Escape'){e.preventDefault();void finish(false);}
      else if(e.key==='Enter'||e.key==='Tab'){
        e.preventDefault();
        if (!e.ctrlKey && !e.metaKey && !e.altKey) void finish(true,navigation(e));
      }
      return;
    }
    if(busy || committing.current || resize.current)return;
    const move = navigation(e);
    if (move) { e.preventDefault();onSelectCell(move.row,move.col,move.extend);return; }
    if(e.ctrlKey||e.metaKey){if(e.key.toLowerCase()==='z'){e.preventDefault();e.shiftKey?props.onRedo():props.onUndo();}else if(e.key.toLowerCase()==='y'){e.preventDefault();props.onRedo();}else if(e.key.toLowerCase()==='d'){e.preventDefault();props.onFillDown();}return;}
    if(e.key==='F2'){e.preventDefault();begin();}
    else if(e.key==='Delete'||e.key==='Backspace'){e.preventDefault();props.onClear();}
    else if(e.key.length===1&&!e.altKey){e.preventDefault();begin(e.key);}
  };
  const rows=paneIndices(startRow,endRow,props.freezeTopRow),cols=paneIndices(startCol,endCol,props.freezeFirstColumn);

  return <div ref={container} role="grid" data-show-formulas={props.showFormulas} aria-label={`${sheetName} worksheet`} aria-busy={busy} tabIndex={0}
    onScroll={e=>setScroll({top:e.currentTarget.scrollTop,left:e.currentTarget.scrollLeft})} onKeyDown={onKeyDown}
    onCopy={e=>{if(!editing.current)props.onCopy(e);}} onPaste={e=>{if(!editing.current&&!busy)props.onPaste(e);}}
    className="relative flex-1 overflow-auto outline-none bg-white select-none">
    <div style={{width:geometry.totalWidth+LEFT,height:totalRows*H+H,position:'relative'}}>
      {rows.map(r=>cols.map(c=>{
        const key=coordToA1(r,c),cell=viewport?.cells[key],active=r===selectedRow&&c===selectedCol,selected=containsCell(selection,r,c);
        const style=cell?.style;
        const presentation=cellPresentation(cell,props.showFormulas);
        const traced = traceRange && containsCell(traceRange, r, c) ? highlight?.direction : undefined;
        return <div key={key} role="gridcell" aria-label={key} aria-selected={selected} data-dependency-highlight={traced}
          onMouseDown={e=>{if(e.button!==0||busy||committing.current)return;e.preventDefault();if(editing.current){void finish(true,{row:r,col:c,extend:e.shiftKey});return;}dragging.current=true;onSelectCell(r,c,e.shiftKey);container.current?.focus();}}
          onMouseEnter={()=>{if(dragging.current)onSelectCell(r,c,true);}}
          onDoubleClick={()=>{if(active)begin();}}
          style={{position:'absolute',left:cellLeft(c),top:cellTop(r),zIndex:paneLayer(r,c,props.freezeTopRow,props.freezeFirstColumn),width:geometry.width(c),height:H,backgroundColor:traced==='upstream'?'#ccfbf1':traced==='downstream'?'#ede9fe':cell?.is_staged?'#fffbeb':selected?'#eff6ff':style?.fill??(cell?.is_table_header?'#f1f5f9':'#ffffff'),fontWeight:style?.bold||cell?.is_table_header?'bold':undefined,fontStyle:style?.italic?'italic':undefined}}
          className={`border-r border-b border-slate-200 px-1.5 flex items-center text-xs overflow-hidden cursor-cell ${active?'ring-2 ring-inset ring-blue-500':''} ${cell?.is_error?'text-rose-700':''} ${cell?.is_table_header?'bg-slate-100':''}`}
          title={traced ? (traced === 'upstream' ? 'Upstream dependency' : 'Downstream dependent') + (presentation.title ? ': ' + presentation.title : '') : presentation.title}>{cell?.is_staged&&<span className="text-amber-600 mr-1">●</span>}<span className="truncate">{presentation.text}</span></div>;
      }))}
      {cols.map(c=><div key={`c${c}`} role="columnheader" aria-label={colIndexToName(c)} style={{position:'absolute',top:scroll.top,left:cellLeft(c),width:geometry.width(c),height:H,zIndex:props.freezeFirstColumn&&c===0?35:30}} className="bg-slate-100 border-r border-b border-slate-300 text-center text-xs py-1">{colIndexToName(c)}
        {props.onColumnResize&&<button type="button" aria-label={`Resize column ${colIndexToName(c)}`} title="Drag to resize column (Escape cancels)" disabled={busy}
          onKeyDown={e=>e.stopPropagation()}
          onMouseDown={e=>{e.preventDefault();e.stopPropagation();if(e.button!==0||busy||committing.current||editing.current)return;dragging.current=false;
            const width=geometry.width(c);resize.current={column:c,startX:e.clientX,original:width,width,sheet:sheetName};setResizePreview({column:c,width});}}
          style={{position:'absolute',right:0,top:0,width:8,height:H,cursor:'col-resize'}}/>}
      </div>)}
      {rows.map(r=><div key={`r${r}`} aria-hidden style={{position:'absolute',left:scroll.left,top:cellTop(r),width:LEFT,height:H,zIndex:props.freezeTopRow&&r===0?35:30}} className="bg-slate-100 border-r border-b border-slate-300 text-center text-xs py-1">{r+1}</div>)}
      <div aria-hidden style={{position:'absolute',top:scroll.top,left:scroll.left,width:LEFT,height:H,zIndex:40}} className="bg-slate-200 border-r border-b border-slate-300"/>
      {resizePreview&&<div aria-hidden style={{position:'absolute',pointerEvents:'none',left:cellLeft(resizePreview.column)+resizePreview.width,top:scroll.top,width:2,height:size.height,zIndex:45,background:'#2563eb'}}/>}
      {edit!==null&&draftMatchesSelection&&<input ref={editor} aria-label={`Edit ${coordToA1(selectedRow,selectedCol)}`} value={edit} title="F4 cycles a formula reference: relative, absolute, row-locked, column-locked" onChange={e=>{referenceSelection.current=null;failedInput.current=null;setEdit(e.target.value);}} onBlur={()=>{void finish(true);}} readOnly={busy} style={{position:'absolute',left:cellLeft(selectedCol),top:cellTop(selectedRow),width:Math.max(geometry.width(selectedCol),160),height:H,zIndex:paneLayer(selectedRow,selectedCol,props.freezeTopRow,props.freezeFirstColumn)+5}} className="px-1 text-xs border-2 border-blue-600 bg-white outline-none"/>}
    </div>
  </div>;
};
