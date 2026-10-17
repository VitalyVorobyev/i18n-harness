# Checks

Every translation — from a model, an agent, a reference file or typed by
hand — is checked against its original by deterministic rules. The
checks never call a model. They find translations that would **break**
the application or contradict the glossary; they do not judge style or
meaning.

There are two severities:

| Severity | Shown as | Effect |
|---|---|---|
| **Error** | red, *Problem* label, counted by **Problems only** | The translation is **not written** to the file. On save, the app keeps the file's previous text for that string (an empty translation if there was none) and reports *Saved, with problems held back*; the translation stays in the app to be fixed. `harness import-batch` and `harness translate` write nothing for a file with any error. |
| **Warning** | yellow | Saved normally. `harness translate-folder --deliver` marks such translations done like clean ones; `import-batch` leaves them unfinished (to check). |

The messages below are those the app shows. For a plural string, a
message about one form ends with *(form N)*. Placeholders are shown in
the file's own notation (`%1`, `%(name)s`, `{name}`).

## Errors

### Placeholders

*Placeholders do not match the original: missing %1; unexpected %2*

Each form of the translation must contain exactly the same placeholders
as the original, each as often (order may change). In ICU messages the
kind of each argument counts too: `{count, plural, …}` rendered as
`{count, množné, …}` is a mismatch, because the application would no
longer recognise it. For a gettext plural, the forms other than the first
are compared with `msgid_plural`. See
[Formats and placeholders](formats.md#placeholders) for what counts as a
placeholder in each format.

### Plural forms

*This string needs 3 plural forms, the translation has 2* — or, for a
translation given as a single text, *This string needs 3 plural forms*.

The expected number is the file's own: the Qt `numerusform` count or the
PO `nplurals`, otherwise the target language's CLDR plural categories.

### Broken message syntax

*The translation has unbalanced braces or a broken placeholder: …*

The translation is not valid ICU MessageFormat — usually a stray `{` or
`}`.

### Empty but done

*Marked as done but the translation is empty*

A string (or one plural form) is marked done but has no text.

### Markup

*Formatting tags do not match the original: missing &lt;b&gt;, &lt;/b&gt;; unexpected &lt;strong&gt;*

When the original contains HTML / Qt rich-text tags, every translation
form must contain the same tags, each as often, with the same
attributes. Tag names are compared case-insensitively, `<br>` equals
`<br/>`, attribute values must be identical (a changed link target is a
problem). Only known tag names count (`b`, `i`, `u`, `s`, `a`, `br`,
`p`, `span`, `div`, `font`, `em`, `strong`, `code`, `sub`, `sup`, `img`,
headings, lists, tables and similar), so literal text such as
`<Plugin not loaded>` or `<empty>` is not mistaken for markup. When the
original has no tags, the check does not run.

### Protected term

*"ParamsManager" must stay unchanged* — or *"ParamsManager" appears 2
time(s) in the original and 1 in the translation*.

Every glossary term marked *Keep unchanged* that occurs in the original
must occur equally often in each translation form: as a whole word (not
glued to other letters or digits), with the same spelling and case. See
[Keep terms unchanged with a glossary](../tutorials/glossary.md).

### Unusable model answer

*The model's answer could not be used: …*

The translation source returned something that could not be read as a
translation.

## Warnings

### Shortcut marker

*Keyboard shortcut marker (&) appears 1 time(s) in the original and 0 in
the translation*

The number of `&` shortcut markers differs (`&&`, a literal ampersand,
does not count). Counted over all plural forms together.

### Length

*The translation is much longer than the original (52 vs 30 characters)
and may not fit*

The translation is longer than the original by more than the language's
length factor (see `length_warn_ratio` in
[The settings file](settings-file.md#localesid)). Originals shorter than
8 characters are not checked. For plural strings, the longest form is
compared with the longest form of the original.

### Agreement

*"der" before %1 may not agree with every value it can take*

An article placed directly before a placeholder may not fit every word
the application inserts there (*der {0}* when `{0}` can be a feminine
noun). The check looks for German and Spanish articles and contractions
(`der`, `die`, `das`, `dem`, `zum`, `im`, `el`, `la`, `un`, `del`, `al`,
…) followed by a space and a placeholder. It runs for Latin-script
languages whose register is formal or informal — German, Spanish,
French, Italian and others — so a French *la {0}* is flagged too. Worth a
look; often fine.

### Glossary translation

*Glossary: "layer" should be translated as "Ebene"*

A glossary term with a fixed translation occurs in the original, but its
translation does not occur in the translation. Matching ignores case and
lets each word of the fixed translation end differently in its last two
letters when it is longer than four letters, so inflected forms pass:
*súbor* is found in *súboru*, *Ebene* in *Ebenen*. See
[Apply a glossary to finished translations](../tutorials/glossary.md#apply-a-glossary-to-finished-translations).

### Chinese punctuation

Full-width punctuation in Chinese translations is recorded by the
command-line `gate` as an informational finding; the app does not show
it.

## Running the checks without translating

```sh
harness gate --locale de_DE app_de.ts
```

prints every finding of every translation in the file and exits non-zero
when there is an error. See [`harness gate`](cli.md#gate).
