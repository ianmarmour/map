//! ## Tokio
//!
//! Async versions of [`MasClient`], [`MnsClient`], [`MasServer`] and [`MnsServer`]
//! for transports implementing tokio's [`AsyncRead`] and [`AsyncWrite`], such as the
//! RFCOMM and L2CAP streams of BlueZ through the `bluer` crate. Available with the
//! `tokio` feature:
//!
//! ```toml
//! [dependencies]
//! obex-protocol-map = { version = "0.2", features = ["tokio"] }
//! ```
//!
//! They mirror the blocking types in [`crate::client`] and [`crate::server`] method
//! for method and share their encoding, so both behave identically and interoperate.
//! [`MessageStore`] and [`NotificationHandler`] are implemented with `async fn`; their
//! futures must be `Send`, so servers can be spawned on a multi-threaded runtime.
//!
//! # Examples
//!
//! A MAP client watching for new messages:
//! ```no_run
//! # async fn example(
//! #     mas_socket: tokio::net::TcpStream,
//! #     mns_listener: tokio::net::TcpListener,
//! # ) -> obex_protocol_map::Result<()> {
//! use obex_protocol_map::event_report::{EventReport, EventType};
//! use obex_protocol_map::mas::GetMessageParams;
//! use obex_protocol_map::tokio::{MasClient, MnsServer};
//! use tokio::sync::mpsc;
//!
//! let mut client = MasClient::new(mas_socket);
//! client.connect().await?;
//!
//! let (events, mut received) = mpsc::unbounded_channel();
//! tokio::spawn(async move {
//!     let (socket, _) = mns_listener.accept().await?;
//!     let handler = move |_instance: u8, report: EventReport| {
//!         let _ = events.send(report);
//!         async {}
//!     };
//!     MnsServer::new(socket, handler).run().await
//! });
//!
//! client.set_notification_registration(true).await?;
//!
//! while let Some(report) = received.recv().await {
//!     if let (EventType::NewMessage, Some(handle)) = (&report.event_type, report.handle) {
//!         let message = client.get_message(handle, &GetMessageParams::default()).await?;
//!         println!("{}", message.text());
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
use std::future::Future;

use ::tokio::io::{AsyncRead, AsyncWrite};
use obex_protocol::packet::{Request, Response};
use obex_protocol::response::ResponseCode;
use obex_protocol::target::{MESSAGE_ACCESS, MESSAGE_NOTIFICATION};
use obex_protocol::tokio::{Client, Handler, Object, Server};

use crate::bmessage::BMessage;
use crate::error::Result;
use crate::event_report::EventReport;
use crate::features::{MapSupportedFeatures, NotificationFilterMask};
use crate::folder_listing::FolderListing;
use crate::mas::{
    self, ConversationListingParams, ConversationListingResponse, FolderListingParams,
    GetMessageParams, MasGet, MasInstanceInformation, MasPut, MessageFraction, MessageStatus,
    MessagesListingParams, MessagesListingResponse, OwnerStatus, PushMessageParams, Reply,
    SetFolder,
};
use crate::types::{ConversationId, FractionRequest, MessageHandle};

/// The async Message Access Service client. See [`crate::client::MasClient`].
pub struct MasClient<T> {
    obex: Client<T>,
    features: Option<MapSupportedFeatures>,
}

impl<T: AsyncRead + AsyncWrite + Unpin> MasClient<T> {
    /// Creates a client over a connected transport.
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

    /// Sets the largest packet the client can receive.
    pub fn with_max_packet_length(mut self, max_packet_length: u16) -> MasClient<T> {
        self.obex = self.obex.with_max_packet_length(max_packet_length);
        self
    }

    /// Requests Single Response Mode for every GET and PUT (GOEP 2.0).
    pub fn with_single_response_mode(mut self, enabled: bool) -> MasClient<T> {
        self.obex = self.obex.with_single_response_mode(enabled);
        self
    }

    /// Sends the MCE's `MapSupportedFeatures` in CONNECT.
    pub fn with_supported_features(mut self, features: MapSupportedFeatures) -> MasClient<T> {
        self.features = Some(features);
        self
    }

    /// The underlying OBEX client.
    pub fn obex(&self) -> &Client<T> {
        &self.obex
    }

    /// The underlying OBEX client, mutably.
    pub fn obex_mut(&mut self) -> &mut Client<T> {
        &mut self.obex
    }

    /// Unwraps the underlying transport.
    pub fn into_inner(self) -> T {
        self.obex.into_inner()
    }

    /// Opens the Message Access Service connection.
    pub async fn connect(&mut self) -> Result<()> {
        let response = self
            .obex
            .connect(mas::connect_request(&MESSAGE_ACCESS, self.features))
            .await?;
        mas::check_connect(&response, &MESSAGE_ACCESS)
    }

    /// Closes the connection.
    pub async fn disconnect(&mut self) -> Result<()> {
        self.obex.disconnect().await?;
        Ok(())
    }

    /// Turns event notifications on or off. See
    /// [`crate::client::MasClient::set_notification_registration`].
    pub async fn set_notification_registration(&mut self, enabled: bool) -> Result<()> {
        let request = mas::notification_registration_request(enabled);
        self.obex.put(request.headers, &request.body).await?;
        Ok(())
    }

    /// Chooses which events the MSE reports.
    pub async fn set_notification_filter(&mut self, mask: NotificationFilterMask) -> Result<()> {
        let request = mas::notification_filter_request(mask);
        self.obex.put(request.headers, &request.body).await?;
        Ok(())
    }

    /// Moves one level in the folder tree.
    pub async fn set_folder(&mut self, step: SetFolder) -> Result<()> {
        let (flags, name) = step.to_set_path();
        self.obex.set_path(flags, name).await?;
        Ok(())
    }

    /// Goes to `path` from the root folder, e.g. `telecom/msg/inbox`.
    pub async fn set_folder_path(&mut self, path: &str) -> Result<()> {
        self.set_folder(SetFolder::Root).await?;

        for folder in path.split('/').filter(|folder| !folder.is_empty()) {
            self.set_folder(SetFolder::Child(folder.to_string()))
                .await?;
        }
        Ok(())
    }

    /// Lists the folders in the current folder.
    pub async fn get_folder_listing(
        &mut self,
        params: &FolderListingParams,
    ) -> Result<FolderListing> {
        let response = self.obex.get(mas::folder_listing_request(params)).await?;
        mas::folder_listing_response(&response)
    }

    /// Counts the folders in the current folder.
    pub async fn get_folder_listing_size(&mut self) -> Result<u16> {
        let response = self.obex.get(mas::folder_listing_size_request()).await?;
        mas::folder_listing_size_response(&response)
    }

    /// Lists the messages in `folder`, a child of the current folder or `""`.
    pub async fn get_messages_listing(
        &mut self,
        folder: &str,
        params: &MessagesListingParams,
    ) -> Result<MessagesListingResponse> {
        let response = self
            .obex
            .get(mas::messages_listing_request(folder, params))
            .await?;
        mas::messages_listing_response(&response)
    }

    /// Downloads a message.
    pub async fn get_message(
        &mut self,
        handle: MessageHandle,
        params: &GetMessageParams,
    ) -> Result<BMessage> {
        let response = self
            .obex
            .get(mas::message_request(handle, params, None))
            .await?;
        mas::message_response(&response)
    }

    /// Downloads a fraction of a fractioned e-mail.
    pub async fn get_message_fraction(
        &mut self,
        handle: MessageHandle,
        params: &GetMessageParams,
        fraction: FractionRequest,
    ) -> Result<MessageFraction> {
        let response = self
            .obex
            .get(mas::message_request(handle, params, Some(fraction)))
            .await?;
        mas::message_fraction_response(&response)
    }

    /// Changes the status of a message.
    pub async fn set_message_status(
        &mut self,
        handle: MessageHandle,
        status: MessageStatus,
    ) -> Result<()> {
        let request = mas::message_status_request(handle, &status);
        self.obex.put(request.headers, &request.body).await?;
        Ok(())
    }

    /// Uploads a message to `folder` and returns its new handle.
    pub async fn push_message(
        &mut self,
        folder: &str,
        message: &BMessage,
        params: &PushMessageParams,
    ) -> Result<MessageHandle> {
        let request = mas::push_message_request(folder, message, params);
        let response = self.obex.put(request.headers, &request.body).await?;
        mas::push_message_response(&response)
    }

    /// Asks the MSE to check the network for new messages.
    pub async fn update_inbox(&mut self) -> Result<()> {
        let request = mas::update_inbox_request();
        self.obex.put(request.headers, &request.body).await?;
        Ok(())
    }

    /// Reads the description of MAS instance `instance_id`.
    pub async fn get_mas_instance_information(
        &mut self,
        instance_id: u8,
    ) -> Result<MasInstanceInformation> {
        let response = self
            .obex
            .get(mas::mas_instance_information_request(instance_id))
            .await?;
        mas::mas_instance_information_response(&response)
    }

    /// Lists conversations.
    pub async fn get_conversation_listing(
        &mut self,
        params: &ConversationListingParams,
    ) -> Result<ConversationListingResponse> {
        let response = self
            .obex
            .get(mas::conversation_listing_request(params))
            .await?;
        mas::conversation_listing_response(&response)
    }

    /// Changes the presence or chat state of the owner.
    pub async fn set_owner_status(&mut self, status: &OwnerStatus) -> Result<()> {
        let request = mas::set_owner_status_request(status)?;
        self.obex.put(request.headers, &request.body).await?;
        Ok(())
    }

    /// Reads the presence and chat state of the owner.
    pub async fn get_owner_status(
        &mut self,
        conversation_id: Option<ConversationId>,
    ) -> Result<OwnerStatus> {
        let response = self
            .obex
            .get(mas::owner_status_request(conversation_id))
            .await?;
        mas::owner_status_response(&response)
    }
}

/// The async Message Notification Service client. See [`crate::client::MnsClient`].
pub struct MnsClient<T> {
    obex: Client<T>,
}

impl<T: AsyncRead + AsyncWrite + Unpin> MnsClient<T> {
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

    /// Requests Single Response Mode (GOEP 2.0).
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
    pub async fn connect(&mut self) -> Result<()> {
        let response = self
            .obex
            .connect(mas::connect_request(&MESSAGE_NOTIFICATION, None))
            .await?;
        mas::check_connect(&response, &MESSAGE_NOTIFICATION)
    }

    /// Closes the connection.
    pub async fn disconnect(&mut self) -> Result<()> {
        self.obex.disconnect().await?;
        Ok(())
    }

    /// Delivers an event about MAS instance `mas_instance_id`.
    pub async fn send_event(&mut self, mas_instance_id: u8, report: &EventReport) -> Result<()> {
        let request = mas::send_event_request(mas_instance_id, report);
        self.obex.put(request.headers, &request.body).await?;
        Ok(())
    }
}

/// The message repository behind an async [`MasServer`]. See
/// [`crate::server::MessageStore`] for the semantics of each method.
pub trait MessageStore {
    /// An MCE connected.
    fn connect(
        &mut self,
        features: Option<MapSupportedFeatures>,
    ) -> impl Future<Output = std::result::Result<(), ResponseCode>> + Send {
        let _ = features;
        async { Ok(()) }
    }

    /// The MCE disconnected.
    fn disconnect(&mut self) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// Changes the current folder (SetFolder).
    fn set_folder(
        &mut self,
        step: SetFolder,
    ) -> impl Future<Output = std::result::Result<(), ResponseCode>> + Send;

    /// Lists every folder in the current folder (GetFolderListing).
    fn get_folder_listing(
        &mut self,
    ) -> impl Future<Output = std::result::Result<FolderListing, ResponseCode>> + Send;

    /// Lists the messages of `folder` (GetMessagesListing).
    fn get_messages_listing(
        &mut self,
        folder: &str,
        params: &MessagesListingParams,
    ) -> impl Future<Output = std::result::Result<MessagesListingResponse, ResponseCode>> + Send;

    /// Returns a message (GetMessage).
    fn get_message(
        &mut self,
        handle: MessageHandle,
        params: &GetMessageParams,
    ) -> impl Future<Output = std::result::Result<BMessage, ResponseCode>> + Send;

    /// Returns a fraction of a fractioned e-mail.
    fn get_message_fraction(
        &mut self,
        handle: MessageHandle,
        params: &GetMessageParams,
        fraction: FractionRequest,
    ) -> impl Future<Output = std::result::Result<MessageFraction, ResponseCode>> + Send {
        let _ = (handle, params, fraction);
        async { Err(ResponseCode::NotImplemented) }
    }

    /// Changes the status of a message (SetMessageStatus).
    fn set_message_status(
        &mut self,
        handle: MessageHandle,
        status: MessageStatus,
    ) -> impl Future<Output = std::result::Result<(), ResponseCode>> + Send {
        let _ = (handle, status);
        async { Err(ResponseCode::NotImplemented) }
    }

    /// Stores, and in the outbox sends, a message (PushMessage).
    fn push_message(
        &mut self,
        folder: &str,
        message: BMessage,
        params: &PushMessageParams,
    ) -> impl Future<Output = std::result::Result<MessageHandle, ResponseCode>> + Send {
        let _ = (folder, message, params);
        async { Err(ResponseCode::NotImplemented) }
    }

    /// Checks the network for new messages (UpdateInbox).
    fn update_inbox(
        &mut self,
    ) -> impl Future<Output = std::result::Result<(), ResponseCode>> + Send {
        async { Err(ResponseCode::NotImplemented) }
    }

    /// Turns notifications on or off (SetNotificationRegistration).
    fn set_notification_registration(
        &mut self,
        enabled: bool,
    ) -> impl Future<Output = std::result::Result<(), ResponseCode>> + Send {
        let _ = enabled;
        async { Err(ResponseCode::NotImplemented) }
    }

    /// Chooses which events are reported (SetNotificationFilter).
    fn set_notification_filter(
        &mut self,
        mask: NotificationFilterMask,
    ) -> impl Future<Output = std::result::Result<(), ResponseCode>> + Send {
        let _ = mask;
        async { Err(ResponseCode::NotImplemented) }
    }

    /// Describes MAS instance `instance_id` (GetMASInstanceInformation).
    fn get_mas_instance_information(
        &mut self,
        instance_id: u8,
    ) -> impl Future<Output = std::result::Result<MasInstanceInformation, ResponseCode>> + Send
    {
        let _ = instance_id;
        async { Err(ResponseCode::NotImplemented) }
    }

    /// Lists conversations (GetConversationListing).
    fn get_conversation_listing(
        &mut self,
        params: &ConversationListingParams,
    ) -> impl Future<Output = std::result::Result<ConversationListingResponse, ResponseCode>> + Send
    {
        let _ = params;
        async { Err(ResponseCode::NotImplemented) }
    }

    /// Changes the owner's presence or chat state (SetOwnerStatus).
    fn set_owner_status(
        &mut self,
        status: OwnerStatus,
    ) -> impl Future<Output = std::result::Result<(), ResponseCode>> + Send {
        let _ = status;
        async { Err(ResponseCode::NotImplemented) }
    }

    /// Returns the owner's presence and chat state (GetOwnerStatus).
    fn get_owner_status(
        &mut self,
        conversation_id: Option<ConversationId>,
    ) -> impl Future<Output = std::result::Result<OwnerStatus, ResponseCode>> + Send {
        let _ = conversation_id;
        async { Err(ResponseCode::NotImplemented) }
    }
}

/// Converts a reply into the object the OBEX server sends.
fn object(reply: Reply) -> Object {
    match reply.body {
        Some(body) => Object::new(reply.headers, body),
        None => Object::headers_only(reply.headers),
    }
}

/// Adapts a [`MessageStore`] to an async OBEX server handler.
struct MasService<S>(S);

impl<S: MessageStore + Send> Handler for MasService<S> {
    async fn connect(&mut self, request: &Request) -> Response {
        let result = match mas::decode_connect(request, &MESSAGE_ACCESS) {
            Ok(features) => self.0.connect(features).await,
            Err(code) => Err(code),
        };
        mas::status_reply(result)
    }

    async fn disconnect(&mut self, _request: &Request) -> Response {
        self.0.disconnect().await;
        Response::new(ResponseCode::Success)
    }

    async fn set_path(&mut self, request: &Request) -> Response {
        mas::status_reply(self.0.set_folder(mas::decode_set_folder(request)).await)
    }

    async fn get(&mut self, request: &Request) -> std::result::Result<Object, Response> {
        let store = &mut self.0;

        let reply = match MasGet::decode(request).map_err(Response::new)? {
            MasGet::FolderListing(params) => store
                .get_folder_listing()
                .await
                .map(|listing| Reply::folder_listing(listing, &params)),
            MasGet::MessagesListing { folder, params } => store
                .get_messages_listing(&folder, &params)
                .await
                .map(Reply::messages_listing),
            MasGet::Message {
                handle,
                params,
                fraction: None,
            } => store
                .get_message(handle, &params)
                .await
                .map(|message| Reply::message(&message)),
            MasGet::Message {
                handle,
                params,
                fraction: Some(fraction),
            } => store
                .get_message_fraction(handle, &params, fraction)
                .await
                .map(Reply::message_fraction),
            MasGet::InstanceInformation(instance_id) => store
                .get_mas_instance_information(instance_id)
                .await
                .map(Reply::mas_instance_information),
            MasGet::ConversationListing(params) => store
                .get_conversation_listing(&params)
                .await
                .map(Reply::conversation_listing),
            MasGet::OwnerStatus(conversation_id) => store
                .get_owner_status(conversation_id)
                .await
                .map(Reply::owner_status),
        };

        reply.map(object).map_err(Response::new)
    }

    async fn put(&mut self, request: &Request, body: Option<Vec<u8>>) -> Response {
        let store = &mut self.0;

        let operation = match MasPut::decode(request, body) {
            Ok(operation) => operation,
            Err(code) => return Response::new(code),
        };

        match operation {
            MasPut::NotificationRegistration(enabled) => {
                mas::status_reply(store.set_notification_registration(enabled).await)
            }
            MasPut::NotificationFilter(mask) => {
                mas::status_reply(store.set_notification_filter(mask).await)
            }
            MasPut::MessageStatus { handle, status } => {
                mas::status_reply(store.set_message_status(handle, status).await)
            }
            MasPut::PushMessage {
                folder,
                message,
                params,
            } => match store.push_message(&folder, *message, &params).await {
                Ok(handle) => mas::push_message_reply(handle),
                Err(code) => Response::new(code),
            },
            MasPut::UpdateInbox => mas::status_reply(store.update_inbox().await),
            MasPut::OwnerStatus(status) => mas::status_reply(store.set_owner_status(status).await),
        }
    }
}

/// The async Message Access Service server. See [`crate::server::MasServer`].
pub struct MasServer<T, S> {
    server: Server<T, MasService<S>>,
}

impl<T: AsyncRead + AsyncWrite + Unpin, S: MessageStore + Send> MasServer<T, S> {
    /// Creates a server over a connected transport.
    pub fn new(transport: T, store: S) -> MasServer<T, S> {
        MasServer {
            server: Server::new(transport, MasService(store)),
        }
    }

    /// Sets the largest packet the server can receive.
    pub fn with_max_packet_length(mut self, max_packet_length: u16) -> MasServer<T, S> {
        self.server = self.server.with_max_packet_length(max_packet_length);
        self
    }

    /// Accepts Single Response Mode, for GOEP 2.0 (L2CAP) transports.
    pub fn with_single_response_mode(mut self, enabled: bool) -> MasServer<T, S> {
        self.server = self.server.with_single_response_mode(enabled);
        self
    }

    /// Sets the largest pushed message accepted, 64 MiB by default.
    pub fn with_max_object_size(mut self, max_object_size: usize) -> MasServer<T, S> {
        self.server = self.server.with_max_object_size(max_object_size);
        self
    }

    /// The message store.
    pub fn store(&self) -> &S {
        &self.server.handler().0
    }

    /// The message store, mutably.
    pub fn store_mut(&mut self) -> &mut S {
        &mut self.server.handler_mut().0
    }

    /// Serves requests until the client closes the transport.
    pub async fn run(&mut self) -> Result<()> {
        Ok(self.server.run().await?)
    }

    /// Reads one request and serves the operation it starts.
    pub async fn handle_next(&mut self) -> Result<()> {
        Ok(self.server.handle_next().await?)
    }

    /// Unwraps the transport and the message store.
    pub fn into_parts(self) -> (T, S) {
        let (transport, service) = self.server.into_parts();
        (transport, service.0)
    }
}

/// Receives the event reports delivered to an async [`MnsServer`]. See
/// [`crate::server::NotificationHandler`].
///
/// Closures taking the MAS instance id and the report and returning a `Send` future
/// implement this trait.
pub trait NotificationHandler {
    /// The MSE connected to deliver events.
    fn connect(&mut self) -> impl Future<Output = std::result::Result<(), ResponseCode>> + Send {
        async { Ok(()) }
    }

    /// An event concerning MAS instance `mas_instance_id` (SendEvent).
    fn event(
        &mut self,
        mas_instance_id: u8,
        report: EventReport,
    ) -> impl Future<Output = std::result::Result<(), ResponseCode>> + Send;

    /// The MSE disconnected, which turns notifications off.
    fn disconnect(&mut self) -> impl Future<Output = ()> + Send {
        async {}
    }
}

impl<F, Fut> NotificationHandler for F
where
    F: FnMut(u8, EventReport) -> Fut + Send,
    Fut: Future<Output = ()> + Send,
{
    fn event(
        &mut self,
        mas_instance_id: u8,
        report: EventReport,
    ) -> impl Future<Output = std::result::Result<(), ResponseCode>> + Send {
        let delivered = self(mas_instance_id, report);
        async move {
            delivered.await;
            Ok(())
        }
    }
}

/// Adapts a [`NotificationHandler`] to an async OBEX server handler.
struct MnsService<H>(H);

impl<H: NotificationHandler + Send> Handler for MnsService<H> {
    async fn connect(&mut self, request: &Request) -> Response {
        let result = match mas::decode_connect(request, &MESSAGE_NOTIFICATION) {
            Ok(_) => self.0.connect().await,
            Err(code) => Err(code),
        };
        mas::status_reply(result)
    }

    async fn disconnect(&mut self, _request: &Request) -> Response {
        self.0.disconnect().await;
        Response::new(ResponseCode::Success)
    }

    async fn put(&mut self, request: &Request, body: Option<Vec<u8>>) -> Response {
        let result = match mas::decode_send_event(request, body) {
            Ok((mas_instance_id, report)) => self.0.event(mas_instance_id, report).await,
            Err(code) => Err(code),
        };
        mas::status_reply(result)
    }
}

/// The async Message Notification Service server. See [`crate::server::MnsServer`].
pub struct MnsServer<T, H> {
    server: Server<T, MnsService<H>>,
}

impl<T: AsyncRead + AsyncWrite + Unpin, H: NotificationHandler + Send> MnsServer<T, H> {
    /// Creates a server over a connected transport.
    pub fn new(transport: T, handler: H) -> MnsServer<T, H> {
        MnsServer {
            server: Server::new(transport, MnsService(handler)),
        }
    }

    /// Sets the largest packet the server can receive.
    pub fn with_max_packet_length(mut self, max_packet_length: u16) -> MnsServer<T, H> {
        self.server = self.server.with_max_packet_length(max_packet_length);
        self
    }

    /// Accepts Single Response Mode, for GOEP 2.0 (L2CAP) transports.
    pub fn with_single_response_mode(mut self, enabled: bool) -> MnsServer<T, H> {
        self.server = self.server.with_single_response_mode(enabled);
        self
    }

    /// The notification handler.
    pub fn handler(&self) -> &H {
        &self.server.handler().0
    }

    /// The notification handler, mutably.
    pub fn handler_mut(&mut self) -> &mut H {
        &mut self.server.handler_mut().0
    }

    /// Serves the MSE until it closes the transport.
    pub async fn run(&mut self) -> Result<()> {
        Ok(self.server.run().await?)
    }

    /// Reads one request and serves the operation it starts.
    pub async fn handle_next(&mut self) -> Result<()> {
        Ok(self.server.handle_next().await?)
    }

    /// Unwraps the transport and the handler.
    pub fn into_parts(self) -> (T, H) {
        let (transport, service) = self.server.into_parts();
        (transport, service.0)
    }
}
