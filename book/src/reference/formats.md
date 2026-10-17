# Formats and placeholders

Three formats are built in. Each is recognised from the file's content
(not only its extension), so a TypeScript `.ts` source file is not
mistaken for a Qt Linguist file.

| Format | Id in the settings file | Extensions | Language taken from |
|---|---|---|---|
| Qt Linguist | `qt-ts` | `.ts` | `<TS language="…">`, else the file name |
| gettext PO | `gettext-po` | `.po`, `.pot` | `Language:` header, else the file name |
| ICU message JSON | `icu-json` | `.json` | the file name (`fr.json`, `messages.fr.json`) |

A language code in a file name is recognised just before the extension,
after `_`, `-` or `.`, or as the whole name: `app_de.ts`, `app-de_DE.ts`,
`messages.zh_Hans.json`, `de.po`.

## Placeholders

Internally every string is held in one notation, ICU MessageFormat, so
that the checks work the same way for all formats. Each format converts
its own placeholders on reading and converts them back on writing. In
the app you always see and type the file's own notation.

### Qt Linguist

| In the file | Meaning | Internal form |
|---|---|---|
| `%1` … `%99` | numbered argument | `{0}` … `{98}` |
| `%L1` … `%L99` | localised number | `{L1}` … `{L99}` |
| `%n` | the count of a plural string | `{count}` |
| `%%` | a literal `%` | `%` |
| `&` | keyboard shortcut marker (`&&` is a literal `&`) | unchanged; see the [shortcut check](checks.md#shortcut-marker) |

Markup inside messages (`<b>`, `<br/>`, `<a href="…">`) is plain text to
the format; the [markup check](checks.md#markup) protects it.

### gettext PO

| In the file | Meaning | Internal form |
|---|---|---|
| `%s`, `%d`, `%i`, `%u`, `%f`, `%c` | arguments in order | `{0}`, `{1}`, … |
| `%1$s`, `%2$d` | numbered arguments | `{0}`, `{1}`, … |
| `%(name)s`, `%(count)d` | named arguments (Python) | `{name}`, `{count}` |
| `%%` | a literal `%` | `%` |

When a translation is written back, each placeholder gets its original
conversion letter back (`%d` stays `%d`). Not supported, and reported as
an error when the file is opened: width and precision (`%-10s`, `%.2f`),
numbered and unnumbered placeholders mixed in one string, and `%n`.

Plural entries (`msgid_plural`, `msgstr[N]`) have as many forms as the
file's `Plural-Forms: nplurals=…` header says.

### ICU message JSON

The values already are ICU messages, so nothing is converted:

| In the file | Meaning |
|---|---|
| `{name}`, `{0}` | argument |
| `{n, number}`, `{d, date, short}` | formatted argument |
| `{count, plural, one {# item} other {# items}}` | plural; `#` is the number |
| `{gender, select, female {…} other {…}}` | choice |
| `''` | a literal apostrophe |

A file must be a JSON object; nested objects give dotted ids
(`app.menu.open`). Values that are not strings (numbers, arrays) are
skipped. Only values are rewritten; keys, order and layout stay. See the
[ICU JSON tutorial](../tutorials/icu-json.md) for current limitations.

## What "written back exactly" means

For every format, reading a file and writing it back **with no
translation changed** gives identical bytes: encoding, byte-order mark,
line endings, indentation, comments, entity escaping, entry order,
obsolete entries — all unchanged. When translations change, only the
bytes of those translations (and, in Qt, the `type="unfinished"`
attribute of strings marked done) are replaced.

You can verify this for any file with
[`harness round-trip <file>`](cli.md#round-trip). The project's test
suite enforces it for every sample file; see
[The round-trip contract](../developers/round-trip.md).

## Strings that are never touched

- Qt messages with `type="vanished"` or `type="obsolete"`.
- PO entries commented out with `#~`.
- The PO header entry (`msgid ""`).
