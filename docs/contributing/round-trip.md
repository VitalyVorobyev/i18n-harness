# The round-trip contract

> For every format and every fixture, reading a file and writing it back
> with **zero unit changes** produces **byte-identical** output.

This is the load-bearing promise of the project. Translators trust the
app with files that are under version control and processed by other
tools; a diff that touches anything but the translations — re-indented
XML, re-wrapped PO lines, re-ordered JSON keys, a lost byte-order mark —
would make every review harder and break that trust.

## How it is enforced

- **Fixtures.** `fixtures/qt/`, `fixtures/po/` and `fixtures/icu-json/`
  hold real-world-shaped files, including the awkward cases: CDATA and
  entities, `<extracomment>`, plural tables of many sizes, obsolete
  entries, multi-line PO strings, mixed placeholder styles, BOMs, nested
  and flat JSON. `fixtures/README.md` describes them.
- **Tests.** `crates/adapter-qt/tests/roundtrip.rs`,
  `crates/catalog/tests/po_roundtrip.rs` and
  `crates/catalog/tests/icu_json_roundtrip.rs` read *every* file in the
  matching fixture folder and assert byte equality. A new fixture is
  picked up automatically.
- **Placeholder converters** have their own property tests: converting a
  string to ICU and back gives the original.
- **CI** runs the whole test suite on every push and pull request.
- **Users** can check any file: `harness round-trip <file>`.

## How formats achieve it

The readers keep, for each unit, where its translation lives in the
original bytes and how it was spelled (entity escaping, PO line
wrapping, the printf conversion letter of each placeholder). The writer
compares the units it is given with what it read and splices new bytes
in only where a translation (or, for Qt, the `type="unfinished"`
attribute) changed. Unchanged units are not re-rendered at all.

## Rules for contributors

- **Never weaken the assertion or exclude a fixture.** If a new fixture
  does not round-trip, fix the format. If a construct genuinely cannot be
  supported, refuse it with a clear error at read time and document it in
  `fixtures/README.md` — do not write it back differently.
- **New fixtures go into the suite**, never beside it.
- **A red round-trip test is rolled back immediately**, not fixed
  forward.
- A change to a writer needs a fixture that exercises the change *and*
  an edited-unit test that shows only the intended bytes moved.
