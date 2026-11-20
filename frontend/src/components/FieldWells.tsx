import React from 'react';

export interface FieldWell {
  label: string;
  fields: string[];
  onChange: (fields: string[]) => void;
  multiple?: boolean;
}

const FIELD_TYPE = 'application/x-gridloom-column';

export function FieldWells({ columns, wells }: { columns: string[]; wells: FieldWell[] }) {
  if (!columns.length) return null;
  return <div className="space-y-2">
    <p className="font-semibold text-slate-700">Available fields</p>
    <p className="text-slate-600">Drag fields below or use the field selectors.</p>
    <ul className="flex flex-wrap gap-1">
      {columns.map(column => <li key={column} draggable
        onDragStart={e => {
          e.dataTransfer.setData(FIELD_TYPE, column);
          e.dataTransfer.effectAllowed = 'copy';
        }}
        className="max-w-full cursor-grab break-all rounded border border-slate-300 bg-white px-2 py-1">{column}</li>)}
    </ul>
    {wells.map(({ label, fields, onChange, multiple }) => {
      const add = (column: string) => {
        if (columns.includes(column) && !fields.includes(column)) {
          onChange(multiple ? [...fields, column] : [column]);
        }
      };
      return <div key={label} role="group" aria-label={label}
        onDragOver={e => {
          if (e.dataTransfer.types.includes(FIELD_TYPE)) {
            e.preventDefault();
            e.dataTransfer.dropEffect = 'copy';
          }
        }}
        onDrop={e => {
          if (!e.dataTransfer.types.includes(FIELD_TYPE)) return;
          e.preventDefault();
          add(e.dataTransfer.getData(FIELD_TYPE));
        }}
        className="rounded border border-dashed border-slate-400 bg-white p-2">
        <p className="mb-1 font-semibold">{label}</p>
        {fields.map(field => <div key={field} className="mb-1 flex items-center justify-between gap-1 rounded bg-slate-100 px-1.5 py-1">
          <span className="break-all">{field}</span>
          <button type="button" aria-label={`Remove ${field} from ${label}`}
            className="rounded px-1 hover:bg-slate-200"
            onClick={() => onChange(fields.filter(f => f !== field))}>×</button>
        </div>)}
        <select aria-label={`Add field to ${label}`} value="" onChange={e => add(e.target.value)}
          className="w-full rounded border border-slate-300 bg-white text-xs">
          <option value="">Choose a field</option>
          {columns.filter(column => !fields.includes(column)).map(column => <option key={column}>{column}</option>)}
        </select>
      </div>;
    })}
  </div>;
}
