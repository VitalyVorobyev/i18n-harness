// Shared helpers for the reference-reuse / remainder / merge surfaces.
//
// The conflict candidate `text` joins plural CLDR forms with the unit
// separator U+001F (per the wire DTO); these helpers decode it. Conflict
// candidates are carried on the review-queue item's `reviewer_note` (the
// reuse pass persists them there), so the conflict view survives a reopen
// without any session-local state.

import type { ReferenceConflictCandidate, ReviewQueueItem } from "./types";

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

// CLDR plural categories in canonical order — matches the form ordering used by
// the editors and the reuse engine.
const CLDR_FORM_LABELS = ["Zero", "One", "Two", "Few", "Many", "Other"];

/**
 * Build a human-readable Markdown report of every reference-reuse conflict in
 * `items`, for offline manual resolution. Each conflict lists its unit id,
 * catalog/locale, source, and every candidate translation with the reference(s)
 * that proposed it. Conflict candidates are read from each item's
 * `reviewer_note` (the JSON the reuse pass persisted there).
 *
 * Items whose review status is not `conflict` are ignored, so callers may pass
 * the full review queue.
 */
export function buildConflictReportMarkdown(
  items: ReviewQueueItem[],
  projectName: string,
): string {
  const conflicts = items.filter((it) => it.review_status === "conflict");
  const lines: string[] = [];
  lines.push(`# Reference-translation conflicts — ${projectName}`);
  lines.push("");
  lines.push(
    `${conflicts.length} conflict${conflicts.length === 1 ? "" : "s"} need manual resolution. Each lists the candidate translations from the reference catalogs that disagreed; the base unit was left untranslated.`,
  );
  lines.push("");

  for (const it of conflicts) {
    lines.push(`## \`${it.unit_id}\``);
    lines.push("");
    lines.push(`- **Catalog:** ${it.catalog_manifest_path} (${it.locale})`);
    lines.push(`- **Source:** ${it.source_preview}`);
    lines.push("");

    const candidates = parseConflictCandidates(it.reviewer_note);
    if (candidates.length === 0) {
      lines.push("_No candidate detail recorded._");
      lines.push("");
      continue;
    }

    candidates.forEach((candidate, idx) => {
      const refs = [candidate.reference, ...candidate.also_from];
      lines.push(`### Candidate ${idx + 1}`);
      const forms = candidateForms(candidate);
      if (candidate.is_plural) {
        forms.forEach((form, fi) => {
          const label = CLDR_FORM_LABELS[fi] ?? `Form ${fi}`;
          lines.push(`- **${label}:** ${form}`);
        });
      } else {
        lines.push(`- ${forms[0] ?? ""}`);
      }
      lines.push(`- _From:_ ${refs.join(", ")}`);
      lines.push("");
    });
  }

  return `${lines.join("\n").trimEnd()}\n`;
}
