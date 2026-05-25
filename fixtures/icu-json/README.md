# ICU-JSON round-trip fixtures

Each `.json` file here is read by `crates/catalog/tests/icu_json_roundtrip.rs`
and verified against the M4.5 round-trip contract:

> `apply(extract(f), extract(f).units(), out)` is byte-identical to `f`.

The PR that adds a new fixture must add it here, never carve one out.

## Fixture coverage

| File | What it exercises |
|---|---|
| `flat.json` | Top-level flat key→string map; ICU placeholder; `\uXXXX` escape; Unicode characters. |
| `nested.json` | Multi-level (3+) nesting; dot-joined unit ids. |
| `with_plural.json` | ICU `plural` selector — placeholders inside a placeholder. |
| `mixed_quoting.json` | Strings with `'`, `"`, `\`, `\n`, `\t`. Escape handling. |
| `empty_object.json` | `{}` — valid but produces zero units. |
| `with_bom.json` | Leading UTF-8 BOM (`EF BB BF`); tolerated and round-tripped verbatim. |
