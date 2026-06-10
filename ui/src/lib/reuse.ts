// Shared helpers for the reference-reuse / remainder / merge surfaces.
//
// The conflict candidate `text` joins plural CLDR forms with the unit
// separator U+001F (per the wire DTO); these helpers decode it. Conflict
// candidates are carried on the review-queue item's `reviewer_note` (the
// reuse pass persists them there), so the conflict view survives a reopen
// without any session-local state.

import type { ReferenceConflictCandidate } from "./types";

/** Unit separator used to join plural forms inside a candidate's `text`. */
export const UNIT_SEPARATOR = "\u{1F}";

/** Split a candidate's `text` into CLDR plural forms when `is_plural`. */
export function candidateForms(
  candidate: ReferenceConflictCandidate,
): string[] {
  return candidate.is_plural
    ? candidate.text.split(UNIT_SEPARATOR)
    : [candidate.text];
}

// A record-separator control char that cannot appear in a path or a Qt unit
// id, so the composite key never collides regardless of path content.
const KEY_SEP = "\u{1E}";

/** Stable composite key for a (catalog path, unit id) review row. */
export function rowKey(catalogPath: string, unitId: string): string {
  return `${catalogPath}${KEY_SEP}${unitId}`;
}

/**
 * Parse the reference-conflict candidate list a reuse pass persisted on a
 * unit's review note. Returns an empty array when the note is absent or not
 * the expected JSON shape — callers render a fallback in that case.
 */
export function parseConflictCandidates(
  note: string | null | undefined,
): ReferenceConflictCandidate[] {
  if (!note) return [];
  try {
    const parsed = JSON.parse(note);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (c): c is ReferenceConflictCandidate =>
        typeof c === "object" &&
        c !== null &&
        typeof c.reference === "string" &&
        typeof c.text === "string",
    );
  } catch {
    return [];
  }
}

/** Shorten a path for display: keep the last `segments` segments. */
export function shortenReusePath(p: string, segments = 2): string {
  const parts = p.replace(/\\/g, "/").split("/");
  return parts.length > segments ? `…/${parts.slice(-segments).join("/")}` : p;
}

/** Basename (filename) of a path, for default-name seeding. */
export function basenameOf(p: string): string {
  const parts = p.replace(/\\/g, "/").split("/");
  return parts[parts.length - 1] ?? p;
}
