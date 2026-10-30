//! Reading and writing the attribute based XML objects MAP uses.
//!
//! Every MAP XML object (folder listing, messages listing, conversation listing and
//! event report) is a root element holding empty or nearly empty elements whose data
//! is entirely in attributes. Parsing flattens a document into open and close nodes
//! with decoded attributes, which the object parsers walk.
//!
//! Devices are known to send slightly broken documents, such as an unescaped `&` in
//! a subject or trailing null bytes, so parsing is deliberately lenient where it can
//! be without guessing. Only what MAP objects use is supported: elements, attributes,
//! entity and character references. Text, comments, processing instructions,
//! declarations and CDATA sections are skipped.

use crate::error::{Error, Result};
use crate::prelude::*;

/// The XML declaration written at the start of every object.
pub(crate) const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";

/// An element and its attributes, in document order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Element {
    pub(crate) name: String,
    pub(crate) attributes: Vec<(String, String)>,
}

impl Element {
    /// The value of the attribute `name`, if present.
    pub(crate) fn get(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The value of the attribute `name`, treating an empty value as absent.
    pub(crate) fn get_non_empty(&self, name: &str) -> Option<&str> {
        self.get(name).filter(|value| !value.trim().is_empty())
    }

    /// The value of the attribute `name` as an owned string.
    pub(crate) fn string(&self, name: &str) -> Option<String> {
        self.get(name).map(str::to_string)
    }

    /// The value of the attribute `name` parsed, ignoring empty and unparseable values.
    pub(crate) fn parsed<T: core::str::FromStr>(&self, name: &str) -> Option<T> {
        self.get_non_empty(name)
            .and_then(|value| value.trim().parse().ok())
    }

    /// The value of a `yes`/`no` attribute.
    pub(crate) fn yes_no(&self, name: &str) -> Option<bool> {
        self.get(name).and_then(parse_yes_no)
    }
}

/// A node of a flattened document. Empty elements produce an `Open` and a `Close`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Node {
    Open(Element),
    Close(String),
}

/// Flattens an XML document into its element nodes.
pub(crate) fn parse(bytes: &[u8], object: &'static str) -> Result<Vec<Node>> {
    let text = String::from_utf8_lossy(bytes);
    let mut rest = text
        .trim_start_matches('\u{FEFF}')
        .trim_end_matches(|c: char| c == '\0' || c.is_whitespace());

    let mut nodes = Vec::new();

    // Text between tags carries nothing in MAP objects and is skipped.
    while let Some(start) = rest.find('<') {
        let markup = &rest[start + 1..];

        rest = if let Some(comment) = markup.strip_prefix("!--") {
            skip_past(comment, "-->", object)?
        } else if let Some(cdata) = markup.strip_prefix("![CDATA[") {
            skip_past(cdata, "]]>", object)?
        } else if markup.starts_with(['?', '!']) {
            skip_past(markup, ">", object)?
        } else if let Some(end_tag) = markup.strip_prefix('/') {
            let (name, after) = end_tag
                .split_once('>')
                .ok_or_else(|| unterminated(object))?;
            nodes.push(Node::Close(name.trim().to_string()));
            after
        } else {
            let (element, empty, after) = start_tag(markup, object)?;
            let name = empty.then(|| element.name.clone());
            nodes.push(Node::Open(element));
            nodes.extend(name.map(Node::Close));
            after
        };
    }

    Ok(nodes)
}

/// Returns what follows the first `terminator` in `text`.
fn skip_past<'a>(text: &'a str, terminator: &str, object: &'static str) -> Result<&'a str> {
    text.split_once(terminator)
        .map(|(_, after)| after)
        .ok_or_else(|| unterminated(object))
}

fn unterminated(object: &'static str) -> Error {
    Error::malformed(object, "unterminated markup")
}

/// Parses a start tag after its `<`, returning the element, whether it is empty
/// (`/>`) and what follows the tag.
fn start_tag<'a>(text: &'a str, object: &'static str) -> Result<(Element, bool, &'a str)> {
    let name_end = text
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .ok_or_else(|| unterminated(object))?;
    if name_end == 0 {
        return Err(Error::malformed(object, "element without a name"));
    }

    let mut element = Element {
        name: text[..name_end].to_string(),
        attributes: Vec::new(),
    };
    let mut rest = &text[name_end..];

    loop {
        rest = rest.trim_start();

        if let Some(after) = rest.strip_prefix("/>") {
            return Ok((element, true, after));
        } else if let Some(after) = rest.strip_prefix('>') {
            return Ok((element, false, after));
        } else if let Some(after) = rest.strip_prefix('/') {
            // A stray slash, as in `<a / b="c">`.
            rest = after;
            continue;
        } else if rest.is_empty() {
            return Err(unterminated(object));
        }

        let key_end = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '=' | '>' | '/'))
            .unwrap_or(rest.len());
        let key = rest[..key_end].to_string();
        rest = rest[key_end..].trim_start();

        // Attributes without a value, as in HTML, are kept with an empty one.
        let mut value = String::new();
        if let Some(after) = rest.strip_prefix('=') {
            let after = after.trim_start();

            rest = match after.chars().next() {
                Some(quote @ ('"' | '\'')) => {
                    let (raw, after) = after[1..]
                        .split_once(quote)
                        .ok_or_else(|| unterminated(object))?;
                    value = unescape(raw);
                    after
                }
                _ => {
                    // An unquoted value runs to whitespace or the end of the tag.
                    let mut end = after
                        .find(|c: char| c.is_whitespace() || c == '>')
                        .unwrap_or(after.len());
                    if after[end..].starts_with('>') && after[..end].ends_with('/') {
                        end -= 1;
                    }
                    value = unescape(&after[..end]);
                    &after[end..]
                }
            };
        }

        element.attributes.push((key, value));
    }
}

/// Decodes entity and character references, keeping anything that is not a valid
/// reference, such as a bare `&`, as it is.
fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;

    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start + 1..];

        // References are short; looking further would only find unrelated text.
        let decoded = rest
            .bytes()
            .take(12)
            .position(|byte| byte == b';')
            .and_then(|end| Some((reference(&rest[..end])?, end)));

        match decoded {
            Some((decoded, end)) => {
                out.push(decoded);
                rest = &rest[end + 1..];
            }
            None => out.push('&'),
        }
    }

    out.push_str(rest);
    out
}

/// The character an entity or character reference, without `&` and `;`, stands for.
fn reference(name: &str) -> Option<char> {
    match name {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

/// Escapes the characters with a special meaning in attribute values.
fn escape(value: &str, out: &mut String) {
    for c in value.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
}

/// Finds the root element `name` and returns it along with the nodes inside it.
pub(crate) fn root<'a>(
    nodes: &'a [Node],
    name: &str,
    object: &'static str,
) -> Result<(&'a Element, &'a [Node])> {
    match nodes.split_first() {
        Some((Node::Open(element), rest)) if element.name.eq_ignore_ascii_case(name) => {
            Ok((element, rest))
        }
        _ => Err(Error::malformed(
            object,
            format!("expected a <{}> root element", name),
        )),
    }
}

/// Parses `yes` and `no`, ignoring case and surrounding whitespace.
pub(crate) fn parse_yes_no(value: &str) -> Option<bool> {
    let value = value.trim();

    if value.eq_ignore_ascii_case("yes") {
        Some(true)
    } else if value.eq_ignore_ascii_case("no") {
        Some(false)
    } else {
        None
    }
}

/// Formats a boolean as `yes` or `no`.
pub(crate) fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

/// Builds the start tag of an element attribute by attribute.
pub(crate) struct Tag<'a> {
    out: &'a mut String,
}

impl<'a> Tag<'a> {
    /// Starts `<name` on a new line.
    pub(crate) fn open(out: &'a mut String, name: &str) -> Tag<'a> {
        out.push('<');
        out.push_str(name);
        Tag { out }
    }

    /// Adds `name="value"`, escaping the value.
    pub(crate) fn attribute(self, name: &str, value: &str) -> Tag<'a> {
        self.out.push(' ');
        self.out.push_str(name);
        self.out.push_str("=\"");
        escape(value, self.out);
        self.out.push('"');
        self
    }

    /// Adds the attribute when `value` is present.
    pub(crate) fn optional<T: ToString>(self, name: &str, value: Option<T>) -> Tag<'a> {
        match value {
            Some(value) => self.attribute(name, &value.to_string()),
            None => self,
        }
    }

    /// Adds a `yes`/`no` attribute when `value` is present.
    pub(crate) fn optional_yes_no(self, name: &str, value: Option<bool>) -> Tag<'a> {
        self.optional(name, value.map(yes_no))
    }

    /// Ends the tag with `/>`.
    pub(crate) fn empty(self) {
        self.out.push_str("/>\n");
    }

    /// Ends the tag with `>`, leaving the element open.
    pub(crate) fn start(self) {
        self.out.push_str(">\n");
    }
}

/// Writes `</name>`.
pub(crate) fn close(out: &mut String, name: &str) {
    out.push_str("</");
    out.push_str(name);
    out.push_str(">\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_is_lenient_with_device_quirks() {
        let document = b"\xEF\xBB\xBF<root a=\"x &amp; y\" b=\"Tom & Jerry\"><child/></root>\0\0";
        let nodes = parse(document, "test").unwrap();

        let (root, rest) = root(&nodes, "root", "test").unwrap();
        assert_eq!(Some("x & y"), root.get("a"));
        assert_eq!(Some("Tom & Jerry"), root.get("b"));
        assert!(matches!(&rest[0], Node::Open(child) if child.name == "child"));
        assert_eq!(Node::Close("child".to_string()), rest[1]);
    }

    #[test]
    fn parse_skips_everything_but_elements() {
        let document = br#"<?xml version="1.0"?>
<!DOCTYPE MAP-msg-listing>
<!-- a comment with <tags> -->
<root>text<![CDATA[<not-a-tag/>]]>
  <a x='single' y=unquoted z="a > b" flag/>
  <b w=last/>
</root >"#;
        let nodes = parse(document, "test").unwrap();

        let (root, rest) = root(&nodes, "root", "test").unwrap();
        assert!(root.attributes.is_empty());
        let Node::Open(a) = &rest[0] else {
            panic!("expected <a>, found {:?}", rest[0]);
        };
        assert_eq!(Some("single"), a.get("x"));
        assert_eq!(Some("unquoted"), a.get("y"));
        assert_eq!(Some("a > b"), a.get("z"));
        assert_eq!(Some(""), a.get("flag"));
        assert_eq!(Node::Close("a".to_string()), rest[1]);
        assert!(matches!(&rest[2], Node::Open(b) if b.get("w") == Some("last")));
        assert_eq!(Node::Close("b".to_string()), rest[3]);
        assert_eq!(Node::Close("root".to_string()), rest[4]);
        assert_eq!(5, rest.len());
    }

    #[test]
    fn references_are_decoded_leniently() {
        assert_eq!("<>&\"'", unescape("&lt;&gt;&amp;&quot;&apos;"));
        assert_eq!("é😀\n", unescape("&#233;&#x1F600;&#10;"));
        assert_eq!("AT&T; R&D &unknown; &", unescape("AT&T; R&D &unknown; &"));
        assert_eq!("&#xD800;", unescape("&#xD800;"));
    }

    #[test]
    fn unterminated_markup_is_malformed() {
        for document in [
            &b"<root"[..],
            b"<root a=\"x/>",
            b"<root><!-- open",
            b"<root></root",
            b"<>",
        ] {
            assert!(
                matches!(parse(document, "test"), Err(Error::Malformed { .. })),
                "{:?}",
                String::from_utf8_lossy(document)
            );
        }
    }

    #[test]
    fn tags_escape_attribute_values() {
        let mut out = String::new();
        Tag::open(&mut out, "msg")
            .attribute("subject", "<\"A&B\">")
            .optional("size", Some(3))
            .optional::<u8>("missing", None)
            .empty();

        assert_eq!(
            "<msg subject=\"&lt;&quot;A&amp;B&quot;&gt;\" size=\"3\"/>\n",
            out
        );
    }
}
