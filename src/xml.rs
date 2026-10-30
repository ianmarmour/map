//! Reading and writing the attribute based XML objects MAP uses.
//!
//! Every MAP XML object (folder listing, messages listing, conversation listing and
//! event report) is a root element holding empty or nearly empty elements whose data
//! is entirely in attributes. Parsing flattens a document into open and close nodes
//! with decoded attributes, which the object parsers walk.
//!
//! Devices are known to send slightly broken documents, such as an unescaped `&` in
//! a subject or trailing null bytes, so parsing is deliberately lenient where it can
//! be without guessing.

use std::borrow::Cow;

use quick_xml::escape::{escape, unescape};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::error::{Error, Result};

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
    pub(crate) fn parsed<T: std::str::FromStr>(&self, name: &str) -> Option<T> {
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
    let text = text
        .trim_start_matches('\u{FEFF}')
        .trim_end_matches(|c: char| c == '\0' || c.is_whitespace());

    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);
    reader.config_mut().check_end_names = false;

    let mut nodes = Vec::new();

    loop {
        let event = reader
            .read_event()
            .map_err(|error| Error::malformed(object, error.to_string()))?;

        match event {
            Event::Start(start) => nodes.push(Node::Open(element(&start, object)?)),
            Event::Empty(start) => {
                let element = element(&start, object)?;
                let name = element.name.clone();
                nodes.push(Node::Open(element));
                nodes.push(Node::Close(name));
            }
            Event::End(end) => {
                nodes.push(Node::Close(
                    String::from_utf8_lossy(end.name().as_ref()).into_owned(),
                ));
            }
            Event::Eof => break,
            _ => {}
        }
    }

    Ok(nodes)
}

fn element(start: &BytesStart<'_>, object: &'static str) -> Result<Element> {
    let name = String::from_utf8_lossy(start.name().as_ref()).into_owned();
    let mut attributes = Vec::new();

    for attribute in start.html_attributes().with_checks(false) {
        let attribute = attribute.map_err(|error| Error::malformed(object, error.to_string()))?;
        let key = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
        let raw = String::from_utf8_lossy(&attribute.value);

        // Keep the raw text when it is not validly escaped, e.g. a bare `&`.
        let value = match unescape(&raw) {
            Ok(value) => value.into_owned(),
            Err(_) => raw.into_owned(),
        };

        attributes.push((key, value));
    }

    Ok(Element { name, attributes })
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
        self.out.push_str(&escape(Cow::Borrowed(value)));
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
