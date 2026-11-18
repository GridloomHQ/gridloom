export function saveBlocker(gridDraft:boolean,formulaDraft:boolean,dialogOpen:boolean,reviewDrafts:Iterable<{note:string;evidence:unknown[];attachments?:unknown[]}>):string|null {
  if(gridDraft||formulaDraft)return 'Finish or cancel the cell edit before saving or replacing the workbook.';
  if(dialogOpen)return 'Finish or cancel your work in the open panel or dialog, then close it before saving or replacing the workbook.';
  for(const draft of reviewDrafts)if(draft.note.trim()||draft.evidence.length||(draft.attachments?.length??0))return 'Finish or remove the retained review draft before saving or replacing the workbook. Reopen Explain / Review for that cell.';
  return null;
}
