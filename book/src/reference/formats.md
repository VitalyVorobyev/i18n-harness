# Formats and placeholders

Five formats are built in: Qt Linguist, gettext PO, ICU message JSON,
and Lingui's PO and JSON catalogs. Each is recognised from the file's
content (not only its extension), so a TypeScript `.ts` source file is
not mistaken for a Qt Linguist file.

| Format (as the app names it) | Id in the settings file | Extensions | Language taken from |
|---|---|---|---|
| Qt Linguist | `qt-ts` | `.ts` | `<TS language="…">`, else the file name |
| gettext PO | `gettext-po` | `.po`, `.pot` | `Language:` header, else the file name |
| ICU MessageFormat JSON | `icu-json` | `.json` | the file or folder name (`fr.json`, `messages.fr.json`, `fr/messages.json`) |
| Lingui (PO) | `lingui-po` | `.po` | `Language:` header, else the file or folder name |
| Lingui (JSON) | `lingui-json` | `.json` | the file or folder name |

A language code in a file name is recognised just before the extension,
after `_`, `-` or `.`, or as the whole name: `app_de.ts`, `app-de_DE.ts`,
`messages.zh_Hans.json`, `de.po`. Failing that, the name of the folder
the file is in counts when it is a language code, as in the usual Lingui
and i18next layouts `locales/fr/messages.po` and
`locales/pt-BR/translation.json`. A file whose language is found
nowhere is left out, with a warning; add it by hand with a `locale`, as
shown in [The settings file](settings-file.md#catalogs).

## How a file's format is recognised

- **Qt Linguist**: an XML file with a `<TS>` root element.
- **Lingui (PO)**: a PO file whose header contains
  `X-Generator: @lingui/cli`, which Lingui's extractor writes.
- **gettext PO**: any other file with `msgid` / `msgstr` entries.
- **Lingui (JSON)**: a JSON object in which at least half of the
  top-level entries are objects with a `translation` key — Lingui's
  default JSON style (`{"id": {"message": …, "translation": …}}`).
- **ICU MessageFormat JSON**: any other JSON object with text values.
  Lingui's *minimal* JSON style, a plain map of ids to translations, is
  read this way. Tool files such as `package.json` and `tsconfig.json`
  are ignored.

When a file is recognised as the wrong format, set `format` in its
`[[catalogs]]` entry of the [settings file](settings-file.md#catalogs).

## Placeholders

Each format has its own placeholder notation. The app recognises the
placeholders in every original, and the
[placeholder check](checks.md#placeholders) makes sure each translation
keeps them: the same placeholders, each as often, in any order. In the
app you always see and type the file's own notation, and messages
about placeholders use it too.

### Qt Linguist

| In the file | Meaning |
|---|---|
| `%1` … `%99` | numbered argument (may be reordered) |
| `%L1` … `%L99` | localised number |
| `%n` | the count of a plural string |
| `%%` | a literal `%` |
| `&` | keyboard shortcut marker (`&&` is a literal `&`); see the [shortcut check](checks.md#shortcut-marker) |

Markup inside messages (`<b>`, `<br/>`, `<a href="…">`) is plain text to
the format; the [markup check](checks.md#markup) protects it.

### gettext PO

| In the file | Meaning |
|---|---|
| `%s`, `%d`, `%i`, `%u`, `%f`, `%c` | arguments in order |
| `%1$s`, `%2$d` | numbered arguments (may be reordered) |
| `%(name)s`, `%(count)d` | named arguments (Python) |
| `%%` | a literal `%` |

When a translation is written back, each placeholder keeps its
conversion letter (`%d` stays `%d`). Not supported, and reported as an
error when the file is opened: width and precision (`%-10s`, `%.2f`),
numbered and unnumbered placeholders mixed in one string, and `%n`.

Plural entries (`msgid_plural`, `msgstr[N]`) have as many forms as the
file's `Plural-Forms: nplurals=…` header says.

### ICU message JSON

The values are ICU messages:

| In the file | Meaning |
|---|---|
| `{name}`, `{0}` | argument |
| `{n, number}`, `{d, date, short}` | formatted argument |
| `{count, plural, one {# item} other {# items}}` | plural; `#` is the number |
| `{gender, select, female {…} other {…}}` | choice |
| `''` | a literal apostrophe |

The argument names and the keywords `plural`, `select` and
`selectordinal` must stay as they are; only the text inside the inner
braces is translated.

A file must be a JSON object; nested objects give dotted ids
(`app.menu.open`). Values that are not strings (numbers, arrays) are
skipped. Only values are rewritten; keys, order and layout stay. Such a
file holds only one language; see the
[ICU JSON tutorial](../tutorials/icu-json.md) for how to translate it
and for current limitations.

### Lingui

Lingui writes ICU messages into both of its catalog formats, so the
placeholders are those of [ICU message JSON](#icu-message-json):
`{name}`, `{0}`, `{count, plural, one {# item} other {# items}}`. Plurals
are `plural` messages inside the text, not separate forms. In a Lingui
PO file `%` is an ordinary character, not a placeholder.

Unlike ICU message JSON, a Lingui catalog keeps the original next to
each translation, so it is translated like a
[gettext PO file](../tutorials/po.md):

- **Lingui (PO)**: the original is the `msgid`, the translation the
  `msgstr`. With Lingui's explicit ids (`#. js-lingui-explicit-id`) the
  `msgid` is the id itself, and the app shows the id as the original.
- **Lingui (JSON)**: each entry's `message` is the original (its id,
  when it has no `message`); only the `translation` value is rewritten.
  `origin`, comments and every other key stay as they are.

## What "written back exactly" means

For every format, reading a file and writing it back **with no
translation changed** gives identical bytes: encoding, byte-order mark,
line endings, indentation, comments, entity escaping, entry order,
obsolete entries — all unchanged. When translations change, only the
bytes of those translations are replaced — and, in Qt, the
`type="unfinished"` mark: removed from strings marked **Done**, added
to strings still under **Check**.

You can verify this for any file with
[`harness round-trip <file>`](cli.md#round-trip). The project's tests
check it for every sample file of every format on each change.

## Strings that are never touched

- Qt messages with `type="vanished"` or `type="obsolete"`.
- PO entries commented out with `#~`.
- The PO header entry (`msgid ""`).
- In Lingui JSON, everything except the `translation` values.
