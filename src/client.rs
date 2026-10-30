//! ## Blocking Clients
//!
//! [`MasClient`] is the MCE side of the Message Access Service: it connects to a MAS
//! instance on the phone (MSE) and browses, downloads, uploads and manages messages.
//! [`MnsClient`] is the MSE side of the Message Notification Service, delivering
//! event reports to the MCE.
//!
//! Both run over any connected `Read + Write` transport. Find the MAS instance's
//! RFCOMM channel or L2CAP PSM with SDP (see [`crate::sdp`]) and open the socket with
//! your Bluetooth stack. Over L2CAP (GOEP 2.0), set the maximum packet length to the
//! channel MTU and enable Single Response Mode; over RFCOMM leave it off.
//!
//! The async equivalents are in [`crate::tokio`].
//!
//! # Examples
//!
//! Listing the newest unread messages of the inbox and reading the first one:
//! ```no_run
//! # fn example(socket: std::net::TcpStream) -> map::Result<()> {
//! use map::client::MasClient;
//! use map::mas::{GetMessageParams, MessagesListingParams, MessageStatus};
//! use map::types::ReadStatusFilter;
//!
//! let mut client = MasClient::new(socket);
//! client.connect()?;
//! client.set_folder_path("telecom/msg")?;
//!
//! let response = client.get_messages_listing(
//!     "inbox",
//!     &MessagesListingParams {
//!         max_list_count: Some(10),
//!         filter_read_status: Some(ReadStatusFilter::Unread),
//!         ..MessagesListingParams::default()
//!     },
//! )?;
//!
//! for entry in response.listing.unwrap_or_default().messages {
//!     let message = client.get_message(entry.handle, &GetMessageParams::default())?;
//!     println!("{:?}: {}", entry.sender_addressing, message.text());
//!     client.set_message_status(entry.handle, MessageStatus::Read(true))?;
//! }
//!
//! client.disconnect()?;
//! # Ok(())
//! # }
//! ```
//!
use std::io::{Read, Write};

use obex::client::Client;
use obex::target::{MESSAGE_ACCESS, MESSAGE_NOTIFICATION};

use crate::bmessage::BMessage;
use crate::error::Result;
use crate::event_report::EventReport;
use crate::features::{MapSupportedFeatures, NotificationFilterMask};
use crate::folder_listing::FolderListing;
use crate::mas::{
    self, ConversationListingParams, ConversationListingResponse, FolderListingParams,
    GetMessageParams, MasInstanceInformation, MessageFraction, MessageStatus,
    MessagesListingParams, MessagesListingResponse, OwnerStatus, PushMessageParams, SetFolder,
};
use crate::types::{ConversationId, FractionRequest, MessageHandle};

/// A Message Access Service client, the MCE role. See [the module level
/// documentation](index.html) for more.
pub struct MasClient<T> {
    obex: Client<T>,
    features: Option<MapSupportedFeatures>,
}

impl<T: Read + Write> MasClient<T> {
    /// Creates a client over a connected transport. Call [`connect`](Self::connect)
    /// before anything else.
    pub fn new(transport: T) -> MasClient<T> {
        MasClient::from_obex(Client::new(transport))
    }

    /// Creates a client from a configured, not yet connected OBEX client.
    pub fn from_obex(obex: Client<T>) -> MasClient<T> {
        MasClient {
            obex,
            features: None,
        }
    }

    /// Sets the largest packet the client can receive. Over L2CAP this must not
    /// exceed the channel MTU.
    pub fn with_max_packet_length(mut self, max_packet_length: u16) -> MasClient<T> {
        self.obex = self.obex.with_max_packet_length(max_packet_length);
        self
    }

    /// Requests Single Response Mode for every GET and PUT, as GOEP 2.0 (L2CAP)
    /// requires. Leave it off over RFCOMM.
    pub fn with_single_response_mode(mut self, enabled: bool) -> MasClient<T> {
        self.obex = self.obex.with_single_response_mode(enabled);
        self
    }

    /// Sends the MCE's `MapSupportedFeatures` in CONNECT. Required when the MSE's SDP
    /// record sets
    /// [`MAP_SUPPORTED_FEATURES_IN_CONNECT_REQUEST`](MapSupportedFeatures::MAP_SUPPORTED_FEATURES_IN_CONNECT_REQUEST).
    pub fn with_supported_features(mut self, features: MapSupportedFeatures) -> MasClient<T> {
        self.features = Some(features);
        self
    }

    /// The underlying OBEX client.
    pub fn obex(&self) -> &Client<T> {
        &self.obex
    }

    /// The underlying OBEX client, mutably, for requests this crate does not wrap.
    pub fn obex_mut(&mut self) -> &mut Client<T> {
        &mut self.obex
    }

    /// Unwraps the underlying transport.
    pub fn into_inner(self) -> T {
        self.obex.into_inner()
    }

    /// Opens the Message Access Service connection (MAP section 6.4.1).
    pub fn connect(&mut self) -> Result<()> {
        let response = self
            .obex
            .connect(mas::connect_request(&MESSAGE_ACCESS, self.features))?;
        mas::check_connect(&response, &MESSAGE_ACCESS)
    }

    /// Closes the connection.
    pub fn disconnect(&mut self) -> Result<()> {
        self.obex.disconnect()?;
        Ok(())
    }

    /// Turns event notifications for this MAS instance on or off (MAP section 5.2).
    ///
    /// When turned on, the MSE connects to the MCE's Message Notification Server, so
    /// run an [`MnsServer`](crate::server::MnsServer) before calling this.
    pub fn set_notification_registration(&mut self, enabled: bool) -> Result<()> {
        let request = mas::notification_registration_request(enabled);
        self.obex.put(request.headers, &request.body)?;
        Ok(())
    }

    /// Chooses which events the MSE reports (MAP section 5.14).
    pub fn set_notification_filter(&mut self, mask: NotificationFilterMask) -> Result<()> {
        let request = mas::notification_filter_request(mask);
        self.obex.put(request.headers, &request.body)?;
        Ok(())
    }

    /// Moves one level in the folder tree (MAP section 5.3).
    pub fn set_folder(&mut self, step: SetFolder) -> Result<()> {
        let (flags, name) = step.to_set_path();
        self.obex.set_path(flags, name)?;
        Ok(())
    }

    /// Goes to `path` from the root folder, one SetFolder per component, e.g.
    /// `telecom/msg/inbox`. An empty path goes to the root.
    pub fn set_folder_path(&mut self, path: &str) -> Result<()> {
        self.set_folder(SetFolder::Root)?;

        for folder in path.split('/').filter(|folder| !folder.is_empty()) {
            self.set_folder(SetFolder::Child(folder.to_string()))?;
        }
        Ok(())
    }

    /// Lists the folders in the current folder (MAP section 5.4).
    pub fn get_folder_listing(&mut self, params: &FolderListingParams) -> Result<FolderListing> {
        let response = self.obex.get(mas::folder_listing_request(params))?;
        mas::folder_listing_response(&response)
    }

    /// Counts the folders in the current folder.
    pub fn get_folder_listing_size(&mut self) -> Result<u16> {
        let response = self.obex.get(mas::folder_listing_size_request())?;
        mas::folder_listing_size_response(&response)
    }

    /// Lists the messages in `folder`, a child of the current folder or `""` for the
    /// current folder itself (MAP section 5.5).
    pub fn get_messages_listing(
        &mut self,
        folder: &str,
        params: &MessagesListingParams,
    ) -> Result<MessagesListingResponse> {
        let response = self
            .obex
            .get(mas::messages_listing_request(folder, params))?;
        mas::messages_listing_response(&response)
    }

    /// Downloads a message (MAP section 5.6).
    pub fn get_message(
        &mut self,
        handle: MessageHandle,
        params: &GetMessageParams,
    ) -> Result<BMessage> {
        let response = self.obex.get(mas::message_request(handle, params, None))?;
        mas::message_response(&response)
    }

    /// Downloads a fraction of a fractioned e-mail (MAP section 5.6.4.3).
    pub fn get_message_fraction(
        &mut self,
        handle: MessageHandle,
        params: &GetMessageParams,
        fraction: FractionRequest,
    ) -> Result<MessageFraction> {
        let response = self
            .obex
            .get(mas::message_request(handle, params, Some(fraction)))?;
        mas::message_fraction_response(&response)
    }

    /// Marks a message read or unread, deletes or undeletes it, or sets its extended
    /// data (MAP section 5.7).
    pub fn set_message_status(
        &mut self,
        handle: MessageHandle,
        status: MessageStatus,
    ) -> Result<()> {
        let request = mas::message_status_request(handle, &status);
        self.obex.put(request.headers, &request.body)?;
        Ok(())
    }

    /// Uploads a message to `folder`, a child of the current folder or `""` for the
    /// current folder, and returns the handle the MSE assigned (MAP section 5.8).
    ///
    /// Messages pushed to the `outbox` folder are sent.
    pub fn push_message(
        &mut self,
        folder: &str,
        message: &BMessage,
        params: &PushMessageParams,
    ) -> Result<MessageHandle> {
        let request = mas::push_message_request(folder, message, params);
        let response = self.obex.put(request.headers, &request.body)?;
        mas::push_message_response(&response)
    }

    /// Asks the MSE to check the network for new messages (MAP section 5.9).
    pub fn update_inbox(&mut self) -> Result<()> {
        let request = mas::update_inbox_request();
        self.obex.put(request.headers, &request.body)?;
        Ok(())
    }

    /// Reads the description of MAS instance `instance_id` (MAP section 5.10).
    pub fn get_mas_instance_information(
        &mut self,
        instance_id: u8,
    ) -> Result<MasInstanceInformation> {
        let response = self
            .obex
            .get(mas::mas_instance_information_request(instance_id))?;
        mas::mas_instance_information_response(&response)
    }

    /// Lists conversations (MAP section 5.13).
    pub fn get_conversation_listing(
        &mut self,
        params: &ConversationListingParams,
    ) -> Result<ConversationListingResponse> {
        let response = self.obex.get(mas::conversation_listing_request(params))?;
        mas::conversation_listing_response(&response)
    }

    /// Changes the presence or chat state of the owner (MAP section 5.11).
    pub fn set_owner_status(&mut self, status: &OwnerStatus) -> Result<()> {
        let request = mas::set_owner_status_request(status)?;
        self.obex.put(request.headers, &request.body)?;
        Ok(())
    }

    /// Reads the presence and chat state of the owner, optionally within a
    /// conversation (MAP section 5.12).
    pub fn get_owner_status(
        &mut self,
        conversation_id: Option<ConversationId>,
    ) -> Result<OwnerStatus> {
        let response = self.obex.get(mas::owner_status_request(conversation_id))?;
        mas::owner_status_response(&response)
    }
}

/// A Message Notification Service client, the MSE role: delivers event reports to an
/// MCE that registered for notifications.
///
/// # Examples
/// ```no_run
/// # fn example(socket: std::net::TcpStream) -> map::Result<()> {
/// use map::client::MnsClient;
/// use map::event_report::{EventReport, EventType};
/// use map::types::{MessageHandle, MessageType};
///
/// let mut client = MnsClient::new(socket);
/// client.connect()?;
/// client.send_event(
///     0,
///     &EventReport::message(
///         EventType::NewMessage,
///         MessageHandle(0x20000100001),
///         "TELECOM/MSG/INBOX",
///         MessageType::SmsGsm,
///     ),
/// )?;
/// # Ok(())
/// # }
/// ```
pub struct MnsClient<T> {
    obex: Client<T>,
}

impl<T: Read + Write> MnsClient<T> {
    /// Creates a client over a connected transport.
    pub fn new(transport: T) -> MnsClient<T> {
        MnsClient::from_obex(Client::new(transport))
    }

    /// Creates a client from a configured, not yet connected OBEX client.
    pub fn from_obex(obex: Client<T>) -> MnsClient<T> {
        MnsClient { obex }
    }

    /// Sets the largest packet the client can receive.
    pub fn with_max_packet_length(mut self, max_packet_length: u16) -> MnsClient<T> {
        self.obex = self.obex.with_max_packet_length(max_packet_length);
        self
    }

    /// Requests Single Response Mode, as GOEP 2.0 (L2CAP) requires.
    pub fn with_single_response_mode(mut self, enabled: bool) -> MnsClient<T> {
        self.obex = self.obex.with_single_response_mode(enabled);
        self
    }

    /// The underlying OBEX client.
    pub fn obex_mut(&mut self) -> &mut Client<T> {
        &mut self.obex
    }

    /// Unwraps the underlying transport.
    pub fn into_inner(self) -> T {
        self.obex.into_inner()
    }

    /// Opens the Message Notification Service connection.
    pub fn connect(&mut self) -> Result<()> {
        let response = self
            .obex
            .connect(mas::connect_request(&MESSAGE_NOTIFICATION, None))?;
        mas::check_connect(&response, &MESSAGE_NOTIFICATION)
    }

    /// Closes the connection.
    pub fn disconnect(&mut self) -> Result<()> {
        self.obex.disconnect()?;
        Ok(())
    }

    /// Delivers an event about MAS instance `mas_instance_id` (MAP section 5.1).
    pub fn send_event(&mut self, mas_instance_id: u8, report: &EventReport) -> Result<()> {
        let request = mas::send_event_request(mas_instance_id, report);
        self.obex.put(request.headers, &request.body)?;
        Ok(())
    }
}
