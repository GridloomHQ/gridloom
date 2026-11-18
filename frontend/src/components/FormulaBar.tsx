import { editableCellInput } from '../utils/cellInput';
import React, { useState, useEffect, useRef } from 'react';
import { Check, X } from 'lucide-react';
import { CellRenderData } from '../types';
import { a1ToCoord, coordToA1 } from '../utils/coords';

interface FormulaBarProps {
  disabled?: boolean;
  selectedRow: number;
  selectedCol: number;
  activeCellData?: CellRenderData;
  onCommit: (rawInput: string) => void;
  onGoTo: (row:number,col:number) => void;
  maxRows:number; maxCols:number;
}

const COMMON_FUNCTIONS = [
  'SUM',
  'SUMIF',
  'SUMIFS',
  'AVERAGE',
  'COUNT',
  'COUNTA',
  'MIN',
  'MAX',
  'IF',
  'IFS',
  'XLOOKUP',
  'VLOOKUP',
  'INDEX',
  'MATCH',
  'FILTER',
  'SORT',
  'UNIQUE',
  'SEQUENCE',
  'SQLVALUE',
  'SQLTABLE',
  'CONCAT',
  'TEXTJOIN',
  'DATE',
  'TODAY',
  'PMT',
  'NPV',
];

export const FormulaBar: React.FC<FormulaBarProps> = ({
  disabled = false,
  selectedRow,
  selectedCol,
  activeCellData,
  onCommit, onGoTo, maxRows, maxCols,
}) => {
  const [address,setAddress]=useState('');
  const [addressError,setAddressError]=useState('');
  const coordA1 = coordToA1(selectedRow, selectedCol);
  useEffect(()=>{setAddress(coordA1);setAddressError('');},[coordA1]);
  const [inputValue, setInputValue] = useState('');
  const [showSuggestions, setShowSuggestions] = useState(false);
  const [filterSuggestions, setFilterSuggestions] = useState<string[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);
  const dirty = useRef(false);
  const identity = useRef(coordA1);
  const reset = () => { dirty.current = false; setInputValue(editableCellInput(activeCellData)); setShowSuggestions(false); };
  const commit = () => { dirty.current = false; setShowSuggestions(false); onCommit(inputValue); };

  useEffect(() => {
    if (identity.current !== coordA1) { dirty.current = false; identity.current = coordA1; setShowSuggestions(false); }
    if (!dirty.current) setInputValue(editableCellInput(activeCellData));
  }, [activeCellData, coordA1]);

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    dirty.current = true;
    const val = e.target.value;
    setInputValue(val);

    if (val.startsWith('=')) {
      const match = val.match(/=([A-Z_]*)$/i);
      if (match) {
        const query = match[1].toUpperCase();
        const filtered = COMMON_FUNCTIONS.filter((fn) => fn.startsWith(query));
        setFilterSuggestions(filtered);
        setShowSuggestions(filtered.length > 0);
        return;
      }
    }
    setShowSuggestions(false);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault(); commit();
    } else if (e.key === 'Escape') {
      e.preventDefault(); reset();
    }
  };

  const selectSuggestion = (fnName: string) => {
    dirty.current = true; setInputValue(`=${fnName}(`);
    setShowSuggestions(false);
    if (inputRef.current) {
      inputRef.current.focus();
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
          ref={inputRef}
          type="text"
          value={inputValue}
          onChange={handleChange}
          onKeyDown={handleKeyDown}
          placeholder="Enter value or formula (e.g. =SUM(A1:A10) or =SQLVALUE(&quot;LocalDB&quot;, &quot;SELECT sum(amount) FROM invoices&quot;))"
          className="w-full px-2 py-1 text-xs font-mono text-slate-800 bg-transparent border-0 focus:outline-none focus:ring-0"
        />

        {/* Autocomplete Dropdown */}
        {showSuggestions && filterSuggestions.length > 0 && (
          <div className="absolute left-0 top-full mt-1 w-64 max-h-48 overflow-y-auto bg-white border border-slate-200 rounded shadow-lg z-50 py-1">
            {filterSuggestions.map((fn) => (
              <button disabled={disabled}
                key={fn}
                onClick={() => selectSuggestion(fn)}
                className="w-full text-left px-3 py-1 text-xs hover:bg-emerald-50 hover:text-emerald-700 font-mono flex items-center justify-between"
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
