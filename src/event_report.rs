//! ## MAP-Event-Report Object
//!
//! The `x-bt/MAP-event-report` object an MSE sends to the MCE's Message Notification
//! Server with SendEvent (MAP sections 3.1.7 and 5.1). Each report describes exactly
//! one event. The attributes present depend on the event type and the report version
//! (1.0, 1.1 or 1.2); unknown attributes and values are ignored.
//!
//! # Examples
//! ```
//! use map::event_report::{EventReport, EventType};
//! use map::types::MessageHandle;
//!
//! let report = EventReport::parse(br#"<MAP-event-report version="1.0">
//!   <event type="NewMessage" handle="12345678" folder="TELECOM/MSG/INBOX" msg_type="SMS_CDMA"/>
//! </MAP-event-report>"#).unwrap();
//!
//! assert_eq!(EventType::NewMessage, report.event_type);
//! assert_eq!(Some(MessageHandle(0x12345678)), report.handle);
//! ```
//!
use std::fmt;
use std::str::FromStr;

use crate::error::{Error, Result};
use crate::features::NotificationFilterMask;
use crate::types::{ConversationId, MessageHandle, MessageType};
use crate::xml::{self, Node, Tag};

const OBJECT: &str = "MAP-event-report";

/// The type of event (MAP section 3.1.7, table 3.2).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventType {
    /// A message was received, or created locally.
    NewMessage,
    /// A message was delivered to its recipient.
    DeliverySuccess,
    /// A message was sent to the network.
    SendingSuccess,
    /// Delivering a message to its recipient failed.
    DeliveryFailure,
    /// Sending a message to the network failed.
    SendingFailure,
    /// The MSE's memory is full.
    MemoryFull,
    /// The MSE's memory is available again.
    MemoryAvailable,
    /// A message was moved to the Deleted folder.
    MessageDeleted,
    /// A message was moved between folders.
    MessageShift,
    /// The read status of a message changed (version 1.1).
    ReadStatusChanged,
    /// A message was irrevocably removed (version 1.2).
    MessageRemoved,
    /// The extended data of a message changed (version 1.2).
    MessageExtendedDataChanged,
    /// The presence of a participant changed (version 1.2).
    ParticipantPresenceChanged,
    /// The chat state of a participant changed (version 1.2).
    ParticipantChatStateChanged,
    /// A conversation or its participants changed (version 1.2).
    ConversationChanged,
    /// An event type unknown to this crate.
    Other(String),
}

static EVENT_TYPES: [(EventType, &str, NotificationFilterMask); 15] = [
    (
        EventType::NewMessage,
        "NewMessage",
        NotificationFilterMask::NEW_MESSAGE,
    ),
    (
        EventType::DeliverySuccess,
        "DeliverySuccess",
        NotificationFilterMask::DELIVERY_SUCCESS,
    ),
    (
        EventType::SendingSuccess,
        "SendingSuccess",
        NotificationFilterMask::SENDING_SUCCESS,
    ),
    (
        EventType::DeliveryFailure,
        "DeliveryFailure",
        NotificationFilterMask::DELIVERY_FAILURE,
    ),
    (
        EventType::SendingFailure,
        "SendingFailure",
        NotificationFilterMask::SENDING_FAILURE,
    ),
    (
        EventType::MemoryFull,
        "MemoryFull",
        NotificationFilterMask::MEMORY_FULL,
    ),
    (
        EventType::MemoryAvailable,
        "MemoryAvailable",
        NotificationFilterMask::MEMORY_AVAILABLE,
    ),
    (
        EventType::MessageDeleted,
        "MessageDeleted",
        NotificationFilterMask::MESSAGE_DELETED,
    ),
    (
        EventType::MessageShift,
        "MessageShift",
        NotificationFilterMask::MESSAGE_SHIFT,
    ),
    (
        EventType::ReadStatusChanged,
        "ReadStatusChanged",
        NotificationFilterMask::READ_STATUS_CHANGED,
    ),
    (
        EventType::MessageRemoved,
        "MessageRemoved",
        NotificationFilterMask::MESSAGE_REMOVED,
    ),
    (
        EventType::MessageExtendedDataChanged,
        "MessageExtendedDataChanged",
        NotificationFilterMask::MESSAGE_EXTENDED_DATA_CHANGED,
    ),
    (
        EventType::ParticipantPresenceChanged,
        "ParticipantPresenceChanged",
        NotificationFilterMask::PARTICIPANT_PRESENCE_CHANGED,
    ),
    (
        EventType::ParticipantChatStateChanged,
        "ParticipantChatStateChanged",
        NotificationFilterMask::PARTICIPANT_CHAT_STATE_CHANGED,
    ),
    (
        EventType::ConversationChanged,
        "ConversationChanged",
        NotificationFilterMask::CONVERSATION_CHANGED,
    ),
];

impl EventType {
    /// The value used for this event type in event reports.
    pub fn as_str(&self) -> &str {
        match self {
            EventType::Other(text) => text,
            known => EVENT_TYPES
                .iter()
                .find(|(event_type, _, _)| event_type == known)
                .map(|(_, text, _)| *text)
                .unwrap_or_default(),
        }
    }

    /// The `NotificationFilterMask` bit that selects this event, if MAP defines one.
    pub fn filter_bit(&self) -> Option<NotificationFilterMask> {
        EVENT_TYPES
            .iter()
            .find(|(event_type, _, _)| event_type == self)
            .map(|(_, _, bit)| *bit)
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for EventType {
    type Err = Error;

    fn from_str(text: &str) -> Result<EventType> {
        let text = text.trim();

        Ok(EVENT_TYPES
            .iter()
            .find(|(_, name, _)| name.eq_ignore_ascii_case(text))
            .map(|(event_type, _, _)| event_type.clone())
            .unwrap_or_else(|| EventType::Other(text.to_string())))
    }
}

/// The `EventReport` type. See [the module level documentation](index.html) for more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventReport {
    /// The report version, `1.0`, `1.1` or `1.2`.
    pub version: String,
    /// The kind of event.
    pub event_type: EventType,
    /// The message the event refers to.
    pub handle: Option<MessageHandle>,
    /// The folder of the message, including its path, e.g. `TELECOM/MSG/INBOX`.
    pub folder: Option<String>,
    /// The folder a message was shifted out of (`MessageShift`).
    pub old_folder: Option<String>,
    /// The type of the message (`msg_type`).
    pub message_type: Option<MessageType>,
    /// Timestamp of the message (version 1.1).
    pub datetime: Option<String>,
    /// Summary of a new message (version 1.1).
    pub subject: Option<String>,
    /// Name of the sender or participant (version 1.1).
    pub sender_name: Option<String>,
    /// Whether a new message is high priority (version 1.1).
    pub priority: Option<bool>,
    /// Name of the conversation (version 1.2).
    pub conversation_name: Option<String>,
    /// The conversation the event refers to (version 1.2).
    pub conversation_id: Option<ConversationId>,
    /// Presence availability of the participant (version 1.2).
    pub presence_availability: Option<u8>,
    /// Presence text of the participant (version 1.2).
    pub presence_text: Option<String>,
    /// Timestamp of the participant's last activity (version 1.2).
    pub last_activity: Option<String>,
    /// Chat state of the participant (version 1.2).
    pub chat_state: Option<u8>,
    /// Read status of the message (version 1.2).
    pub read_status: Option<bool>,
    /// Extended data of the message, see MAP section 3.1.13 (version 1.2).
    pub extended_data: Option<String>,
    /// UCI of the participant (version 1.2).
    pub participant_uci: Option<String>,
    /// PBAP contact UID of the participant (version 1.2).
    pub contact_uid: Option<String>,
}

impl EventReport {
    /// Creates a version 1.0 report of `event_type` with no attributes.
    pub fn new(event_type: EventType) -> EventReport {
        EventReport {
            version: "1.0".to_string(),
            event_type,
            handle: None,
            folder: None,
            old_folder: None,
            message_type: None,
            datetime: None,
            subject: None,
            sender_name: None,
            priority: None,
            conversation_name: None,
            conversation_id: None,
            presence_availability: None,
            presence_text: None,
            last_activity: None,
            chat_state: None,
            read_status: None,
            extended_data: None,
            participant_uci: None,
            contact_uid: None,
        }
    }

    /// Creates a version 1.0 report about a message, e.g. a `NewMessage` event.
    ///
    /// # Examples
    /// ```
    /// # use map::event_report::{EventReport, EventType};
    /// # use map::types::{MessageHandle, MessageType};
    /// let report = EventReport::message(
    ///     EventType::NewMessage,
    ///     MessageHandle(1),
    ///     "TELECOM/MSG/INBOX",
    ///     MessageType::SmsGsm,
    /// );
    ///
    /// assert_eq!(report, EventReport::parse(&report.to_bytes()).unwrap());
    /// ```
    pub fn message(
        event_type: EventType,
        handle: MessageHandle,
        folder: impl Into<String>,
        message_type: MessageType,
    ) -> EventReport {
        EventReport {
            handle: Some(handle),
            folder: Some(folder.into()),
            message_type: Some(message_type),
            ..EventReport::new(event_type)
        }
    }

    /// Parses an event report.
    pub fn parse(bytes: &[u8]) -> Result<EventReport> {
        let nodes = xml::parse(bytes, OBJECT)?;
        let (root, children) = xml::root(&nodes, OBJECT, OBJECT)?;

        let event = children
            .iter()
            .find_map(|node| match node {
                Node::Open(element) if element.name == "event" => Some(element),
                _ => None,
            })
            .ok_or_else(|| Error::malformed(OBJECT, "missing <event> element"))?;

        let event_type = event
            .get("type")
            .ok_or_else(|| Error::malformed(OBJECT, "missing event type"))?
            .parse()?;

        Ok(EventReport {
            version: root.string("version").unwrap_or_else(|| "1.0".to_string()),
            event_type,
            handle: event.parsed("handle"),
            folder: event.string("folder"),
            old_folder: event.string("old_folder"),
            message_type: event.parsed("msg_type"),
            datetime: event.string("datetime"),
            subject: event.string("subject"),
            sender_name: event.string("sender_name"),
            priority: event.yes_no("priority"),
            conversation_name: event.string("conversation_name"),
            conversation_id: event.parsed("conversation_id"),
            presence_availability: event.parsed("presence_availability"),
            presence_text: event.string("presence_text"),
            last_activity: event.string("last_activity"),
            chat_state: event.parsed("chat_state"),
            read_status: event.yes_no("read_status"),
            extended_data: event.string("extended_data"),
            participant_uci: event.string("participant_uci"),
            contact_uid: event.string("contact_uid"),
        })
    }

    /// Encodes the report as XML, writing only the attributes that are present.
    ///
    /// The sender is responsible for leaving out attributes the receiving MCE's report
    /// version does not define (MAP section 3.1.7).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = String::from(xml::DECLARATION);
        Tag::open(&mut out, OBJECT)
            .attribute("version", &self.version)
            .start();

        Tag::open(&mut out, "event")
            .attribute("type", self.event_type.as_str())
            .optional(
                "handle",
                self.handle.map(|handle| format!("{:X}", handle.0)),
            )
            .optional("folder", self.folder.as_deref())
            .optional("old_folder", self.old_folder.as_deref())
            .optional("msg_type", self.message_type)
            .optional("datetime", self.datetime.as_deref())
            .optional("subject", self.subject.as_deref())
            .optional("sender_name", self.sender_name.as_deref())
            .optional_yes_no("priority", self.priority)
            .optional("conversation_name", self.conversation_name.as_deref())
            .optional("conversation_id", self.conversation_id)
            .optional("presence_availability", self.presence_availability)
            .optional("presence_text", self.presence_text.as_deref())
            .optional("last_activity", self.last_activity.as_deref())
            .optional("chat_state", self.chat_state)
            .optional_yes_no("read_status", self.read_status)
            .optional("extended_data", self.extended_data.as_deref())
            .optional("participant_uci", self.participant_uci.as_deref())
            .optional("contact_uid", self.contact_uid.as_deref())
            .empty();

        xml::close(&mut out, OBJECT);
        out.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version_1_2_reports() {
        let report = EventReport::parse(
            br#"<MAP-event-report version = "1.2">
<event type = "ConversationChanged" msg_type="IM" sender_name="Mads"
conversation_name="friends" conversation_id="A1A2A3A4B1B2B3B4C1C2C3C4D1D2D3D4"
participant_uci="4916579864@s.whateverapp.net" unknown="ignored"/>
</MAP-event-report>"#,
        )
        .unwrap();

        assert_eq!("1.2", report.version);
        assert_eq!(EventType::ConversationChanged, report.event_type);
        assert_eq!(Some(MessageType::Im), report.message_type);
        assert_eq!(Some("friends"), report.conversation_name.as_deref());
        assert_eq!(None, report.handle);
    }

    #[test]
    fn unknown_event_types_are_kept() {
        let report = EventReport::new(EventType::Other("FutureEvent".to_string()));
        let parsed = EventReport::parse(&report.to_bytes()).unwrap();

        assert_eq!(
            EventType::Other("FutureEvent".to_string()),
            parsed.event_type
        );
        assert_eq!(None, parsed.event_type.filter_bit());
        assert_eq!(
            Some(NotificationFilterMask::MESSAGE_SHIFT),
            EventType::MessageShift.filter_bit()
        );
    }

    #[test]
    fn reports_without_events_are_rejected() {
        assert!(EventReport::parse(b"<MAP-event-report version=\"1.0\"/>").is_err());
        assert!(EventReport::parse(b"<MAP-event-report><event/></MAP-event-report>").is_err());
    }
}
