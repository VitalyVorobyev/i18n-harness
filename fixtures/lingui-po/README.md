# Lingui PO fixtures

Catalogs in the shape `lingui extract` writes with the PO formatter: the
`X-Generator: @lingui/cli` header, `#:` origins, ICU messages inside
msgid/msgstr (`{name}`, an inline `{count, plural, …}`), a literal `%`, an
explicit-id entry (`#. js-lingui-explicit-id`) and an obsolete `#~` entry.
Both files must round-trip byte-identically.
