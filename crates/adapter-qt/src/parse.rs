//! `.ts` → [`Catalog`] parser. Hand-managed `quick-xml` reader that records
//! the byte ranges of every editable region so [`crate::apply`] can rewrite
//! by byte-splice rather than reserializing.

use std::fs;
use std::path::Path;

use i18n_harness_core::{
    Placeholder, PlaceholderKind, Provenance, Target, Unit, UnitId, UnitState, compute_source_hash,
};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::QName;

use crate::catalog::{Catalog, EditPoint, TypeAttr};
use crate::error::ExtractError;
use crate::placeholder::to_icu;

/// Read a Qt `.ts` file into a [`Catalog`].
///
/// See the crate-level docs for the byte-stability contract.
///
/// # Errors
///
/// See [`ExtractError`] for the failure modes and what the caller should do.
pub fn extract(path: &Path) -> Result<Catalog, ExtractError> {
    let bytes = fs::read(path).map_err(|source| ExtractError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    let mut state = ParseState::new();
    state.run(&bytes).map_err(|e| match e {
        ParseError::Xml(source) => ExtractError::Xml {
            path: path.to_path_buf(),
            source,
        },
        ParseError::NotTsFile => ExtractError::NotTsFile {
            path: path.to_path_buf(),
        },
    })?;

    Ok(Catalog {
        source_path: path.to_path_buf(),
        source_bytes: bytes,
        language: state.language,
        edit_points: state.edit_points,
        units: state.units,
    })
}

#[derive(Debug)]
enum ParseError {
    Xml(quick_xml::Error),
    NotTsFile,
}

impl From<quick_xml::Error> for ParseError {
    fn from(e: quick_xml::Error) -> Self {
        Self::Xml(e)
    }
}

impl From<quick_xml::events::attributes::AttrError> for ParseError {
    fn from(e: quick_xml::events::attributes::AttrError) -> Self {
        Self::Xml(quick_xml::Error::InvalidAttr(e))
    }
}

#[derive(Default)]
struct ParseState {
    saw_ts_root: bool,
    language: Option<String>,
    current_context: Option<String>,
    units: Vec<Unit>,
    edit_points: Vec<EditPoint>,
}

impl ParseState {
    fn new() -> Self {
        Self::default()
    }

    fn run(&mut self, bytes: &[u8]) -> Result<(), ParseError> {
        let mut reader = Reader::from_reader(bytes);
        let cfg = reader.config_mut();
        cfg.trim_text(false);
        cfg.expand_empty_elements = false;
        cfg.check_end_names = true;

        let mut buf = Vec::new();
        loop {
            // Position *before* the event starts.
            let event_start = reader.buffer_position() as usize;
            match reader.read_event_into(&mut buf)? {
                Event::Eof => break,
                Event::Start(e) => {
                    let event_end = reader.buffer_position() as usize;
                    self.handle_start(bytes, &e, event_start, event_end, &mut reader)?;
                }
                Event::Empty(e) => {
                    // Self-closing; nothing inside. We don't currently extract
                    // location data into the Unit (we keep it byte-stable via
                    // the catalog's original bytes), but a future provenance
                    // enhancement would parse <location ... /> here.
                    self.handle_empty(&e)?;
                }
                Event::Text(_) | Event::CData(_) | Event::Comment(_) => {
                    // Document-level text / comments outside <translation> /
                    // <numerusform> need no special handling — byte stability
                    // handles them.
                }
                Event::End(_) => {
                    // Likewise.
                }
                Event::Decl(_) | Event::DocType(_) | Event::PI(_) => {
                    // Preserved via byte stability.
                }
                Event::GeneralRef(_) => {
                    // Unexpanded entity reference; nothing to track at the
                    // structural level.
                }
            }
            buf.clear();
        }

        if !self.saw_ts_root {
            return Err(ParseError::NotTsFile);
        }
        Ok(())
    }

    fn handle_empty(&mut self, _e: &BytesStart<'_>) -> Result<(), ParseError> {
        Ok(())
    }

    fn handle_start(
        &mut self,
        bytes: &[u8],
        e: &BytesStart<'_>,
        event_start: usize,
        event_end: usize,
        reader: &mut Reader<&[u8]>,
    ) -> Result<(), ParseError> {
        match e.name() {
            QName(b"TS") => {
                self.saw_ts_root = true;
                if let Some(v) = attr_value(e, b"language")? {
                    self.language = Some(v);
                }
            }
            QName(b"context") => {
                self.current_context = None;
            }
            QName(b"name") => {
                // The <name> directly under <context> sets the context name.
                // Read text + matching end tag.
                let text = read_text_until_close(reader, b"name")?;
                if self.current_context.is_none() {
                    self.current_context = Some(text);
                }
            }
            QName(b"message") => {
                let numerus = attr_value(e, b"numerus")?
                    .as_deref()
                    .is_some_and(|v| v == "yes");
                let _ = (event_start, event_end); // available for future provenance enhancements.
                self.parse_message(bytes, reader, numerus)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn parse_message(
        &mut self,
        bytes: &[u8],
        reader: &mut Reader<&[u8]>,
        numerus: bool,
    ) -> Result<(), ParseError> {
        let mut source_text: Option<String> = None;
        let mut comment: Option<String> = None; // disambiguation
        let mut extracomment: Option<String> = None; // developer comment; hashed but not stored
        let mut translation_open: Option<(usize, usize)> = None; // (start of <, end of >)
        let mut translation_body_start: Option<usize> = None;
        let mut translation_body_end: Option<usize> = None;
        let mut translation_end: Option<usize> = None; // end position of </translation>
        let mut type_attr_info: Option<TypeAttr> = None;
        let mut state_from_attr = UnitState::Finished;
        let mut numerus_bodies: Vec<(usize, usize)> = Vec::new();
        let mut numerus_originals: Vec<Vec<u8>> = Vec::new();
        let mut translation_body_text_singular: Option<String> = None;

        let mut buf = Vec::new();
        loop {
            let event_start = reader.buffer_position() as usize;
            match reader.read_event_into(&mut buf)? {
                Event::Eof => break,
                Event::Empty(e) if e.name() == QName(b"translation") => {
                    // Self-closing <translation/> — empty body, but treat as
                    // existing translation element with empty content.
                    let event_end = reader.buffer_position() as usize;
                    translation_open = Some((event_start, event_end));
                    translation_body_start = Some(event_end);
                    translation_body_end = Some(event_end);
                    translation_end = Some(event_end);
                    type_attr_info = parse_type_attr(bytes, event_start, event_end, &e)?;
                    state_from_attr = state_from_type_attr(type_attr_info.as_ref());
                }
                Event::Start(e) => match e.name() {
                    QName(b"source") => {
                        let txt = read_text_until_close(reader, b"source")?;
                        source_text = Some(txt);
                    }
                    QName(b"comment") => {
                        let txt = read_text_until_close(reader, b"comment")?;
                        comment = Some(txt);
                    }
                    QName(b"extracomment") => {
                        let txt = read_text_until_close(reader, b"extracomment")?;
                        // Multiple <extracomment> blocks concatenate with \n in source order.
                        extracomment = Some(match extracomment.take() {
                            Some(prev) => format!("{prev}\n{txt}"),
                            None => txt,
                        });
                    }
                    QName(b"translation") => {
                        let event_end = reader.buffer_position() as usize;
                        translation_open = Some((event_start, event_end));
                        translation_body_start = Some(event_end);
                        type_attr_info = parse_type_attr(bytes, event_start, event_end, &e)?;
                        state_from_attr = state_from_type_attr(type_attr_info.as_ref());

                        if numerus {
                            // Walk children: each <numerusform>...</numerusform>
                            // (or self-closing) plus surrounding whitespace.
                            parse_translation_children_plural(
                                reader,
                                bytes,
                                &mut numerus_bodies,
                                &mut numerus_originals,
                                &mut translation_body_end,
                                &mut translation_end,
                            )?;
                        } else {
                            parse_translation_children_singular(
                                reader,
                                bytes,
                                &mut translation_body_text_singular,
                                &mut translation_body_end,
                                &mut translation_end,
                            )?;
                        }
                    }
                    _ => {
                        // extracomment, translatorcomment, oldsource,
                        // userdata, etc. — preserved by byte stability;
                        // we still need to skip past them.
                        skip_to_close(reader, e.name())?;
                    }
                },
                Event::Empty(_) | Event::Text(_) | Event::CData(_) | Event::Comment(_) => {
                    // Ignored structurally; byte-preserved.
                }
                Event::End(e) if e.name() == QName(b"message") => {
                    break;
                }
                Event::End(_)
                | Event::Decl(_)
                | Event::DocType(_)
                | Event::PI(_)
                | Event::GeneralRef(_) => {}
            }
            buf.clear();
        }

        // If we didn't find a <translation> element, skip; not a valid unit
        // for round-trip (but valid Qt: <message> with only <source>).
        let Some(translation_open) = translation_open else {
            return Ok(());
        };
        let translation_body_start = translation_body_start.unwrap_or(translation_open.1);
        let translation_body_end = translation_body_end.unwrap_or(translation_body_start);
        let translation_end = translation_end.unwrap_or(translation_body_end);

        let Some(source_text) = source_text else {
            // <message> without <source> is invalid; skip.
            return Ok(());
        };

        // Build the unit id: context::source[::comment]
        let context = self.current_context.as_deref().unwrap_or("");
        let id_str = match &comment {
            Some(c) => format!("{context}::{source_text}::{c}"),
            None => format!("{context}::{source_text}"),
        };

        let icu_source = to_icu(&source_text);
        let placeholders = scan_placeholders(&icu_source);

        let target = if numerus {
            let mut forms = Vec::with_capacity(numerus_originals.len());
            for body in &numerus_originals {
                let txt = std::str::from_utf8(body).unwrap_or("");
                let icu = to_icu(unescape_xml(txt).as_str());
                forms.push(Some(icu));
            }
            // If state is untranslated/unfinished and forms are empty bodies,
            // surface them as None to make "needs filling" obvious.
            let any_nonempty = forms
                .iter()
                .any(|f| f.as_deref().is_some_and(|s| !s.is_empty()));
            if !any_nonempty && state_from_attr != UnitState::Finished {
                Target::Plural {
                    forms: vec![None; numerus_originals.len()],
                }
            } else {
                Target::Plural { forms }
            }
        } else {
            let text = translation_body_text_singular.as_deref().unwrap_or("");
            if text.is_empty() && state_from_attr != UnitState::Finished {
                Target::Singular { text: None }
            } else {
                Target::Singular {
                    text: Some(to_icu(&unescape_xml(text))),
                }
            }
        };

        // Compute source_hash for active units; vanished/obsolete get None
        // because the harness never translates them (§3.3 of the design doc).
        let source_hash = if matches!(state_from_attr, UnitState::Vanished | UnitState::Obsolete) {
            None
        } else {
            Some(compute_source_hash(
                &icu_source,
                comment.as_deref().unwrap_or(""),
                extracomment.as_deref().unwrap_or(""),
                numerus,
            ))
        };

        let unit = Unit {
            id: UnitId::from(id_str.clone()),
            source: icu_source,
            target,
            placeholders,
            plural_arity: numerus.then_some(numerus_originals.len() as u32),
            flags: Default::default(),
            provenance: Provenance::default(),
            state: state_from_attr,
            source_hash,
            review_status: None,
            source_changed_since_review: false,
        };

        let _ = translation_end; // computed by the children walkers but not currently retained.

        self.units.push(unit);
        self.edit_points.push(EditPoint {
            translation_body: (translation_body_start, translation_body_end),
            type_attr: type_attr_info,
            numerus_bodies,
            translation_open_tag: translation_open,
            original_state: state_from_attr,
            original_translation_body: bytes[translation_body_start..translation_body_end].to_vec(),
            original_numerus_bodies: numerus_originals,
        });
        Ok(())
    }
}

fn parse_translation_children_singular(
    reader: &mut Reader<&[u8]>,
    bytes: &[u8],
    body_text: &mut Option<String>,
    body_end: &mut Option<usize>,
    translation_end: &mut Option<usize>,
) -> Result<(), ParseError> {
    let mut buf = Vec::new();
    // The body is whatever appears between the <translation> open we already
    // consumed and the matching </translation>. We accumulate raw text /
    // CDATA into body_text, and remember the byte position at which
    // </translation> starts.
    loop {
        let event_start = reader.buffer_position() as usize;
        match reader.read_event_into(&mut buf)? {
            Event::Eof => break,
            Event::End(e) if e.name() == QName(b"translation") => {
                *body_end = Some(event_start);
                *translation_end = Some(reader.buffer_position() as usize);
                break;
            }
            Event::Text(t) => {
                // Raw bytes inside the body. We keep the *escaped* form
                // because the byte slice will be reused on apply.
                let raw = &bytes[event_start..reader.buffer_position() as usize];
                let acc = body_text.get_or_insert_with(String::new);
                if let Ok(s) = std::str::from_utf8(raw) {
                    acc.push_str(s);
                }
                let _ = t; // suppress unused
            }
            Event::CData(c) => {
                let acc = body_text.get_or_insert_with(String::new);
                // CDATA content carries through verbatim (the bytes are the
                // raw payload between <![CDATA[ and ]]>).
                if let Ok(s) = std::str::from_utf8(c.as_ref()) {
                    acc.push_str(s);
                }
            }
            Event::Comment(_) | Event::PI(_) | Event::Decl(_) | Event::DocType(_) => {}
            Event::Start(_) | Event::Empty(_) => {
                // Unexpected child element inside <translation> body for a
                // non-numerus message. Skip its subtree.
                if let Event::Start(s) = reader.read_event_into(&mut buf)? {
                    skip_to_close(reader, s.name())?;
                }
            }
            Event::GeneralRef(_) => {
                // quick-xml v0.38 splits XML entity references out of the
                // surrounding text. The reader advanced over the literal
                // `&name;` bytes; capture them here so `unescape_xml` can
                // resolve them when this body is consumed downstream.
                let raw = &bytes[event_start..reader.buffer_position() as usize];
                let acc = body_text.get_or_insert_with(String::new);
                if let Ok(s) = std::str::from_utf8(raw) {
                    acc.push_str(s);
                }
            }
            Event::End(_) => {}
        }
        buf.clear();
    }
    Ok(())
}

fn parse_translation_children_plural(
    reader: &mut Reader<&[u8]>,
    bytes: &[u8],
    numerus_bodies: &mut Vec<(usize, usize)>,
    numerus_originals: &mut Vec<Vec<u8>>,
    body_end: &mut Option<usize>,
    translation_end: &mut Option<usize>,
) -> Result<(), ParseError> {
    let mut buf = Vec::new();
    loop {
        let event_start = reader.buffer_position() as usize;
        match reader.read_event_into(&mut buf)? {
            Event::Eof => break,
            Event::End(e) if e.name() == QName(b"translation") => {
                *body_end = Some(event_start);
                *translation_end = Some(reader.buffer_position() as usize);
                break;
            }
            Event::Empty(e) if e.name() == QName(b"numerusform") => {
                // <numerusform/> — empty body.
                let event_end = reader.buffer_position() as usize;
                numerus_bodies.push((event_end, event_end));
                numerus_originals.push(Vec::new());
                let _ = event_start; // unused for empty form
            }
            Event::Start(e) if e.name() == QName(b"numerusform") => {
                let open_end = reader.buffer_position() as usize;
                // Walk to close.
                let body_start = open_end;
                let mut body_close_start = open_end;
                loop {
                    let inner_start = reader.buffer_position() as usize;
                    match reader.read_event_into(&mut buf)? {
                        Event::Eof => break,
                        Event::End(ee) if ee.name() == QName(b"numerusform") => {
                            body_close_start = inner_start;
                            break;
                        }
                        _ => {}
                    }
                }
                numerus_bodies.push((body_start, body_close_start));
                numerus_originals.push(bytes[body_start..body_close_start].to_vec());
            }
            Event::Text(_) | Event::CData(_) | Event::Comment(_) | Event::PI(_) => {}
            Event::Decl(_) | Event::DocType(_) | Event::Empty(_) | Event::Start(_) => {}
            Event::End(_) | Event::GeneralRef(_) => {}
        }
        buf.clear();
    }
    Ok(())
}

fn read_text_until_close(reader: &mut Reader<&[u8]>, tag: &[u8]) -> Result<String, ParseError> {
    let mut buf = Vec::new();
    let mut out = String::new();
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Eof => break,
            Event::End(e) if e.name() == QName(tag) => break,
            Event::Text(t) => {
                let decoded = t.decode().map_err(quick_xml::Error::from)?;
                out.push_str(&decoded);
            }
            Event::CData(c) => {
                if let Ok(s) = std::str::from_utf8(c.as_ref()) {
                    out.push_str(s);
                }
            }
            Event::GeneralRef(r) => {
                // quick-xml v0.38 splits XML entity references (`&amp;`,
                // `&lt;`, …) out of the surrounding text into separate
                // events. Resolve the five predefined entities; any other
                // entity name is reconstructed as `&<name>;` because this
                // adapter does not maintain a custom DTD.
                let name = std::str::from_utf8(r.as_ref()).unwrap_or("");
                match name {
                    "amp" => out.push('&'),
                    "lt" => out.push('<'),
                    "gt" => out.push('>'),
                    "quot" => out.push('"'),
                    "apos" => out.push('\''),
                    _ => {
                        out.push('&');
                        out.push_str(name);
                        out.push(';');
                    }
                }
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(out)
}

fn skip_to_close(reader: &mut Reader<&[u8]>, tag: QName<'_>) -> Result<(), ParseError> {
    let mut buf = Vec::new();
    let mut depth = 1;
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Eof => break,
            Event::Start(e) if e.name() == tag => depth += 1,
            Event::End(e) if e.name() == tag => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(())
}

fn attr_value(e: &BytesStart<'_>, name: &[u8]) -> Result<Option<String>, ParseError> {
    for attr in e.attributes() {
        let attr = attr?;
        if attr.key.as_ref() == name {
            let decoded = attr.unescape_value()?;
            return Ok(Some(decoded.into_owned()));
        }
    }
    Ok(None)
}

fn parse_type_attr(
    bytes: &[u8],
    open_start: usize,
    open_end: usize,
    e: &BytesStart<'_>,
) -> Result<Option<TypeAttr>, ParseError> {
    // We want the byte position of the `type` attribute within the open tag.
    // Strategy: walk attributes, find the one with key==`type`, then locate
    // its byte range inside `bytes[open_start..open_end]` by string search.
    // This is robust because `type=` is a unique-enough token within a
    // `<translation>` open tag in Qt files (the open tag is always short).
    let mut found_value: Option<String> = None;
    for attr in e.attributes() {
        let attr = attr?;
        if attr.key.as_ref() == b"type" {
            let decoded = attr.unescape_value()?;
            found_value = Some(decoded.into_owned());
            break;
        }
    }
    let Some(value) = found_value else {
        return Ok(None);
    };

    let tag_bytes = &bytes[open_start..open_end];
    // Search for ` type=` (with leading space, since attributes always have
    // whitespace separating them from the element name or other attrs in
    // well-formed XML).
    let needle_eq = b"type";
    let mut search_from = 0usize;
    let mut name_start = None;
    while let Some(pos) = find_subslice(&tag_bytes[search_from..], needle_eq) {
        let abs = search_from + pos;
        // Validate it's actually the attribute (preceded by whitespace, not
        // part of another word) and followed by `=`.
        let before_ok = abs == 0 || tag_bytes[abs - 1].is_ascii_whitespace();
        let after_pos = abs + needle_eq.len();
        let after_ok = after_pos < tag_bytes.len()
            && (tag_bytes[after_pos] == b'=' || tag_bytes[after_pos].is_ascii_whitespace());
        if before_ok && after_ok {
            name_start = Some(abs);
            break;
        }
        search_from = abs + 1;
    }
    let Some(name_start_rel) = name_start else {
        // Should not happen if attributes() returned `type`; bail without an
        // edit point for the attr.
        return Ok(None);
    };

    // Find `=`, then the opening quote, then the matching closing quote.
    let mut p = name_start_rel + needle_eq.len();
    while p < tag_bytes.len() && tag_bytes[p] != b'=' {
        p += 1;
    }
    if p >= tag_bytes.len() {
        return Ok(None);
    }
    p += 1; // past `=`
    while p < tag_bytes.len() && tag_bytes[p].is_ascii_whitespace() {
        p += 1;
    }
    if p >= tag_bytes.len() {
        return Ok(None);
    }
    let quote = tag_bytes[p];
    if quote != b'"' && quote != b'\'' {
        return Ok(None);
    }
    let value_start_rel = p + 1;
    let mut value_end_rel = value_start_rel;
    while value_end_rel < tag_bytes.len() && tag_bytes[value_end_rel] != quote {
        value_end_rel += 1;
    }
    if value_end_rel >= tag_bytes.len() {
        return Ok(None);
    }
    let full_end_rel = value_end_rel + 1; // past closing quote

    // Extend the full range backwards through any whitespace separating the
    // attribute from the previous token, so we can remove the attribute
    // cleanly later.
    let mut full_start_rel = name_start_rel;
    while full_start_rel > 0 && tag_bytes[full_start_rel - 1].is_ascii_whitespace() {
        full_start_rel -= 1;
    }

    Ok(Some(TypeAttr {
        full_range: (open_start + full_start_rel, open_start + full_end_rel),
        original_value: value,
    }))
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn state_from_type_attr(attr: Option<&TypeAttr>) -> UnitState {
    match attr.map(|a| a.original_value.as_str()) {
        Some("unfinished") => UnitState::Untranslated,
        Some("vanished") => UnitState::Vanished,
        Some("obsolete") => UnitState::Obsolete,
        Some(_) => UnitState::Finished, // unknown value: treat as finished, do not touch
        None => UnitState::Finished,
    }
}

/// Minimal scanner for placeholders inside an ICU-form string. We only need
/// the placeholder *occurrences* for the gate's multiset check; we do not
/// need to parse arbitrary ICU MessageFormat here. (Full ICU parsing is a
/// gate concern, M1.)
fn scan_placeholders(icu: &str) -> Vec<Placeholder> {
    let bytes = icu.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let start = i;
            let body_start = i + 1;
            let mut j = body_start;
            while j < bytes.len() && bytes[j] != b'}' {
                j += 1;
            }
            if j >= bytes.len() {
                break;
            }
            let body = std::str::from_utf8(&bytes[body_start..j]).unwrap_or("");
            let offset = start as u32;
            if body == "count" {
                out.push(Placeholder::plural_count(offset));
            } else if let Some(rest) = body.strip_prefix('L')
                && !rest.is_empty()
                && rest.bytes().all(|b| b.is_ascii_digit())
            {
                let n: u32 = rest.parse().unwrap_or(0);
                let mut p = Placeholder::locale_aware_int(n, offset);
                p.kind = PlaceholderKind::LocaleAwareInt;
                // The icu_form we generate via the constructor encodes n in
                // 0-indexed form; override to the actual locale-aware token.
                p.icu_form.token = format!("{{L{n}}}");
                out.push(p);
            } else if !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit()) {
                let n: u32 = body.parse().unwrap_or(0);
                out.push(Placeholder::positional(n, offset));
            } else if !body.is_empty() {
                out.push(Placeholder::named(body, offset));
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

fn unescape_xml(s: &str) -> String {
    // Minimal entity decoder for the round-trip path. We resolve the five
    // predefined XML entities (`&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;`)
    // and pass everything else through. Numeric character references are not
    // handled here; Qt-generated `.ts` files almost never use them, and if
    // one appears the gate's ICU-parse check will catch any damage.
    let mut out = String::with_capacity(s.len());
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c != '&' {
            out.push(c);
            continue;
        }
        // Look for `;` within a small window.
        let rest = &s[i + 1..];
        let Some(semi) = rest.find(';') else {
            out.push('&');
            continue;
        };
        let entity = &rest[..semi];
        let resolved: Option<char> = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => None,
        };
        if let Some(ch) = resolved {
            out.push(ch);
            // Advance the iterator past the entity body and the semicolon.
            for _ in 0..entity.chars().count() + 1 {
                chars.next();
            }
        } else {
            out.push('&');
        }
    }
    out
}
