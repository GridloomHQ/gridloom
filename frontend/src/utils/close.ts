import type { LiveCloseAccount } from '../types';
/** Imported IDs remain the mapping API keys even if a worksheet label has changed. */
export function liveMappingDraft(accounts: readonly LiveCloseAccount[]): Record<string, string> {
  return Object.fromEntries(accounts.map(a => [a.imported_account, a.mapping]));
}
export function mappingDraftChanged(accounts: readonly LiveCloseAccount[], draft: Readonly<Record<string, string>>): boolean {
  return accounts.some(a => (draft[a.imported_account] ?? '') !== a.mapping);
}
export function closeAmount(value: string | null): string {
  return value === null ? 'Invalid amount' : value;
}
