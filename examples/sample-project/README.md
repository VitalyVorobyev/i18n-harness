# i18n-harness sample

A small example to try i18n-harness on: the translation files of an
imaginary application, in German, Spanish and Chinese (Qt Linguist `.ts`
files under `translations/`), and a glossary with three terms.

Most strings are not translated yet. A few things to look at:

- **Translate** fills them in with the model you choose.
- *%n unread message(s)* has plural forms: German has two, Chinese one.
- *Open %1 from %2* has placeholders the translation must keep; delete
  one in the editor and the string is marked as a problem.
- **Glossary** in the toolbar lists the terms: *Cancel*, *Save* and
  *Theme* must be translated the same way everywhere.
- **Save** writes the translations into the files in `translations/`.

The app keeps its own bookkeeping in a hidden `.i18n-harness` folder next
to this file. To start over, delete this folder; **Try a sample** on the
start screen makes a fresh copy.

The user guide: <https://vitalyvorobyev.github.io/i18n-harness/>
