//! ## Messages-Listing Object
//!
//! The `x-bt/MAP-msg-listing` object returned by GetMessagesListing (MAP sections
//! 3.1.6 and 5.5). Entries are sorted newest first. Which attributes are present
//! depends on the listing version, the `ParameterMask` of the request and the device,
//! so every attribute except the handle is optional; attributes and values unknown to
//! this crate are ignored, as MAP requires.
//!
//! # Examples
//! ```
//! use bluetooth_map::messages_listing::MessagesListing;
//! use bluetooth_map::types::{MessageHandle, MessageType};
//!
//! let listing = MessagesListing::parse(br#"<MAP-msg-listing version="1.0">
//!   <msg handle="20000100001" subject="Hello" datetime="20071213T130510"
//!        sender_name="Jamie" sender_addressing="+1-987-6543210"
//!        recipient_addressing="+1-0123-456789" type="SMS_GSM" size="256"
//!        attachment_size="0" priority="no" read="yes" sent="no" protected="no"/>
//! </MAP-msg-listing>"#).unwrap();
//!
//! let message = &listing.messages[0];
//! assert_eq!(MessageHandle(0x20000100001), message.handle);
//! assert_eq!(Some(MessageType::SmsGsm), message.message_type);
//! assert_eq!(Some(true), message.read);
//! ```
//!
use std::fmt;
use std::str::FromStr;

use crate::error::{Error, Result};
use crate::types::{ConversationId, MessageHandle, MessageType};
use crate::xml::{self, Node, Tag};

const OBJECT: &str = "MAP-msg-listing";

macro_rules! keyword {
    ($(#[$meta:meta])* $name:ident, $object:literal { $($(#[$variant_meta:meta])* $variant:ident = $text:literal,)* }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($(#[$variant_meta])* $variant,)*
        }

        impl $name {
            /// The value used in MAP objects.
            pub fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $text,)*
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = Error;

            fn from_str(text: &str) -> Result<$name> {
                let text = text.trim();
                $(
                    if text.eq_ignore_ascii_case($text) {
                        return Ok($name::$variant);
                    }
                )*
                Err(Error::malformed($object, format!("unknown value {:?}", text)))
            }
        }
    };
}

keyword!(
    /// How much of a message the MSE has received (`reception_status`).
    ReceptionStatus, "reception status" {
        /// The complete message has been received.
        Complete = "complete",
        /// Only part of the message has been received, e.g. a pushed e-mail fraction.
        Fractioned = "fractioned",
        /// Only a notification of the message has been received.
        Notification = "notification",
    }
);

keyword!(
    /// The delivery status of a message (`delivery_status`, listing version 1.1).
    DeliveryStatus, "delivery status" {
        /// Delivered to the recipient.
        Delivered = "delivered",
        /// Sent to the remote network.
        Sent = "sent",
        /// The MSE has no knowledge of the delivery status.
        Unknown = "unknown",
    }
);

keyword!(
    /// The direction of a message (`direction`, listing version 1.1).
    Direction, "direction" {
        /// A received message.
        Incoming = "incoming",
        /// A message being sent or successfully sent.
        Outgoing = "outgoing",
        /// An outgoing draft.
        OutgoingDraft = "outgoingdraft",
        /// A message waiting to be sent, including ones that failed.
        OutgoingPending = "outgoingpending",
    }
);

/// A message in a [`MessagesListing`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MessageEntry {
    /// The message handle, used with GetMessage and SetMessageStatus.
    pub handle: MessageHandle,
    /// Summary of the message, at most 256 bytes.
    pub subject: Option<String>,
    /// Timestamp, see [`Timestamp`](crate::types::Timestamp).
    pub datetime: Option<String>,
    /// Name of the sender.
    pub sender_name: Option<String>,
    /// Address of the sender: e-mail address, phone number or IM UCI.
    pub sender_addressing: Option<String>,
    /// Reply-To address of an e-mail.
    pub replyto_addressing: Option<String>,
    /// Name of the recipient.
    pub recipient_name: Option<String>,
    /// Recipient addresses, separated by `;`.
    pub recipient_addressing: Option<String>,
    /// The message type (`type`).
    pub message_type: Option<MessageType>,
    /// Size of the original message in bytes.
    pub size: Option<u64>,
    /// Whether the message has textual content.
    pub text: Option<bool>,
    /// How much of the message has been received.
    pub reception_status: Option<ReceptionStatus>,
    /// Total size of the attachments in bytes.
    pub attachment_size: Option<u64>,
    /// Whether the message is high priority.
    pub priority: Option<bool>,
    /// Whether the message has been read.
    pub read: Option<bool>,
    /// Whether the message has been sent.
    pub sent: Option<bool>,
    /// Whether the message is DRM protected.
    pub protected: Option<bool>,
    /// Delivery status (version 1.1).
    pub delivery_status: Option<DeliveryStatus>,
    /// The conversation the message belongs to (version 1.1).
    pub conversation_id: Option<ConversationId>,
    /// Name of the conversation (version 1.1).
    pub conversation_name: Option<String>,
    /// Direction of the message (version 1.1).
    pub direction: Option<Direction>,
    /// MIME types of the attachments (version 1.1).
    pub attachment_mime_types: Vec<String>,
}

impl MessageEntry {
    /// Creates an entry with only a handle.
    pub fn new(handle: MessageHandle) -> MessageEntry {
        MessageEntry {
            handle,
            ..MessageEntry::default()
        }
    }

    /// The recipient addresses, split on `;` (a `\;` is an escaped delimiter).
    pub fn recipients(&self) -> Vec<String> {
        split_addresses(self.recipient_addressing.as_deref().unwrap_or(""))
    }
}

/// Splits a `;` delimited address list, honoring `\` escapes.
fn split_addresses(addresses: &str) -> Vec<String> {
    let mut list = Vec::new();
    let mut current = String::new();
    let mut chars = addresses.chars();

    while let Some(c) = chars.next() {
        match c {
            '\\' => current.extend(chars.next()),
            ';' => list.push(std::mem::take(&mut current)),
            c => current.push(c),
        }
    }
    list.push(current);

    list.into_iter()
        .map(|address| address.trim().to_string())
        .filter(|address| !address.is_empty())
        .collect()
}

/// The `MessagesListing` type. See [the module level documentation](index.html) for more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessagesListing {
    /// The listing format version, `1.0` or `1.1`.
    pub version: String,
    /// The messages, newest first.
    pub messages: Vec<MessageEntry>,
}

impl Default for MessagesListing {
    fn default() -> MessagesListing {
        MessagesListing::new(Vec::new())
    }
}

impl MessagesListing {
    /// Creates a version 1.0 listing.
    pub fn new(messages: Vec<MessageEntry>) -> MessagesListing {
        MessagesListing {
            version: "1.0".to_string(),
            messages,
        }
    }

    /// Parses a messages listing.
    ///
    /// Entries without a valid handle cannot be addressed and are skipped.
    pub fn parse(bytes: &[u8]) -> Result<MessagesListing> {
        let nodes = xml::parse(bytes, OBJECT)?;
        let (root, children) = xml::root(&nodes, OBJECT, OBJECT)?;

        let messages = children
            .iter()
            .filter_map(|node| match node {
                Node::Open(element) if element.name == "msg" => Some(element),
                _ => None,
            })
            .filter_map(|element| {
                Some(MessageEntry {
                    handle: element.parsed("handle")?,
                    subject: element.string("subject"),
                    datetime: element.string("datetime"),
                    sender_name: element.string("sender_name"),
                    sender_addressing: element.string("sender_addressing"),
                    replyto_addressing: element.string("replyto_addressing"),
                    recipient_name: element.string("recipient_name"),
                    recipient_addressing: element.string("recipient_addressing"),
                    message_type: element.parsed("type"),
                    size: element.parsed("size"),
                    text: element.yes_no("text"),
                    reception_status: element.parsed("reception_status"),
                    attachment_size: element.parsed("attachment_size"),
                    priority: element.yes_no("priority"),
                    read: element.yes_no("read"),
                    sent: element.yes_no("sent"),
                    protected: element.yes_no("protected"),
                    delivery_status: element.parsed("delivery_status"),
                    conversation_id: element.parsed("conversation_id"),
                    conversation_name: element.string("conversation_name"),
                    direction: element.parsed("direction"),
                    attachment_mime_types: element
                        .get("attachment_mime_types")
                        .unwrap_or("")
                        .split(',')
                        .map(str::trim)
                        .filter(|mime| !mime.is_empty())
                        .map(str::to_string)
                        .collect(),
                })
            })
            .collect();

        Ok(MessagesListing {
            version: root.string("version").unwrap_or_else(|| "1.0".to_string()),
            messages,
        })
    }

    /// Encodes the listing as XML, writing only the attributes that are present.
    ///
    /// # Examples
    /// ```
    /// # use bluetooth_map::messages_listing::{MessageEntry, MessagesListing};
    /// # use bluetooth_map::types::MessageHandle;
    /// let mut entry = MessageEntry::new(MessageHandle(1));
    /// entry.subject = Some("Fish & chips".to_string());
    /// let listing = MessagesListing::new(vec![entry]);
    ///
    /// assert_eq!(listing, MessagesListing::parse(&listing.to_bytes()).unwrap());
    /// ```
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = String::from(xml::DECLARATION);
        Tag::open(&mut out, OBJECT)
            .attribute("version", &self.version)
            .start();

        for message in &self.messages {
            let mime_types = message.attachment_mime_types.join(",");

            Tag::open(&mut out, "msg")
                .attribute("handle", &format!("{:X}", message.handle.0))
                .optional("subject", message.subject.as_deref())
                .optional("datetime", message.datetime.as_deref())
                .optional("sender_name", message.sender_name.as_deref())
                .optional("sender_addressing", message.sender_addressing.as_deref())
                .optional("replyto_addressing", message.replyto_addressing.as_deref())
                .optional("recipient_name", message.recipient_name.as_deref())
                .optional(
                    "recipient_addressing",
                    message.recipient_addressing.as_deref(),
                )
                .optional("type", message.message_type)
                .optional("size", message.size)
                .optional_yes_no("text", message.text)
                .optional("reception_status", message.reception_status)
                .optional("attachment_size", message.attachment_size)
                .optional_yes_no("priority", message.priority)
                .optional_yes_no("read", message.read)
                .optional_yes_no("sent", message.sent)
                .optional_yes_no("protected", message.protected)
                .optional("delivery_status", message.delivery_status)
                .optional("conversation_id", message.conversation_id)
                .optional("conversation_name", message.conversation_name.as_deref())
                .optional("direction", message.direction)
                .optional(
                    "attachment_mime_types",
                    Some(mime_types.as_str()).filter(|mime| !mime.is_empty()),
                )
                .empty();
        }

        xml::close(&mut out, OBJECT);
        out.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_version_1_1_example() {
        let listing = MessagesListing::parse(
            br#"<MAP-msg-listing version="1.1">
 <msg handle="20000100002" subject= "What's the progress Max?"
 datetime="20140705T092200+0100" sender_name="Jonas"
 sender_addressing="4913579864@s.whateverapp.net" recipient_addressing = "" type="IM"
 size="512" attachment_size="8671724" priority="no" read="yes" sent="yes" protected="no"
 conversation_id="E1E2E3E4F1F2F3F4A1A2A3A4B1B2B3B4" direction="incoming"
 attachment_mime_types="video/mpeg, image/png" delivery_status="weird"/>
 <msg subject="no handle"/>
</MAP-msg-listing>"#,
        )
        .unwrap();

        assert_eq!("1.1", listing.version);
        assert_eq!(1, listing.messages.len());

        let message = &listing.messages[0];
        assert_eq!(Some(MessageType::Im), message.message_type);
        assert_eq!(Some(Direction::Incoming), message.direction);
        assert_eq!(None, message.delivery_status);
        assert_eq!(Some(8671724), message.attachment_size);
        assert_eq!(
            Some(ConversationId(0xE1E2E3E4F1F2F3F4A1A2A3A4B1B2B3B4)),
            message.conversation_id
        );
        assert_eq!(
            vec!["video/mpeg", "image/png"],
            message.attachment_mime_types
        );
        assert!(message.recipients().is_empty());
    }

    #[test]
    fn full_entries_round_trip() {
        let entry = MessageEntry {
            handle: MessageHandle(0xABCDEF),
            subject: Some("<hi>".to_string()),
            datetime: Some("20071213T130510".to_string()),
            sender_name: Some("Jamie".to_string()),
            sender_addressing: Some("+1".to_string()),
            replyto_addressing: Some("a@b".to_string()),
            recipient_name: Some("Me".to_string()),
            recipient_addressing: Some("a@b.c;d\\;e@f".to_string()),
            message_type: Some(MessageType::Email),
            size: Some(10),
            text: Some(true),
            reception_status: Some(ReceptionStatus::Fractioned),
            attachment_size: Some(0),
            priority: Some(false),
            read: Some(false),
            sent: Some(true),
            protected: Some(false),
            delivery_status: Some(DeliveryStatus::Delivered),
            conversation_id: Some(ConversationId(7)),
            conversation_name: Some("".to_string()),
            direction: Some(Direction::OutgoingPending),
            attachment_mime_types: vec!["image/png".to_string(), "text/plain".to_string()],
        };
        let listing = MessagesListing {
            version: "1.1".to_string(),
            messages: vec![entry],
        };

        let parsed = MessagesListing::parse(&listing.to_bytes()).unwrap();
        assert_eq!(listing, parsed);
        assert_eq!(vec!["a@b.c", "d;e@f"], parsed.messages[0].recipients());
    }
}
