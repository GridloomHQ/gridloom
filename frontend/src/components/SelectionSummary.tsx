import React, {useEffect,useState} from 'react';
import {api} from '../api';
import type {CellRange} from '../types';
export interface SummaryMetric {value:string|null;quality:'exact'|'approximate'|'rounded'|'unavailable';reason:string|null;}
export interface RangeSummary {
  selected_count:number;numeric_count:number;
  counts:Record<'integer'|'decimal'|'float'|'text'|'boolean'|'date'|'null'|'blank'|'error'|'other',number>;
  sum:SummaryMetric;average:SummaryMetric;
}
export function SelectionSummary({sheetName,range,revision,busy}:{sheetName:string;range:CellRange;revision:string;busy:boolean}){
  const key=`${revision}:${sheetName}:${range.start.row}:${range.start.col}:${range.end.row}:${range.end.col}`;
  const count=(range.end.row-range.start.row+1)*(range.end.col-range.start.col+1);
  const [state,setState]=useState<{key:string;result?:RangeSummary;error?:string}|null>(null);
  useEffect(()=>{
    let cancelled=false;setState(null);
    if(busy||count>10000)return;
    const timer=setTimeout(()=>{
      api.rangeSummary(sheetName,range).then(result=>{if(!cancelled)setState({key,result});}).catch(e=>{if(!cancelled)setState({key,error:e instanceof Error?e.message:String(e)});});
    },150);
    return()=>{cancelled=true;clearTimeout(timer);};
  },[key,busy]);
  const current=state?.key===key?state:null,result=!busy?current?.result:undefined;
  const metric=(m:SummaryMetric)=>`${m.value??'—'}${m.quality==='approximate'?' (approximate)':m.quality==='rounded'?' (rounded)':''}`;
  const details=result?`Underlying cached values, not rounded display text. Includes staged values; no database refresh. Selected ${result.selected_count}; numbers ${result.numeric_count}; excluded text ${result.counts.text}, booleans ${result.counts.boolean}, dates/times ${result.counts.date}, NULL ${result.counts.null}, blanks ${result.counts.blank}, other ${result.counts.other}; errors ${result.counts.error}. ${result.sum.reason??''} ${result.average.reason??''}`:'';
  return <section aria-label="Selection summary" className="shrink-0 h-7 flex items-center gap-4 overflow-x-auto whitespace-nowrap border-t bg-slate-50 px-3 text-[11px] text-slate-600" title={details}>
    {count>10000?<span>Summary unavailable: select at most 10,000 cells ({count} selected).</span>:busy?<span>Updating selection summary…</span>:current?.error?<span role="alert">Summary unavailable: {current.error}</span>:result?<>
      <span>Sum: <output aria-label="Selection sum">{metric(result.sum)}</output></span>
      <span>Average: <output aria-label="Selection average">{metric(result.average)}</output></span>
      <span>Numbers: <output aria-label="Selection numeric count">{result.numeric_count}</output></span>
      <span>Selected: {result.selected_count}</span>
      {result.sum.quality==='unavailable'?<span>{result.sum.reason}</span>:<span className="text-slate-400">Underlying values · non-numeric cells excluded</span>}
    </>:<span>Summarizing selection…</span>}
  </section>;
}
