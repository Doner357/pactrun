export const readingSequences: Record<string, string[]>;
export type ReadingNavigation = {kind: 'sequence' | 'lookup' | 'record' | 'hub'; audience?: string; previous?: string; next?: string; back?: string; label?: string};
export function readingNavigation(source: string, state?: string): ReadingNavigation;
