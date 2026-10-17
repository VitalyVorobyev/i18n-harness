// Recently opened folders, per viewer. Browser storage can be unavailable
// (private windows, blocked storage); the list is then just empty.

const KEY = "i18n-harness:recent-folders";
const LIMIT = 8;

export function recentFolders(): string[] {
  try {
    const raw = localStorage.getItem(KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed) ? parsed.filter((p): p is string => typeof p === "string") : [];
  } catch {
    return [];
  }
}

export function rememberFolder(path: string): void {
  try {
    const next = [path, ...recentFolders().filter((p) => p !== path)].slice(0, LIMIT);
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {
    // Not remembering a folder is harmless.
  }
}

export function forgetFolder(path: string): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(recentFolders().filter((p) => p !== path)));
  } catch {
    // Ignore.
  }
}

/** Last path segment, for display. */
export function folderName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts.at(-1) ?? path;
}
