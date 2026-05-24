// Tokenize a source/target string for visual highlighting in the
// editor: pick out placeholders and accelerator markers so they stand
// out against translated prose. The regex is intentionally
// conservative — false positives would distort the editor's display of
// real translator output. The structural truth lives in the Rust
// `Placeholder` type; this is presentation only.

export type Token =
  | { kind: "text"; value: string }
  | { kind: "placeholder"; value: string }
  | { kind: "accel"; value: string };

// ICU `{name}` or `{n, plural, ...}`-style, Qt `%1` / `%n` / `%L1`,
// gettext `%s` / `%(name)s`, and `&L` accelerator markers.
const PATTERN =
  /(\{[^{}]+\}|%L?\d+|%[ndsi]|%\([A-Za-z_][\w]*\)[sdif]?|&(?=[A-Za-z]))/g;

export function tokenize(input: string): Token[] {
  const out: Token[] = [];
  let cursor = 0;
  for (const match of input.matchAll(PATTERN)) {
    const start = match.index ?? 0;
    const value = match[0];
    if (start > cursor) {
      out.push({ kind: "text", value: input.slice(cursor, start) });
    }
    out.push({
      kind: value === "&" || value.startsWith("&") ? "accel" : "placeholder",
      value,
    });
    cursor = start + value.length;
  }
  if (cursor < input.length) {
    out.push({ kind: "text", value: input.slice(cursor) });
  }
  return out;
}
