# Translate ICU message JSON

Web applications using react-intl / FormatJS, i18next (with its ICU
plugin) or Lingui keep one JSON file per language. Each value is an ICU
message: `Hello, {name}!`, `{count, plural, one {# photo} other {# photos}}`.

Unlike Qt and PO files, such a file holds **only one language**: there is
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
2. Open `json-demo` in the app. The files `en` (under *English*) and `fr`
   (under *French*) both have a full green bar: the French file still
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
| `app.photos` | `{count, pluriel, un {# photo} les autres {# photos}}` | **clean — but broken** |
| `actions.delete` | `Supprimer` | clean |
| `actions.share` | `Partager avec {personne}` | red: missing `{person}` |

The two red ones are held back on save: `fr.json` keeps `Hello, {name}!`
there until you type `Bonjour, {name} !` and `Partager avec {person}`.

The plural message shows a current gap: the whole ICU message is sent to
the model as one text, and the model translated the keywords `plural`,
`one` and `other`, which must stay English. The checks do not yet catch
this, so the broken message would be saved. **Check every message with
`plural`, `select` or `selectordinal` by hand**: only the text inside the
inner braces may change —
`{count, plural, one {# photo} other {# photos}}`.

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
