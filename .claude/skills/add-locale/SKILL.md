---
name: add-locale
description: Use when adding a new target locale to i18n-harness (e.g., `es_ES`, `zh_Hans`, `fr_FR`, `ja_JP`). Looks up CLDR plural arity, writes the locale record, decides register/variant/length_warn_ratio/script, extends the gate's fixture table, and adds at least one round-trip fixture exercising the new arity. Catches the failure mode where a language is added but its plural arity is never tested.
---

# add-locale

Adds a new target locale as a *data* row, exercising the §4 "languages
as data" invariant.

## Inputs you need before starting

- **Target locale code** (BCP 47-ish: `de_DE`, `es_ES`, `zh_Hans`,
  `zh_Hant`, `es_419`, …). Confirm the variant **before** any
  translations accumulate — moving from `zh_Hans` to `zh_Hant` after the
  fact is destructive.
- **Register**: formal (`Sie`, `usted`) or informal. Default to formal
  unless the maintainer specifies otherwise.
- **Script**: `latin`, `han`, `arab`, `cyrl`, … — affects whitespace,
  punctuation, length expectations.

## Steps

1. **Look up CLDR plural arity** for the locale variant. Source of
   truth: the CLDR `plurals.xml` for the cardinal category. For example:

   | Locale   | CLDR plural categories             | Arity |
   |----------|------------------------------------|-------|
   | `de_DE`  | `[one, other]`                     | 2     |
   | `en`     | `[one, other]`                     | 2     |
   | `es_ES`  | `[one, other]`                     | 2     |
   | `fr_FR`  | `[one, many, other]`               | 3     |
   | `pl_PL`  | `[one, few, many, other]`          | 4     |
   | `zh_Hans`| `[other]`                          | 1     |
   | `ja_JP`  | `[other]`                          | 1     |
   | `ar`     | `[zero, one, two, few, many, other]` | 6   |

2. **Pick `length_warn_ratio`** based on known typographic norms:

   | Family            | Suggested `length_warn_ratio` | Note                    |
   |-------------------|-------------------------------|-------------------------|
   | German            | 1.4                           | runs 30–40% longer      |
   | Romance (es/fr/it)| 1.3                           |                         |
   | Slavic            | 1.2                           |                         |
   | CJK               | 0.6                           | often shorter           |
   | Arabic            | 1.25                          | RTL; check direction    |

3. **Write the locale record** in `crates/locales/` following the format
   of the existing `de_DE` row. Fields: `cldr_plural`, `register`,
   `variant`, `length_warn_ratio`, `script`.
4. **Extend the gate's fixture table** in `crates/gate/tests/` with a
   positive and negative case for the new arity. If the arity differs
   from any existing locale's, you MUST add a fresh plural fixture.
5. **Add at least one Qt round-trip fixture** under `fixtures/qt/` with
   at least one plural-form `<message>` for the new locale. Verify:
   - `cargo test -p i18n-harness-adapter-qt` is green.
   - `cargo run -p i18n-harness-cli -- round-trip fixtures/qt/<new>.ts`
     reports zero diffs.
6. **Update `CLAUDE.md`** if the new locale is in M0–M3 scope (otherwise
   leave it for M4+).

## What you do NOT do

- Do not write a `match` arm on the locale anywhere in the codebase.
- Do not assume English plural arity (2). Always check CLDR.
- Do not invent a `length_warn_ratio` without evidence — borrow from
  the table above or leave a comment citing the source.

## Verification

`cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --all-targets`
must pass. The new locale's round-trip fixture must be byte-stable.
