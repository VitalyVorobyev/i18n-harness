# Translate a gettext PO file

gettext `.po` files are used by C, Python, PHP, Django, WordPress and
many other projects. The app reads and writes them directly.

## The example

A folder `po-demo` with `po/de.po`:

```po
# German translation of myapp.
msgid ""
msgstr ""
"Project-Id-Version: myapp 1.0\n"
"Language: de\n"
"MIME-Version: 1.0\n"
"Content-Type: text/plain; charset=UTF-8\n"
"Content-Transfer-Encoding: 8bit\n"
"Plural-Forms: nplurals=2; plural=(n != 1);\n"

#: src/main.py:12
msgid "Welcome back, %(name)s!"
msgstr ""

#: src/main.py:20
msgid "Loaded %s in %d ms"
msgstr ""

#: src/main.py:31
msgctxt "menu"
msgid "Open"
msgstr ""

#: src/main.py:40
msgid "%d new message"
msgid_plural "%d new messages"
msgstr[0] ""
msgstr[1] ""

#: src/main.py:52
msgid "Cancel"
msgstr "Abbrechen"
```

The language comes from the `Language:` header (or from the file name,
such as `de.po`). The number of plural forms comes from `Plural-Forms`.

## Steps

1. Open `po-demo` in the app. `de.po` shows *German · gettext PO* with
   *4 to do*.
2. Pick a model and click **Translate 4 strings**.
3. Review under **Check**. `msgctxt` appears as the string's context
   (here *menu*), and `#.` comments from the developers appear as notes.
4. Click **Done** on each accepted string, then **Save**.

## Expected result

A run with a chat model produced:

```po
#: src/main.py:12
msgid "Welcome back, %(name)s!"
msgstr "Willkommen zurück, %(name)s!"

#: src/main.py:20
msgid "Loaded %s in %d ms"
msgstr "%s geladen in %d ms"

#: src/main.py:31
msgctxt "menu"
msgid "Open"
msgstr "Öffnen"

#: src/main.py:40
msgid "%d new message"
msgid_plural "%d new messages"
msgstr[0] "%d neue Nachricht"
msgstr[1] "%d neue Nachricht"
```

Placeholders are kept exactly as written: `%(name)s`, `%s`, `%d`, and
positional ones such as `%1$s`. The header, comments, references and
the order of entries are unchanged.

The second plural form is wrong (*Nachricht* instead of
*Nachrichten*). Each plural form is translated separately with an
example number in place of `%d`, but the text sent for every form is the
singular `msgid`, so the model sees *"0 new message"*. **Check the plural
forms of PO files by hand.**

## Placeholder rules for PO

| In the file | Meaning | Must the translation keep it? |
|---|---|---|
| `%s`, `%d`, `%i`, `%u`, `%f`, `%c` | values in order | yes, the same number of each |
| `%1$s`, `%2$d` | numbered values (may be reordered) | yes |
| `%(name)s` | named value (Python) | yes, with the same name |
| `%%` | a literal percent sign | — |

Strings that mix numbered and unnumbered placeholders, or use width or
precision (`%-10s`, `%.2f`), cannot be represented. A file containing
one cannot be opened, and currently neither can the folder: the error
reads *unsupported placeholder syntax: unsupported `%-` at offset 6*.
Run `harness gate --locale <id> <file>` on each PO file to find the
culprit. See
[Formats and placeholders](../reference/formats.md).

## Notes

- Fuzzy entries (`#, fuzzy`) count as **Check**.
- Obsolete entries (`#~`) are never touched.
- A `.pot` template has no translations and no language; translate the
  `.po` files created from it (`msginit`, `msgmerge`).
