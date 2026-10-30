//! ## Message Access Service Functions
//!
//! The parameters and results of the MAP functions (MAP section 5), shared by the
//! client ([`MasClient`](crate::client::MasClient)) and server
//! ([`MasServer`](crate::server::MasServer)), blocking and async alike.
//!
//! This module also holds the OBEX object types and the Application Parameters tags
//! MAP defines, for applications that drive [`obex_protocol`] directly.
//!
// The request and reply encoding below serves the role modules, which a build with
// neither `std` nor `embedded-io-async` leaves out.
#![cfg_attr(
    not(any(feature = "std", feature = "embedded-io-async")),
    allow(dead_code)
)]
use core::str::FromStr;

use obex_protocol::header::Header;
use obex_protocol::packet::{Request, Response, SetPathFlags};
use obex_protocol::response::ResponseCode;
use obex_protocol::tlv::ApplicationParameters;

use crate::bmessage::{BMessage, BMessageBody};
use crate::conversation_listing::ConversationListing;
use crate::error::{Error, Result};
use crate::event_report::EventReport;
use crate::features::{
    ConvParameterMask, MapSupportedFeatures, MessageTypeFilter, NotificationFilterMask,
    ParameterMask,
};
use crate::folder_listing::FolderListing;
use crate::messages_listing::{MessageEntry, MessagesListing};
use crate::prelude::*;
use crate::types::{
    Charset, ConversationId, DatabaseIdentifier, FractionDeliver, FractionRequest, MessageHandle,
    ModifyText, PriorityFilter, ReadStatusFilter, Timestamp, UtcOffset, VersionCounter,
};

/// Type of the Folder-Listing object, `x-obex/folder-listing`.
pub const FOLDER_LISTING_TYPE: &str = obex_protocol::target::FOLDER_LISTING_TYPE;
/// Type of the Messages-Listing object, `x-bt/MAP-msg-listing`.
pub const MESSAGES_LISTING_TYPE: &str = "x-bt/MAP-msg-listing";
/// Type of the bMessage object, `x-bt/message`.
pub const MESSAGE_TYPE: &str = "x-bt/message";
/// Type of SetMessageStatus, `x-bt/messageStatus`.
pub const MESSAGE_STATUS_TYPE: &str = "x-bt/messageStatus";
/// Type of SetNotificationRegistration, `x-bt/MAP-NotificationRegistration`.
pub const NOTIFICATION_REGISTRATION_TYPE: &str = "x-bt/MAP-NotificationRegistration";
/// Type of UpdateInbox, `x-bt/MAP-messageUpdate`.
pub const MESSAGE_UPDATE_TYPE: &str = "x-bt/MAP-messageUpdate";
/// Type of the MAP-Event-Report object, `x-bt/MAP-event-report`.
pub const EVENT_REPORT_TYPE: &str = "x-bt/MAP-event-report";
/// Type of GetMASInstanceInformation, `x-bt/MASInstanceInformation`.
pub const MAS_INSTANCE_INFORMATION_TYPE: &str = "x-bt/MASInstanceInformation";
/// Type of the Conversation-Listing object, `x-bt/MAP-convo-listing`.
pub const CONVERSATION_LISTING_TYPE: &str = "x-bt/MAP-convo-listing";
/// Type of SetOwnerStatus and GetOwnerStatus, `x-bt/ownerStatus`.
pub const OWNER_STATUS_TYPE: &str = "x-bt/ownerStatus";
/// Type of SetNotificationFilter, `x-bt/MAP-notification-filter`.
pub const NOTIFICATION_FILTER_TYPE: &str = "x-bt/MAP-notification-filter";

/// The body of PUT requests that carry no object, so they are not taken as a delete
/// (MAP section 5.2.4).
pub const FILLER_BYTE: u8 = 0x30;

/// The number of listing entries returned when `MaxListCount` is absent.
pub const DEFAULT_MAX_LIST_COUNT: u16 = 1024;

/// Application Parameters tags (MAP section 6.3.1, table 6.6).
pub mod tag {
    /// `MaxListCount`, 2 bytes.
    pub const MAX_LIST_COUNT: u8 = 0x01;
    /// `ListStartOffset`, 2 bytes.
    pub const LIST_START_OFFSET: u8 = 0x02;
    /// `FilterMessageType`, 1 byte bit mask.
    pub const FILTER_MESSAGE_TYPE: u8 = 0x03;
    /// `FilterPeriodBegin`, timestamp string.
    pub const FILTER_PERIOD_BEGIN: u8 = 0x04;
    /// `FilterPeriodEnd`, timestamp string.
    pub const FILTER_PERIOD_END: u8 = 0x05;
    /// `FilterReadStatus`, 1 byte.
    pub const FILTER_READ_STATUS: u8 = 0x06;
    /// `FilterRecipient`, UTF-8 text.
    pub const FILTER_RECIPIENT: u8 = 0x07;
    /// `FilterOriginator`, UTF-8 text.
    pub const FILTER_ORIGINATOR: u8 = 0x08;
    /// `FilterPriority`, 1 byte.
    pub const FILTER_PRIORITY: u8 = 0x09;
    /// `Attachment`, 1 byte on/off.
    pub const ATTACHMENT: u8 = 0x0A;
    /// `Transparent`, 1 byte on/off.
    pub const TRANSPARENT: u8 = 0x0B;
    /// `Retry`, 1 byte on/off.
    pub const RETRY: u8 = 0x0C;
    /// `NewMessage`, 1 byte on/off.
    pub const NEW_MESSAGE: u8 = 0x0D;
    /// `NotificationStatus`, 1 byte on/off.
    pub const NOTIFICATION_STATUS: u8 = 0x0E;
    /// `MASInstanceID`, 1 byte.
    pub const MAS_INSTANCE_ID: u8 = 0x0F;
    /// `ParameterMask`, 4 bytes.
    pub const PARAMETER_MASK: u8 = 0x10;
    /// `FolderListingSize`, 2 bytes.
    pub const FOLDER_LISTING_SIZE: u8 = 0x11;
    /// `ListingSize`, 2 bytes.
    pub const LISTING_SIZE: u8 = 0x12;
    /// `SubjectLength`, 1 byte.
    pub const SUBJECT_LENGTH: u8 = 0x13;
    /// `Charset`, 1 byte.
    pub const CHARSET: u8 = 0x14;
    /// `FractionRequest`, 1 byte.
    pub const FRACTION_REQUEST: u8 = 0x15;
    /// `FractionDeliver`, 1 byte.
    pub const FRACTION_DELIVER: u8 = 0x16;
    /// `StatusIndicator`, 1 byte.
    pub const STATUS_INDICATOR: u8 = 0x17;
    /// `StatusValue`, 1 byte.
    pub const STATUS_VALUE: u8 = 0x18;
    /// `MSETime`, timestamp string.
    pub const MSE_TIME: u8 = 0x19;
    /// `DatabaseIdentifier`, hexadecimal string.
    pub const DATABASE_IDENTIFIER: u8 = 0x1A;
    /// `ConversationListingVersionCounter`, hexadecimal string.
    pub const CONVERSATION_LISTING_VERSION_COUNTER: u8 = 0x1B;
    /// `PresenceAvailability`, 1 byte.
    pub const PRESENCE_AVAILABILITY: u8 = 0x1C;
    /// `PresenceText`, UTF-8 text.
    pub const PRESENCE_TEXT: u8 = 0x1D;
    /// `LastActivity`, timestamp string.
    pub const LAST_ACTIVITY: u8 = 0x1E;
    /// `FilterLastActivityBegin`, timestamp string.
    pub const FILTER_LAST_ACTIVITY_BEGIN: u8 = 0x1F;
    /// `FilterLastActivityEnd`, timestamp string.
    pub const FILTER_LAST_ACTIVITY_END: u8 = 0x20;
    /// `ChatState`, 1 byte.
    pub const CHAT_STATE: u8 = 0x21;
    /// `ConversationID`, hexadecimal string.
    pub const CONVERSATION_ID: u8 = 0x22;
    /// `FolderVersionCounter`, hexadecimal string.
    pub const FOLDER_VERSION_COUNTER: u8 = 0x23;
    /// `FilterMessageHandle`, hexadecimal string.
    pub const FILTER_MESSAGE_HANDLE: u8 = 0x24;
    /// `NotificationFilterMask`, 4 bytes.
    pub const NOTIFICATION_FILTER_MASK: u8 = 0x25;
    /// `ConvParameterMask`, 4 bytes.
    pub const CONV_PARAMETER_MASK: u8 = 0x26;
    /// `OwnerUCI`, UTF-8 text.
    pub const OWNER_UCI: u8 = 0x27;
    /// `ExtendedData`, UTF-8 text.
    pub const EXTENDED_DATA: u8 = 0x28;
    /// `MapSupportedFeatures`, 4 bytes.
    pub const MAP_SUPPORTED_FEATURES: u8 = 0x29;
    /// `MessageHandle`, hexadecimal string.
    pub const MESSAGE_HANDLE: u8 = 0x2A;
    /// `ModifyText`, 1 byte.
    pub const MODIFY_TEXT: u8 = 0x2B;
}

/// A navigation step of SetFolder (MAP section 5.3).
///
/// SETPATH moves one level at a time; use
/// [`MasClient::set_folder_path`](crate::client::MasClient::set_folder_path) to go to
/// a path such as `telecom/msg/inbox`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SetFolder {
    /// Go back to the root folder.
    Root,
    /// Go up one level.
    Parent,
    /// Go down into a child folder of the current folder.
    Child(String),
    /// Go up one level and then down into `name`, a sibling of the current folder.
    Sibling(String),
}

impl SetFolder {
    /// The SETPATH flags and Name header for this step. MAP never creates folders.
    pub fn to_set_path(&self) -> (SetPathFlags, Option<&str>) {
        let flags = |backup| SetPathFlags {
            backup,
            no_create: true,
        };

        match self {
            SetFolder::Root => (flags(false), Some("")),
            SetFolder::Parent => (flags(true), None),
            SetFolder::Child(name) => (flags(false), Some(name)),
            SetFolder::Sibling(name) => (flags(true), Some(name)),
        }
    }

    /// Interprets a SETPATH request.
    pub fn from_set_path(flags: SetPathFlags, name: Option<&str>) -> SetFolder {
        let name = name.filter(|name| !name.is_empty()).map(str::to_string);

        match (flags.backup, name) {
            (false, None) => SetFolder::Root,
            (false, Some(name)) => SetFolder::Child(name),
            (true, None) => SetFolder::Parent,
            (true, Some(name)) => SetFolder::Sibling(name),
        }
    }
}

/// Parameters of GetFolderListing (MAP section 5.4).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct FolderListingParams {
    /// The most folders to list; 1024 when absent.
    pub max_list_count: Option<u16>,
    /// The number of folders to skip.
    pub list_start_offset: Option<u16>,
}

impl FolderListingParams {
    fn to_parameters(&self) -> ApplicationParameters {
        let mut parameters = ApplicationParameters::new();
        put_u16(&mut parameters, tag::MAX_LIST_COUNT, self.max_list_count);
        put_u16(
            &mut parameters,
            tag::LIST_START_OFFSET,
            self.list_start_offset,
        );
        parameters
    }

    fn from_parameters(
        parameters: &Parameters<'_>,
    ) -> core::result::Result<FolderListingParams, Invalid> {
        Ok(FolderListingParams {
            max_list_count: parameters.u16(tag::MAX_LIST_COUNT)?,
            list_start_offset: parameters.u16(tag::LIST_START_OFFSET)?,
        })
    }

    /// Applies `ListStartOffset` and `MaxListCount` to the complete list of folders.
    pub fn apply(&self, listing: FolderListing) -> FolderListing {
        let offset = usize::from(self.list_start_offset.unwrap_or(0));
        let count = usize::from(self.max_list_count.unwrap_or(DEFAULT_MAX_LIST_COUNT));

        FolderListing::new(listing.folders.into_iter().skip(offset).take(count))
    }
}

/// Parameters of GetMessagesListing (MAP section 5.5).
///
/// Filters combine with AND. All are optional; the defaults list every message.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct MessagesListingParams {
    /// The most messages to list; 1024 when absent. 0 asks only for the listing size.
    pub max_list_count: Option<u16>,
    /// The number of messages to skip.
    pub list_start_offset: Option<u16>,
    /// The longest `subject` to return, 1 to 255.
    pub subject_length: Option<u8>,
    /// The attributes to return.
    pub parameter_mask: Option<ParameterMask>,
    /// Message types to leave out.
    pub filter_message_type: Option<MessageTypeFilter>,
    /// Only messages from this time on.
    pub filter_period_begin: Option<Timestamp>,
    /// Only messages older than this time.
    pub filter_period_end: Option<Timestamp>,
    /// Only read or only unread messages.
    pub filter_read_status: Option<ReadStatusFilter>,
    /// Only messages with a recipient matching this text.
    pub filter_recipient: Option<String>,
    /// Only messages with an originator matching this text.
    pub filter_originator: Option<String>,
    /// Only high or only non-high priority messages.
    pub filter_priority: Option<PriorityFilter>,
    /// Only messages of this conversation, from every folder.
    pub conversation_id: Option<ConversationId>,
    /// Only the message with this handle, from any folder.
    pub filter_message_handle: Option<MessageHandle>,
}

impl MessagesListingParams {
    fn to_parameters(&self) -> ApplicationParameters {
        let mut parameters = ApplicationParameters::new();
        put_u16(&mut parameters, tag::MAX_LIST_COUNT, self.max_list_count);
        put_u16(
            &mut parameters,
            tag::LIST_START_OFFSET,
            self.list_start_offset,
        );
        put_u8(&mut parameters, tag::SUBJECT_LENGTH, self.subject_length);
        put_u32(
            &mut parameters,
            tag::PARAMETER_MASK,
            self.parameter_mask.map(ParameterMask::bits),
        );
        put_u8(
            &mut parameters,
            tag::FILTER_MESSAGE_TYPE,
            self.filter_message_type.map(MessageTypeFilter::bits),
        );
        put_text(
            &mut parameters,
            tag::FILTER_PERIOD_BEGIN,
            self.filter_period_begin,
        );
        put_text(
            &mut parameters,
            tag::FILTER_PERIOD_END,
            self.filter_period_end,
        );
        put_u8(
            &mut parameters,
            tag::FILTER_READ_STATUS,
            self.filter_read_status.map(read_status_value),
        );
        put_text(
            &mut parameters,
            tag::FILTER_RECIPIENT,
            self.filter_recipient.as_deref(),
        );
        put_text(
            &mut parameters,
            tag::FILTER_ORIGINATOR,
            self.filter_originator.as_deref(),
        );
        put_u8(
            &mut parameters,
            tag::FILTER_PRIORITY,
            self.filter_priority.map(|priority| match priority {
                PriorityFilter::High => 1,
                PriorityFilter::NonHigh => 2,
            }),
        );
        put_text(&mut parameters, tag::CONVERSATION_ID, self.conversation_id);
        put_text(
            &mut parameters,
            tag::FILTER_MESSAGE_HANDLE,
            self.filter_message_handle,
        );
        parameters
    }

    fn from_parameters(
        parameters: &Parameters<'_>,
    ) -> core::result::Result<MessagesListingParams, Invalid> {
        Ok(MessagesListingParams {
            max_list_count: parameters.u16(tag::MAX_LIST_COUNT)?,
            list_start_offset: parameters.u16(tag::LIST_START_OFFSET)?,
            subject_length: parameters.u8(tag::SUBJECT_LENGTH)?,
            parameter_mask: parameters
                .u32(tag::PARAMETER_MASK)?
                .map(ParameterMask::from_bits),
            filter_message_type: parameters
                .u8(tag::FILTER_MESSAGE_TYPE)?
                .map(MessageTypeFilter::from_bits),
            filter_period_begin: parameters.parsed(tag::FILTER_PERIOD_BEGIN)?,
            filter_period_end: parameters.parsed(tag::FILTER_PERIOD_END)?,
            filter_read_status: read_status_filter(parameters.u8(tag::FILTER_READ_STATUS)?),
            filter_recipient: parameters.text(tag::FILTER_RECIPIENT),
            filter_originator: parameters.text(tag::FILTER_ORIGINATOR),
            filter_priority: match parameters.u8(tag::FILTER_PRIORITY)? {
                Some(1) => Some(PriorityFilter::High),
                Some(2) => Some(PriorityFilter::NonHigh),
                _ => None,
            },
            conversation_id: parameters.parsed(tag::CONVERSATION_ID)?,
            filter_message_handle: parameters.parsed(tag::FILTER_MESSAGE_HANDLE)?,
        })
    }

    /// Whether `message` passes every filter of these parameters.
    ///
    /// Recipients and originators are matched as case insensitive substrings of the
    /// name and address attributes; `*` wildcards are ignored. Periods compare the
    /// local date and time of `datetime`.
    pub fn matches(&self, message: &MessageEntry) -> bool {
        let local_time = |timestamp: Timestamp| timestamp.with_offset(UtcOffset::Local).to_string();
        let message_time = message
            .datetime
            .as_deref()
            .and_then(|datetime| datetime.parse::<Timestamp>().ok())
            .map(local_time);

        let in_period = match (self.filter_period_begin, self.filter_period_end) {
            (None, None) => true,
            (begin, end) => message_time.is_some_and(|time| {
                begin.is_none_or(|begin| time >= local_time(begin))
                    && end.is_none_or(|end| time < local_time(end))
            }),
        };

        self.filter_message_handle
            .is_none_or(|handle| message.handle == handle)
            && self
                .conversation_id
                .is_none_or(|id| message.conversation_id == Some(id))
            && self.filter_message_type.is_none_or(|filter| {
                message
                    .message_type
                    .is_none_or(|message_type| filter.includes(message_type))
            })
            && in_period
            && match self.filter_read_status {
                None => true,
                Some(ReadStatusFilter::Read) => message.read == Some(true),
                Some(ReadStatusFilter::Unread) => message.read != Some(true),
            }
            && match self.filter_priority {
                None => true,
                Some(PriorityFilter::High) => message.priority == Some(true),
                Some(PriorityFilter::NonHigh) => message.priority != Some(true),
            }
            && matches_text(
                self.filter_recipient.as_deref(),
                [&message.recipient_name, &message.recipient_addressing],
            )
            && matches_text(
                self.filter_originator.as_deref(),
                [&message.sender_name, &message.sender_addressing],
            )
    }

    /// Builds the complete response from every message of the requested folder, for
    /// MSE implementations: filters, sorts newest first, counts, segments with
    /// `ListStartOffset` and `MaxListCount`, shortens subjects and applies the
    /// `ParameterMask`.
    ///
    /// `MSETime`, the database identifier and the folder version counter are left for
    /// the caller to fill in.
    ///
    /// # Examples
    /// ```
    /// # use obex_protocol_map::mas::MessagesListingParams;
    /// # use obex_protocol_map::messages_listing::MessageEntry;
    /// # use obex_protocol_map::types::{MessageHandle, ReadStatusFilter};
    /// let mut unread = MessageEntry::new(MessageHandle(1));
    /// unread.read = Some(false);
    /// let mut read = MessageEntry::new(MessageHandle(2));
    /// read.read = Some(true);
    ///
    /// let params = MessagesListingParams {
    ///     filter_read_status: Some(ReadStatusFilter::Unread),
    ///     ..MessagesListingParams::default()
    /// };
    /// let response = params.apply(vec![unread, read]);
    ///
    /// assert_eq!(Some(1), response.listing_size);
    /// assert_eq!(Some(true), response.new_message);
    /// assert_eq!(MessageHandle(1), response.listing.unwrap().messages[0].handle);
    /// ```
    pub fn apply(
        &self,
        messages: impl IntoIterator<Item = MessageEntry>,
    ) -> MessagesListingResponse {
        let mut selected: Vec<MessageEntry> = messages
            .into_iter()
            .filter(|message| self.matches(message))
            .collect();
        selected.sort_by(|a, b| b.datetime.cmp(&a.datetime));

        let listing_size = u16::try_from(selected.len()).unwrap_or(u16::MAX);
        let new_message = selected.iter().any(|message| message.read == Some(false));

        let listing = match self.max_list_count {
            Some(0) => None,
            count => Some(MessagesListing::new(
                selected
                    .into_iter()
                    .skip(usize::from(self.list_start_offset.unwrap_or(0)))
                    .take(usize::from(count.unwrap_or(DEFAULT_MAX_LIST_COUNT)))
                    .map(|message| self.shape(message))
                    .collect(),
            )),
        };

        MessagesListingResponse {
            listing,
            new_message: Some(new_message),
            listing_size: Some(listing_size),
            ..MessagesListingResponse::default()
        }
    }

    /// Shortens the subject and drops the attributes the parameter mask leaves out.
    fn shape(&self, mut message: MessageEntry) -> MessageEntry {
        if let (Some(length), Some(subject)) = (self.subject_length, message.subject.as_mut()) {
            truncate(subject, usize::from(length.max(1)));
        }

        let mask = match self.parameter_mask {
            Some(mask) if !mask.is_empty() => mask,
            _ => return message,
        };

        macro_rules! keep {
            ($($bit:ident => $field:ident),* $(,)?) => {
                $(
                    if !mask.contains(ParameterMask::$bit) {
                        message.$field = Default::default();
                    }
                )*
            };
        }

        keep!(
            SUBJECT => subject,
            DATETIME => datetime,
            SENDER_NAME => sender_name,
            SENDER_ADDRESSING => sender_addressing,
            RECIPIENT_NAME => recipient_name,
            RECIPIENT_ADDRESSING => recipient_addressing,
            TYPE => message_type,
            SIZE => size,
            RECEPTION_STATUS => reception_status,
            TEXT => text,
            ATTACHMENT_SIZE => attachment_size,
            PRIORITY => priority,
            READ => read,
            SENT => sent,
            PROTECTED => protected,
            REPLYTO_ADDRESSING => replyto_addressing,
            DELIVERY_STATUS => delivery_status,
            CONVERSATION_ID => conversation_id,
            CONVERSATION_NAME => conversation_name,
            DIRECTION => direction,
            ATTACHMENT_MIME => attachment_mime_types,
        );

        message
    }
}

/// Whether one of `fields` contains `pattern`, ignoring case and `*` wildcards.
fn matches_text(pattern: Option<&str>, fields: [&Option<String>; 2]) -> bool {
    let Some(pattern) = pattern else {
        return true;
    };

    let pattern = pattern.replace('*', "").trim().to_lowercase();
    pattern.is_empty()
        || fields
            .into_iter()
            .flatten()
            .any(|field| field.to_lowercase().contains(&pattern))
}

/// Truncates `text` to at most `length` bytes on a character boundary.
fn truncate(text: &mut String, length: usize) {
    if text.len() > length {
        let mut end = length;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
}

/// The result of GetMessagesListing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MessagesListingResponse {
    /// The listing; absent when `MaxListCount` was 0.
    pub listing: Option<MessagesListing>,
    /// Whether there are unread messages in the listing (`NewMessage`).
    pub new_message: Option<bool>,
    /// The MSE's current local time and UTC offset (`MSETime`).
    ///
    /// Servers fill in the current UTC time when this is `None`, except without
    /// `std`, where there is no clock and stores should set it.
    pub mse_time: Option<String>,
    /// The number of messages matching the filters (`ListingSize`).
    pub listing_size: Option<u16>,
    /// The database identifier.
    pub database_identifier: Option<DatabaseIdentifier>,
    /// The Folder Version Counter.
    pub folder_version_counter: Option<VersionCounter>,
}

/// Parameters of GetMessage (MAP section 5.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GetMessageParams {
    /// Whether to include attachments.
    pub attachment: bool,
    /// Whether to transcode text to UTF-8; SMS can also be fetched as native PDUs.
    pub charset: Charset,
}

impl Default for GetMessageParams {
    /// UTF-8 text without attachments.
    fn default() -> GetMessageParams {
        GetMessageParams {
            attachment: false,
            charset: Charset::Utf8,
        }
    }
}

/// What a GetMessage request with `FractionRequest` returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FractionContent {
    /// The first fraction, as a complete bMessage.
    Message(Box<BMessage>),
    /// A following fraction, as a lone body.
    Body(BMessageBody),
}

/// A fraction of a fractioned e-mail (MAP section 5.6.4.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageFraction {
    /// The delivered fraction.
    pub content: FractionContent,
    /// Whether more fractions follow.
    pub deliver: Option<FractionDeliver>,
}

/// A status change made by SetMessageStatus (MAP section 5.7).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MessageStatus {
    /// Marks the message read (`true`) or unread.
    Read(bool),
    /// Moves the message to the Deleted folder (`true`) or back out of it.
    Deleted(bool),
    /// Replaces the message's extended data (MAP section 3.1.13).
    ExtendedData(String),
}

/// Forwarding an existing message with PushMessage (Message Forwarding feature,
/// MAP section 5.8.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Forward {
    /// The message to forward.
    pub message_handle: MessageHandle,
    /// Whether to forward its attachments.
    pub attachment: bool,
    /// How the pushed text combines with the original text.
    pub modify_text: ModifyText,
}

/// Parameters of PushMessage (MAP section 5.8).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct PushMessageParams {
    /// `true` asks the MSE not to keep a copy in the Sent folder; off by default.
    pub transparent: Option<bool>,
    /// Whether to retry sending if the network is unavailable; on by default.
    pub retry: Option<bool>,
    /// The charset of the pushed bMessage.
    pub charset: Charset,
    /// Adds the message to this conversation instead of its recipients.
    pub conversation_id: Option<ConversationId>,
    /// Forwards an existing message.
    pub forward: Option<Forward>,
}

impl PushMessageParams {
    fn to_parameters(&self) -> ApplicationParameters {
        let mut parameters = ApplicationParameters::new();
        put_flag(&mut parameters, tag::TRANSPARENT, self.transparent);
        put_flag(&mut parameters, tag::RETRY, self.retry);
        put_u8(
            &mut parameters,
            tag::CHARSET,
            Some(charset_value(self.charset)),
        );
        put_text(&mut parameters, tag::CONVERSATION_ID, self.conversation_id);

        if let Some(forward) = &self.forward {
            put_text(
                &mut parameters,
                tag::MESSAGE_HANDLE,
                Some(forward.message_handle),
            );
            put_flag(&mut parameters, tag::ATTACHMENT, Some(forward.attachment));
            put_u8(
                &mut parameters,
                tag::MODIFY_TEXT,
                Some(match forward.modify_text {
                    ModifyText::Replace => 0,
                    ModifyText::Prepend => 1,
                }),
            );
        }
        parameters
    }

    fn from_parameters(
        parameters: &Parameters<'_>,
    ) -> core::result::Result<PushMessageParams, Invalid> {
        let forward = match parameters.parsed(tag::MESSAGE_HANDLE)? {
            Some(message_handle) => Some(Forward {
                message_handle,
                attachment: parameters.flag(tag::ATTACHMENT)?.unwrap_or(false),
                modify_text: match parameters.u8(tag::MODIFY_TEXT)? {
                    Some(1) => ModifyText::Prepend,
                    _ => ModifyText::Replace,
                },
            }),
            None => None,
        };

        Ok(PushMessageParams {
            transparent: parameters.flag(tag::TRANSPARENT)?,
            retry: parameters.flag(tag::RETRY)?,
            charset: charset(parameters.u8(tag::CHARSET)?),
            conversation_id: parameters.parsed(tag::CONVERSATION_ID)?,
            forward,
        })
    }
}

/// The result of GetMASInstanceInformation (MAP section 5.10).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct MasInstanceInformation {
    /// User readable description of the MAS instance, at most 199 bytes.
    pub information: String,
    /// The UCI of the instance's owner, for accounts shared with PBAP.
    pub owner_uci: Option<String>,
}

/// Parameters of GetConversationListing (MAP section 5.13).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ConversationListingParams {
    /// The most conversations to list; 1024 when absent. 0 asks only for the size.
    pub max_list_count: Option<u16>,
    /// The number of conversations to skip.
    pub list_start_offset: Option<u16>,
    /// Only conversations active from this time on.
    pub filter_last_activity_begin: Option<Timestamp>,
    /// Only conversations last active before this time.
    pub filter_last_activity_end: Option<Timestamp>,
    /// Only read or only unread conversations.
    pub filter_read_status: Option<ReadStatusFilter>,
    /// Only conversations with a participant matching this text.
    pub filter_recipient: Option<String>,
    /// Only this conversation.
    pub conversation_id: Option<ConversationId>,
    /// The attributes to return.
    pub conv_parameter_mask: Option<ConvParameterMask>,
}

impl ConversationListingParams {
    fn to_parameters(&self) -> ApplicationParameters {
        let mut parameters = ApplicationParameters::new();
        put_u16(&mut parameters, tag::MAX_LIST_COUNT, self.max_list_count);
        put_u16(
            &mut parameters,
            tag::LIST_START_OFFSET,
            self.list_start_offset,
        );
        put_text(
            &mut parameters,
            tag::FILTER_LAST_ACTIVITY_BEGIN,
            self.filter_last_activity_begin,
        );
        put_text(
            &mut parameters,
            tag::FILTER_LAST_ACTIVITY_END,
            self.filter_last_activity_end,
        );
        put_u8(
            &mut parameters,
            tag::FILTER_READ_STATUS,
            self.filter_read_status.map(read_status_value),
        );
        put_text(
            &mut parameters,
            tag::FILTER_RECIPIENT,
            self.filter_recipient.as_deref(),
        );
        put_text(&mut parameters, tag::CONVERSATION_ID, self.conversation_id);
        put_u32(
            &mut parameters,
            tag::CONV_PARAMETER_MASK,
            self.conv_parameter_mask.map(ConvParameterMask::bits),
        );
        parameters
    }

    fn from_parameters(
        parameters: &Parameters<'_>,
    ) -> core::result::Result<ConversationListingParams, Invalid> {
        Ok(ConversationListingParams {
            max_list_count: parameters.u16(tag::MAX_LIST_COUNT)?,
            list_start_offset: parameters.u16(tag::LIST_START_OFFSET)?,
            filter_last_activity_begin: parameters.parsed(tag::FILTER_LAST_ACTIVITY_BEGIN)?,
            filter_last_activity_end: parameters.parsed(tag::FILTER_LAST_ACTIVITY_END)?,
            filter_read_status: read_status_filter(parameters.u8(tag::FILTER_READ_STATUS)?),
            filter_recipient: parameters.text(tag::FILTER_RECIPIENT),
            conversation_id: parameters.parsed(tag::CONVERSATION_ID)?,
            conv_parameter_mask: parameters
                .u32(tag::CONV_PARAMETER_MASK)?
                .map(ConvParameterMask::from_bits),
        })
    }
}

/// The result of GetConversationListing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConversationListingResponse {
    /// The listing; absent when `MaxListCount` was 0.
    pub listing: Option<ConversationListing>,
    /// The Conversation-Listing Version Counter.
    pub version_counter: Option<VersionCounter>,
    /// The number of conversations matching the filters (`ListingSize`).
    pub listing_size: Option<u16>,
    /// The database identifier.
    pub database_identifier: Option<DatabaseIdentifier>,
    /// The MSE's current local time and UTC offset (`MSETime`).
    pub mse_time: Option<String>,
}

/// Presence and chat state of the MSE's owner, for SetOwnerStatus and GetOwnerStatus
/// (MAP sections 5.11 and 5.12).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct OwnerStatus {
    /// Presence availability, a value from the MAP assigned numbers.
    pub presence_availability: Option<u8>,
    /// User defined presence text.
    pub presence_text: Option<String>,
    /// Timestamp of the owner's last activity.
    pub last_activity: Option<String>,
    /// Chat state, a value from the MAP assigned numbers.
    pub chat_state: Option<u8>,
    /// The conversation the chat state applies to.
    pub conversation_id: Option<ConversationId>,
}

impl OwnerStatus {
    fn to_parameters(&self) -> ApplicationParameters {
        let mut parameters = ApplicationParameters::new();
        put_u8(
            &mut parameters,
            tag::PRESENCE_AVAILABILITY,
            self.presence_availability,
        );
        put_text(
            &mut parameters,
            tag::PRESENCE_TEXT,
            self.presence_text.as_deref(),
        );
        put_text(
            &mut parameters,
            tag::LAST_ACTIVITY,
            self.last_activity.as_deref(),
        );
        put_u8(&mut parameters, tag::CHAT_STATE, self.chat_state);
        put_text(&mut parameters, tag::CONVERSATION_ID, self.conversation_id);
        parameters
    }

    fn from_parameters(parameters: &Parameters<'_>) -> core::result::Result<OwnerStatus, Invalid> {
        Ok(OwnerStatus {
            presence_availability: parameters.u8(tag::PRESENCE_AVAILABILITY)?,
            presence_text: parameters.text(tag::PRESENCE_TEXT),
            last_activity: parameters.text(tag::LAST_ACTIVITY),
            chat_state: parameters.u8(tag::CHAT_STATE)?,
            conversation_id: parameters.parsed(tag::CONVERSATION_ID)?,
        })
    }
}

// Encoding of Application Parameters values.

fn put_u8(parameters: &mut ApplicationParameters, tag: u8, value: Option<u8>) {
    if let Some(value) = value {
        parameters.push(obex_protocol::tlv::TagLengthValue::new(tag, [value]));
    }
}

fn put_u16(parameters: &mut ApplicationParameters, tag: u8, value: Option<u16>) {
    if let Some(value) = value {
        parameters.push(obex_protocol::tlv::TagLengthValue::new(
            tag,
            value.to_be_bytes(),
        ));
    }
}

fn put_u32(parameters: &mut ApplicationParameters, tag: u8, value: Option<u32>) {
    if let Some(value) = value {
        parameters.push(obex_protocol::tlv::TagLengthValue::new(
            tag,
            value.to_be_bytes(),
        ));
    }
}

fn put_flag(parameters: &mut ApplicationParameters, tag: u8, value: Option<bool>) {
    put_u8(parameters, tag, value.map(u8::from));
}

/// Text values, including timestamps and hexadecimal identifiers, are UTF-8 without
/// a null terminator.
fn put_text<T: ToString>(parameters: &mut ApplicationParameters, tag: u8, value: Option<T>) {
    if let Some(value) = value {
        parameters.push(obex_protocol::tlv::TagLengthValue::new(
            tag,
            value.to_string().into_bytes(),
        ));
    }
}

fn read_status_value(filter: ReadStatusFilter) -> u8 {
    match filter {
        ReadStatusFilter::Unread => 1,
        ReadStatusFilter::Read => 2,
    }
}

fn read_status_filter(value: Option<u8>) -> Option<ReadStatusFilter> {
    match value {
        Some(1) => Some(ReadStatusFilter::Unread),
        Some(2) => Some(ReadStatusFilter::Read),
        _ => None,
    }
}

fn charset_value(charset: Charset) -> u8 {
    match charset {
        Charset::Native => 0,
        Charset::Utf8 => 1,
    }
}

fn charset(value: Option<u8>) -> Charset {
    match value {
        Some(0) => Charset::Native,
        _ => Charset::Utf8,
    }
}

/// An Application Parameter whose value has the wrong length or format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Invalid(u8);

impl From<Invalid> for ResponseCode {
    fn from(_: Invalid) -> ResponseCode {
        ResponseCode::BadRequest
    }
}

impl From<Invalid> for Error {
    fn from(invalid: Invalid) -> Error {
        Error::malformed(
            "application parameter",
            format!("invalid value for tag 0x{:02X}", invalid.0),
        )
    }
}

/// Typed access to the Application Parameters of a request or response.
pub(crate) struct Parameters<'a>(Option<&'a ApplicationParameters>);

impl<'a> Parameters<'a> {
    pub(crate) fn of(headers: &'a obex_protocol::header::Headers) -> Parameters<'a> {
        Parameters(headers.application_parameters())
    }

    fn bytes(&self, tag: u8) -> Option<&'a [u8]> {
        self.0.and_then(|parameters| parameters.get_bytes(tag))
    }

    fn array<const N: usize>(&self, tag: u8) -> core::result::Result<Option<[u8; N]>, Invalid> {
        self.bytes(tag)
            .map(|value| value.try_into().map_err(|_| Invalid(tag)))
            .transpose()
    }

    pub(crate) fn u8(&self, tag: u8) -> core::result::Result<Option<u8>, Invalid> {
        Ok(self.array::<1>(tag)?.map(u8::from_be_bytes))
    }

    pub(crate) fn u16(&self, tag: u8) -> core::result::Result<Option<u16>, Invalid> {
        Ok(self.array::<2>(tag)?.map(u16::from_be_bytes))
    }

    pub(crate) fn u32(&self, tag: u8) -> core::result::Result<Option<u32>, Invalid> {
        Ok(self.array::<4>(tag)?.map(u32::from_be_bytes))
    }

    pub(crate) fn flag(&self, tag: u8) -> core::result::Result<Option<bool>, Invalid> {
        Ok(self.u8(tag)?.map(|value| value != 0))
    }

    /// A text value, dropping null terminators some devices add.
    pub(crate) fn text(&self, tag: u8) -> Option<String> {
        self.bytes(tag).map(|value| {
            String::from_utf8_lossy(value)
                .trim_end_matches('\0')
                .to_string()
        })
    }

    pub(crate) fn parsed<T: FromStr>(&self, tag: u8) -> core::result::Result<Option<T>, Invalid> {
        self.text(tag)
            .filter(|text| !text.trim().is_empty())
            .map(|text| text.trim().parse().map_err(|_| Invalid(tag)))
            .transpose()
    }
}

// Client side: building requests and reading responses.

/// Headers of a PUT request and its body.
pub(crate) struct Put {
    pub(crate) headers: Vec<Header>,
    pub(crate) body: Vec<u8>,
}

impl Put {
    fn filler(content_type: &str, parameters: ApplicationParameters) -> Put {
        Put {
            headers: typed(content_type, None, parameters),
            body: vec![FILLER_BYTE],
        }
    }
}

/// Type, Name and Application Parameters headers, leaving out empty parameters.
fn typed(
    content_type: &str,
    name: Option<String>,
    parameters: ApplicationParameters,
) -> Vec<Header> {
    let mut headers = Vec::new();
    if let Some(name) = name {
        headers.push(Header::Name(name));
    }
    headers.push(Header::Type(content_type.to_string()));
    if !parameters.is_empty() {
        headers.push(Header::ApplicationParameters(parameters));
    }
    headers
}

/// The headers of a CONNECT request to `target`.
pub(crate) fn connect_request(
    target: &[u8],
    features: Option<MapSupportedFeatures>,
) -> Vec<Header> {
    let mut headers = vec![Header::Target(target.to_vec())];
    if let Some(features) = features {
        let mut parameters = ApplicationParameters::new();
        put_u32(
            &mut parameters,
            tag::MAP_SUPPORTED_FEATURES,
            Some(features.bits()),
        );
        headers.push(Header::ApplicationParameters(parameters));
    }
    headers
}

/// Checks that CONNECT reached the requested service.
pub(crate) fn check_connect(response: &Response, target: &[u8]) -> Result<()> {
    if response.headers.connection_id().is_none() {
        return Err(Error::MissingParameter("Connection Id"));
    }

    match response.headers.who() {
        Some(who) if who != target => Err(Error::UnexpectedService),
        _ => Ok(()),
    }
}

pub(crate) fn notification_registration_request(enabled: bool) -> Put {
    let mut parameters = ApplicationParameters::new();
    put_flag(&mut parameters, tag::NOTIFICATION_STATUS, Some(enabled));
    Put::filler(NOTIFICATION_REGISTRATION_TYPE, parameters)
}

pub(crate) fn notification_filter_request(mask: NotificationFilterMask) -> Put {
    let mut parameters = ApplicationParameters::new();
    put_u32(
        &mut parameters,
        tag::NOTIFICATION_FILTER_MASK,
        Some(mask.bits()),
    );
    Put::filler(NOTIFICATION_FILTER_TYPE, parameters)
}

pub(crate) fn message_status_request(handle: MessageHandle, status: &MessageStatus) -> Put {
    let mut parameters = ApplicationParameters::new();
    let (indicator, value) = match status {
        MessageStatus::Read(read) => (0, *read),
        MessageStatus::Deleted(deleted) => (1, *deleted),
        MessageStatus::ExtendedData(_) => (2, true),
    };
    put_u8(&mut parameters, tag::STATUS_INDICATOR, Some(indicator));
    put_flag(&mut parameters, tag::STATUS_VALUE, Some(value));
    if let MessageStatus::ExtendedData(data) = status {
        put_text(&mut parameters, tag::EXTENDED_DATA, Some(data));
    }

    Put {
        headers: typed(MESSAGE_STATUS_TYPE, Some(handle.to_string()), parameters),
        body: vec![FILLER_BYTE],
    }
}

pub(crate) fn push_message_request(
    folder: &str,
    message: &BMessage,
    params: &PushMessageParams,
) -> Put {
    Put {
        headers: typed(
            MESSAGE_TYPE,
            Some(folder.to_string()),
            params.to_parameters(),
        ),
        body: message.to_bytes(),
    }
}

pub(crate) fn update_inbox_request() -> Put {
    Put::filler(MESSAGE_UPDATE_TYPE, ApplicationParameters::new())
}

pub(crate) fn set_owner_status_request(status: &OwnerStatus) -> Result<Put> {
    let parameters = status.to_parameters();
    if parameters.is_empty() {
        return Err(Error::InvalidArgument(
            "SetOwnerStatus needs at least one value",
        ));
    }
    Ok(Put::filler(OWNER_STATUS_TYPE, parameters))
}

/// Reads the handle PushMessage assigned from its response.
pub(crate) fn push_message_response(response: &Response) -> Result<MessageHandle> {
    response
        .headers
        .name()
        .ok_or(Error::MissingParameter(
            "Name header with the message handle",
        ))?
        .parse()
}

pub(crate) fn folder_listing_request(params: &FolderListingParams) -> Vec<Header> {
    typed(FOLDER_LISTING_TYPE, None, params.to_parameters())
}

pub(crate) fn folder_listing_response(
    response: &obex_protocol::packet::GetResponse,
) -> Result<FolderListing> {
    FolderListing::parse(&response.body)
}

pub(crate) fn folder_listing_size_request() -> Vec<Header> {
    folder_listing_request(&FolderListingParams {
        max_list_count: Some(0),
        list_start_offset: None,
    })
}

pub(crate) fn folder_listing_size_response(
    response: &obex_protocol::packet::GetResponse,
) -> Result<u16> {
    Parameters::of(&response.headers)
        .u16(tag::FOLDER_LISTING_SIZE)?
        .ok_or(Error::MissingParameter("FolderListingSize"))
}

pub(crate) fn messages_listing_request(
    folder: &str,
    params: &MessagesListingParams,
) -> Vec<Header> {
    typed(
        MESSAGES_LISTING_TYPE,
        Some(folder.to_string()),
        params.to_parameters(),
    )
}

pub(crate) fn messages_listing_response(
    response: &obex_protocol::packet::GetResponse,
) -> Result<MessagesListingResponse> {
    let parameters = Parameters::of(&response.headers);

    Ok(MessagesListingResponse {
        listing: match response.body.is_empty() {
            true => None,
            false => Some(MessagesListing::parse(&response.body)?),
        },
        new_message: parameters.flag(tag::NEW_MESSAGE)?,
        mse_time: parameters.text(tag::MSE_TIME),
        listing_size: parameters.u16(tag::LISTING_SIZE)?,
        database_identifier: parameters.parsed(tag::DATABASE_IDENTIFIER)?,
        folder_version_counter: parameters.parsed(tag::FOLDER_VERSION_COUNTER)?,
    })
}

pub(crate) fn message_request(
    handle: MessageHandle,
    params: &GetMessageParams,
    fraction: Option<FractionRequest>,
) -> Vec<Header> {
    let mut parameters = ApplicationParameters::new();
    put_flag(&mut parameters, tag::ATTACHMENT, Some(params.attachment));
    put_u8(
        &mut parameters,
        tag::CHARSET,
        Some(charset_value(params.charset)),
    );
    put_u8(
        &mut parameters,
        tag::FRACTION_REQUEST,
        fraction.map(|fraction| match fraction {
            FractionRequest::First => 0,
            FractionRequest::Next => 1,
        }),
    );
    typed(MESSAGE_TYPE, Some(handle.to_string()), parameters)
}

pub(crate) fn message_response(response: &obex_protocol::packet::GetResponse) -> Result<BMessage> {
    BMessage::parse(&response.body)
}

pub(crate) fn message_fraction_response(
    response: &obex_protocol::packet::GetResponse,
) -> Result<MessageFraction> {
    let deliver = match Parameters::of(&response.headers).u8(tag::FRACTION_DELIVER)? {
        Some(0) => Some(FractionDeliver::More),
        Some(_) => Some(FractionDeliver::Last),
        None => None,
    };

    let starts_with_bmsg = String::from_utf8_lossy(&response.body)
        .trim_start_matches('\u{FEFF}')
        .trim_start()
        .to_ascii_uppercase()
        .starts_with("BEGIN:BMSG");

    let content = if starts_with_bmsg {
        FractionContent::Message(Box::new(BMessage::parse(&response.body)?))
    } else {
        FractionContent::Body(BMessageBody::parse(&response.body)?)
    };

    Ok(MessageFraction { content, deliver })
}

pub(crate) fn mas_instance_information_request(instance_id: u8) -> Vec<Header> {
    let mut parameters = ApplicationParameters::new();
    put_u8(&mut parameters, tag::MAS_INSTANCE_ID, Some(instance_id));
    typed(MAS_INSTANCE_INFORMATION_TYPE, None, parameters)
}

pub(crate) fn mas_instance_information_response(
    response: &obex_protocol::packet::GetResponse,
) -> Result<MasInstanceInformation> {
    Ok(MasInstanceInformation {
        information: String::from_utf8_lossy(&response.body)
            .trim_end_matches('\0')
            .to_string(),
        owner_uci: Parameters::of(&response.headers).text(tag::OWNER_UCI),
    })
}

pub(crate) fn conversation_listing_request(params: &ConversationListingParams) -> Vec<Header> {
    typed(CONVERSATION_LISTING_TYPE, None, params.to_parameters())
}

pub(crate) fn conversation_listing_response(
    response: &obex_protocol::packet::GetResponse,
) -> Result<ConversationListingResponse> {
    let parameters = Parameters::of(&response.headers);

    Ok(ConversationListingResponse {
        listing: match response.body.is_empty() {
            true => None,
            false => Some(ConversationListing::parse(&response.body)?),
        },
        version_counter: parameters.parsed(tag::CONVERSATION_LISTING_VERSION_COUNTER)?,
        listing_size: parameters.u16(tag::LISTING_SIZE)?,
        database_identifier: parameters.parsed(tag::DATABASE_IDENTIFIER)?,
        mse_time: parameters.text(tag::MSE_TIME),
    })
}

pub(crate) fn owner_status_request(conversation_id: Option<ConversationId>) -> Vec<Header> {
    let mut parameters = ApplicationParameters::new();
    put_text(&mut parameters, tag::CONVERSATION_ID, conversation_id);
    typed(OWNER_STATUS_TYPE, None, parameters)
}

pub(crate) fn owner_status_response(
    response: &obex_protocol::packet::GetResponse,
) -> Result<OwnerStatus> {
    Ok(OwnerStatus::from_parameters(&Parameters::of(
        &response.headers,
    ))?)
}

// Server side: decoding requests and encoding responses.

/// A decoded GET request of the Message Access Service.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MasGet {
    FolderListing(FolderListingParams),
    MessagesListing {
        folder: String,
        params: MessagesListingParams,
    },
    Message {
        handle: MessageHandle,
        params: GetMessageParams,
        fraction: Option<FractionRequest>,
    },
    InstanceInformation(u8),
    ConversationListing(ConversationListingParams),
    OwnerStatus(Option<ConversationId>),
}

/// The Type header of a request, compared case insensitively as OBEX requires.
fn is_type(request: &Request, content_type: &str) -> bool {
    request
        .headers
        .content_type()
        .is_some_and(|value| value.eq_ignore_ascii_case(content_type))
}

impl MasGet {
    /// Recognizes a GET by its Type header. Unknown functions are a Bad Request (MAP
    /// section 6.3.3).
    pub(crate) fn decode(request: &Request) -> core::result::Result<MasGet, ResponseCode> {
        let parameters = Parameters::of(&request.headers);
        let name = request.headers.name().unwrap_or("").to_string();

        if is_type(request, FOLDER_LISTING_TYPE) {
            Ok(MasGet::FolderListing(FolderListingParams::from_parameters(
                &parameters,
            )?))
        } else if is_type(request, MESSAGES_LISTING_TYPE) {
            Ok(MasGet::MessagesListing {
                folder: name,
                params: MessagesListingParams::from_parameters(&parameters)?,
            })
        } else if is_type(request, MESSAGE_TYPE) {
            Ok(MasGet::Message {
                handle: name.parse().map_err(|_| ResponseCode::BadRequest)?,
                params: GetMessageParams {
                    attachment: parameters.flag(tag::ATTACHMENT)?.unwrap_or(false),
                    charset: charset(parameters.u8(tag::CHARSET)?),
                },
                fraction: match parameters.u8(tag::FRACTION_REQUEST)? {
                    Some(0) => Some(FractionRequest::First),
                    Some(_) => Some(FractionRequest::Next),
                    None => None,
                },
            })
        } else if is_type(request, MAS_INSTANCE_INFORMATION_TYPE) {
            Ok(MasGet::InstanceInformation(
                parameters
                    .u8(tag::MAS_INSTANCE_ID)?
                    .ok_or(ResponseCode::BadRequest)?,
            ))
        } else if is_type(request, CONVERSATION_LISTING_TYPE) {
            Ok(MasGet::ConversationListing(
                ConversationListingParams::from_parameters(&parameters)?,
            ))
        } else if is_type(request, OWNER_STATUS_TYPE) {
            Ok(MasGet::OwnerStatus(
                parameters.parsed(tag::CONVERSATION_ID)?,
            ))
        } else {
            Err(ResponseCode::BadRequest)
        }
    }
}

/// A decoded PUT request of the Message Access Service.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MasPut {
    NotificationRegistration(bool),
    NotificationFilter(NotificationFilterMask),
    MessageStatus {
        handle: MessageHandle,
        status: MessageStatus,
    },
    PushMessage {
        folder: String,
        message: Box<BMessage>,
        params: PushMessageParams,
    },
    UpdateInbox,
    OwnerStatus(OwnerStatus),
}

impl MasPut {
    /// Recognizes a PUT by its Type header. A PUT without a body would be a delete,
    /// which MAP does not use.
    pub(crate) fn decode(
        request: &Request,
        body: Option<Vec<u8>>,
    ) -> core::result::Result<MasPut, ResponseCode> {
        let parameters = Parameters::of(&request.headers);
        let body = body.ok_or(ResponseCode::BadRequest)?;
        let name = request.headers.name().unwrap_or("");

        if is_type(request, NOTIFICATION_REGISTRATION_TYPE) {
            Ok(MasPut::NotificationRegistration(
                parameters
                    .flag(tag::NOTIFICATION_STATUS)?
                    .ok_or(ResponseCode::BadRequest)?,
            ))
        } else if is_type(request, NOTIFICATION_FILTER_TYPE) {
            Ok(MasPut::NotificationFilter(
                NotificationFilterMask::from_bits(
                    parameters
                        .u32(tag::NOTIFICATION_FILTER_MASK)?
                        .ok_or(ResponseCode::BadRequest)?,
                ),
            ))
        } else if is_type(request, MESSAGE_STATUS_TYPE) {
            let value = parameters.flag(tag::STATUS_VALUE)?;
            let status = match parameters.u8(tag::STATUS_INDICATOR)? {
                Some(0) => MessageStatus::Read(value.ok_or(ResponseCode::BadRequest)?),
                Some(1) => MessageStatus::Deleted(value.ok_or(ResponseCode::BadRequest)?),
                Some(2) => MessageStatus::ExtendedData(
                    parameters
                        .text(tag::EXTENDED_DATA)
                        .ok_or(ResponseCode::BadRequest)?,
                ),
                _ => return Err(ResponseCode::BadRequest),
            };
            Ok(MasPut::MessageStatus {
                handle: name.parse().map_err(|_| ResponseCode::BadRequest)?,
                status,
            })
        } else if is_type(request, MESSAGE_TYPE) {
            Ok(MasPut::PushMessage {
                folder: name.to_string(),
                message: Box::new(BMessage::parse(&body).map_err(|_| ResponseCode::BadRequest)?),
                params: PushMessageParams::from_parameters(&parameters)?,
            })
        } else if is_type(request, MESSAGE_UPDATE_TYPE) {
            Ok(MasPut::UpdateInbox)
        } else if is_type(request, OWNER_STATUS_TYPE) {
            Ok(MasPut::OwnerStatus(OwnerStatus::from_parameters(
                &parameters,
            )?))
        } else {
            Err(ResponseCode::BadRequest)
        }
    }
}

/// The headers and body answering a GET.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Reply {
    pub(crate) headers: Vec<Header>,
    pub(crate) body: Option<Vec<u8>>,
}

impl Reply {
    fn new(parameters: ApplicationParameters, body: Option<Vec<u8>>) -> Reply {
        let mut headers = Vec::new();
        if !parameters.is_empty() {
            headers.push(Header::ApplicationParameters(parameters));
        }
        Reply { headers, body }
    }

    pub(crate) fn folder_listing(listing: FolderListing, params: &FolderListingParams) -> Reply {
        let mut parameters = ApplicationParameters::new();

        if params.max_list_count == Some(0) {
            let size = u16::try_from(listing.folders.len()).unwrap_or(u16::MAX);
            put_u16(&mut parameters, tag::FOLDER_LISTING_SIZE, Some(size));
            return Reply::new(parameters, None);
        }

        Reply::new(parameters, Some(params.apply(listing).to_bytes()))
    }

    /// Encodes a messages listing, defaulting the parameters MAP requires on success.
    pub(crate) fn messages_listing(response: MessagesListingResponse) -> Reply {
        let mut parameters = ApplicationParameters::new();
        let entries = response.listing.as_ref().map(|listing| &listing.messages);

        let new_message = response.new_message.or_else(|| {
            entries.map(|messages| messages.iter().any(|message| message.read == Some(false)))
        });
        let listing_size = response
            .listing_size
            .or_else(|| entries.map(|messages| u16::try_from(messages.len()).unwrap_or(u16::MAX)));
        // Without `std` there is no clock, so stores must provide the time themselves.
        #[cfg(feature = "std")]
        let mse_time = response.mse_time.or_else(|| {
            Some(
                Timestamp::now()
                    .with_offset(UtcOffset::Minutes(0))
                    .to_string(),
            )
        });
        #[cfg(not(feature = "std"))]
        let mse_time = response.mse_time;

        put_flag(&mut parameters, tag::NEW_MESSAGE, new_message);
        put_text(&mut parameters, tag::MSE_TIME, mse_time);
        put_u16(&mut parameters, tag::LISTING_SIZE, listing_size);
        put_text(
            &mut parameters,
            tag::DATABASE_IDENTIFIER,
            response.database_identifier,
        );
        put_text(
            &mut parameters,
            tag::FOLDER_VERSION_COUNTER,
            response.folder_version_counter,
        );

        Reply::new(
            parameters,
            response.listing.map(|listing| listing.to_bytes()),
        )
    }

    pub(crate) fn message(message: &BMessage) -> Reply {
        Reply::new(ApplicationParameters::new(), Some(message.to_bytes()))
    }

    pub(crate) fn message_fraction(fraction: MessageFraction) -> Reply {
        let mut parameters = ApplicationParameters::new();
        put_u8(
            &mut parameters,
            tag::FRACTION_DELIVER,
            fraction.deliver.map(|deliver| match deliver {
                FractionDeliver::More => 0,
                FractionDeliver::Last => 1,
            }),
        );

        let body = match fraction.content {
            FractionContent::Message(message) => message.to_bytes(),
            FractionContent::Body(body) => body.to_bytes(),
        };
        Reply::new(parameters, Some(body))
    }

    pub(crate) fn mas_instance_information(information: MasInstanceInformation) -> Reply {
        let mut parameters = ApplicationParameters::new();
        put_text(&mut parameters, tag::OWNER_UCI, information.owner_uci);

        // A null terminated UTF-8 string of at most 200 bytes (MAP section 5.10.3).
        let mut text = information.information;
        truncate(&mut text, 199);
        let mut body = text.into_bytes();
        body.push(0);

        Reply::new(parameters, Some(body))
    }

    pub(crate) fn conversation_listing(response: ConversationListingResponse) -> Reply {
        let mut parameters = ApplicationParameters::new();
        let listing_size = response.listing_size.or_else(|| {
            response
                .listing
                .as_ref()
                .map(|listing| u16::try_from(listing.conversations.len()).unwrap_or(u16::MAX))
        });

        put_text(
            &mut parameters,
            tag::CONVERSATION_LISTING_VERSION_COUNTER,
            response.version_counter,
        );
        put_u16(&mut parameters, tag::LISTING_SIZE, listing_size);
        put_text(
            &mut parameters,
            tag::DATABASE_IDENTIFIER,
            Some(response.database_identifier.unwrap_or_default()),
        );
        put_text(&mut parameters, tag::MSE_TIME, response.mse_time);

        Reply::new(
            parameters,
            response.listing.map(|listing| listing.to_bytes()),
        )
    }

    pub(crate) fn owner_status(status: OwnerStatus) -> Reply {
        let status = OwnerStatus {
            presence_availability: Some(status.presence_availability.unwrap_or(0)),
            presence_text: Some(status.presence_text.unwrap_or_default()),
            conversation_id: None,
            ..status
        };

        // The response ends with an empty End-of-Body (MAP section 5.12).
        Reply::new(status.to_parameters(), Some(Vec::new()))
    }
}

/// The response to a successful PushMessage, carrying the new handle.
pub(crate) fn push_message_reply(handle: MessageHandle) -> Response {
    Response::new(ResponseCode::Success).with(Header::Name(handle.to_string()))
}

/// The response to a function returning nothing.
pub(crate) fn status_reply(result: core::result::Result<(), ResponseCode>) -> Response {
    Response::new(result.err().unwrap_or(ResponseCode::Success))
}

/// Accepts a directed CONNECT to `target`, reading the client's `MapSupportedFeatures`.
pub(crate) fn decode_connect(
    request: &Request,
    target: &[u8],
) -> core::result::Result<Option<MapSupportedFeatures>, ResponseCode> {
    if request.headers.target() != Some(target) {
        return Err(ResponseCode::ServiceUnavailable);
    }

    Ok(Parameters::of(&request.headers)
        .u32(tag::MAP_SUPPORTED_FEATURES)?
        .map(MapSupportedFeatures::from_bits))
}

/// Interprets a SETPATH request as a SetFolder step.
pub(crate) fn decode_set_folder(request: &Request) -> SetFolder {
    SetFolder::from_set_path(request.set_path.unwrap_or_default(), request.headers.name())
}

// The Message Notification Service: SendEvent (MAP section 5.1).

pub(crate) fn send_event_request(mas_instance_id: u8, report: &EventReport) -> Put {
    let mut parameters = ApplicationParameters::new();
    put_u8(&mut parameters, tag::MAS_INSTANCE_ID, Some(mas_instance_id));

    Put {
        headers: typed(EVENT_REPORT_TYPE, None, parameters),
        body: report.to_bytes(),
    }
}

/// Decodes a SendEvent PUT into the MAS instance it concerns and its report.
pub(crate) fn decode_send_event(
    request: &Request,
    body: Option<Vec<u8>>,
) -> core::result::Result<(u8, EventReport), ResponseCode> {
    if !is_type(request, EVENT_REPORT_TYPE) {
        return Err(ResponseCode::BadRequest);
    }

    // MASInstanceID is mandatory, but instance 0 is the only one many MSEs have.
    let mas_instance_id = Parameters::of(&request.headers)
        .u8(tag::MAS_INSTANCE_ID)?
        .unwrap_or(0);
    let body = body.ok_or(ResponseCode::BadRequest)?;
    let report = EventReport::parse(&body).map_err(|_| ResponseCode::BadRequest)?;

    Ok((mas_instance_id, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages_listing::Direction;
    use crate::types::MessageType;
    use obex_protocol::opcode::Opcode;

    fn get_request(headers: Vec<Header>) -> Request {
        Request::new(Opcode::Get, true, headers)
    }

    #[test]
    fn set_folder_round_trips_through_set_path() {
        for step in [
            SetFolder::Root,
            SetFolder::Parent,
            SetFolder::Child("telecom".to_string()),
            SetFolder::Sibling("outbox".to_string()),
        ] {
            let (flags, name) = step.to_set_path();
            assert!(flags.no_create);
            assert_eq!(step, SetFolder::from_set_path(flags, name));
        }
    }

    #[test]
    fn messages_listing_parameters_round_trip() {
        let params = MessagesListingParams {
            max_list_count: Some(10),
            list_start_offset: Some(5),
            subject_length: Some(20),
            parameter_mask: Some(ParameterMask::SUBJECT | ParameterMask::READ),
            filter_message_type: Some(MessageTypeFilter::only(&[MessageType::SmsGsm])),
            filter_period_begin: Some("20240101T000000".parse().unwrap()),
            filter_period_end: Some("20250101T000000+0100".parse().unwrap()),
            filter_read_status: Some(ReadStatusFilter::Unread),
            filter_recipient: Some("Alice".to_string()),
            filter_originator: Some("+1555".to_string()),
            filter_priority: Some(PriorityFilter::NonHigh),
            conversation_id: Some(ConversationId(0xAB)),
            filter_message_handle: Some(MessageHandle(0x12)),
        };

        let request = get_request(messages_listing_request("inbox", &params));
        assert_eq!(
            MasGet::MessagesListing {
                folder: "inbox".to_string(),
                params,
            },
            MasGet::decode(&request).unwrap()
        );
    }

    #[test]
    fn wire_encoding_matches_the_specification() {
        let headers = message_request(
            MessageHandle(0x20000100001),
            &GetMessageParams::default(),
            None,
        );

        assert_eq!(Header::Name("0000020000100001".to_string()), headers[0]);
        assert_eq!(Header::Type("x-bt/message".to_string()), headers[1]);
        let Header::ApplicationParameters(parameters) = &headers[2] else {
            panic!("expected application parameters");
        };
        assert_eq!(Some(&[0u8][..]), parameters.get_bytes(tag::ATTACHMENT));
        assert_eq!(Some(&[1u8][..]), parameters.get_bytes(tag::CHARSET));

        let request = notification_registration_request(true);
        assert_eq!(vec![0x30], request.body);
        assert_eq!(
            Header::Type("x-bt/MAP-NotificationRegistration".to_string()),
            request.headers[0]
        );

        let filter = messages_listing_request(
            "",
            &MessagesListingParams {
                filter_period_begin: Some("20240102T030405".parse().unwrap()),
                ..MessagesListingParams::default()
            },
        );
        let Header::ApplicationParameters(parameters) = &filter[2] else {
            panic!("expected application parameters");
        };
        assert_eq!(
            Some(&b"20240102T030405"[..]),
            parameters.get_bytes(tag::FILTER_PERIOD_BEGIN)
        );
    }

    #[test]
    fn put_requests_decode() {
        let decode = |put: Put| {
            MasPut::decode(
                &Request::new(Opcode::Put, true, put.headers),
                Some(put.body),
            )
        };

        assert_eq!(
            Ok(MasPut::NotificationRegistration(false)),
            decode(notification_registration_request(false))
        );
        assert_eq!(
            Ok(MasPut::MessageStatus {
                handle: MessageHandle(9),
                status: MessageStatus::ExtendedData("0:54;".to_string()),
            }),
            decode(message_status_request(
                MessageHandle(9),
                &MessageStatus::ExtendedData("0:54;".to_string())
            ))
        );
        assert_eq!(Ok(MasPut::UpdateInbox), decode(update_inbox_request()));
        assert!(set_owner_status_request(&OwnerStatus::default()).is_err());

        let unknown = Request::new(Opcode::Put, true, vec![Header::Type("x-bt/unknown".into())]);
        assert_eq!(
            Err(ResponseCode::BadRequest),
            MasPut::decode(&unknown, Some(vec![0x30]))
        );
        assert_eq!(
            Err(ResponseCode::BadRequest),
            MasPut::decode(
                &Request::new(Opcode::Put, true, update_inbox_request().headers),
                None
            )
        );
    }

    #[test]
    fn malformed_parameters_are_bad_requests() {
        let mut parameters = ApplicationParameters::new();
        parameters.push(obex_protocol::tlv::TagLengthValue::new(
            tag::MAX_LIST_COUNT,
            [1u8],
        ));
        let request = get_request(vec![
            Header::Type(FOLDER_LISTING_TYPE.to_string()),
            Header::ApplicationParameters(parameters),
        ]);

        assert_eq!(Err(ResponseCode::BadRequest), MasGet::decode(&request));
    }

    #[test]
    fn apply_filters_sorts_segments_and_masks() {
        let entry = |handle: u64, datetime: &str, kind: MessageType, sender: &str| MessageEntry {
            datetime: Some(datetime.to_string()),
            message_type: Some(kind),
            sender_name: Some(sender.to_string()),
            subject: Some("Grüße aus Berlin".to_string()),
            read: Some(true),
            direction: Some(Direction::Incoming),
            ..MessageEntry::new(MessageHandle(handle))
        };
        let messages = vec![
            entry(1, "20240101T100000", MessageType::SmsGsm, "Alice"),
            entry(2, "20240301T100000", MessageType::SmsGsm, "Bob"),
            entry(3, "20240201T100000", MessageType::Email, "alice"),
            entry(4, "20230101T100000", MessageType::SmsGsm, "Alice"),
        ];

        let params = MessagesListingParams {
            filter_period_begin: Some("20240101T000000".parse().unwrap()),
            filter_originator: Some("*ALICE*".to_string()),
            subject_length: Some(5),
            parameter_mask: Some(ParameterMask::SUBJECT | ParameterMask::DATETIME),
            ..MessagesListingParams::default()
        };
        let response = params.apply(messages.clone());
        let listing = response.listing.unwrap();

        assert_eq!(Some(2), response.listing_size);
        assert_eq!(Some(false), response.new_message);
        assert_eq!(
            vec![MessageHandle(3), MessageHandle(1)],
            listing
                .messages
                .iter()
                .map(|m| m.handle)
                .collect::<Vec<_>>()
        );
        // "Grüße" is cut on a character boundary.
        assert_eq!(Some("Grü"), listing.messages[0].subject.as_deref());
        assert_eq!(None, listing.messages[0].sender_name);
        assert_eq!(None, listing.messages[0].direction);

        let only_size = MessagesListingParams {
            max_list_count: Some(0),
            filter_message_type: Some(MessageTypeFilter::only(&[MessageType::SmsGsm])),
            ..MessagesListingParams::default()
        }
        .apply(messages);
        assert_eq!(None, only_size.listing);
        assert_eq!(Some(3), only_size.listing_size);
    }

    #[test]
    fn replies_fill_mandatory_parameters() {
        let reply = Reply::messages_listing(MessagesListingResponse {
            listing: Some(MessagesListing::new(vec![MessageEntry {
                read: Some(false),
                ..MessageEntry::new(MessageHandle(1))
            }])),
            ..MessagesListingResponse::default()
        });
        let Header::ApplicationParameters(parameters) = &reply.headers[0] else {
            panic!("expected application parameters");
        };
        assert_eq!(Some(1), parameters.get_u8(tag::NEW_MESSAGE));
        assert_eq!(Some(1), parameters.get_u16(tag::LISTING_SIZE));
        // Without `std` there is no clock to take the time from.
        assert_eq!(
            cfg!(feature = "std"),
            parameters
                .get_bytes(tag::MSE_TIME)
                .is_some_and(|time| time.ends_with(b"+0000"))
        );

        let size = Reply::folder_listing(
            FolderListing::new([crate::folder_listing::Folder::new("inbox")]),
            &FolderListingParams {
                max_list_count: Some(0),
                list_start_offset: None,
            },
        );
        assert_eq!(None, size.body);

        let information = Reply::mas_instance_information(MasInstanceInformation {
            information: "x".repeat(300),
            owner_uci: None,
        });
        assert_eq!(200, information.body.unwrap().len());
    }
}
