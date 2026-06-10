//! Focused ICU MessageFormat subparser.
//!
//! Per `docs/implementation_plan.md` §13 decision #2, the gate ships a
//! **deliberately narrow** ICU parser: it extracts placeholder tokens and
//! plural/select arity but does not interpret number/date/time skeletons,
//! does not evaluate `{=N}` exact matches beyond recognizing them, and does
//! not render messages. The full grammar belongs to `icu_messageformat`-class
//! crates we have chosen not to depend on.
//!
//! # What this parser recognizes
//!
//! - **Plain text** with `'` as the ICU literal-escape marker (a single
//!   quote starts/ends an escaped region; a doubled `''` is a literal quote).
//! - **Placeholders.** `{name}` where `name` is one or more ASCII letters,
//!   digits, or `_`, optionally followed by `,` and a type/style segment
//!   the parser only checks for arity (plural/select), otherwise skips.
//! - **`{count, plural, …}`** and **`{kind, select, …}`** with brace-balanced
//!   arms. The parser records the set of arm selectors (e.g. `one`, `other`,
//!   `male`, `=0`) and the body strings, so a future arity check can compare
//!   them against the CLDR table.
//! - **Nested constructs.** Plural/select bodies may themselves contain
//!   placeholders and (recursively) plural/select; the parser walks them.
//!
//! # What this parser does NOT recognize
//!
//! - `{n, number, currency}`, `{d, date, short}`, and similar formatted
//!   placeholders. The parser treats the type segment (`number`, `date`,
//!   `time`, `spellout`, `ordinal`, `duration`) as opaque — it records the
//!   placeholder name and moves past the closing brace without parsing the
//!   style. Hardness lives elsewhere; the gate's question is "is
//!   this placeholder present" and "do the plural arms cover the locale's
//!   categories", not "is the format style well-formed".
//!
//! # Error model
//!
//! [`parse`] returns either a [`Message`] or a [`ParseError`] carrying the
//! byte offset and a human-readable cause. The gate surfaces this in a
//! [`crate::IcuParseDetail`] finding.

use std::fmt;

/// One placeholder occurrence in an ICU message (or one of its arms).
///
/// Captured by [`parse`] in left-to-right document order. The same name
/// appearing twice produces two entries — multiset semantics matter (see
/// [`crate::PlaceholderMismatchDetail`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct PlaceholderRef {
    /// Token as it should appear in the multiset, including the `{count}`
    /// form for plural arms. We use the **whole token** (e.g. `{0}`,
    /// `{name}`, `{count}`) so the multiset key is unambiguous across name
    /// vs positional placeholders.
    pub token: String,
}

/// Kind of a `{name, plural|select, …}` selector head.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectorKind {
    Plural,
    Select,
    /// `selectordinal` — same arity rules as `plural` for our purposes
    /// (CLDR categories on the ordinal side), but recorded distinctly so
    /// the gate can route to the right CLDR table when one lands.
    SelectOrdinal,
}

/// A single `{… , plural|select, arms}` construct.
///
/// Captured so a future arity check on **inline** ICU plurals (ICU-JSON
/// catalogs that store the whole plural inside one string) can compare the
/// arm set against the locale's CLDR table. Qt's adapter explodes plurals
/// into `Target::Plural` forms before the gate sees them, so the gate's
/// arity check currently runs on `Target::Plural::forms.len()` not on
/// this struct.
#[derive(Debug, Clone)]
#[allow(dead_code)] // wired in for ICU-JSON inline plurals
pub(crate) struct Selector {
    /// Name of the selector variable. For CLDR plurals this is the
    /// argument whose value the plural categories switch on (typically
    /// `count`).
    pub name: String,
    /// Whether this is a plural-like or a free-form select.
    pub kind: SelectorKind,
    /// The arm selectors present, in document order. For plural these are
    /// things like `=0`, `zero`, `one`, `two`, `few`, `many`, `other`. For
    /// select they are application-defined.
    pub arms: Vec<String>,
}

/// Parsed message.
///
/// Carries enough information for the gate's multiset and arity checks; no
/// AST is built beyond that.
#[derive(Debug, Clone, Default)]
pub(crate) struct Message {
    /// Every placeholder occurrence in the message, in left-to-right order,
    /// across all selector arms. Multiset semantics — duplicates are kept.
    pub placeholders: Vec<PlaceholderRef>,
    /// Every selector construct encountered, in left-to-right order. A
    /// nested plural inside a select arm produces two entries.
    pub selectors: Vec<Selector>,
}

/// ICU parse failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParseError {
    /// Byte offset within the parsed message where the error was detected.
    pub byte_offset: usize,
    /// Human-readable cause.
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ICU parse error at byte {}: {}",
            self.byte_offset, self.message
        )
    }
}

impl std::error::Error for ParseError {}

/// Parse an ICU-form message.
///
/// Returns [`Message`] on success; [`ParseError`] on the first syntactic
/// problem. The parser is one-pass and does not attempt recovery — once a
/// brace is unbalanced we cannot meaningfully continue.
pub(crate) fn parse(input: &str) -> Result<Message, ParseError> {
    let mut p = Parser::new(input);
    let mut msg = Message::default();
    p.parse_message_into(&mut msg, /*inside_arm=*/ false)?;
    if p.pos != p.bytes.len() {
        // Excess input is only possible if a `}` was seen at the top level.
        return Err(ParseError {
            byte_offset: p.pos,
            message: format!("unexpected '{}' at top level", p.bytes[p.pos] as char),
        });
    }
    Ok(msg)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(s: &'a str) -> Self {
        Self {
            bytes: s.as_bytes(),
            pos: 0,
        }
    }

    fn parse_message_into(
        &mut self,
        msg: &mut Message,
        inside_arm: bool,
    ) -> Result<(), ParseError> {
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b'{' => {
                    self.parse_brace(msg)?;
                }
                b'}' => {
                    if inside_arm {
                        return Ok(());
                    }
                    return Err(ParseError {
                        byte_offset: self.pos,
                        message: "unmatched '}' (no opening '{')".to_owned(),
                    });
                }
                b'\'' => {
                    self.skip_apostrophe();
                }
                _ => {
                    self.pos += 1;
                }
            }
        }
        Ok(())
    }

    /// ICU treats `'` specially: `''` is a literal `'`; `'…'` quotes a span
    /// containing `{` or `}` so they are read as literals. We do not need
    /// to *render* anything, but we must skip the right number of bytes so
    /// `{` and `}` inside a quoted span do not look like syntax.
    fn skip_apostrophe(&mut self) {
        debug_assert_eq!(self.bytes[self.pos], b'\'');
        let next = self.bytes.get(self.pos + 1).copied();
        match next {
            Some(b'\'') => {
                // ''  → literal apostrophe; consume two bytes, no quoting.
                self.pos += 2;
            }
            Some(c) if c == b'{' || c == b'}' || c == b'#' || c == b'|' => {
                // Begin a quoted span. Per ICU, the span runs until the next
                // unpaired apostrophe.
                self.pos += 1; // consume opening '
                while self.pos < self.bytes.len() {
                    if self.bytes[self.pos] == b'\'' {
                        // ''  inside the span = literal '
                        if self.bytes.get(self.pos + 1) == Some(&b'\'') {
                            self.pos += 2;
                            continue;
                        }
                        // Single ' ends the span.
                        self.pos += 1;
                        break;
                    }
                    self.pos += 1;
                }
            }
            _ => {
                // Stray ' that does not introduce a quoted span. ICU treats
                // it as a literal; we just consume it.
                self.pos += 1;
            }
        }
    }

    fn parse_brace(&mut self, msg: &mut Message) -> Result<(), ParseError> {
        let start = self.pos;
        debug_assert_eq!(self.bytes[self.pos], b'{');
        self.pos += 1;
        self.skip_ws();
        let name = self.read_name().ok_or_else(|| ParseError {
            byte_offset: self.pos,
            message: "expected placeholder name".to_owned(),
        })?;
        self.skip_ws();
        // Capture the placeholder token (including the {…}) for the multiset.
        // We choose the *normalized* token shape `{name}` rather than the
        // raw span so equal-modulo-whitespace placeholders compare equal.
        match self.bytes.get(self.pos).copied() {
            Some(b'}') => {
                self.pos += 1;
                msg.placeholders.push(PlaceholderRef {
                    token: format!("{{{name}}}"),
                });
                Ok(())
            }
            Some(b',') => {
                self.pos += 1;
                self.skip_ws();
                let kind_name = self.read_name().ok_or_else(|| ParseError {
                    byte_offset: self.pos,
                    message: "expected selector type after ','".to_owned(),
                })?;
                self.skip_ws();
                // We record the placeholder occurrence regardless of the
                // selector type (CLDR plural-count argument is still a
                // placeholder in the multiset).
                msg.placeholders.push(PlaceholderRef {
                    token: format!("{{{name}}}"),
                });
                match kind_name.as_str() {
                    "plural" | "selectordinal" | "select" => {
                        let kind = match kind_name.as_str() {
                            "plural" => SelectorKind::Plural,
                            "selectordinal" => SelectorKind::SelectOrdinal,
                            _ => SelectorKind::Select,
                        };
                        // The next byte must be ',' (introducing the arms).
                        match self.bytes.get(self.pos).copied() {
                            Some(b',') => {
                                self.pos += 1;
                            }
                            _ => {
                                return Err(ParseError {
                                    byte_offset: self.pos,
                                    message: format!(
                                        "expected ',' after '{kind_name}' selector type"
                                    ),
                                });
                            }
                        }
                        let arms = self.parse_selector_arms(msg)?;
                        msg.selectors.push(Selector { name, kind, arms });
                        // After arms we must be at '}'.
                        match self.bytes.get(self.pos).copied() {
                            Some(b'}') => {
                                self.pos += 1;
                                Ok(())
                            }
                            Some(c) => Err(ParseError {
                                byte_offset: self.pos,
                                message: format!(
                                    "expected '}}' to close selector, found '{}'",
                                    c as char
                                ),
                            }),
                            None => Err(ParseError {
                                byte_offset: self.pos,
                                message: "unexpected end of input while reading selector"
                                    .to_owned(),
                            }),
                        }
                    }
                    _ => {
                        // Opaque type — number/date/time/spellout/ordinal/
                        // duration or an unknown one. Skip to the matching
                        // closing brace without parsing the style.
                        self.skip_to_matching_brace(start)?;
                        Ok(())
                    }
                }
            }
            Some(c) => Err(ParseError {
                byte_offset: self.pos,
                message: format!(
                    "expected '}}' or ',' after placeholder name, found '{}'",
                    c as char
                ),
            }),
            None => Err(ParseError {
                byte_offset: self.pos,
                message: "unexpected end of input inside placeholder".to_owned(),
            }),
        }
    }

    fn parse_selector_arms(&mut self, msg: &mut Message) -> Result<Vec<String>, ParseError> {
        let mut arms = Vec::new();
        loop {
            self.skip_ws();
            match self.bytes.get(self.pos).copied() {
                Some(b'}') => {
                    if arms.is_empty() {
                        return Err(ParseError {
                            byte_offset: self.pos,
                            message: "plural/select needs at least one arm".to_owned(),
                        });
                    }
                    return Ok(arms);
                }
                None => {
                    return Err(ParseError {
                        byte_offset: self.pos,
                        message: "unexpected end of input inside selector".to_owned(),
                    });
                }
                _ => {}
            }
            let selector = self.read_arm_selector().ok_or_else(|| ParseError {
                byte_offset: self.pos,
                message: "expected arm selector (e.g. 'one', 'other', '=0')".to_owned(),
            })?;
            self.skip_ws();
            match self.bytes.get(self.pos).copied() {
                Some(b'{') => {
                    self.pos += 1;
                    self.parse_message_into(msg, /*inside_arm=*/ true)?;
                    match self.bytes.get(self.pos).copied() {
                        Some(b'}') => {
                            self.pos += 1;
                        }
                        _ => {
                            return Err(ParseError {
                                byte_offset: self.pos,
                                message: "expected '}' to close arm body".to_owned(),
                            });
                        }
                    }
                }
                Some(c) => {
                    return Err(ParseError {
                        byte_offset: self.pos,
                        message: format!(
                            "expected '{{' to start arm body for '{selector}', found '{}'",
                            c as char
                        ),
                    });
                }
                None => {
                    return Err(ParseError {
                        byte_offset: self.pos,
                        message: "unexpected end of input before arm body".to_owned(),
                    });
                }
            }
            arms.push(selector);
        }
    }

    fn read_arm_selector(&mut self) -> Option<String> {
        let start = self.pos;
        if self.bytes.get(self.pos) == Some(&b'=') {
            // `=N` exact match. Consume `=` and read digits.
            self.pos += 1;
            let digits_start = self.pos;
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
            if self.pos == digits_start {
                // `=` with no digits — restore and fail.
                self.pos = start;
                return None;
            }
            return Some(
                std::str::from_utf8(&self.bytes[start..self.pos])
                    .expect("ascii in selector")
                    .to_owned(),
            );
        }
        self.read_name()
    }

    fn read_name(&mut self) -> Option<String> {
        let start = self.pos;
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            None
        } else {
            Some(
                std::str::from_utf8(&self.bytes[start..self.pos])
                    .expect("ascii name")
                    .to_owned(),
            )
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// Skip past the matching `}` of the `{` at `start`. Honors apostrophe
    /// quoting and nested braces. Used for opaque formatted placeholders
    /// where we do not parse the style — we just want to be past them.
    fn skip_to_matching_brace(&mut self, start: usize) -> Result<(), ParseError> {
        let mut depth = 1u32; // we are inside the {…} that started at `start`
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b'\'' => self.skip_apostrophe(),
                b'{' => {
                    depth += 1;
                    self.pos += 1;
                }
                b'}' => {
                    depth -= 1;
                    self.pos += 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                _ => self.pos += 1,
            }
        }
        Err(ParseError {
            byte_offset: start,
            message: "unterminated '{' in formatted placeholder".to_owned(),
        })
    }
}

/// Sort the placeholder tokens into a canonical multiset representation.
///
/// We sort by the token string itself, which is enough to make the result
/// `Eq`-comparable across two messages. Multisets keep duplicates.
pub(crate) fn placeholder_multiset(msg: &Message) -> Vec<String> {
    let mut tokens: Vec<String> = msg.placeholders.iter().map(|p| p.token.clone()).collect();
    tokens.sort();
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholders(s: &str) -> Vec<String> {
        let msg = parse(s).expect("parse");
        placeholder_multiset(&msg)
    }

    #[test]
    fn empty_message_has_no_placeholders() {
        let m = parse("").expect("parse empty");
        assert!(m.placeholders.is_empty());
        assert!(m.selectors.is_empty());
    }

    #[test]
    fn plain_text_has_no_placeholders() {
        let m = parse("Hello, world!").expect("parse");
        assert!(m.placeholders.is_empty());
    }

    #[test]
    fn positional_placeholders_are_captured() {
        assert_eq!(placeholders("Hello {0} and {1}"), vec!["{0}", "{1}"]);
    }

    #[test]
    fn duplicate_placeholders_are_multiset() {
        assert_eq!(placeholders("{0} and {0}"), vec!["{0}", "{0}"]);
    }

    #[test]
    fn named_placeholders_are_captured() {
        assert_eq!(placeholders("Hello, {name}!"), vec!["{name}".to_owned()]);
    }

    #[test]
    fn plural_records_arms_and_count_placeholder() {
        let m = parse("{count, plural, one {# msg} other {# msgs}}").expect("parse");
        assert_eq!(m.selectors.len(), 1);
        let sel = &m.selectors[0];
        assert_eq!(sel.kind, SelectorKind::Plural);
        assert_eq!(sel.name, "count");
        assert_eq!(sel.arms, vec!["one", "other"]);
        let tokens: Vec<_> = m.placeholders.iter().map(|p| p.token.as_str()).collect();
        assert!(tokens.contains(&"{count}"));
    }

    #[test]
    fn select_records_arms() {
        let m = parse("{kind, select, male {he} female {she} other {they}}").expect("parse");
        assert_eq!(m.selectors.len(), 1);
        assert_eq!(m.selectors[0].kind, SelectorKind::Select);
        assert_eq!(m.selectors[0].arms, vec!["male", "female", "other"]);
    }

    #[test]
    fn exact_arm_selectors_are_recorded() {
        let m = parse("{n, plural, =0 {none} =1 {one} other {many}}").expect("parse");
        assert_eq!(m.selectors[0].arms, vec!["=0", "=1", "other"]);
    }

    #[test]
    fn nested_plural_inside_select() {
        let m = parse("{kind, select, male {{count, plural, one {a} other {b}}} other {x}}")
            .expect("parse");
        assert_eq!(m.selectors.len(), 2);
        assert_eq!(m.selectors[0].kind, SelectorKind::Plural);
        assert_eq!(m.selectors[1].kind, SelectorKind::Select);
        // {count} occurs once inside the nested plural; {kind} once at the outer.
        let toks: Vec<_> = m.placeholders.iter().map(|p| p.token.as_str()).collect();
        assert!(toks.contains(&"{count}"));
        assert!(toks.contains(&"{kind}"));
    }

    #[test]
    fn escaped_brace_inside_apostrophe_is_literal() {
        // `'{'` is the ICU way to write a literal `{`.
        let m = parse("Use '{' carefully").expect("parse");
        assert!(m.placeholders.is_empty(), "{:?}", m.placeholders);
    }

    #[test]
    fn double_apostrophe_is_literal_quote() {
        let m = parse("It''s a {name}").expect("parse");
        assert_eq!(m.placeholders.len(), 1);
        assert_eq!(m.placeholders[0].token, "{name}");
    }

    #[test]
    fn opaque_formatted_placeholder_records_name_only() {
        let m = parse("Price: {amount, number, currency}").expect("parse");
        assert_eq!(m.placeholders.len(), 1);
        assert_eq!(m.placeholders[0].token, "{amount}");
        // No selector recorded — `number` is opaque to us.
        assert!(m.selectors.is_empty());
    }

    #[test]
    fn date_placeholder_is_opaque() {
        let m = parse("on {when, date, short}").expect("parse");
        assert_eq!(m.placeholders[0].token, "{when}");
        assert!(m.selectors.is_empty());
    }

    #[test]
    fn unmatched_brace_errors() {
        let err = parse("Hello {0").unwrap_err();
        assert!(err.message.contains("end of input") || err.message.contains("'}'"));
    }

    #[test]
    fn unmatched_closing_brace_errors() {
        let err = parse("Hello }").unwrap_err();
        assert!(err.message.contains("unmatched"));
    }

    #[test]
    fn empty_plural_arms_errors() {
        let err = parse("{count, plural, }").unwrap_err();
        assert!(err.message.contains("at least one arm"));
    }

    #[test]
    fn missing_arm_body_errors() {
        let err = parse("{count, plural, one }").unwrap_err();
        assert!(err.message.contains("arm body"));
    }

    #[test]
    fn bad_placeholder_name_errors() {
        let err = parse("Hello {!}").unwrap_err();
        assert!(err.message.contains("placeholder name"));
    }
}
