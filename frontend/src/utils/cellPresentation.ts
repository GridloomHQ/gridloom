import type {CellRenderData} from '../types';
/** Presentation only: never changes raw values or clipboard/export representations. */
export function cellPresentation(cell:Pick<CellRenderData,'formula'|'display_text'>|undefined,showFormulas:boolean):{text:string;title:string} {
  const value=cell?.display_text??'';
  if(showFormulas&&cell?.formula)return {text:cell.formula,title:`Formula: ${cell.formula}\nEvaluated value: ${value}`};
  return {text:value,title:value};
}
