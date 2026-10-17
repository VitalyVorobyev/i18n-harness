# Translate a Qt Linguist file

Qt applications keep their translations in `.ts` files produced by
`lupdate`. This tutorial translates a small German file and shows what
changes in the file.

## The example

Create a folder `qt-demo` with one file, `app_de.ts`:

```xml
<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE">
<context>
    <name>MainWindow</name>
    <message>
        <location filename="../src/mainwindow.cpp" line="42"/>
        <source>&amp;Open...</source>
        <translation type="unfinished"></translation>
    </message>
    <message>
        <location filename="../src/mainwindow.cpp" line="57"/>
        <source>Saved %1 to %2</source>
        <translation type="unfinished"></translation>
    </message>
    <message>
        <location filename="../src/mainwindow.cpp" line="63"/>
        <source>&lt;b&gt;Warning:&lt;/b&gt; the file is read-only.</source>
        <translation type="unfinished"></translation>
    </message>
    <message numerus="yes">
        <location filename="../src/mainwindow.cpp" line="80"/>
        <source>%n file(s) selected</source>
        <translation type="unfinished">
            <numerusform></numerusform>
            <numerusform></numerusform>
        </translation>
    </message>
    <message>
        <location filename="../src/mainwindow.cpp" line="91"/>
        <source>Quit</source>
        <translation>Beenden</translation>
    </message>
</context>
</TS>
```

It has the usual ingredients of a Qt UI file:

- `&Open...` — the `&` marks the keyboard shortcut (Alt+O).
- `%1`, `%2` — values the application fills in at run time.
- `<b>…</b>` — rich-text markup.
- `%n file(s)` — a plural string (`numerus="yes"`), one form per plural
  category of the target language (German has two).
- `Quit` — already translated.

## Steps

1. Open the app, click **Open folder…** and choose `qt-demo`.
   **Files** shows `app_de.ts` with *German · Qt Linguist* and
   *4 to do*.
2. Pick a model in the toolbar.
3. Click **Translate 4 strings**. Wait for *Finished. 4 translated*.
4. Click **Check** and go through the four suggestions.
   - For the plural string the editor shows two boxes, *German — one
     (e.g. 1)* and *German — other (e.g. 0)*. Each form is translated separately,
     with a real number put in place of `%n`, so the model sees
     *"1 file(s) selected"* and *"0 file(s) selected"* (one example number per form);
     `%n` is put back afterwards.
   - If the model dropped the `<b>` tags, the string shows a red
     problem: *Formatting tags do not match the original: missing
     &lt;b&gt;, &lt;/b&gt;*. Add the tags back.
   - If it dropped the `&`, a yellow warning says *Keyboard shortcut
     marker (&) appears 1 time(s) in the original and 0 in the
     translation*. Warnings do not stop saving, but you usually want the
     shortcut back: type `&Öffnen...`.
5. Click **Done** on each string you accept.
6. Click **Save**.

## Expected result

With a typical model the file now reads (only the translations changed):

```xml
    <message>
        <location filename="../src/mainwindow.cpp" line="42"/>
        <source>&amp;Open...</source>
        <translation>&amp;Öffnen...</translation>
    </message>
    <message>
        <location filename="../src/mainwindow.cpp" line="57"/>
        <source>Saved %1 to %2</source>
        <translation>%1 wurde in %2 gespeichert</translation>
    </message>
    <message>
        <location filename="../src/mainwindow.cpp" line="63"/>
        <source>&lt;b&gt;Warning:&lt;/b&gt; the file is read-only.</source>
        <translation>&lt;b&gt;Warnung:&lt;/b&gt; Die Datei ist schreibgeschützt.</translation>
    </message>
    <message numerus="yes">
        <location filename="../src/mainwindow.cpp" line="80"/>
        <source>%n file(s) selected</source>
        <translation>
            <numerusform>%n Datei ausgewählt</numerusform>
            <numerusform>%n Dateien ausgewählt</numerusform>
        </translation>
    </message>
```

Your model's wording will differ. What does not differ:

- **Done** strings lose `type="unfinished"`; Qt's `lrelease` includes
  them in the compiled `.qm`.
- Strings you left under **Check** keep `type="unfinished"`, so
  `lrelease` skips them by default — unchecked machine translations do
  not ship by accident.
- A translation with a red problem is not written at all.
- Everything else — the XML declaration, `<location>` lines,
  indentation, entity escaping such as `&amp;` and `&lt;` — is byte for
  byte what it was.

## Notes

- **Language detection.** The language comes from the file: the
  `language` attribute, or a language code in the file name
  (`app_de.ts`, `app-de_DE.ts`). The app records it in
  `i18n-harness.toml` (here as `de`). To change the language the app uses
  for a file, edit `locale` in that file's `[[catalogs]]` entry; see
  [The settings file](../reference/settings-file.md).
- **Translator comments** (`<comment>`, `<extracomment>`) are shown in the
  editor as *Meaning* and *Note*, and are passed to chat models as
  context.
- **Vanished and obsolete messages** (`type="vanished"`,
  `type="obsolete"`) are never touched.
- To check a `.ts` file without translating anything, the command-line
  tool has [`harness round-trip` and `harness gate`](../reference/cli.md).
