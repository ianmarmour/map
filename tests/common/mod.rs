//! An in-memory phone (MSE) used by the end to end tests.

#![allow(dead_code)]

use map::bmessage::{BMessage, BMessageBody, VCard};
use map::conversation_listing::{Conversation, ConversationListing, Participant};
use map::features::{MapSupportedFeatures, NotificationFilterMask};
use map::folder_listing::{Folder, FolderListing};
use map::mas::{
    ConversationListingParams, ConversationListingResponse, FractionContent, GetMessageParams,
    MasInstanceInformation, MessageFraction, MessageStatus, MessagesListingParams,
    MessagesListingResponse, OwnerStatus, PushMessageParams, SetFolder,
};
use map::messages_listing::MessageEntry;
use map::server::MessageStore;
use map::types::{
    ConversationId, DatabaseIdentifier, FractionDeliver, FractionRequest, MessageHandle,
    MessageType, VersionCounter,
};
use obex::response::ResponseCode;

/// The virtual folder tree of MAP section 3.1.4.
const FOLDERS: [&str; 7] = [
    "telecom",
    "telecom/msg",
    "telecom/msg/inbox",
    "telecom/msg/outbox",
    "telecom/msg/sent",
    "telecom/msg/deleted",
    "telecom/msg/draft",
];

pub const CONVERSATION: ConversationId = ConversationId(0xE1E2E3E4F1F2F3F4A1A2A3A4B1B2B3B4);

pub struct Stored {
    pub folder: String,
    pub entry: MessageEntry,
    pub message: BMessage,
}

#[derive(Default)]
pub struct Phone {
    pub path: Vec<String>,
    pub messages: Vec<Stored>,
    pub next_handle: u64,
    pub remote_features: Option<MapSupportedFeatures>,
    pub registrations: Vec<bool>,
    pub filters: Vec<NotificationFilterMask>,
    pub pushed: Vec<PushMessageParams>,
    pub extended_data: Vec<(MessageHandle, String)>,
    pub owner: OwnerStatus,
    pub disconnected: bool,
}

/// A large e-mail body, to force multi-packet transfers.
pub fn long_text() -> String {
    (0..4000)
        .map(|line| format!("Line {} of a long e-mail.\r\n", line))
        .collect()
}

impl Phone {
    pub fn new() -> Phone {
        let mut phone = Phone {
            next_handle: 0x20000100000,
            ..Phone::default()
        };

        phone.add(
            "telecom/msg/inbox",
            "20240301T101500",
            "Alice",
            "+15550001",
            false,
            "See you at 10?",
        );
        phone.add(
            "telecom/msg/inbox",
            "20240302T080000",
            "Bob",
            "+15550002",
            true,
            "Running late",
        );
        phone.add(
            "telecom/msg/inbox",
            "20240215T120000",
            "Alice",
            "+15550001",
            true,
            "Lunch?",
        );

        let handle = phone.add(
            "telecom/msg/inbox",
            "20240101T000000",
            "Carol",
            "carol@example.com",
            false,
            &long_text(),
        );
        let stored = phone.find(handle).unwrap();
        stored.entry.message_type = Some(MessageType::Email);
        stored.message.message_type = MessageType::Email;

        phone
    }

    fn add(
        &mut self,
        folder: &str,
        datetime: &str,
        sender: &str,
        address: &str,
        read: bool,
        text: &str,
    ) -> MessageHandle {
        let handle = MessageHandle(self.next_handle);
        self.next_handle += 1;

        let entry = MessageEntry {
            subject: Some(text.chars().take(20).collect()),
            datetime: Some(datetime.to_string()),
            sender_name: Some(sender.to_string()),
            sender_addressing: Some(address.to_string()),
            recipient_addressing: Some("+15559999".to_string()),
            message_type: Some(MessageType::SmsGsm),
            size: Some(text.len() as u64),
            read: Some(read),
            conversation_id: (sender == "Alice").then_some(CONVERSATION),
            ..MessageEntry::new(handle)
        };

        let mut message = BMessage::new(MessageType::SmsGsm, BMessageBody::from_text(text))
            .with_originator(VCard::telephone(address).with_name(sender))
            .with_recipient(VCard::telephone("+15559999"));
        message.read = read;
        message.folder = folder.to_uppercase();

        self.messages.push(Stored {
            folder: folder.to_string(),
            entry,
            message,
        });
        handle
    }

    pub fn find(&mut self, handle: MessageHandle) -> Option<&mut Stored> {
        self.messages
            .iter_mut()
            .find(|stored| stored.entry.handle == handle)
    }

    fn resolve(&self, child: &str) -> Result<String, ResponseCode> {
        let mut path = self.path.clone();
        if !child.is_empty() {
            path.push(child.to_string());
        }
        let path = path.join("/");

        if path.is_empty() || FOLDERS.contains(&path.as_str()) {
            Ok(path)
        } else {
            Err(ResponseCode::NotFound)
        }
    }
}

impl MessageStore for Phone {
    fn connect(&mut self, features: Option<MapSupportedFeatures>) -> Result<(), ResponseCode> {
        self.remote_features = features;
        Ok(())
    }

    fn disconnect(&mut self) {
        self.disconnected = true;
    }

    fn set_folder(&mut self, step: SetFolder) -> Result<(), ResponseCode> {
        match step {
            SetFolder::Root => self.path.clear(),
            SetFolder::Parent => {
                self.path.pop().ok_or(ResponseCode::NotFound)?;
            }
            SetFolder::Child(name) => {
                self.resolve(&name)?;
                self.path.push(name);
            }
            SetFolder::Sibling(name) => {
                let parent = self.path.pop().ok_or(ResponseCode::NotFound)?;
                if let Err(code) = self.resolve(&name) {
                    self.path.push(parent);
                    return Err(code);
                }
                self.path.push(name);
            }
        }
        Ok(())
    }

    fn get_folder_listing(&mut self) -> Result<FolderListing, ResponseCode> {
        let current = self.path.join("/");

        Ok(FolderListing::new(FOLDERS.iter().filter_map(|folder| {
            let (parent, name) = folder.rsplit_once('/').unwrap_or(("", folder));
            (parent == current).then(|| Folder::new(name))
        })))
    }

    fn get_messages_listing(
        &mut self,
        folder: &str,
        params: &MessagesListingParams,
    ) -> Result<MessagesListingResponse, ResponseCode> {
        let everywhere = params.conversation_id.is_some() || params.filter_message_handle.is_some();
        let folder = if everywhere {
            String::new()
        } else {
            self.resolve(folder)?
        };

        let entries = self
            .messages
            .iter()
            .filter(|stored| everywhere || stored.folder == folder)
            .map(|stored| stored.entry.clone());

        Ok(MessagesListingResponse {
            mse_time: Some("20240310T120000+0100".to_string()),
            database_identifier: Some(DatabaseIdentifier(1)),
            folder_version_counter: Some(VersionCounter(7)),
            ..params.apply(entries)
        })
    }

    fn get_message(
        &mut self,
        handle: MessageHandle,
        _params: &GetMessageParams,
    ) -> Result<BMessage, ResponseCode> {
        self.find(handle)
            .map(|stored| stored.message.clone())
            .ok_or(ResponseCode::NotFound)
    }

    fn get_message_fraction(
        &mut self,
        handle: MessageHandle,
        params: &GetMessageParams,
        fraction: FractionRequest,
    ) -> Result<MessageFraction, ResponseCode> {
        Ok(match fraction {
            FractionRequest::First => MessageFraction {
                content: FractionContent::Message(Box::new(self.get_message(handle, params)?)),
                deliver: Some(FractionDeliver::More),
            },
            FractionRequest::Next => MessageFraction {
                content: FractionContent::Body(BMessageBody {
                    part_id: Some(1),
                    ..BMessageBody::from_text("second fraction")
                }),
                deliver: Some(FractionDeliver::Last),
            },
        })
    }

    fn set_message_status(
        &mut self,
        handle: MessageHandle,
        status: MessageStatus,
    ) -> Result<(), ResponseCode> {
        let stored = self.find(handle).ok_or(ResponseCode::NotFound)?;

        match status {
            MessageStatus::Read(read) => {
                stored.entry.read = Some(read);
                stored.message.read = read;
            }
            MessageStatus::Deleted(true) => stored.folder = "telecom/msg/deleted".to_string(),
            MessageStatus::Deleted(false) => stored.folder = "telecom/msg/inbox".to_string(),
            MessageStatus::ExtendedData(data) => self.extended_data.push((handle, data)),
        }
        Ok(())
    }

    fn push_message(
        &mut self,
        folder: &str,
        mut message: BMessage,
        params: &PushMessageParams,
    ) -> Result<MessageHandle, ResponseCode> {
        let folder = self.resolve(folder)?;
        let handle = MessageHandle(self.next_handle);
        self.next_handle += 1;

        message.folder = folder.to_uppercase();
        self.pushed.push(params.clone());
        self.messages.push(Stored {
            folder,
            entry: MessageEntry {
                message_type: Some(message.message_type),
                read: Some(true),
                ..MessageEntry::new(handle)
            },
            message,
        });
        Ok(handle)
    }

    fn set_notification_registration(&mut self, enabled: bool) -> Result<(), ResponseCode> {
        self.registrations.push(enabled);
        Ok(())
    }

    fn set_notification_filter(
        &mut self,
        mask: NotificationFilterMask,
    ) -> Result<(), ResponseCode> {
        self.filters.push(mask);
        Ok(())
    }

    fn get_mas_instance_information(
        &mut self,
        instance_id: u8,
    ) -> Result<MasInstanceInformation, ResponseCode> {
        match instance_id {
            0 => Ok(MasInstanceInformation {
                information: "SMS/MMS on SIM 1".to_string(),
                owner_uci: Some("tel:+15559999".to_string()),
            }),
            _ => Err(ResponseCode::NotFound),
        }
    }

    fn get_conversation_listing(
        &mut self,
        params: &ConversationListingParams,
    ) -> Result<ConversationListingResponse, ResponseCode> {
        let conversation = Conversation {
            name: Some("Alice".to_string()),
            last_activity: Some("20240301T101500".to_string()),
            read_status: Some(false),
            participants: vec![Participant {
                display_name: Some("Alice".to_string()),
                chat_state: Some(2),
                ..Participant::new("+15550001")
            }],
            ..Conversation::new(CONVERSATION)
        };

        Ok(ConversationListingResponse {
            listing: (params.max_list_count != Some(0))
                .then(|| ConversationListing::new(vec![conversation])),
            listing_size: Some(1),
            version_counter: (params.max_list_count == Some(0)).then_some(VersionCounter(3)),
            database_identifier: Some(DatabaseIdentifier(1)),
            mse_time: None,
        })
    }

    fn set_owner_status(&mut self, status: OwnerStatus) -> Result<(), ResponseCode> {
        if status.conversation_id.is_some_and(|id| id != CONVERSATION) {
            return Err(ResponseCode::NotFound);
        }
        self.owner = status;
        Ok(())
    }

    fn get_owner_status(
        &mut self,
        conversation_id: Option<ConversationId>,
    ) -> Result<OwnerStatus, ResponseCode> {
        if conversation_id.is_some_and(|id| id != CONVERSATION) {
            return Err(ResponseCode::NotFound);
        }
        Ok(self.owner.clone())
    }
}
