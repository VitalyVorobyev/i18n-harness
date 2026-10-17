import type { CatalogSummary } from "../api";

/** Files that share a target language, an existing-text language, a format and a folder. */
export interface FileGroup {
  key: string;
  /** "Slovak · from German · Qt Linguist", plus the folder when files live in several. */
  title: string;
  files: CatalogSummary[];
}

const folderOf = (c: CatalogSummary) => {
  const i = c.relative_path.lastIndexOf("/");
  return i < 0 ? "" : c.relative_path.slice(0, i);
};

/** The files grouped so that what they share is said once, in the folder's order. */
export function groupFiles(catalogs: CatalogSummary[]): FileGroup[] {
  const severalFolders = new Set(catalogs.map(folderOf)).size > 1;
  const groups = new Map<string, FileGroup>();
  for (const c of catalogs) {
    const folder = severalFolders ? folderOf(c) : "";
    const key = [c.locale, c.pivot ?? "", c.format, folder].join("\u0000");
    let group = groups.get(key);
    if (!group) {
      const parts = [c.language];
      if (c.pivot_language) parts.push(`from ${c.pivot_language}`);
      parts.push(c.format_name);
      if (severalFolders) parts.push(folder === "" ? "top folder" : folder);
      group = { key, title: parts.join(" · "), files: [] };
      groups.set(key, group);
    }
    group.files.push(c);
  }
  return [...groups.values()];
}

/**
 * A file's name without what its group already says: the extension and a
 * trailing locale (`AcfSln_sk_SK.ts` → `AcfSln`, `sk.json` stays `sk`).
 */
export function shortName(c: CatalogSummary): string {
  const dot = c.name.lastIndexOf(".");
  const stem = dot > 0 ? c.name.slice(0, dot) : c.name;
  const language = c.locale.split(/[_-]/)[0] ?? c.locale;
  for (const tag of [c.locale, c.locale.replace("_", "-"), language]) {
    for (const sep of ["_", "-", "."]) {
      const suffix = `${sep}${tag}`;
      if (stem.length > suffix.length && stem.toLowerCase().endsWith(suffix.toLowerCase())) {
        const rest = stem.slice(0, -suffix.length);
        // `sk_SK.ts` is named by its locale alone; keep it whole.
        if (!c.locale.toLowerCase().startsWith(rest.toLowerCase())) return rest;
      }
    }
  }
  return stem;
}
