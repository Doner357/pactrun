export type ReaderRole = 'Users' | 'Authors';
export function readerRole(pathname: string, base?: string): ReaderRole | null;
export function allowReaderNavigation(current: string, target: string, base?: string): boolean;
