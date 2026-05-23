---
name: add-format
description: Use when adding a new catalog format serializer to i18n-harness (e.g., a vendor-specific JSON dialect, an XLIFF flavor, a Fluent variant). Scaffolds the module under crates/catalog, writes the placeholder converter to/from ICU with a property test, sets up the round-trip fixture directory under fixtures/<format>/, and extends the gate fixture coverage for format-specific quirks. Qt has its own crate (adapter-qt) because the parser dominates — this skill is for non-Qt formats.
---

# add-format

Adds a new `CatalogFormat` serializer to `crates/catalog`.

## When you DO need this skill

- A new catalog format the team wants to support (e.g., XLIFF 2.x,
  Fluent, Properties, a vendor JSON dialect).

## When you do NOT need this skill

- The format already exists with a different file extension — extend
  the existing serializer to recognize the extension instead.
- The format is exotic enough to need a hand-managed parser (e.g.,
  Qt `.ts`) — give it its own crate alongside `adapter-qt`.

## Steps

1. **Pick the format name** (`po`, `icu-json`, `xliff2`, `fluent`, …).
   The Cargo feature uses underscores (`icu_json`, `xliff2`).
2. **Add a Cargo feature** in `crates/catalog/Cargo.toml`:
   ```toml
   [features]
   xliff2 = ["dep:<parser_crate>"]
   ```
3. **Add the module** at `crates/catalog/src/xliff2.rs`, gated by
   `#[cfg(feature = "xliff2")]`. Implement `CatalogFormat`:
   - `read(path) -> Result<Vec<Unit>>` parses the file and returns
     `Unit`s with **placeholders normalized to ICU form** (this is
     non-negotiable — every serializer normalizes; backend prompts and
     the gate only ever see ICU).
   - `write(units, path) -> Result<()>` writes back. The combination
     `read → write` with zero changes must be byte-identical on disk.
4. **Write the placeholder converter** in the same module:
   - `to_icu(native: &str) -> String` — extract direction.
   - `from_icu(icu: &str) -> String` — apply direction.
   - A `proptest` property test that `from_icu(to_icu(s)) == s` for a
     generated corpus of valid native placeholder strings.
5. **Create the fixture directory** `fixtures/xliff2/` with at least:
   - A minimal hand-crafted file covering placeholder kinds, plural
     arities, accelerators (if applicable), comments, whitespace,
     escapes.
   - A round-trip test in `crates/catalog/tests/xliff2_roundtrip.rs`
     iterating all files in `fixtures/xliff2/` and asserting byte-
     identity after `read → write`.
6. **Extend the gate fixture coverage** in `crates/gate/tests/` with at
   least one fixture demonstrating any format-specific quirk the gate
   needs to know about (e.g., PO's plural `msgstr[N]` form).
7. **Register the serializer** in `crates/adapter-react` (if relevant)
   and `crates/cli/src/main.rs` so it appears in `--format xliff2`.

## What you do NOT do

- Do not leak the native placeholder syntax into the intermediate — the
  intermediate is ICU. If conversion is lossy for some construct, the
  intermediate carries enough info to reconstruct it on `apply`, but
  the backend and gate still see ICU.
- Do not skip the round-trip property test for the placeholder
  converter.
- Do not edit `adapter-qt` in this skill — Qt is its own adapter.

## Verification

- `cargo build --workspace --features xliff2` succeeds.
- `cargo test --workspace --features xliff2` passes all round-trip
  fixtures.
- Running `harness round-trip fixtures/xliff2/*` reports zero diffs.
