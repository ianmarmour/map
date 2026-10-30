//! ## Blocking Servers
//!
//! [`MnsServer`] is the MCE side of the Message Notification Service: the phone (MSE)
//! connects to it to deliver event reports such as new message arrivals, which are
//! handed to a [`NotificationHandler`]. [`MasServer`] is the MSE side of the Message
//! Access Service, answering MCE requests from a [`MessageStore`].
//!
//! Both take a connected `Read + Write` transport: accept the connection on the
//! RFCOMM channel or L2CAP PSM advertised in your SDP record (see [`crate::sdp`]),
//! then hand the socket to the server and [`run`](MnsServer::run) it, typically on its
//! own thread. The async equivalents are in [`crate::tokio`].
//!
//! # Examples
//!
//! Receiving notifications next to a [`MasClient`](crate::client::MasClient):
//! ```no_run
//! # fn example(
//! #     mas_socket: std::net::TcpStream,
//! #     mns_listener: std::net::TcpListener,
//! # ) -> bluetooth_map::Result<()> {
//! use bluetooth_map::client::MasClient;
//! use bluetooth_map::event_report::EventReport;
//! use bluetooth_map::server::MnsServer;
//!
//! let mut client = MasClient::new(mas_socket);
//! client.connect()?;
//!
//! let notifications = std::thread::spawn(move || -> bluetooth_map::Result<()> {
//!     let (socket, _) = mns_listener.accept()?;
//!     MnsServer::new(socket, |instance: u8, report: EventReport| {
//!         println!("MAS {}: {} {:?}", instance, report.event_type, report.handle);
//!     })
//!     .run()
//! });
//!
//! // The phone now connects to the Message Notification Server.
//! client.set_notification_registration(true)?;
//! # notifications.join().unwrap()?;
//! # Ok(())
//! # }
//! ```
//!
use std::io::{Read, Write};

use obex::packet::{Request, Response};
use obex::response::ResponseCode;
use obex::server::{Handler, Object, Server};
use obex::target::{MESSAGE_ACCESS, MESSAGE_NOTIFICATION};

use crate::bmessage::BMessage;
use crate::error::Result;
use crate::event_report::EventReport;
use crate::features::{MapSupportedFeatures, NotificationFilterMask};
use crate::folder_listing::FolderListing;
use crate::mas::{
    self, ConversationListingParams, ConversationListingResponse, GetMessageParams, MasGet,
    MasInstanceInformation, MasPut, MessageFraction, MessageStatus, MessagesListingParams,
    MessagesListingResponse, OwnerStatus, PushMessageParams, Reply, SetFolder,
};
use crate::types::{ConversationId, FractionRequest, MessageHandle};

/// The message repository behind a [`MasServer`], implemented by MSE applications.
///
/// Each method answers one MAP function; returning an error code refuses the request
/// with it (see MAP section 6.3.3 for their meanings, e.g.
/// [`ResponseCode::NotFound`] for an unknown handle or folder). Functions an MSE
/// does not support keep the default implementation, which answers
/// [`ResponseCode::NotImplemented`].
///
/// The store keeps track of the current folder, which starts at the root of the
/// virtual folder tree `telecom/msg/{inbox,outbox,sent,deleted,draft}` (MAP section
/// 3.1.4).
pub trait MessageStore {
    /// An MCE connected. `features` are its `MapSupportedFeatures`, if it sent them.
    fn connect(
        &mut self,
        features: Option<MapSupportedFeatures>,
    ) -> std::result::Result<(), ResponseCode> {
        let _ = features;
        Ok(())
    }

    /// The MCE disconnected.
    fn disconnect(&mut self) {}

    /// Changes the current folder (SetFolder).
    fn set_folder(&mut self, step: SetFolder) -> std::result::Result<(), ResponseCode>;

    /// Lists every folder in the current folder (GetFolderListing). The server
    /// applies the request's offset and count.
    fn get_folder_listing(&mut self) -> std::result::Result<FolderListing, ResponseCode>;

    /// Lists the messages of `folder`, a child of the current folder or `""` for the
    /// current folder (GetMessagesListing). [`MessagesListingParams::apply`] does the
    /// filtering, ordering and segmentation given every message of the folder.
    fn get_messages_listing(
        &mut self,
        folder: &str,
        params: &MessagesListingParams,
    ) -> std::result::Result<MessagesListingResponse, ResponseCode>;

    /// Returns a message (GetMessage).
    fn get_message(
        &mut self,
        handle: MessageHandle,
        params: &GetMessageParams,
    ) -> std::result::Result<BMessage, ResponseCode>;

    /// Returns a fraction of a fractioned e-mail (GetMessage with `FractionRequest`).
    fn get_message_fraction(
        &mut self,
        handle: MessageHandle,
        params: &GetMessageParams,
        fraction: FractionRequest,
    ) -> std::result::Result<MessageFraction, ResponseCode> {
        let _ = (handle, params, fraction);
        Err(ResponseCode::NotImplemented)
    }

    /// Changes the status of a message (SetMessageStatus).
    fn set_message_status(
        &mut self,
        handle: MessageHandle,
        status: MessageStatus,
    ) -> std::result::Result<(), ResponseCode> {
        let _ = (handle, status);
        Err(ResponseCode::NotImplemented)
    }

    /// Stores, and in the outbox sends, a message; returns its new handle (PushMessage).
    fn push_message(
        &mut self,
        folder: &str,
        message: BMessage,
        params: &PushMessageParams,
    ) -> std::result::Result<MessageHandle, ResponseCode> {
        let _ = (folder, message, params);
        Err(ResponseCode::NotImplemented)
    }

    /// Checks the network for new messages (UpdateInbox).
    fn update_inbox(&mut self) -> std::result::Result<(), ResponseCode> {
        Err(ResponseCode::NotImplemented)
    }

    /// Turns notifications on or off (SetNotificationRegistration). When turned on,
    /// the MSE connects an [`MnsClient`](crate::client::MnsClient) to the MCE.
    fn set_notification_registration(
        &mut self,
        enabled: bool,
    ) -> std::result::Result<(), ResponseCode> {
        let _ = enabled;
        Err(ResponseCode::NotImplemented)
    }

    /// Chooses which events are reported (SetNotificationFilter).
    fn set_notification_filter(
        &mut self,
        mask: NotificationFilterMask,
    ) -> std::result::Result<(), ResponseCode> {
        let _ = mask;
        Err(ResponseCode::NotImplemented)
    }

    /// Describes MAS instance `instance_id` (GetMASInstanceInformation).
    fn get_mas_instance_information(
        &mut self,
        instance_id: u8,
    ) -> std::result::Result<MasInstanceInformation, ResponseCode> {
        let _ = instance_id;
        Err(ResponseCode::NotImplemented)
    }

    /// Lists conversations (GetConversationListing).
    fn get_conversation_listing(
        &mut self,
        params: &ConversationListingParams,
    ) -> std::result::Result<ConversationListingResponse, ResponseCode> {
        let _ = params;
        Err(ResponseCode::NotImplemented)
    }

    /// Changes the owner's presence or chat state (SetOwnerStatus).
    fn set_owner_status(&mut self, status: OwnerStatus) -> std::result::Result<(), ResponseCode> {
        let _ = status;
        Err(ResponseCode::NotImplemented)
    }

    /// Returns the owner's presence and chat state (GetOwnerStatus).
    fn get_owner_status(
        &mut self,
        conversation_id: Option<ConversationId>,
    ) -> std::result::Result<OwnerStatus, ResponseCode> {
        let _ = conversation_id;
        Err(ResponseCode::NotImplemented)
    }
}

/// Converts a reply into the object the OBEX server sends.
fn object(reply: Reply) -> Object {
    match reply.body {
        Some(body) => Object::new(reply.headers, body),
        None => Object::headers_only(reply.headers),
    }
}

/// Adapts a [`MessageStore`] to an OBEX server handler.
struct MasService<S>(S);

impl<S: MessageStore> Handler for MasService<S> {
    fn connect(&mut self, request: &Request) -> Response {
        let result = mas::decode_connect(request, &MESSAGE_ACCESS)
            .and_then(|features| self.0.connect(features));
        mas::status_reply(result)
    }

    fn disconnect(&mut self, _request: &Request) -> Response {
        self.0.disconnect();
        Response::new(ResponseCode::Success)
    }

    fn set_path(&mut self, request: &Request) -> Response {
        mas::status_reply(self.0.set_folder(mas::decode_set_folder(request)))
    }

    fn get(&mut self, request: &Request) -> std::result::Result<Object, Response> {
        let store = &mut self.0;

        let reply = match MasGet::decode(request).map_err(Response::new)? {
            MasGet::FolderListing(params) => store
                .get_folder_listing()
                .map(|listing| Reply::folder_listing(listing, &params)),
            MasGet::MessagesListing { folder, params } => store
                .get_messages_listing(&folder, &params)
                .map(Reply::messages_listing),
            MasGet::Message {
                handle,
                params,
                fraction: None,
            } => store
                .get_message(handle, &params)
                .map(|message| Reply::message(&message)),
            MasGet::Message {
                handle,
                params,
                fraction: Some(fraction),
            } => store
                .get_message_fraction(handle, &params, fraction)
                .map(Reply::message_fraction),
            MasGet::InstanceInformation(instance_id) => store
                .get_mas_instance_information(instance_id)
                .map(Reply::mas_instance_information),
            MasGet::ConversationListing(params) => store
                .get_conversation_listing(&params)
                .map(Reply::conversation_listing),
            MasGet::OwnerStatus(conversation_id) => store
                .get_owner_status(conversation_id)
                .map(Reply::owner_status),
        };

        reply.map(object).map_err(Response::new)
    }

    fn put(&mut self, request: &Request, body: Option<Vec<u8>>) -> Response {
        let store = &mut self.0;

        let operation = match MasPut::decode(request, body) {
            Ok(operation) => operation,
            Err(code) => return Response::new(code),
        };

        match operation {
            MasPut::NotificationRegistration(enabled) => {
                mas::status_reply(store.set_notification_registration(enabled))
            }
            MasPut::NotificationFilter(mask) => {
                mas::status_reply(store.set_notification_filter(mask))
            }
            MasPut::MessageStatus { handle, status } => {
                mas::status_reply(store.set_message_status(handle, status))
            }
            MasPut::PushMessage {
                folder,
                message,
                params,
            } => match store.push_message(&folder, *message, &params) {
                Ok(handle) => mas::push_message_reply(handle),
                Err(code) => Response::new(code),
            },
            MasPut::UpdateInbox => mas::status_reply(store.update_inbox()),
            MasPut::OwnerStatus(status) => mas::status_reply(store.set_owner_status(status)),
        }
    }
}

/// A Message Access Service server, the MSE role, answering requests from a
/// [`MessageStore`].
///
/// # Examples
/// ```no_run
/// # fn example(socket: std::net::TcpStream) -> bluetooth_map::Result<()> {
/// use bluetooth_map::bmessage::BMessage;
/// use bluetooth_map::folder_listing::{Folder, FolderListing};
/// use bluetooth_map::mas::{GetMessageParams, MessagesListingParams, MessagesListingResponse, SetFolder};
/// use bluetooth_map::server::{MasServer, MessageStore};
/// use bluetooth_map::types::MessageHandle;
/// use bluetooth_map::obex::response::ResponseCode;
///
/// struct Phone;
///
/// impl MessageStore for Phone {
///     fn set_folder(&mut self, step: SetFolder) -> Result<(), ResponseCode> {
///         Ok(())
///     }
///
///     fn get_folder_listing(&mut self) -> Result<FolderListing, ResponseCode> {
///         Ok(FolderListing::new([Folder::new("inbox"), Folder::new("sent")]))
///     }
///
///     fn get_messages_listing(
///         &mut self,
///         folder: &str,
///         params: &MessagesListingParams,
///     ) -> Result<MessagesListingResponse, ResponseCode> {
///         Ok(params.apply(Vec::new()))
///     }
///
///     fn get_message(
///         &mut self,
///         handle: MessageHandle,
///         params: &GetMessageParams,
///     ) -> Result<BMessage, ResponseCode> {
///         Err(ResponseCode::NotFound)
///     }
/// }
///
/// MasServer::new(socket, Phone).run()?;
/// # Ok(())
/// # }
/// ```
pub struct MasServer<T, S> {
    server: Server<T, MasService<S>>,
}

impl<T: Read + Write, S: MessageStore> MasServer<T, S> {
    /// Creates a server over a connected transport.
    pub fn new(transport: T, store: S) -> MasServer<T, S> {
        MasServer {
            server: Server::new(transport, MasService(store)),
        }
    }

    /// Sets the largest packet the server can receive. Over L2CAP this must not
    /// exceed the channel MTU.
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
    pub fn run(&mut self) -> Result<()> {
        Ok(self.server.run()?)
    }

    /// Reads one request and serves the operation it starts.
    pub fn handle_next(&mut self) -> Result<()> {
        Ok(self.server.handle_next()?)
    }

    /// Unwraps the transport and the message store.
    pub fn into_parts(self) -> (T, S) {
        let (transport, service) = self.server.into_parts();
        (transport, service.0)
    }
}

/// Receives the event reports delivered to an [`MnsServer`], implemented by MCE
/// applications.
///
/// Closures taking the MAS instance id and the report implement this trait.
pub trait NotificationHandler {
    /// The MSE connected to deliver events.
    fn connect(&mut self) -> std::result::Result<(), ResponseCode> {
        Ok(())
    }

    /// An event concerning MAS instance `mas_instance_id` (SendEvent). Returning an
    /// error code refuses it, e.g. [`ResponseCode::ServiceUnavailable`].
    fn event(
        &mut self,
        mas_instance_id: u8,
        report: EventReport,
    ) -> std::result::Result<(), ResponseCode>;

    /// The MSE disconnected, which turns notifications off.
    fn disconnect(&mut self) {}
}

impl<F: FnMut(u8, EventReport)> NotificationHandler for F {
    fn event(
        &mut self,
        mas_instance_id: u8,
        report: EventReport,
    ) -> std::result::Result<(), ResponseCode> {
        self(mas_instance_id, report);
        Ok(())
    }
}

/// Adapts a [`NotificationHandler`] to an OBEX server handler.
struct MnsService<H>(H);

impl<H: NotificationHandler> Handler for MnsService<H> {
    fn connect(&mut self, request: &Request) -> Response {
        let result =
            mas::decode_connect(request, &MESSAGE_NOTIFICATION).and_then(|_| self.0.connect());
        mas::status_reply(result)
    }

    fn disconnect(&mut self, _request: &Request) -> Response {
        self.0.disconnect();
        Response::new(ResponseCode::Success)
    }

    fn put(&mut self, request: &Request, body: Option<Vec<u8>>) -> Response {
        let result = mas::decode_send_event(request, body)
            .and_then(|(mas_instance_id, report)| self.0.event(mas_instance_id, report));
        mas::status_reply(result)
    }
}

/// A Message Notification Service server, the MCE role, handing event reports to a
/// [`NotificationHandler`]. See [the module level documentation](index.html) for an
/// example.
pub struct MnsServer<T, H> {
    server: Server<T, MnsService<H>>,
}

impl<T: Read + Write, H: NotificationHandler> MnsServer<T, H> {
    /// Creates a server over a connected transport.
    pub fn new(transport: T, handler: H) -> MnsServer<T, H> {
        MnsServer {
            server: Server::new(transport, MnsService(handler)),
        }
    }

    /// Sets the largest packet the server can receive. Over L2CAP this must not
    /// exceed the channel MTU.
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
    pub fn run(&mut self) -> Result<()> {
        Ok(self.server.run()?)
    }

    /// Reads one request and serves the operation it starts.
    pub fn handle_next(&mut self) -> Result<()> {
        Ok(self.server.handle_next()?)
    }

    /// Unwraps the transport and the handler.
    pub fn into_parts(self) -> (T, H) {
        let (transport, service) = self.server.into_parts();
        (transport, service.0)
    }
}
