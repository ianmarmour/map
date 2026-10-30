//! ## Feature Bits and Bit Masks
//!
//! MAP describes capabilities and selections with bit masks: the
//! `MapSupportedFeatures` both devices advertise (MAP sections 6.3.1 and 7.1), the
//! message types a MAS instance offers, and the masks of several functions. Each is
//! a small `Copy` type with associated constants that combine with `|`.
//!
//! Bits not known to this crate are preserved, so values received from newer devices
//! survive a round trip.
//!
//! # Examples
//! ```
//! use obex_protocol_map::features::MapSupportedFeatures;
//!
//! let features = MapSupportedFeatures::NOTIFICATION_REGISTRATION
//!     | MapSupportedFeatures::NOTIFICATION
//!     | MapSupportedFeatures::BROWSING;
//!
//! assert!(features.contains(MapSupportedFeatures::BROWSING));
//! assert_eq!(0x07, features.bits());
//! ```
//!
use std::fmt;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign};

use crate::types::MessageType;

macro_rules! bit_mask {
    (
        $(#[$meta:meta])*
        $name:ident($bits:ty) {
            $(
                $(#[$flag_meta:meta])*
                $flag:ident = $bit:literal;
            )*
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
        pub struct $name(pub $bits);

        impl $name {
            $(
                $(#[$flag_meta])*
                pub const $flag: $name = $name(1 << $bit);
            )*

            /// No bits set.
            pub const fn empty() -> $name {
                $name(0)
            }

            /// Every bit defined by MAP set.
            pub const fn all() -> $name {
                $name(0 $(| (1 << $bit))*)
            }

            /// The raw bits.
            pub const fn bits(self) -> $bits {
                self.0
            }

            /// Creates a value from raw bits, keeping bits unknown to this crate.
            pub const fn from_bits(bits: $bits) -> $name {
                $name(bits)
            }

            /// Whether every bit of `other` is set.
            pub const fn contains(self, other: $name) -> bool {
                self.0 & other.0 == other.0
            }

            /// Whether no bits are set.
            pub const fn is_empty(self) -> bool {
                self.0 == 0
            }

            /// Sets the bits of `other`.
            pub fn insert(&mut self, other: $name) {
                self.0 |= other.0;
            }

            /// Clears the bits of `other`.
            pub fn remove(&mut self, other: $name) {
                self.0 &= !other.0;
            }

            /// Returns this value with the bits of `other` set or cleared.
            pub fn with(mut self, other: $name, set: bool) -> $name {
                if set {
                    self.insert(other);
                } else {
                    self.remove(other);
                }
                self
            }
        }

        impl BitOr for $name {
            type Output = $name;

            fn bitor(self, other: $name) -> $name {
                $name(self.0 | other.0)
            }
        }

        impl BitOrAssign for $name {
            fn bitor_assign(&mut self, other: $name) {
                self.0 |= other.0;
            }
        }

        impl BitAnd for $name {
            type Output = $name;

            fn bitand(self, other: $name) -> $name {
                $name(self.0 & other.0)
            }
        }

        impl BitAndAssign for $name {
            fn bitand_assign(&mut self, other: $name) {
                self.0 &= other.0;
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let mut names: Vec<&str> = Vec::new();
                let mut known = 0;
                $(
                    known |= 1 << $bit;
                    if self.contains($name::$flag) {
                        names.push(stringify!($flag));
                    }
                )*
                let unknown = self.0 & !known;
                write!(f, "{}(", stringify!($name))?;
                write!(f, "{}", names.join(" | "))?;
                if unknown != 0 {
                    if !names.is_empty() {
                        f.write_str(" | ")?;
                    }
                    write!(f, "{:#x}", unknown)?;
                }
                f.write_str(")")
            }
        }
    };
}

bit_mask!(
    /// The MAP features a device supports (`MapSupportedFeatures`, MAP sections 6.3.1
    /// and 7.1.1).
    ///
    /// Advertised in the SDP records of both devices and sent by the MCE in CONNECT
    /// when the MSE sets [`MAP_SUPPORTED_FEATURES_IN_CONNECT_REQUEST`](Self::MAP_SUPPORTED_FEATURES_IN_CONNECT_REQUEST).
    MapSupportedFeatures(u32) {
        /// Notification Registration feature.
        NOTIFICATION_REGISTRATION = 0;
        /// Notification feature.
        NOTIFICATION = 1;
        /// Browsing feature.
        BROWSING = 2;
        /// Uploading feature.
        UPLOADING = 3;
        /// Delete feature.
        DELETE = 4;
        /// Instance Information feature.
        INSTANCE_INFORMATION = 5;
        /// Extended Event Report 1.1.
        EXTENDED_EVENT_REPORT_1_1 = 6;
        /// Event Report Version 1.2.
        EVENT_REPORT_1_2 = 7;
        /// Message Format Version 1.1.
        MESSAGE_FORMAT_1_1 = 8;
        /// Messages-Listing Format Version 1.1.
        MESSAGES_LISTING_FORMAT_1_1 = 9;
        /// Persistent Message Handles.
        PERSISTENT_MESSAGE_HANDLES = 10;
        /// Database Identifier.
        DATABASE_IDENTIFIER = 11;
        /// Folder Version Counter.
        FOLDER_VERSION_COUNTER = 12;
        /// Conversation Version Counters.
        CONVERSATION_VERSION_COUNTERS = 13;
        /// Participant Presence Change Notification.
        PARTICIPANT_PRESENCE_CHANGE_NOTIFICATION = 14;
        /// Participant Chat State Change Notification.
        PARTICIPANT_CHAT_STATE_CHANGE_NOTIFICATION = 15;
        /// PBAP Contact Cross Reference.
        PBAP_CONTACT_CROSS_REFERENCE = 16;
        /// Notification Filtering.
        NOTIFICATION_FILTERING = 17;
        /// UTC Offset Timestamp Format.
        UTC_OFFSET_TIMESTAMP_FORMAT = 18;
        /// The MSE expects `MapSupportedFeatures` in the CONNECT request (MSE SDP record only).
        MAP_SUPPORTED_FEATURES_IN_CONNECT_REQUEST = 19;
        /// Conversation listing.
        CONVERSATION_LISTING = 20;
        /// Owner status.
        OWNER_STATUS = 21;
        /// Message Forwarding.
        MESSAGE_FORWARDING = 22;
    }
);

impl MapSupportedFeatures {
    /// The features assumed for a device whose SDP record has no
    /// `MapSupportedFeatures` attribute (MAP section 7.1, `0x0000001F`).
    pub const DEFAULT: MapSupportedFeatures = MapSupportedFeatures(0x1F);
}

bit_mask!(
    /// The message types a MAS instance offers (`SupportedMessageTypes`, MAP section 7.1.1).
    SupportedMessageTypes(u8) {
        /// E-mail.
        EMAIL = 0;
        /// GSM SMS.
        SMS_GSM = 1;
        /// CDMA SMS.
        SMS_CDMA = 2;
        /// MMS.
        MMS = 3;
        /// Instant messages.
        IM = 4;
    }
);

impl SupportedMessageTypes {
    /// The bit for `message_type`.
    pub fn of(message_type: MessageType) -> SupportedMessageTypes {
        match message_type {
            MessageType::Email => SupportedMessageTypes::EMAIL,
            MessageType::SmsGsm => SupportedMessageTypes::SMS_GSM,
            MessageType::SmsCdma => SupportedMessageTypes::SMS_CDMA,
            MessageType::Mms => SupportedMessageTypes::MMS,
            MessageType::Im => SupportedMessageTypes::IM,
        }
    }
}

bit_mask!(
    /// Message types to leave out of a Messages-Listing (`FilterMessageType`, MAP
    /// section 6.3.1).
    ///
    /// A set bit *filters out* that type. [`MessageTypeFilter::only`] builds the mask
    /// that keeps just the given types.
    ///
    /// # Examples
    /// ```
    /// # use obex_protocol_map::features::MessageTypeFilter;
    /// # use obex_protocol_map::types::MessageType;
    /// let filter = MessageTypeFilter::only(&[MessageType::SmsGsm, MessageType::Mms]);
    ///
    /// assert!(filter.includes(MessageType::SmsGsm));
    /// assert!(!filter.includes(MessageType::Email));
    /// assert_eq!(0b10110, filter.bits());
    /// ```
    MessageTypeFilter(u8) {
        /// Leave out GSM SMS.
        SMS_GSM = 0;
        /// Leave out CDMA SMS.
        SMS_CDMA = 1;
        /// Leave out e-mail.
        EMAIL = 2;
        /// Leave out MMS.
        MMS = 3;
        /// Leave out instant messages.
        IM = 4;
    }
);

impl MessageTypeFilter {
    /// The bit that filters out `message_type`.
    pub fn of(message_type: MessageType) -> MessageTypeFilter {
        match message_type {
            MessageType::SmsGsm => MessageTypeFilter::SMS_GSM,
            MessageType::SmsCdma => MessageTypeFilter::SMS_CDMA,
            MessageType::Email => MessageTypeFilter::EMAIL,
            MessageType::Mms => MessageTypeFilter::MMS,
            MessageType::Im => MessageTypeFilter::IM,
        }
    }

    /// A filter that keeps only `types` and leaves out every other type.
    pub fn only(types: &[MessageType]) -> MessageTypeFilter {
        let mut filter = MessageTypeFilter::all();
        for message_type in types {
            filter.remove(MessageTypeFilter::of(*message_type));
        }
        filter
    }

    /// Whether messages of `message_type` pass this filter.
    pub fn includes(self, message_type: MessageType) -> bool {
        !self.contains(MessageTypeFilter::of(message_type))
    }
}

bit_mask!(
    /// The attributes requested in a Messages-Listing (`ParameterMask`, MAP section 5.5.4.4).
    ///
    /// The message handle is always present. An empty mask asks for every required
    /// attribute.
    ParameterMask(u32) {
        /// `subject`.
        SUBJECT = 0;
        /// `datetime`.
        DATETIME = 1;
        /// `sender_name`.
        SENDER_NAME = 2;
        /// `sender_addressing`.
        SENDER_ADDRESSING = 3;
        /// `recipient_name`.
        RECIPIENT_NAME = 4;
        /// `recipient_addressing`.
        RECIPIENT_ADDRESSING = 5;
        /// `type`.
        TYPE = 6;
        /// `size`.
        SIZE = 7;
        /// `reception_status`.
        RECEPTION_STATUS = 8;
        /// `text`.
        TEXT = 9;
        /// `attachment_size`.
        ATTACHMENT_SIZE = 10;
        /// `priority`.
        PRIORITY = 11;
        /// `read`.
        READ = 12;
        /// `sent`.
        SENT = 13;
        /// `protected`.
        PROTECTED = 14;
        /// `replyto_addressing`.
        REPLYTO_ADDRESSING = 15;
        /// `delivery_status`.
        DELIVERY_STATUS = 16;
        /// `conversation_id`.
        CONVERSATION_ID = 17;
        /// `conversation_name`.
        CONVERSATION_NAME = 18;
        /// `direction`.
        DIRECTION = 19;
        /// `attachment_mime_types`.
        ATTACHMENT_MIME = 20;
    }
);

bit_mask!(
    /// The attributes requested in a Conversation-Listing (`ConvParameterMask`, MAP
    /// section 5.13.3.10).
    ///
    /// The conversation id is always present. If any participant bit is set,
    /// [`PARTICIPANTS`](Self::PARTICIPANTS) must be set as well.
    ConvParameterMask(u32) {
        /// Conversation `name`.
        NAME = 0;
        /// Conversation `last_activity`.
        LAST_ACTIVITY = 1;
        /// Conversation `read_status`.
        READ_STATUS = 2;
        /// Conversation `version_counter`.
        VERSION_COUNTER = 3;
        /// Conversation `summary`.
        SUMMARY = 4;
        /// Participant elements.
        PARTICIPANTS = 5;
        /// Participant `uci`.
        PARTICIPANT_UCI = 6;
        /// Participant `display_name`.
        PARTICIPANT_DISPLAY_NAME = 7;
        /// Participant `chat_state`.
        PARTICIPANT_CHAT_STATE = 8;
        /// Participant `last_activity`.
        PARTICIPANT_LAST_ACTIVITY = 9;
        /// Participant `x_bt_uid`.
        PARTICIPANT_X_BT_UID = 10;
        /// Participant `name`.
        PARTICIPANT_NAME = 11;
        /// Participant `presence_availability`.
        PARTICIPANT_PRESENCE_AVAILABILITY = 12;
        /// Participant `presence_text`.
        PARTICIPANT_PRESENCE_TEXT = 13;
        /// Participant `priority`.
        PARTICIPANT_PRIORITY = 14;
    }
);

bit_mask!(
    /// The event reports an MCE wants to receive (`NotificationFilterMask`, MAP
    /// section 5.14.3.1). A cleared bit stops the MSE sending that event.
    NotificationFilterMask(u32) {
        /// `NewMessage`.
        NEW_MESSAGE = 0;
        /// `MessageDeleted`.
        MESSAGE_DELETED = 1;
        /// `MessageShift`.
        MESSAGE_SHIFT = 2;
        /// `SendingSuccess`.
        SENDING_SUCCESS = 3;
        /// `SendingFailure`.
        SENDING_FAILURE = 4;
        /// `DeliverySuccess`.
        DELIVERY_SUCCESS = 5;
        /// `DeliveryFailure`.
        DELIVERY_FAILURE = 6;
        /// `MemoryFull`.
        MEMORY_FULL = 7;
        /// `MemoryAvailable`.
        MEMORY_AVAILABLE = 8;
        /// `ReadStatusChanged`.
        READ_STATUS_CHANGED = 9;
        /// `ConversationChanged`.
        CONVERSATION_CHANGED = 10;
        /// `ParticipantPresenceChanged`.
        PARTICIPANT_PRESENCE_CHANGED = 11;
        /// `ParticipantChatStateChanged`.
        PARTICIPANT_CHAT_STATE_CHANGED = 12;
        /// `MessageExtendedDataChanged`.
        MESSAGE_EXTENDED_DATA_CHANGED = 13;
        /// `MessageRemoved`.
        MESSAGE_REMOVED = 14;
    }
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_lists_known_and_unknown_bits() {
        let features = MapSupportedFeatures::BROWSING | MapSupportedFeatures::from_bits(1 << 30);
        assert_eq!(
            "MapSupportedFeatures(BROWSING | 0x40000000)",
            format!("{:?}", features)
        );
        assert_eq!("ParameterMask()", format!("{:?}", ParameterMask::empty()));
    }

    #[test]
    fn masks_combine() {
        let mut mask = NotificationFilterMask::all();
        mask.remove(NotificationFilterMask::MEMORY_FULL);
        assert!(!mask.contains(NotificationFilterMask::MEMORY_FULL));
        assert_eq!(0x7FFF & !0x80, mask.bits());
        assert_eq!(0x1F, MessageTypeFilter::all().bits());
        assert_eq!(
            MessageTypeFilter::empty(),
            MessageTypeFilter::only(&[
                MessageType::Email,
                MessageType::SmsGsm,
                MessageType::SmsCdma,
                MessageType::Mms,
                MessageType::Im,
            ])
        );
    }
}
