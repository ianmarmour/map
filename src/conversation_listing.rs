//! ## Conversation-Listing Object
//!
//! The `x-bt/MAP-convo-listing` object returned by GetConversationListing (MAP
//! sections 3.1.9 and 5.13). Conversations are sorted by last activity, newest first,
//! and may list their participants.
//!
//! # Examples
//! ```
//! use obex_protocol_map::conversation_listing::ConversationListing;
//!
//! let listing = ConversationListing::parse(br#"<MAP-convo-listing version="1.0">
//!   <conversation id="E1E2E3E4F1F2F3F4A1A2A3A4B1B2B3B4" name="Beergarden"
//!                 last_activity="20140612T105430+0100" read_status="no">
//!     <participant uci="4986925814@s.whateverapp.net" display_name="Tien" chat_state="3"/>
//!   </conversation>
//! </MAP-convo-listing>"#).unwrap();
//!
//! let conversation = &listing.conversations[0];
//! assert_eq!(Some("Beergarden"), conversation.name.as_deref());
//! assert_eq!(Some(false), conversation.read_status);
//! assert_eq!(Some(3), conversation.participants[0].chat_state);
//! ```
//!
use crate::error::Result;
use crate::types::{ConversationId, VersionCounter};
use crate::xml::{self, Element, Node, Tag};

const OBJECT: &str = "MAP-convo-listing";

/// A participant of a [`Conversation`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Participant {
    /// Uniquely identifies the participant, e.g. `4986925814@s.whateverapp.net`.
    pub uci: String,
    /// The name to display for the participant.
    pub display_name: Option<String>,
    /// Chat state, a value from the MAP assigned numbers (0 is unknown).
    pub chat_state: Option<u8>,
    /// Timestamp of the participant's last activity.
    pub last_activity: Option<String>,
    /// The PBAP contact matching the participant, as 32 hexadecimal digits.
    pub x_bt_uid: Option<String>,
    /// Full formatted name of the participant.
    pub name: Option<String>,
    /// Presence availability, a value from the MAP assigned numbers (0 is unknown).
    pub presence_availability: Option<u8>,
    /// User defined presence text.
    pub presence_text: Option<String>,
    /// Priority from 0 (lowest) to 100; 50 is neutral.
    pub priority: Option<u8>,
}

impl Participant {
    /// Creates a participant with only a UCI.
    pub fn new(uci: impl Into<String>) -> Participant {
        Participant {
            uci: uci.into(),
            ..Participant::default()
        }
    }

    fn from_element(element: &Element) -> Participant {
        Participant {
            uci: element.string("uci").unwrap_or_default(),
            display_name: element.string("display_name"),
            chat_state: element.parsed("chat_state"),
            last_activity: element.string("last_activity"),
            x_bt_uid: element
                .get_non_empty("x_bt_uid")
                .map(|uid| uid.trim().to_string()),
            name: element.string("name"),
            presence_availability: element.parsed("presence_availability"),
            presence_text: element.string("presence_text"),
            priority: element.parsed("priority"),
        }
    }
}

/// A conversation in a [`ConversationListing`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Conversation {
    /// Identifies the conversation, see GetMessagesListing and PushMessage.
    pub id: ConversationId,
    /// Name of the conversation; empty when it has none.
    pub name: Option<String>,
    /// Timestamp of the last activity.
    pub last_activity: Option<String>,
    /// Whether the conversation has been read.
    pub read_status: Option<bool>,
    /// The Conversation Version Counter.
    pub version_counter: Option<VersionCounter>,
    /// Summary, typically the most recent message.
    pub summary: Option<String>,
    /// The participants, when requested.
    pub participants: Vec<Participant>,
}

impl Conversation {
    /// Creates a conversation with only an id.
    pub fn new(id: ConversationId) -> Conversation {
        Conversation {
            id,
            ..Conversation::default()
        }
    }
}

/// The `ConversationListing` type. See [the module level documentation](index.html) for more.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConversationListing {
    /// The conversations, most recently active first.
    pub conversations: Vec<Conversation>,
}

impl ConversationListing {
    /// Creates a listing of `conversations`.
    pub fn new(conversations: Vec<Conversation>) -> ConversationListing {
        ConversationListing { conversations }
    }

    /// Parses a conversation listing. Conversations without a valid id are skipped.
    pub fn parse(bytes: &[u8]) -> Result<ConversationListing> {
        let nodes = xml::parse(bytes, OBJECT)?;
        let (_, children) = xml::root(&nodes, OBJECT, OBJECT)?;

        let mut conversations = Vec::new();
        let mut current: Option<Option<Conversation>> = None;

        for node in children {
            match node {
                Node::Open(element) if element.name == "conversation" => {
                    let conversation = element.parsed("id").map(|id| Conversation {
                        id,
                        name: element.string("name"),
                        last_activity: element.string("last_activity"),
                        read_status: element.yes_no("read_status"),
                        version_counter: element.parsed("version_counter"),
                        summary: element.string("summary"),
                        participants: Vec::new(),
                    });
                    current = Some(conversation);
                }
                Node::Open(element) if element.name == "participant" => {
                    if let Some(Some(conversation)) = current.as_mut() {
                        conversation
                            .participants
                            .push(Participant::from_element(element));
                    }
                }
                Node::Close(name) if name == "conversation" => {
                    conversations.extend(current.take().flatten());
                }
                _ => {}
            }
        }

        Ok(ConversationListing { conversations })
    }

    /// Encodes the listing as XML, writing only the attributes that are present.
    ///
    /// # Examples
    /// ```
    /// # use obex_protocol_map::conversation_listing::{Conversation, ConversationListing, Participant};
    /// # use obex_protocol_map::types::ConversationId;
    /// let mut conversation = Conversation::new(ConversationId(1));
    /// conversation.participants.push(Participant::new("alice@example.com"));
    /// let listing = ConversationListing::new(vec![conversation]);
    ///
    /// assert_eq!(listing, ConversationListing::parse(&listing.to_bytes()).unwrap());
    /// ```
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = String::from(xml::DECLARATION);
        Tag::open(&mut out, OBJECT)
            .attribute("version", "1.0")
            .start();

        for conversation in &self.conversations {
            let tag = Tag::open(&mut out, "conversation")
                .attribute("id", &conversation.id.to_string())
                .optional("name", conversation.name.as_deref())
                .optional("last_activity", conversation.last_activity.as_deref())
                .optional_yes_no("read_status", conversation.read_status)
                .optional("version_counter", conversation.version_counter)
                .optional("summary", conversation.summary.as_deref());

            if conversation.participants.is_empty() {
                tag.empty();
                continue;
            }
            tag.start();

            for participant in &conversation.participants {
                Tag::open(&mut out, "participant")
                    .attribute("uci", &participant.uci)
                    .optional("display_name", participant.display_name.as_deref())
                    .optional("chat_state", participant.chat_state)
                    .optional("last_activity", participant.last_activity.as_deref())
                    .optional("x_bt_uid", participant.x_bt_uid.as_deref())
                    .optional("name", participant.name.as_deref())
                    .optional("presence_availability", participant.presence_availability)
                    .optional("presence_text", participant.presence_text.as_deref())
                    .optional("priority", participant.priority)
                    .empty();
            }

            xml::close(&mut out, "conversation");
        }

        xml::close(&mut out, OBJECT);
        out.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_and_empty_conversations() {
        let listing = ConversationListing::parse(
            br#"<MAP-convo-listing version = "1.0">
<conversation id="C1C2C3C4D1D2D3D4E1E2E3E4F1F2F3F4" name=""
last_activity="20140801T012900+0100" read_status="yes"
version_counter="0A0A1B1B2C2C3D3D4E4E5F5F6A6A7B7B">
<participant uci="malo@email.de" display_name="Mari" chat_state="2"
last_activity="20140801T012900+0100" x_bt_uid=" A1A2A3A4B1B2C1C2D1D2E1E2E3E4F1F2"
name="Mareike Loh" presence_availability="2" presence_text="Wow it's hot today"
priority="100"/>
</conversation>
<conversation id="not hex"><participant uci="ignored"/></conversation>
<conversation id="F1" read_status=""/>
</MAP-convo-listing>"#,
        )
        .unwrap();

        assert_eq!(2, listing.conversations.len());

        let first = &listing.conversations[0];
        assert_eq!(Some(""), first.name.as_deref());
        assert_eq!(Some(true), first.read_status);
        assert_eq!(
            Some(VersionCounter(0x0A0A1B1B2C2C3D3D4E4E5F5F6A6A7B7B)),
            first.version_counter
        );
        assert_eq!(
            Some("A1A2A3A4B1B2C1C2D1D2E1E2E3E4F1F2"),
            first.participants[0].x_bt_uid.as_deref()
        );
        assert_eq!(Some(100), first.participants[0].priority);

        let second = &listing.conversations[1];
        assert_eq!(ConversationId(0xF1), second.id);
        assert_eq!(None, second.read_status);
        assert!(second.participants.is_empty());
    }
}
