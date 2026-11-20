import React, { useState, useRef, useEffect, useMemo, useLayoutEffect } from 'react';
import { ViewportResponse, DependencyHighlight } from '../types';
import { editableCellInput } from '../utils/cellInput';
import { colIndexToName, coordToA1 } from '../utils/coords';
import { containsCell, GridRange } from '../utils/selection';
import { gridNavigation, NavigationKey } from '../utils/gridNavigation';
import { gridShortcut } from '../utils/gridShortcuts';
import { columnGeometry, clampColumnWidth } from '../utils/columnGeometry';
import { paneIndices, paneOffset, paneLayer, revealAxis } from '../utils/frozenPanes';
import { isCellLoaded } from '../utils/frozenViewport';
import {cycleReference} from '../utils/referenceCycle';
import {cellPresentation} from '../utils/cellPresentation';
import {applyFunctionCompletion,completionEvent,functionCompletion,type FunctionCompletion} from '../utils/functionCompletion';
import {draftReferences,referenceColors,referenceSlot,referenceText} from '../utils/formulaDraft';


interface GridProps {
  sheetName: string; viewport: ViewportResponse | null; selectedRow: number; selectedCol: number;
  selection: GridRange; busy: boolean; traceHighlight?: DependencyHighlight | null;
  onSelectCell: (row: number, col: number, extend?: boolean) => void;
  onCommitCell: (row: number, col: number, value: string) => Promise<boolean>;
  onViewportChange: (startRow: number, startCol: number, endRow: number, endCol: number, visibleFrozenRows: number, visibleFrozenColumns: number) => void;
  onCopy: (event: React.ClipboardEvent) => void;
  onPaste: (event: React.ClipboardEvent) => void;
  onUndo: () => void; onRedo: () => void; onFillDown: () => void;
  onClear: () => void; onFillRight: () => void; onPasteSpecial: () => void;
  onDraftChange:(active:boolean)=>void;
  externalDraft?:string|null;
  functionNames:string[];
  columnWidths: Record<number,number>;
  showFormulas:boolean;
  frozenRows:number; frozenColumns:number;
  onColumnResize?: (column:number,width:number) => void;
}
const H = 26, LEFT = 48;
const scrollEdge = (size:number) => Math.max(0, size - 0.01);
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
  const pickMode=useRef(false);
  const picked=useRef<{start:number;end:number;anchor:GridRange['start'];cursor:GridRange['start']}|null>(null);
  const [completion,setCompletion]=useState<FunctionCompletion|null>(null);
  const [suggestionIndex,setSuggestionIndex]=useState(0);
  const completionKey=useRef<string|null>(null);
  const suggestions=completion?props.functionNames.filter(name=>name.startsWith(completion.prefix)):[];
  const showSuggestions=suggestions.length>0;
  useEffect(()=>{
    if(showSuggestions)document.getElementById(`inline-function-${suggestionIndex}`)?.scrollIntoView({block:'nearest',container:'nearest'} as ScrollIntoViewOptions);
  },[showSuggestions,suggestionIndex]);
  const updateCompletion=(value:string,start:number|null,end:number|null,inputChanged=false)=>{
    const event=completionEvent(completionKey.current,value,start,end,inputChanged);
    if(!event.update)return;
    completionKey.current=event.key;setSuggestionIndex(0);setCompletion(functionCompletion(value,start??0,end??start??0));
  };
  const acceptSuggestion=(name:string)=>{
    if(!completion||edit===null||busy||committing.current||!name)return;
    const result=applyFunctionCompletion(edit,completion,name);
    referenceSelection.current={start:result.caret,end:result.caret};failedInput.current=null;
    if(result.value===edit){editor.current?.setSelectionRange(result.caret,result.caret);referenceSelection.current=null;}
    setEdit(result.value);setCompletion(null);
  };

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
  const formulaRanges=useMemo(()=>draftReferences(edit??props.externalDraft??'',sheetName),[edit,props.externalDraft,sheetName]);
  const totalRows = viewport?.total_rows ?? 1000, totalCols = viewport?.total_cols ?? 100;
  const geometry = useMemo(()=>columnGeometry(totalCols,props.columnWidths),[totalCols,props.columnWidths]);
  const frozenRows = Math.min(props.frozenRows, totalRows - 1);
  const frozenColumns = Math.min(props.frozenColumns, totalCols - 1);
  const frozenHeight = frozenRows * H, frozenWidth = geometry.offset(frozenColumns);
  // Only materialize the pinned prefix that fits on screen. A deep split
  // must not request or render every row/column preceding the selected cell.
  const bodyHeight = Math.max(0, size.height-H), bodyWidth = Math.max(0, size.width-LEFT);
  const visibleFrozenRows = Math.min(frozenRows, Math.ceil(bodyHeight / H));
  const visibleFrozenColumns = Math.min(frozenColumns, bodyWidth > 0 ? geometry.atPixel(scrollEdge(bodyWidth))+1 : 0);
  const cellLeft=(c:number)=>paneOffset(c,geometry.offset(c),scroll.left,frozenColumns)+LEFT;
  const cellTop=(r:number)=>paneOffset(r,r*H,scroll.top,frozenRows)+H;
  const selectedX=geometry.offset(selectedCol), selectedWidth=geometry.width(selectedCol);
  const traceFirst=highlight?.target.range?.start ?? highlight?.target.coord;
  const traceX=traceFirst?geometry.offset(traceFirst.col):null, traceWidth=traceFirst?geometry.width(traceFirst.col):null;
  const startRow = frozenHeight >= bodyHeight && frozenRows > 0 ? 0 : Math.max(0, Math.floor((scroll.top + frozenHeight) / H) - 3);
  const endRow = frozenHeight >= bodyHeight && frozenRows > 0 ? 0 : Math.min(totalRows - 1, Math.floor((scroll.top + bodyHeight) / H) + 3);
  const startCol = frozenWidth >= bodyWidth && frozenColumns > 0 ? 0 : Math.max(0, geometry.atPixel(scroll.left + frozenWidth) - 2);
  const endCol = frozenWidth >= bodyWidth && frozenColumns > 0 ? 0 : Math.min(totalCols - 1, geometry.atPixel(scroll.left + bodyWidth) + 2);
  useEffect(() => { const el=container.current; if (!el) return; const ro=new ResizeObserver(([entry])=>setSize({width:entry.contentRect.width,height:entry.contentRect.height}));ro.observe(el);return ()=>ro.disconnect(); }, []);
  useEffect(() => { onViewportChange(startRow,startCol,endRow,endCol,visibleFrozenRows,visibleFrozenColumns); }, [startRow,startCol,endRow,endCol,visibleFrozenRows,visibleFrozenColumns,onViewportChange]);
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
  useLayoutEffect(() => { editGeneration.current++;editing.current=false;draftTarget.current=null;failedInput.current=null;referenceSelection.current=null;pickMode.current=false;picked.current=null;completionKey.current=null;setCompletion(null);setEdit(null); },[sheetName,selectedRow,selectedCol]);
  useEffect(() => { if (edit !== null) editor.current?.focus(); }, [edit !== null]);
  useEffect(() => {
    const el=container.current;if(!el)return;
    const x=selectedX,w=selectedWidth,y=selectedRow*H;
    el.scrollLeft=revealAxis(x,w,el.scrollLeft,el.clientWidth-LEFT,frozenWidth,selectedCol<frozenColumns);
    el.scrollTop=revealAxis(y,H,el.scrollTop,el.clientHeight-H,frozenHeight,selectedRow<frozenRows);
  },[selectedRow,selectedCol,sheetName,selectedX,selectedWidth,frozenWidth,frozenHeight]);
  useEffect(() => {
    const el=container.current, target=highlight?.target;
    const first=target?.range?.start ?? target?.coord;
    if (!el || !first) return;
    // Reveal the first cell of an offscreen trace without moving the selection.
    const x=geometry.offset(first.col),w=geometry.width(first.col), y=first.row*H;
    el.scrollLeft=revealAxis(x,w,el.scrollLeft,el.clientWidth-LEFT,frozenWidth,first.col<frozenColumns);
    el.scrollTop=revealAxis(y,H,el.scrollTop,el.clientHeight-H,frozenHeight,first.row<frozenRows);
  },[highlight,traceX,traceWidth,frozenWidth,frozenHeight]);
  const navigation = (event: NavigationKey) => gridNavigation(event,
    {row:selectedRow,col:selectedCol}, totalRows, totalCols, Math.floor((size.height-H-frozenHeight)/H));
  const begin = (initial?: string) => {
    if(busy || committing.current || resize.current)return;
    if (selectedRow < frozenRows && selectedRow >= visibleFrozenRows
      || selectedCol < frozenColumns && selectedCol >= visibleFrozenColumns) return;
    // Missing viewport data is not an empty cell: never initialize an edit from
    // stale/offscreen content while an address or sheet jump is still loading.
    if (!viewport || viewport.sheet_name !== sheetName || !isCellLoaded(viewport,selectedRow,selectedCol)) return;
    const cell=viewport?.cells[coordToA1(selectedRow,selectedCol)];
    pickMode.current=true;picked.current=null;completionKey.current=null;setCompletion(null);
    editGeneration.current++;draftTarget.current={sheetName,selectedRow,selectedCol};failedInput.current=null;editing.current=true;setEdit(initial ?? editableCellInput(cell));
  };
  const finish = async (commit: boolean, move?: ReturnType<typeof navigation>) => {
    if(!editing.current || committing.current)return;
    const target=draftTarget.current,current=currentTarget.current;
    // An old blur/event callback must not submit a draft after programmatic
    // navigation, even before the selection-reset effect has run.
    if (!target || target.sheetName!==current.sheetName || target.selectedRow!==current.selectedRow || target.selectedCol!==current.selectedCol || target.sheetName!==sheetName || target.selectedRow!==selectedRow || target.selectedCol!==selectedCol) return;
    if (!commit) {
      editGeneration.current++;editing.current=false;draftTarget.current=null;picked.current=null;setCompletion(null);setEdit(null);container.current?.focus();return;
    }
    if (edit === null) return;
    // A failed draft needs an explicit retry or a change, not another network
    // write every time the user focuses a navigation or error-recovery control.
    if (!move && failedInput.current === edit) return;
    const generation = editGeneration.current;
    committing.current=true;
    let success = false;
    try {
      // A typed value pasted from a formula may look like a formula, number or
      // boolean in the editor. Opening and closing an unchanged draft isn't an edit.
      const original=editableCellInput(viewport?.cells[coordToA1(target.selectedRow,target.selectedCol)]);
      success = original===edit || await onCommitCell(target.selectedRow,target.selectedCol,edit);
    }
    catch { /* Parent reports mutation failures. Keep the draft available for retry. */ }
    finally { committing.current=false; }
    const after = currentTarget.current;
    if (generation !== editGeneration.current || after.sheetName !== target.sheetName) return;
    if (after.selectedRow !== target.selectedRow || after.selectedCol !== target.selectedCol) {
      editing.current=false;draftTarget.current=null;picked.current=null;setCompletion(null);setEdit(null);return;
    }
    // A blur may have opened a dialog or moved focus to another control. Do not
    // steal it when the network response arrives. Keyboard commits retain focus.
    const hasEditorFocus = document.activeElement === editor.current;
    if (!success) { failedInput.current=edit; if (move || hasEditorFocus) editor.current?.focus();return; }
    editing.current=false;draftTarget.current=null;picked.current=null;setCompletion(null);setEdit(null);
    if (move) onSelectCell(move.row,move.col,move.extend);
    if (move || hasEditorFocus) container.current?.focus();
  };
  const pointReference=(point:GridRange['start'],extend:boolean)=>{
    if(edit===null||!editor.current||busy||committing.current)return false;
    const previous=picked.current;
    const slot=previous??referenceSlot(edit,editor.current.selectionStart??0,editor.current.selectionEnd??0);
    if(!slot)return false;
    const anchor=extend?(previous?.anchor??{row:selectedRow,col:selectedCol}):point;
    const token=referenceText(anchor,point);
    const replacement=edit.slice(0,slot.start)+token+edit.slice(slot.end);
    const end=slot.start+token.length;
    picked.current={...slot,end,anchor,cursor:point};
    referenceSelection.current={start:end,end};
    if(edit===replacement){editor.current.setSelectionRange(end,end);referenceSelection.current=null;}
    setEdit(replacement);setCompletion(null);failedInput.current=null;
    const el=container.current;
    if(el){
      el.scrollLeft=revealAxis(geometry.offset(point.col),geometry.width(point.col),el.scrollLeft,el.clientWidth-LEFT,frozenWidth,point.col<frozenColumns);
      el.scrollTop=revealAxis(point.row*H,H,el.scrollTop,el.clientHeight-H,frozenHeight,point.row<frozenRows);
    }
    return true;
  };
  const onKeyDown = (e: React.KeyboardEvent) => {
    // Composition Enter confirms the IME candidate, not the spreadsheet edit.
    if (e.nativeEvent.isComposing) return;
    if (editing.current) {
      const plain=!e.ctrlKey&&!e.metaKey&&!e.altKey;
      if(plain&&!e.shiftKey&&e.key==='F2'){
        e.preventDefault();pickMode.current=!pickMode.current;picked.current=null;setCompletion(null);return;
      }
      if(plain&&!e.shiftKey&&showSuggestions){
        if(e.key==='ArrowDown'||e.key==='ArrowUp'){
          e.preventDefault();setSuggestionIndex(i=>(i+(e.key==='ArrowDown'?1:-1)+suggestions.length)%suggestions.length);return;
        }
        if(e.key==='Tab'||e.key==='Enter'){e.preventDefault();acceptSuggestion(suggestions[Math.min(suggestionIndex,suggestions.length-1)]);return;}
        if(e.key==='Escape'){e.preventDefault();setCompletion(null);return;}
      }
      if(plain&&pickMode.current&&['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(e.key)){
        const move=gridNavigation(e,picked.current?.cursor??{row:selectedRow,col:selectedCol},totalRows,totalCols,Math.floor((size.height-H-frozenHeight)/H));
        if(move&&pointReference(move,e.shiftKey)){e.preventDefault();return;}
      }
      if(['ArrowLeft','ArrowRight','ArrowUp','ArrowDown','Home','End'].includes(e.key))picked.current=null;
      if(e.key==='F4'&&!e.altKey&&!e.ctrlKey&&!e.metaKey&&!e.shiftKey){
        e.preventDefault();if(busy||committing.current||edit===null||!editor.current)return;
        const result=cycleReference(edit,editor.current.selectionStart??0,editor.current.selectionEnd??0);
        if(result){picked.current=null;failedInput.current=null;referenceSelection.current={start:result.selectionStart,end:result.selectionEnd};setEdit(result.value);}return;
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
    if(e.ctrlKey||e.metaKey){
      const shortcut=gridShortcut(e);
      if(shortcut) {
        e.preventDefault();
        const actions={undo:props.onUndo,redo:props.onRedo,'fill-down':props.onFillDown,'fill-right':props.onFillRight,'paste-special':props.onPasteSpecial};
        actions[shortcut]();
      }
      return;
    }
    if(e.key==='F2'){e.preventDefault();begin();}
    else if(e.key==='Delete'||e.key==='Backspace'){e.preventDefault();props.onClear();}
    else if(e.key.length===1&&!e.altKey){e.preventDefault();begin(e.key);}
  };
  const rows=paneIndices(startRow,endRow,frozenRows,visibleFrozenRows),cols=paneIndices(startCol,endCol,frozenColumns,visibleFrozenColumns);

  return <div ref={container} role="grid" data-show-formulas={props.showFormulas} aria-label={`${sheetName} worksheet`} aria-busy={busy} tabIndex={0}
    onScroll={e=>setScroll({top:e.currentTarget.scrollTop,left:e.currentTarget.scrollLeft})} onKeyDown={onKeyDown}
    onCopy={e=>{if(!editing.current)props.onCopy(e);}} onPaste={e=>{if(!editing.current&&!busy)props.onPaste(e);}}
    className="relative flex-1 overflow-auto outline-none bg-white select-none">
    <div style={{width:geometry.totalWidth+LEFT,height:totalRows*H+H,position:'relative'}}>
      {rows.map(r=>cols.map(c=>{
        const key=coordToA1(r,c),cell=viewport?.cells[key],active=r===selectedRow&&c===selectedCol,selected=containsCell(selection,r,c);
        const style=cell?.style;
        const presentation=cellPresentation(cell,props.showFormulas);
        const reference=formulaRanges.findIndex(range=>containsCell(range,r,c));
        const traced = traceRange && containsCell(traceRange, r, c) ? highlight?.direction : undefined;
        return <div key={key} role="gridcell" aria-label={key} aria-selected={selected} data-dependency-highlight={traced} data-frozen-row={r<frozenRows||undefined} data-frozen-column={c<frozenColumns||undefined} data-formula-reference={reference>=0?reference+1:undefined}
          onMouseDown={e=>{if(e.button!==0||busy||committing.current)return;e.preventDefault();if(editing.current){if(pickMode.current&&pointReference({row:r,col:c},e.shiftKey))return;void finish(true,{row:r,col:c,extend:e.shiftKey});return;}dragging.current=true;onSelectCell(r,c,e.shiftKey);container.current?.focus();}}
          onMouseEnter={()=>{if(dragging.current)onSelectCell(r,c,true);}}
          onDoubleClick={()=>{if(active)begin();}}
          style={{position:'absolute',left:cellLeft(c),top:cellTop(r),zIndex:paneLayer(r,c,frozenRows,frozenColumns),width:geometry.width(c),height:H,backgroundColor:traced==='upstream'?'#ccfbf1':traced==='downstream'?'#ede9fe':selected?'#eff6ff':style?.fill??(cell?.is_staged?'#fffbeb':cell?.is_table_header?'#f1f5f9':'#ffffff'),fontWeight:style?.bold?'bold':undefined,fontStyle:style?.italic?'italic':undefined,borderBottomColor:r===frozenRows-1?'#94a3b8':undefined,borderRightColor:c===frozenColumns-1?'#94a3b8':undefined,boxShadow:reference>=0?`inset 0 0 0 2px ${referenceColors[reference%referenceColors.length]}`:undefined}}
          className={`border-r border-b border-slate-200 px-1.5 flex items-center text-xs overflow-hidden cursor-cell ${active?'ring-2 ring-inset ring-blue-500':''} ${cell?.is_error?'text-rose-700':''} ${cell?.is_table_header?'bg-slate-100':''}`}
          title={traced ? (traced === 'upstream' ? 'Upstream dependency' : 'Downstream dependent') + (presentation.title ? ': ' + presentation.title : '') : presentation.title}>{cell?.is_staged&&<span className="text-amber-600 mr-1">●</span>}<span className="truncate">{presentation.text}</span></div>;
      }))}
      {cols.map(c=><div key={`c${c}`} role="columnheader" aria-label={colIndexToName(c)} style={{position:'absolute',top:scroll.top,left:cellLeft(c),width:geometry.width(c),height:H,zIndex:c<frozenColumns?35:30}} className="bg-slate-100 border-r border-b border-slate-300 text-center text-xs py-1">{colIndexToName(c)}
        {props.onColumnResize&&<button type="button" aria-label={`Resize column ${colIndexToName(c)}`} title="Drag to resize column (Escape cancels)" disabled={busy}
          onKeyDown={e=>e.stopPropagation()}
          onMouseDown={e=>{e.preventDefault();e.stopPropagation();if(e.button!==0||busy||committing.current||editing.current)return;dragging.current=false;
            const width=geometry.width(c);resize.current={column:c,startX:e.clientX,original:width,width,sheet:sheetName};setResizePreview({column:c,width});}}
          style={{position:'absolute',right:0,top:0,width:8,height:H,cursor:'col-resize'}}/>}
      </div>)}
      {rows.map(r=><div key={`r${r}`} aria-hidden style={{position:'absolute',left:scroll.left,top:cellTop(r),width:LEFT,height:H,zIndex:r<frozenRows?35:30}} className="bg-slate-100 border-r border-b border-slate-300 text-center text-xs py-1">{r+1}</div>)}
      <div aria-hidden style={{position:'absolute',top:scroll.top,left:scroll.left,width:LEFT,height:H,zIndex:40}} className="bg-slate-200 border-r border-b border-slate-300"/>
      {resizePreview&&<div aria-hidden style={{position:'absolute',pointerEvents:'none',left:cellLeft(resizePreview.column)+resizePreview.width,top:scroll.top,width:2,height:size.height,zIndex:45,background:'#2563eb'}}/>}
      {edit!==null&&draftMatchesSelection&&<div className="absolute" style={{left:selectedCol<frozenColumns?cellLeft(selectedCol):Math.min(Math.max(cellLeft(selectedCol),scroll.left+LEFT+frozenWidth),scroll.left+Math.max(LEFT+frozenWidth,size.width-170)),
        top:selectedRow<frozenRows?cellTop(selectedRow):Math.max(scroll.top+H+frozenHeight,Math.min(cellTop(selectedRow),scroll.top+size.height-H)),
        width:Math.max(geometry.width(selectedCol),160),height:H,zIndex:45}}>
        <input ref={editor} aria-label={`Edit ${coordToA1(selectedRow,selectedCol)}`} value={edit}
          aria-autocomplete="list" aria-controls={showSuggestions?'inline-function-suggestions':undefined}
          aria-activedescendant={showSuggestions?`inline-function-${suggestionIndex}`:undefined}
          title="Use arrows to point to a cell (Shift extends the range); F2 toggles caret movement. F4 cycles reference locks."
          onChange={e=>{referenceSelection.current=null;picked.current=null;failedInput.current=null;setEdit(e.target.value);updateCompletion(e.target.value,e.target.selectionStart,e.target.selectionEnd,true);}}
          onSelect={e=>updateCompletion(e.currentTarget.value,e.currentTarget.selectionStart,e.currentTarget.selectionEnd)}
          onClick={()=>{picked.current=null;}}
          onBlur={()=>{setCompletion(null);void finish(true);}} readOnly={busy}
          className="h-full w-full px-1 text-xs border-2 border-blue-600 bg-white outline-none"/>
        {showSuggestions&&<div id="inline-function-suggestions" role="listbox" aria-label="Function suggestions"
          className="absolute left-0 z-50 w-64 max-h-40 overflow-y-auto rounded border border-slate-200 bg-white py-1 shadow"
          style={cellTop(selectedRow)-scroll.top>size.height-190?{bottom:H}:{top:H}}>
          {suggestions.map((name,index)=><button key={name} id={`inline-function-${index}`} type="button" role="option" aria-label={name} aria-selected={index===suggestionIndex}
            onMouseDown={e=>e.preventDefault()} onClick={()=>acceptSuggestion(name)}
            className={`block w-full px-3 py-1 text-left font-mono text-xs hover:bg-emerald-50 ${index===suggestionIndex?'bg-emerald-50':''}`}>{name}</button>)}
        </div>}
      </div>}
    </div>
  </div>;
};
