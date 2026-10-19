# Translate ICU message JSON

Web applications using react-intl / FormatJS or i18next (with its ICU
plugin) keep one JSON file per language. Each value is an ICU message:
`Hello, {name}!`, `{count, plural, one {# photo} other {# photos}}`.

Lingui projects are different: their PO and JSON catalogs keep the
original next to each translation, so they are translated like a
[gettext PO file](po.md), without the copy step below. See
[Lingui catalogs](../reference/formats.md#lingui) for how they are
recognised. (Lingui's *minimal* JSON style, a plain map of ids to
translations, is read as ICU message JSON and follows this tutorial.)

Unlike Qt, PO and Lingui files, an ICU message JSON file holds **only
one language**: there is
no separate "original" text next to each translation. The way to
translate one with i18n-harness is therefore to start the new language
as a **copy of the English file** and tell the app that its texts are
English — the same mechanism as in
[Files pre-filled with another language](pivot.md).

## The example

A folder `json-demo` with `locales/en.json`:

```json
{
  "app.title": "Photo Library",
  "app.greeting": "Hello, {name}!",
  "app.photos": "{count, plural, one {# photo} other {# photos}}",
  "actions.delete": "Delete",
  "actions.share": "Share with {person}"
}
```

> **Nested files.** Keys may be nested objects (`{"app": {"title": …}}`);
> the string's id is then the dotted path, `app.title`. However, a file is
> currently only recognised when **at least one top-level value is a
> string**. A file whose top level consists only of nested objects is not
> found, and cannot be added by hand either. Use flat dotted keys, as in
> this example, until that is fixed.

## Steps

1. Copy `locales/en.json` to `locales/fr.json`.
2. Open `json-demo` in the app. The files `en` (under *English · ICU
   MessageFormat JSON*) and `fr` (under *French · ICU MessageFormat
   JSON*) both have a full green bar: the French file still
   contains English, and the app cannot know that yet.
3. Click **⋯** next to `fr.json`, choose **Language of the existing
   text…**, pick **English** and click **Apply**. All five French strings
   are now **To do**, with the English text in the **From English**
   column.
4. Select `fr.json`, pick a model and click **Translate 5 strings**.
5. Review under **Check** and click **Done** for each string.
6. Click **Save**.
7. Once every string is **Done**, open **Language of the existing text…**
   for `fr.json` again and choose *French (already translated)*. From now
   on the file's texts count as French.

Do not set a language of the existing text on `en.json`.

## Expected result

A run with a dedicated translation model gave:

| Id | Suggestion | Check result |
|---|---|---|
| `app.title` | `Bibliothèque de photos` | clean |
| `app.greeting` | `Bonjour, {nom}!` | red: *Placeholders do not match the original: missing {name}; unexpected {nom}* |
| `app.photos` | `{count, pluriel, un {# photo} les autres {# photos}}` | red: *Placeholders do not match the original: missing {count, plural}; unexpected {count, pluriel}* |
| `actions.delete` | `Supprimer` | clean |
| `actions.share` | `Partager avec {personne}` | red: missing `{person}` |

The three red ones are held back on save: `fr.json` keeps the English
text there until you correct them — `Bonjour, {name} !`,
`Partager avec {person}`, and the plural message below.

In the plural message the model translated the keywords `plural`, `one`
and `other`, which must stay English: the application would no longer
recognise the message. The check catches a changed `plural`, `select` or
`selectordinal`. Only the text inside the inner braces may change:

```text
{count, plural, one {# photo} other {# photos}}
```

The category names (`one`, `few`, `other`, …) are not checked, so still
look at every plural or select message yourself.

## Notes

- Placeholders are ICU arguments: `{name}`, `{0}`, `{count, number}`,
  `{count, plural, …}`. Argument names must stay exactly as they are.
- The **Original** column shows the text the file held when it was
  opened. After step 7 that is the French text: the file has nothing
  else to show.
- Key order, indentation and escaping of the file are kept; only changed
  values are rewritten.
- On the command line, `harness translate-folder --pivot en` would mark
  **every** file in the folder, including `en.json`, as English input.
  Put only the files to translate in the folder you pass, or set the
  language in the app.
