import { editableCellInput } from '../utils/cellInput';
import React, { useState, useEffect, useRef, useLayoutEffect } from 'react';
import { Check, X } from 'lucide-react';
import { CellRenderData } from '../types';
import { a1ToCoord, coordToA1 } from '../utils/coords';
import {cycleReference} from '../utils/referenceCycle';
import {api} from '../api';
import {functionCompletion,applyFunctionCompletion,completionEvent,FunctionCompletion} from '../utils/functionCompletion';

interface FormulaBarProps {
  disabled?: boolean;
  selectedRow: number;
  selectedCol: number;
  activeCellData?: CellRenderData;
  onCommit: (rawInput: string) => Promise<boolean>;
  onDraftChange:(active:boolean)=>void;
  sheetName:string;
  onGoTo: (row:number,col:number) => void;
  maxRows:number; maxCols:number;
}

export const FormulaBar: React.FC<FormulaBarProps> = ({
  disabled = false,
  selectedRow,
  selectedCol,
  activeCellData,
  onCommit, onGoTo, maxRows, maxCols, onDraftChange,sheetName,
}) => {
  const [address,setAddress]=useState('');
  const [addressError,setAddressError]=useState('');
  const coordA1 = coordToA1(selectedRow, selectedCol);
  useEffect(()=>{setAddress(coordA1);setAddressError('');},[coordA1]);
  const [inputValue, setInputValue] = useState('');
  const [showSuggestions, setShowSuggestions] = useState(false);
  const [functionNames,setFunctionNames]=useState<string[]>([]);
  const [completion,setCompletion]=useState<FunctionCompletion|null>(null);
  const [suggestionIndex,setSuggestionIndex]=useState(0);
  const completionKey=useRef<string|null>(null);
  const filterSuggestions=completion?functionNames.filter(name=>name.startsWith(completion.prefix)):[];
  const suggestionsVisible=showSuggestions&&filterSuggestions.length>0;
  useEffect(()=>{let current=true;api.functionCatalog().then(data=>{if(current)setFunctionNames(data.functions);}).catch(()=>{});return()=>{current=false;};},[]);
  useEffect(()=>{if(suggestionsVisible)document.getElementById(`formula-function-${suggestionIndex}`)?.scrollIntoView({block:'nearest'});},[suggestionsVisible,suggestionIndex]);
  const inputRef = useRef<HTMLInputElement>(null);
  const referenceSelection=useRef<{start:number;end:number}|null>(null);
  useLayoutEffect(()=>{const selection=referenceSelection.current;referenceSelection.current=null;if(selection&&inputRef.current)inputRef.current.setSelectionRange(selection.start,selection.end);},[inputValue]);
  const dirty = useRef(false);
  const identity = useRef(`${sheetName}:${coordA1}`);
  const currentIdentity=`${sheetName}:${coordA1}`;
  const latestIdentity=useRef(currentIdentity);latestIdentity.current=currentIdentity;
  const committing=useRef(false);
  useEffect(()=>()=>onDraftChange(false),[onDraftChange]);
  const reset = () => { completionKey.current=null;dirty.current = false; onDraftChange(false);setInputValue(editableCellInput(activeCellData)); setShowSuggestions(false); };
  const commit = async () => {
    if(committing.current||disabled)return;committing.current=true;const target=currentIdentity, submitted=inputValue;
    try {const success=await onCommit(submitted);if(success&&latestIdentity.current===target){dirty.current=false;onDraftChange(false);setShowSuggestions(false);}}
    finally {committing.current=false;}
  };

  useEffect(() => {
    if (identity.current !== currentIdentity) { dirty.current = false; identity.current = currentIdentity;completionKey.current=null;onDraftChange(false); setShowSuggestions(false); }
    if (!dirty.current) setInputValue(editableCellInput(activeCellData));
  }, [activeCellData, currentIdentity,onDraftChange]);

  const updateCompletion=(value:string,start:number|null,end:number|null,inputChanged=false)=>{
    // React can deliver onSelect after onChange for the same caret. Do not reset
    // a keyboard choice (or reopen a dismissed popup) for that duplicate event.
    const event=completionEvent(completionKey.current,value,start,end,inputChanged);if(!event.update)return;
    completionKey.current=event.key;
    const context=functionCompletion(value,start??0,end??start??0);
    setCompletion(context);setSuggestionIndex(0);setShowSuggestions(!!context);
  };
  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    referenceSelection.current=null;dirty.current=true;onDraftChange(true);
    setInputValue(e.target.value);updateCompletion(e.target.value,e.target.selectionStart,e.target.selectionEnd,true);
  };
  const selectSuggestion = (fnName: string) => {
    if(disabled||committing.current||!completion||!functionNames.includes(fnName))return;
    const result=applyFunctionCompletion(inputValue,completion,fnName);
    dirty.current=true;onDraftChange(true);setInputValue(result.value);setShowSuggestions(false);setCompletion(null);
    referenceSelection.current={start:result.caret,end:result.caret};inputRef.current?.focus();if(result.value===inputValue){inputRef.current?.setSelectionRange(result.caret,result.caret);referenceSelection.current=null;}
  };
  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if(e.nativeEvent.isComposing)return;
    if(e.ctrlKey||e.metaKey||e.altKey||e.shiftKey)return;
    if(suggestionsVisible&&!disabled&&!committing.current){
      if(e.key==='ArrowDown'||e.key==='ArrowUp'){e.preventDefault();setSuggestionIndex(i=>(i+(e.key==='ArrowDown'?1:-1)+filterSuggestions.length)%filterSuggestions.length);return;}
      if(e.key==='Tab'||e.key==='Enter'){e.preventDefault();selectSuggestion(filterSuggestions[Math.min(suggestionIndex,filterSuggestions.length-1)]);return;}
      if(e.key==='Escape'){e.preventDefault();setShowSuggestions(false);return;}
    }
    if(e.key==='F4'&&!e.altKey&&!e.ctrlKey&&!e.metaKey&&!e.shiftKey){
      e.preventDefault();if(disabled||committing.current)return;
      const result=cycleReference(inputValue,e.currentTarget.selectionStart??0,e.currentTarget.selectionEnd??0);
      if(result){dirty.current=true;onDraftChange(true);referenceSelection.current={start:result.selectionStart,end:result.selectionEnd};setInputValue(result.value);setShowSuggestions(false);}return;
    }
    if (e.key === 'Enter') {
      e.preventDefault(); commit();
    } else if (e.key === 'Escape') {
      e.preventDefault(); reset();
    }
  };

  return (
    <div className="relative flex items-center border-b border-slate-200 bg-white px-3 py-1 gap-2 text-xs select-none">
      {/* Active Coord Display */}
      <form onSubmit={e=>{e.preventDefault();const coord=a1ToCoord(address);
        if(!coord || coord.row>=maxRows || coord.col>=maxCols){setAddressError('Enter a cell address inside this worksheet.');return;}
        setAddressError('');onGoTo(coord.row,coord.col);
      }}>
        <input aria-label="Go to cell" title="Go to cell: enter an address, for example D200" disabled={disabled} value={address}
          onChange={e=>{setAddress(e.target.value);setAddressError('');}}
          onKeyDown={e=>{if(e.key==='Escape'){setAddress(coordA1);setAddressError('');}}}
          className="w-20 text-center font-mono font-semibold text-slate-800 bg-slate-50 border border-slate-200 py-1 rounded px-1.5"/>
        {addressError&&<span role="alert" className="absolute top-full left-0 z-50 bg-white border text-rose-700 p-2">{addressError}</span>}
      </form>

      {/* Badges */}
      <div className="flex items-center gap-1">
        {activeCellData?.is_staged && (
          <span className="px-1.5 py-0.5 rounded text-[10px] font-bold bg-amber-100 text-amber-800 border border-amber-300">
            STAGED
          </span>
        )}
        {activeCellData?.is_spill && (
          <span className="px-1.5 py-0.5 rounded text-[10px] font-bold bg-blue-100 text-blue-800 border border-blue-300">
            SPILL
          </span>
        )}
        {activeCellData?.is_error && (
          <span className="px-1.5 py-0.5 rounded text-[10px] font-bold bg-rose-100 text-rose-800 border border-rose-300">
            ERROR
          </span>
        )}
        {activeCellData?.table_name && (
          <span className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-emerald-50 text-emerald-700 border border-emerald-200">
            {activeCellData.table_name}
          </span>
        )}
      </div>

      {/* Action icons */}
      <div className="flex items-center gap-0.5 text-slate-400">
        <button disabled={disabled}
          onClick={reset}
          className="p-1 hover:text-slate-600 rounded hover:bg-slate-100"
          title="Cancel"
        >
          <X className="w-3.5 h-3.5" />
        </button>
        <button disabled={disabled}
          onClick={commit}
          className="p-1 hover:text-emerald-600 rounded hover:bg-slate-100"
          title="Commit (Enter)"
        >
          <Check className="w-3.5 h-3.5" />
        </button>
      </div>

      {/* Function indicator fx */}
      <div className="font-serif italic font-bold text-slate-400 text-sm px-1">
        fx
      </div>

      {/* Formula Input */}
      <div className="relative flex-1">
        <input
          disabled={disabled}
          aria-label="Formula input"
          title="Function suggestions: Up/Down to choose, Tab/Enter to accept, Escape to dismiss. F4 cycles reference locks."
          ref={inputRef}
          type="text"
          value={inputValue}
          onChange={handleChange}
          onKeyDown={handleKeyDown}
          onSelect={e=>{if(!disabled)updateCompletion(e.currentTarget.value,e.currentTarget.selectionStart,e.currentTarget.selectionEnd);}}
          onBlur={()=>setShowSuggestions(false)}
          aria-autocomplete="list" aria-controls={suggestionsVisible?'formula-function-suggestions':undefined}
          aria-activedescendant={suggestionsVisible?`formula-function-${suggestionIndex}`:undefined}
          placeholder="Enter value or formula (e.g. =SUM(A1:A10) or =SQLVALUE(&quot;LocalDB&quot;, &quot;SELECT sum(amount) FROM invoices&quot;))"
          className="w-full px-2 py-1 text-xs font-mono text-slate-800 bg-transparent border-0 focus:outline-none focus:ring-0"
        />

        {/* Autocomplete Dropdown */}
        {suggestionsVisible && (
          <div id="formula-function-suggestions" role="listbox" aria-label="Function suggestions" className="absolute left-0 top-full mt-1 w-64 max-h-48 overflow-y-auto bg-white border border-slate-200 rounded shadow-lg z-50 py-1">
            {filterSuggestions.map((fn,index) => (
              <button disabled={disabled}
                key={fn} id={`formula-function-${index}`} role="option" aria-label={fn} aria-selected={index===suggestionIndex}
                onMouseDown={e=>e.preventDefault()}
                onClick={() => selectSuggestion(fn)}
                className={`w-full text-left px-3 py-1 text-xs hover:bg-emerald-50 hover:text-emerald-700 font-mono flex items-center justify-between ${index===suggestionIndex?'bg-emerald-50':''}`}
              >
                <span className="font-bold">{fn}</span>
                <span className="text-[10px] text-slate-400">function</span>
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
};
