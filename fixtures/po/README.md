# PO fixtures

Hand-crafted gettext `.po` fixtures for the M4.4 round-trip contract.

Every file here MUST satisfy:

```text
apply(extract(f), extract(f).units(), out)  ==  f   (byte-for-byte)
```

See `crates/catalog/tests/po_roundtrip.rs` for the executable form.

| File                          | Covers                                             |
|-------------------------------|----------------------------------------------------|
| `singular_basic.po`           | header, plain singular entries, `#:` source refs, `#.` extracted comments, empty (untranslated) msgstr |
| `plurals_polish.po`           | 3-form plural (Polish) via `msgstr[0..2]`           |
| `plurals_arabic.po`           | 6-form plural (Arabic, `nplurals=6`)               |
| `msgctxt.po`                  | `msgctxt` disambiguation of identical msgids       |
| `mixed_placeholders.po`       | `%s`, `%d`, positional `%1$s`, Python `%(name)s`, escaped `%%` |
| `multiline_continuations.po`  | string continuation across multiple `"..."` lines, `\n` escapes, `\"` escapes |

When adding a fixture for a new edge case, add a row above and verify the
round-trip test still passes before opening the PR. Do not relax the
assertion to accommodate a new fixture.
