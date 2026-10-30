//! ## bMessage Object
//!
//! The `x-bt/message` object exchanged by GetMessage and PushMessage (MAP section
//! 3.1.3). A bMessage wraps the message itself (an RFC 2822 e-mail, the text or PDUs
//! of an SMS, an MMS converted to e-mail format, or an instant message) with its read
//! status, type, folder, originator and recipients:
//!
//! ```text
//! BEGIN:BMSG
//! VERSION:1.0
//! STATUS:UNREAD
//! TYPE:SMS_GSM
//! FOLDER:TELECOM/MSG/INBOX
//! BEGIN:VCARD          <- originator
//! ...
//! END:VCARD
//! BEGIN:BENV           <- envelope, nesting up to three levels
//! BEGIN:VCARD          <- recipients
//! ...
//! END:VCARD
//! BEGIN:BBODY
//! CHARSET:UTF-8
//! LENGTH:39
//! BEGIN:MSG
//! Let's go fishing!
//! END:MSG
//! END:BBODY
//! END:BENV
//! END:BMSG
//! ```
//!
//! Message content is kept as bytes because SMS PDUs and 8-bit e-mail are not
//! necessarily UTF-8. When encoding, `LENGTH` is computed and lines starting with
//! `END:MSG` inside the content are escaped as the specification requires; parsing
//! reverses the escaping and finds the end of the content by its `END:MSG` line, so
//! devices that compute `LENGTH` incorrectly are still understood.
//!
//! # Examples
//!
//! Composing an SMS to push to the outbox:
//! ```
//! use bluetooth_map::bmessage::{BMessage, BMessageBody, VCard};
//! use bluetooth_map::types::MessageType;
//!
//! let message = BMessage::new(MessageType::SmsGsm, BMessageBody::from_text("Hello!"))
//!     .with_recipient(VCard::telephone("+15551234567"));
//!
//! let bytes = message.to_bytes();
//! assert!(bytes.starts_with(b"BEGIN:BMSG\r\nVERSION:1.0\r\n"));
//! assert_eq!(message, BMessage::parse(&bytes).unwrap());
//! ```
//!
use crate::error::{Error, Result};
use crate::types::MessageType;

const OBJECT: &str = "bMessage";
const END_MSG: &[u8] = b"END:MSG";

/// A vCard identifying an originator or recipient (MAP section 3.1.3).
///
/// MAP allows only a few properties: `VERSION`, `N`, `FN`, `TEL` and `EMAIL`, plus
/// `X-BT-UID` and `X-BT-UCI` with message format 1.1. Quoted-printable values, common
/// in vCard 2.1, are decoded when parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VCard {
    /// The vCard version, `2.1` or `3.0`.
    pub version: String,
    /// The structured name (`N`), e.g. `Doe;John`.
    pub name: Option<String>,
    /// The formatted name (`FN`), required by vCard 3.0.
    pub formatted_name: Option<String>,
    /// Phone numbers (`TEL`).
    pub telephones: Vec<String>,
    /// E-mail addresses (`EMAIL`).
    pub emails: Vec<String>,
    /// PBAP contact identifier, 32 hexadecimal digits (`X-BT-UID`).
    pub bt_uid: Option<String>,
    /// Instant messaging identifier of the user (`X-BT-UCI`).
    pub bt_uci: Option<String>,
}

impl Default for VCard {
    fn default() -> VCard {
        VCard::new()
    }
}

impl VCard {
    /// Creates an empty version 2.1 vCard.
    pub fn new() -> VCard {
        VCard {
            version: "2.1".to_string(),
            name: None,
            formatted_name: None,
            telephones: Vec::new(),
            emails: Vec::new(),
            bt_uid: None,
            bt_uci: None,
        }
    }

    /// A vCard with only a phone number, the recipient of an SMS.
    pub fn telephone(number: impl Into<String>) -> VCard {
        VCard::new().with_telephone(number)
    }

    /// A vCard with only an e-mail address, the recipient of an e-mail.
    pub fn email(address: impl Into<String>) -> VCard {
        VCard::new().with_email(address)
    }

    /// Sets the structured name (`N`).
    pub fn with_name(mut self, name: impl Into<String>) -> VCard {
        self.name = Some(name.into());
        self
    }

    /// Sets the formatted name (`FN`).
    pub fn with_formatted_name(mut self, name: impl Into<String>) -> VCard {
        self.formatted_name = Some(name.into());
        self
    }

    /// Adds a phone number (`TEL`).
    pub fn with_telephone(mut self, number: impl Into<String>) -> VCard {
        self.telephones.push(number.into());
        self
    }

    /// Adds an e-mail address (`EMAIL`).
    pub fn with_email(mut self, address: impl Into<String>) -> VCard {
        self.emails.push(address.into());
        self
    }

    /// Sets the instant messaging identifier (`X-BT-UCI`).
    pub fn with_uci(mut self, uci: impl Into<String>) -> VCard {
        self.bt_uci = Some(uci.into());
        self
    }

    fn parse(reader: &mut Reader<'_>) -> Result<VCard> {
        let mut card = VCard::new();

        loop {
            let mut property = reader.property()?;
            if property.is_end("VCARD") {
                return Ok(card);
            }

            let quoted_printable = property.params.iter().any(|param| {
                param.eq_ignore_ascii_case("ENCODING=QUOTED-PRINTABLE")
                    || param.eq_ignore_ascii_case("QUOTED-PRINTABLE")
            });

            let value = if quoted_printable {
                // A trailing `=` is a soft line break joining the next line.
                while property.value.ends_with('=') {
                    property.value.pop();
                    match reader.raw_line() {
                        Some(line) => property.value.push_str(&String::from_utf8_lossy(line)),
                        None => break,
                    }
                }
                String::from_utf8_lossy(&decode_quoted_printable(&property.value)).into_owned()
            } else {
                property.value
            };
            let value = value.trim().to_string();

            match property.name.as_str() {
                "VERSION" => card.version = value,
                // MAP allows empty names when they are not known, e.g. for an SMS.
                "N" => card.name = Some(value).filter(|name| !name.is_empty()),
                "FN" => card.formatted_name = Some(value).filter(|name| !name.is_empty()),
                "TEL" if !value.is_empty() => card.telephones.push(value),
                "EMAIL" if !value.is_empty() => card.emails.push(value),
                "X-BT-UID" => card.bt_uid = Some(value),
                "X-BT-UCI" => card.bt_uci = Some(value),
                _ => {}
            }
        }
    }

    fn encode(&self, out: &mut Vec<u8>) {
        line(out, "BEGIN", "VCARD");
        line(out, "VERSION", &self.version);
        line(out, "N", self.name.as_deref().unwrap_or(""));

        if self.formatted_name.is_some() || self.version.trim() == "3.0" {
            line(out, "FN", self.formatted_name.as_deref().unwrap_or(""));
        }
        for number in &self.telephones {
            line(out, "TEL", number);
        }
        for address in &self.emails {
            line(out, "EMAIL", address);
        }
        if let Some(uid) = &self.bt_uid {
            line(out, "X-BT-UID", uid);
        }
        if let Some(uci) = &self.bt_uci {
            line(out, "X-BT-UCI", uci);
        }

        line(out, "END", "VCARD");
    }
}

/// The body of a bMessage (`BEGIN:BBODY` .. `END:BBODY`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BMessageBody {
    /// Index of this fraction of a fractioned e-mail (`PARTID`).
    pub part_id: Option<u16>,
    /// Encoding of the content (`ENCODING`), e.g. `8BIT` or `G-7BIT`.
    pub encoding: Option<String>,
    /// Character set of textual content (`CHARSET`), `UTF-8` when transcoded.
    pub charset: Option<String>,
    /// Language of the content (`LANGUAGE`).
    pub language: Option<String>,
    /// The message content, one entry per `BEGIN:MSG` block; concatenated SMS sent
    /// as PDUs have one block per PDU.
    pub content: Vec<Vec<u8>>,
}

impl BMessageBody {
    /// A body holding UTF-8 text, as used for SMS text and e-mail.
    ///
    /// # Examples
    /// ```
    /// # use bluetooth_map::bmessage::BMessageBody;
    /// let body = BMessageBody::from_text("Hi");
    ///
    /// assert_eq!(Some("UTF-8"), body.charset.as_deref());
    /// assert_eq!("Hi", body.text());
    /// ```
    pub fn from_text(text: impl Into<String>) -> BMessageBody {
        BMessageBody {
            charset: Some("UTF-8".to_string()),
            content: vec![text.into().into_bytes()],
            ..BMessageBody::default()
        }
    }

    /// The content as text, decoding invalid UTF-8 lossily and joining blocks.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .map(|block| String::from_utf8_lossy(block))
            .collect()
    }

    /// The value of `LENGTH`: the bytes from the first `BEGIN:MSG` to the end of the
    /// last `END:MSG` line, after escaping.
    pub fn length(&self) -> usize {
        self.content
            .iter()
            .map(|block| b"BEGIN:MSG\r\n".len() + escape(block).len() + b"\r\nEND:MSG\r\n".len())
            .sum()
    }

    /// Parses a lone `BEGIN:BBODY` .. `END:BBODY` object, as delivered for the next
    /// fraction of a fractioned e-mail (MAP section 5.6.4.3).
    pub fn parse(bytes: &[u8]) -> Result<BMessageBody> {
        let mut reader = Reader::new(bytes);
        reader.expect_begin("BBODY")?;
        BMessageBody::parse_rest(&mut reader)
    }

    fn parse_rest(reader: &mut Reader<'_>) -> Result<BMessageBody> {
        let mut body = BMessageBody::default();

        loop {
            let property = reader.property()?;
            if property.is_end("BBODY") {
                return Ok(body);
            }
            if property.is_begin("MSG") {
                body.content.push(reader.message_content()?);
                continue;
            }

            let value = property.value.trim();
            match property.name.as_str() {
                "PARTID" => body.part_id = value.parse().ok(),
                "ENCODING" => body.encoding = Some(value.to_string()),
                "CHARSET" => body.charset = Some(value.to_string()),
                "LANGUAGE" => body.language = Some(value.to_string()),
                _ => {}
            }
        }
    }

    /// Encodes the body as a standalone `BEGIN:BBODY` .. `END:BBODY` object.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.encode(&mut out);
        out
    }

    fn encode(&self, out: &mut Vec<u8>) {
        line(out, "BEGIN", "BBODY");
        if let Some(part_id) = self.part_id {
            line(out, "PARTID", &part_id.to_string());
        }
        if let Some(encoding) = &self.encoding {
            line(out, "ENCODING", encoding);
        }
        if let Some(charset) = &self.charset {
            line(out, "CHARSET", charset);
        }
        if let Some(language) = &self.language {
            line(out, "LANGUAGE", language);
        }
        line(out, "LENGTH", &self.length().to_string());

        for block in &self.content {
            out.extend_from_slice(b"BEGIN:MSG\r\n");
            out.extend_from_slice(&escape(block));
            out.extend_from_slice(b"\r\nEND:MSG\r\n");
        }

        line(out, "END", "BBODY");
    }
}

/// What an [`Envelope`] contains after its recipients.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvelopeContent {
    /// A nested envelope, for messages that were forwarded.
    Envelope(Box<Envelope>),
    /// The message body.
    Body(BMessageBody),
}

/// A bMessage envelope (`BEGIN:BENV` .. `END:BENV`).
///
/// The outermost envelope holds the final recipients; nested envelopes hold the
/// recipients of earlier hops (MAP figure 3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    /// The recipients.
    pub recipients: Vec<VCard>,
    /// A nested envelope or the body.
    pub content: EnvelopeContent,
}

impl Envelope {
    /// The body at the bottom of this envelope and any nested envelopes.
    pub fn body(&self) -> &BMessageBody {
        match &self.content {
            EnvelopeContent::Envelope(envelope) => envelope.body(),
            EnvelopeContent::Body(body) => body,
        }
    }

    /// The body at the bottom of this envelope, mutably.
    pub fn body_mut(&mut self) -> &mut BMessageBody {
        match &mut self.content {
            EnvelopeContent::Envelope(envelope) => envelope.body_mut(),
            EnvelopeContent::Body(body) => body,
        }
    }

    fn parse(reader: &mut Reader<'_>) -> Result<Envelope> {
        let mut recipients = Vec::new();
        let mut content = None;

        loop {
            let property = reader.property()?;

            if property.is_end("BENV") {
                let content =
                    content.ok_or_else(|| Error::malformed(OBJECT, "envelope without a body"))?;
                return Ok(Envelope {
                    recipients,
                    content,
                });
            } else if property.is_begin("VCARD") {
                recipients.push(VCard::parse(reader)?);
            } else if property.is_begin("BENV") {
                content = Some(EnvelopeContent::Envelope(Box::new(Envelope::parse(
                    reader,
                )?)));
            } else if property.is_begin("BBODY") {
                content = Some(EnvelopeContent::Body(BMessageBody::parse_rest(reader)?));
            }
        }
    }

    fn encode(&self, out: &mut Vec<u8>) {
        line(out, "BEGIN", "BENV");
        for recipient in &self.recipients {
            recipient.encode(out);
        }
        match &self.content {
            EnvelopeContent::Envelope(envelope) => envelope.encode(out),
            EnvelopeContent::Body(body) => body.encode(out),
        }
        line(out, "END", "BENV");
    }
}

/// The `BMessage` type. See [the module level documentation](index.html) for more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BMessage {
    /// The bMessage version, `1.0` or `1.1`.
    pub version: String,
    /// Whether the message has been read (`STATUS:READ` or `STATUS:UNREAD`).
    pub read: bool,
    /// The message type (`TYPE`).
    pub message_type: MessageType,
    /// The folder holding the message, including its path (`FOLDER`). Empty when
    /// pushing, as the folder is given by the PushMessage request.
    pub folder: String,
    /// Extended data, see MAP section 3.1.13 (`EXTENDEDDATA`, version 1.1).
    pub extended_data: Option<String>,
    /// The original sender of the message.
    pub originator: Option<VCard>,
    /// The envelope with the recipients and the body.
    pub envelope: Envelope,
}

impl BMessage {
    /// Creates an unread version 1.0 bMessage with no originator or recipients and an
    /// empty folder, as pushed by an MCE.
    pub fn new(message_type: MessageType, body: BMessageBody) -> BMessage {
        BMessage {
            version: "1.0".to_string(),
            read: false,
            message_type,
            folder: String::new(),
            extended_data: None,
            originator: None,
            envelope: Envelope {
                recipients: Vec::new(),
                content: EnvelopeContent::Body(body),
            },
        }
    }

    /// Adds a recipient to the outermost envelope.
    pub fn with_recipient(mut self, recipient: VCard) -> BMessage {
        self.envelope.recipients.push(recipient);
        self
    }

    /// Sets the originator.
    pub fn with_originator(mut self, originator: VCard) -> BMessage {
        self.originator = Some(originator);
        self
    }

    /// The final recipients, from the outermost envelope.
    pub fn recipients(&self) -> &[VCard] {
        &self.envelope.recipients
    }

    /// The message body.
    pub fn body(&self) -> &BMessageBody {
        self.envelope.body()
    }

    /// The message body, mutably.
    pub fn body_mut(&mut self) -> &mut BMessageBody {
        self.envelope.body_mut()
    }

    /// The message content as text; see [`BMessageBody::text`].
    pub fn text(&self) -> String {
        self.body().text()
    }

    /// Parses a bMessage.
    ///
    /// # Examples
    /// ```
    /// # use bluetooth_map::bmessage::BMessage;
    /// # use bluetooth_map::types::MessageType;
    /// let message = BMessage::parse(b"BEGIN:BMSG\r\nVERSION:1.0\r\nSTATUS:READ\r\n\
    ///     TYPE:SMS_GSM\r\nFOLDER:telecom/msg/inbox\r\nBEGIN:VCARD\r\nVERSION:2.1\r\n\
    ///     N:Joachim\r\nTEL:+498912345678\r\nEND:VCARD\r\nBEGIN:BENV\r\nBEGIN:BBODY\r\n\
    ///     CHARSET:UTF-8\r\nLENGTH:39\r\nBEGIN:MSG\r\nLet's go fishing!\r\nEND:MSG\r\n\
    ///     END:BBODY\r\nEND:BENV\r\nEND:BMSG\r\n").unwrap();
    ///
    /// assert!(message.read);
    /// assert_eq!(MessageType::SmsGsm, message.message_type);
    /// assert_eq!(vec!["+498912345678"], message.originator.as_ref().unwrap().telephones);
    /// assert_eq!("Let's go fishing!", message.body().text());
    /// ```
    pub fn parse(bytes: &[u8]) -> Result<BMessage> {
        let mut reader = Reader::new(bytes);
        reader.expect_begin("BMSG")?;

        let mut version = None;
        let mut read = false;
        let mut message_type = None;
        let mut folder = String::new();
        let mut extended_data = None;
        let mut originator = None;

        let envelope = loop {
            let property = reader.property()?;

            if property.is_begin("VCARD") {
                originator = Some(VCard::parse(&mut reader)?);
                continue;
            }
            if property.is_begin("BENV") {
                break Envelope::parse(&mut reader)?;
            }

            let value = property.value.trim();
            match property.name.as_str() {
                "VERSION" => version = Some(value.to_string()),
                "STATUS" => read = value.eq_ignore_ascii_case("READ"),
                "TYPE" => message_type = Some(value.parse::<MessageType>()?),
                "FOLDER" => folder = value.to_string(),
                "EXTENDEDDATA" => extended_data = Some(value.to_string()),
                _ => {}
            }
        };

        reader.expect_end("BMSG")?;

        Ok(BMessage {
            version: version.unwrap_or_else(|| "1.0".to_string()),
            read,
            message_type: message_type.ok_or_else(|| Error::malformed(OBJECT, "missing TYPE"))?,
            folder,
            extended_data,
            originator,
            envelope,
        })
    }

    /// Encodes the bMessage with CRLF line endings.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();

        line(&mut out, "BEGIN", "BMSG");
        line(&mut out, "VERSION", &self.version);
        line(
            &mut out,
            "STATUS",
            if self.read { "READ" } else { "UNREAD" },
        );
        line(&mut out, "TYPE", self.message_type.as_str());
        line(&mut out, "FOLDER", &self.folder);
        if let Some(extended_data) = &self.extended_data {
            line(&mut out, "EXTENDEDDATA", extended_data);
        }
        if let Some(originator) = &self.originator {
            originator.encode(&mut out);
        }
        self.envelope.encode(&mut out);
        line(&mut out, "END", "BMSG");

        out
    }
}

/// Writes `name:value` and CRLF. Line breaks inside values would end the property
/// early, so they are replaced with spaces.
fn line(out: &mut Vec<u8>, name: &str, value: &str) {
    out.extend_from_slice(name.as_bytes());
    out.push(b':');
    out.extend(value.bytes().map(|byte| {
        if byte == b'\r' || byte == b'\n' {
            b' '
        } else {
            byte
        }
    }));
    out.extend_from_slice(b"\r\n");
}

/// A property line split into its name, parameters and value.
struct Property {
    /// Upper case name.
    name: String,
    params: Vec<String>,
    value: String,
}

impl Property {
    fn split(line: &[u8]) -> Property {
        let line = String::from_utf8_lossy(line);
        let (head, value) = line.split_once(':').unwrap_or((line.as_ref(), ""));
        let mut parts = head.split(';');

        Property {
            name: parts.next().unwrap_or("").trim().to_ascii_uppercase(),
            params: parts.map(|param| param.trim().to_string()).collect(),
            value: value.to_string(),
        }
    }

    fn is_begin(&self, object: &str) -> bool {
        self.name == "BEGIN" && self.value.trim().eq_ignore_ascii_case(object)
    }

    fn is_end(&self, object: &str) -> bool {
        self.name == "END" && self.value.trim().eq_ignore_ascii_case(object)
    }
}

/// Reads a bMessage line by line, accepting CRLF and bare LF line endings.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Reader<'a> {
        Reader { data, pos: 0 }
    }

    /// The next line without its line ending.
    fn raw_line(&mut self) -> Option<&'a [u8]> {
        if self.pos >= self.data.len() {
            return None;
        }

        let rest = &self.data[self.pos..];
        let (line, advance) = match rest.iter().position(|&byte| byte == b'\n') {
            Some(end) => (&rest[..end], end + 1),
            None => (rest, rest.len()),
        };
        self.pos += advance;

        Some(line.strip_suffix(b"\r").unwrap_or(line))
    }

    /// The next non-blank line as a property.
    fn property(&mut self) -> Result<Property> {
        loop {
            let line = self
                .raw_line()
                .ok_or_else(|| Error::malformed(OBJECT, "unexpected end of object"))?
                .trim_ascii();

            if !line.is_empty() {
                return Ok(Property::split(line));
            }
        }
    }

    fn expect_begin(&mut self, object: &str) -> Result<()> {
        // Tolerate a byte order mark before the first property.
        if self.data[self.pos..].starts_with("\u{FEFF}".as_bytes()) {
            self.pos += 3;
        }

        if self.property()?.is_begin(object) {
            Ok(())
        } else {
            Err(Error::malformed(
                OBJECT,
                format!("expected BEGIN:{}", object),
            ))
        }
    }

    fn expect_end(&mut self, object: &str) -> Result<()> {
        if self.property()?.is_end(object) {
            Ok(())
        } else {
            Err(Error::malformed(OBJECT, format!("expected END:{}", object)))
        }
    }

    /// Reads the content after a `BEGIN:MSG` line up to its `END:MSG` line.
    fn message_content(&mut self) -> Result<Vec<u8>> {
        let start = self.pos;
        let mut line_start = start;

        while line_start < self.data.len() {
            let rest = &self.data[line_start..];
            let line_end = rest
                .iter()
                .position(|&byte| byte == b'\n')
                .map(|end| line_start + end);
            let line = &self.data[line_start..line_end.unwrap_or(self.data.len())];

            if line.trim_ascii_end().eq_ignore_ascii_case(END_MSG) {
                // The line break before END:MSG belongs to the bMessage, not the content.
                let content = &self.data[start..line_start];
                let content = content.strip_suffix(b"\n").unwrap_or(content);
                let content = content.strip_suffix(b"\r").unwrap_or(content);

                self.pos = line_end.map_or(self.data.len(), |end| end + 1);
                return Ok(unescape(content));
            }

            match line_end {
                Some(end) => line_start = end + 1,
                None => break,
            }
        }

        Err(Error::malformed(OBJECT, "BEGIN:MSG without END:MSG"))
    }
}

/// Whether `content[at..]` is one or more `/` followed by `END:MSG`, returning the
/// number of slashes.
fn escaped_end(content: &[u8], at: usize) -> Option<usize> {
    let slashes = content[at..]
        .iter()
        .take_while(|&&byte| byte == b'/')
        .count();
    let rest = &content[at + slashes..];
    (rest.len() >= END_MSG.len() && rest[..END_MSG.len()].eq_ignore_ascii_case(END_MSG))
        .then_some(slashes)
}

/// Prefixes every line starting with `END:MSG`, `/END:MSG`, `//END:MSG`, ... with one
/// more `/` (MAP section 3.1.3).
fn escape(content: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(content.len());
    let mut line_start = true;

    for (index, &byte) in content.iter().enumerate() {
        if line_start && escaped_end(content, index).is_some() {
            out.push(b'/');
        }
        out.push(byte);
        line_start = byte == b'\n';
    }

    out
}

/// Reverses [`escape`], removing one `/` from lines like `/END:MSG`.
fn unescape(content: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(content.len());
    let mut line_start = true;
    let mut index = 0;

    while index < content.len() {
        if line_start && escaped_end(content, index).is_some_and(|slashes| slashes > 0) {
            index += 1;
        }
        let byte = content[index];
        out.push(byte);
        line_start = byte == b'\n';
        index += 1;
    }

    out
}

/// Decodes quoted-printable text; malformed escapes are kept literally.
fn decode_quoted_printable(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        let hex = bytes
            .get(index + 1..index + 3)
            .and_then(|digits| std::str::from_utf8(digits).ok())
            .and_then(|digits| u8::from_str_radix(digits, 16).ok());

        match (bytes[index], hex) {
            (b'=', Some(byte)) => {
                out.push(byte);
                index += 3;
            }
            (byte, _) => {
                out.push(byte);
                index += 1;
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMAIL: &str = "BEGIN:BMSG\r\n\
VERSION:1.0\r\n\
STATUS:UNREAD\r\n\
TYPE:EMAIL\r\n\
FOLDER:TELECOM/MSG/INBOX\r\n\
BEGIN:VCARD\r\n\
VERSION:2.1\r\n\
N:Mat\r\n\
EMAIL:ma@abc.edu\r\n\
END:VCARD\r\n\
BEGIN:BENV\r\n\
BEGIN:VCARD\r\n\
VERSION:2.1\r\n\
N:Tanaka\r\n\
EMAIL:tanaka@def.edu\r\n\
END:VCARD\r\n\
BEGIN:BENV\r\n\
BEGIN:VCARD\r\n\
VERSION:2.1\r\n\
N:Laurent\r\n\
EMAIL:laurent@ghi.edu\r\n\
END:VCARD\r\n\
BEGIN:BBODY\r\n\
ENCODING:8BIT\r\n\
LENGTH:125\r\n\
BEGIN:MSG\r\n\
Date: 20 Jun 96\r\n\
Subject: Fish\r\n\
From: tanaka@def.edu\r\n\
To: laurent@ghi.edu\r\n\
\r\n\
Let's go fishing!\r\n\
BR, Mat\r\n\
END:MSG\r\n\
END:BBODY\r\n\
END:BENV\r\n\
END:BENV\r\n\
END:BMSG\r\n";

    #[test]
    fn parses_and_reproduces_the_specification_email() {
        let message = BMessage::parse(EMAIL.as_bytes()).unwrap();

        assert!(!message.read);
        assert_eq!(MessageType::Email, message.message_type);
        assert_eq!("TELECOM/MSG/INBOX", message.folder);
        assert_eq!(vec!["tanaka@def.edu"], message.recipients()[0].emails);

        let EnvelopeContent::Envelope(inner) = &message.envelope.content else {
            panic!("expected a nested envelope");
        };
        assert_eq!(Some("Laurent"), inner.recipients[0].name.as_deref());
        assert_eq!(Some("8BIT"), message.body().encoding.as_deref());
        assert!(message.text().ends_with("Let's go fishing!\r\nBR, Mat"));

        // LENGTH matches the value in the specification's example.
        assert_eq!(125, message.body().length());
        assert_eq!(EMAIL.as_bytes(), message.to_bytes().as_slice());
    }

    #[test]
    fn computes_the_specification_sms_pdu_length() {
        let body = BMessageBody {
            encoding: Some("G-7BIT".to_string()),
            content: vec![b"0191000E9100949821436587000011303231\
12928211CC32FD34079DDF20737A8E4EBBCF21"
                .to_vec()],
            ..BMessageBody::default()
        };

        assert_eq!(96, body.length());
    }

    #[test]
    fn escapes_end_msg_inside_content() {
        let text = "END:MSG\r\n/END:MSG\nnot END:MSG\r\nend:msg";
        let message = BMessage::new(MessageType::Email, BMessageBody::from_text(text));
        let bytes = message.to_bytes();
        let encoded = String::from_utf8(bytes.clone()).unwrap();

        assert!(encoded
            .contains("BEGIN:MSG\r\n/END:MSG\r\n//END:MSG\nnot END:MSG\r\n/end:msg\r\nEND:MSG"));
        assert_eq!(text, BMessage::parse(&bytes).unwrap().text());
        assert_eq!(text.len() + 22 + 3, message.body().length());
    }

    #[test]
    fn tolerates_lf_line_endings_bad_lengths_and_empty_content() {
        let message = BMessage::parse(
            b"BEGIN:BMSG\nVERSION:1.1\nSTATUS:READ\nTYPE:IM\nFOLDER:\nEXTENDEDDATA:0:3;\n\
BEGIN:BENV\nBEGIN:BBODY\nENCODING:8BIT\nLENGTH:\nBEGIN:MSG\nEND:MSG\nBEGIN:MSG\n\nEND:MSG\n\
END:BBODY\nEND:BENV\nEND:BMSG\n",
        )
        .unwrap();

        assert_eq!(MessageType::Im, message.message_type);
        assert_eq!(Some("0:3;"), message.extended_data.as_deref());
        assert_eq!(vec![Vec::<u8>::new(), Vec::new()], message.body().content);
    }

    #[test]
    fn decodes_quoted_printable_vcards() {
        let message = BMessage::parse(
            "BEGIN:BMSG\r\nVERSION:1.0\r\nSTATUS:UNREAD\r\nTYPE:SMS_GSM\r\nFOLDER:\r\n\
BEGIN:VCARD\r\nVERSION:2.1\r\nN;ENCODING=QUOTED-PRINTABLE;CHARSET=UTF-8:=E5=BC=A0=\r\n\
=E4=B8=89\r\nFN;QUOTED-PRINTABLE:caf=C3=A9\r\nTEL;CELL:+1 555\r\nTEL:\r\nEND:VCARD\r\n\
BEGIN:BENV\r\nBEGIN:BBODY\r\nLENGTH:24\r\nBEGIN:MSG\r\nhi\r\nEND:MSG\r\nEND:BBODY\r\n\
END:BENV\r\nEND:BMSG\r\n"
                .as_bytes(),
        )
        .unwrap();

        let originator = message.originator.unwrap();
        assert_eq!(Some("张三"), originator.name.as_deref());
        assert_eq!(Some("café"), originator.formatted_name.as_deref());
        assert_eq!(vec!["+1 555"], originator.telephones);
    }

    #[test]
    fn rejects_incomplete_messages() {
        assert!(BMessage::parse(b"").is_err());
        assert!(BMessage::parse(b"BEGIN:VCARD\r\n").is_err());
        assert!(BMessage::parse(
            b"BEGIN:BMSG\r\nTYPE:SMS_GSM\r\nBEGIN:BENV\r\nBEGIN:BBODY\r\nBEGIN:MSG\r\nhi\r\n"
        )
        .is_err());
        assert!(BMessage::parse(b"BEGIN:BMSG\r\nTYPE:FAX\r\n").is_err());
        assert!(BMessage::parse(b"BEGIN:BMSG\r\nBEGIN:BENV\r\nEND:BENV\r\nEND:BMSG\r\n").is_err());
    }

    #[test]
    fn standalone_bodies_round_trip() {
        let body = BMessageBody {
            part_id: Some(1),
            ..BMessageBody::from_text("second fraction")
        };

        assert_eq!(body, BMessageBody::parse(&body.to_bytes()).unwrap());
    }
}
