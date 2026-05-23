# Fixtures

Test inputs for the round-trip (M0) and gate (M1) suites.

## Layout

- `qt/` — Qt Linguist `.ts` files for `adapter-qt` round-trip + gate tests.
- `po/` — gettext / Lingui PO files for `crates/catalog` round-trip + gate
  tests (lands in M4).

## Sourcing policy

Each fixture is one of:

1. **Hand-crafted** — a small synthetic file exercising specific edge cases
   (placeholder kinds, plural arities, accelerators, escapes, whitespace,
   CDATA, comments, message states). Add a leading comment explaining
   exactly what the file is testing.
2. **Borrowed from a permissively-licensed OSS project** — Apache-2.0,
   MIT, BSD, or similar. The fixture filename is prefixed
   `vendor_<project>_`, and the project's license, commit SHA, and
   upstream URL are recorded in the "Borrowed fixtures" section below.
   Borrowed fixtures land at the commit they were extracted from and are
   never edited (we want to see what a real project produces, not what we
   would prefer it to produce).

## The round-trip contract

For every file in this tree, `extract → apply` with zero unit changes must
produce a byte-identical file on disk. This is enforced in CI; do not add
a fixture that cannot satisfy this without explaining (in a leading
comment) which adapter quirk is being demonstrated and why a deviation is
intentional.

## Borrowed fixtures

*(none yet — add an entry here whenever you vendor a fixture)*

Entry template:

```
- File: `qt/vendor_<project>_<name>.ts`
  Upstream: <URL>
  Commit: <SHA>
  License: <SPDX>
  Notes: <why this fixture; what quirk it exercises>
```
