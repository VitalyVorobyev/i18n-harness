# The glossary file

The glossary is a TOML file, by default `glossary.toml` next to
`i18n-harness.toml`, referenced from the settings file:

```toml
[glossary]
path = "glossary.toml"
```

The app's **Settings → Glossary** tab edits it; you can also edit it by
hand. The app writes terms in alphabetical order.

## Example

```toml
[meta]
schema_version = 1

[[term]]
source = "ParamsManager"
do_not_translate = true
notes = "Class name"

[[term]]
source = "layer"
do_not_translate = false
notes = "Image layer, not a geological one"

[term.translations]
de = "Ebene"
sk_SK = "vrstva"

[locale.es_ES]
register = "informal"
```

## `[meta]`

| Key | Type | Meaning |
|---|---|---|
| `schema_version` | integer | Required. Currently `1`. |

## `[[term]]`

| Key | Type | Meaning |
|---|---|---|
| `source` | string | The term as it appears in the original text. Unique within the file; case-sensitive. |
| `do_not_translate` | boolean | `true`: the term must stay unchanged in every language (*Keep unchanged* in the app). |
| `notes` | string, optional | For people. Shown in the app; not sent to the model. |
| `[term.translations]` | table | Fixed translation per locale id. A term without translations and with `do_not_translate = false` loads with a warning. |

A fixed translation is looked up under the file's locale id (`de_DE`) and
then under its language (`de`), so one entry under `de` serves `de_DE`,
`de_AT` and `de_CH` files.

## `[locale.<id>]`

Optional per-language overrides that take precedence over the settings
file's `[locales.<id>]`:

| Key | Type | Meaning |
|---|---|---|
| `register` | `"formal"`, `"informal"`, `"neutral"` | How to address the user. |
| `variant` | string | Variant tag. Recorded; not used by the current translation path. |

## How each entry is used

See [Keep terms unchanged with a glossary](../tutorials/glossary.md#how-the-glossary-is-used)
for what is told to the model and what is checked. In short: *keep
unchanged* terms are both told to chat models and enforced by the
[protected term check](checks.md#protected-term); fixed translations are
only told to chat models.
