//! A native implementation of the Bluetooth Message Access Profile (MAP 1.4)
//!
//! MAP lets a device such as a car kit, a watch or a PC (the *MCE*, Message Client
//! Equipment) browse, read, send and delete the messages (SMS, MMS, e-mail and
//! instant messages) stored on a phone (the *MSE*, Message Server Equipment), and be
//! notified when they change. It runs on OBEX, implemented by the [`obex`] crate,
//! and defines two OBEX services:
//!
//! * the **Message Access Service** (MAS), where the MCE is the client and the MSE
//!   the server, carrying the MAP functions such as GetMessagesListing and
//!   PushMessage; and
//! * the **Message Notification Service** (MNS), where the MSE is the client and
//!   the MCE the server, carrying event reports such as `NewMessage`.
//!
//! | Role | Message Access Service | Message Notification Service |
//! |------|------------------------|------------------------------|
//! | MCE (car kit, PC) | [`client::MasClient`] | [`server::MnsServer`] + [`server::NotificationHandler`] |
//! | MSE (phone) | [`server::MasServer`] + [`server::MessageStore`] | [`client::MnsClient`] |
//!
//! With the `tokio` feature, [`tokio`](mod@crate::tokio) has async versions of all four,
//! with the same methods. The blocking and async types share their encoding, so they
//! behave identically and interoperate.
//!
//! The crate is organized in layers, each usable on its own:
//!
//! * **Objects**: [`bmessage`], [`folder_listing`], [`messages_listing`],
//!   [`conversation_listing`] and [`event_report`] parse and encode the MAP object
//!   formats. They never perform I/O.
//! * **Functions**: [`mas`] holds the parameters and results of every MAP function,
//!   the Application Parameters tags and the OBEX types MAP uses.
//! * **Values**: [`types`] (message handles, conversation ids, timestamps, ...),
//!   [`features`] (`MapSupportedFeatures` and the other bit masks) and [`sdp`]
//!   (service discovery UUIDs and attributes).
//! * **Roles**: [`client`], [`server`] and [`tokio`](mod@crate::tokio).
//!
//! ## Transports
//!
//! MAP runs over an RFCOMM channel (GOEP 1.1) or an L2CAP channel in Enhanced
//! Retransmission Mode (GOEP 2.0), both found through SDP (see [`sdp`]). Opening
//! sockets, pairing and SDP belong to a Bluetooth stack, such as BlueZ through the
//! `bluer` crate, or the platform APIs. Anything implementing [`std::io::Read`] and
//! [`std::io::Write`], or tokio's `AsyncRead` and `AsyncWrite`, can be handed to
//! this crate. Over L2CAP, set the maximum packet length to the channel MTU and
//! enable Single Response Mode on both clients and servers; over RFCOMM leave it off.
//!
//! ## Example
//!
//! Reading the unread SMS on a phone, as an MCE:
//! ```no_run
//! # fn example(socket: std::net::TcpStream) -> map::Result<()> {
//! use map::client::MasClient;
//! use map::features::MessageTypeFilter;
//! use map::mas::{GetMessageParams, MessagesListingParams};
//! use map::types::{MessageType, ReadStatusFilter};
//!
//! let mut client = MasClient::new(socket);
//! client.connect()?;
//! client.set_folder_path("telecom/msg")?;
//!
//! let unread = client.get_messages_listing(
//!     "inbox",
//!     &MessagesListingParams {
//!         filter_read_status: Some(ReadStatusFilter::Unread),
//!         filter_message_type: Some(MessageTypeFilter::only(&[MessageType::SmsGsm])),
//!         ..MessagesListingParams::default()
//!     },
//! )?;
//!
//! for entry in unread.listing.unwrap_or_default().messages {
//!     let message = client.get_message(entry.handle, &GetMessageParams::default())?;
//!     println!("{}: {}", entry.sender_addressing.unwrap_or_default(), message.text());
//! }
//!
//! client.disconnect()?;
//! # Ok(())
//! # }
//! ```
//!
//! Sending an SMS through the phone:
//! ```no_run
//! # fn example(client: &mut map::client::MasClient<std::net::TcpStream>) -> map::Result<()> {
//! use map::bmessage::{BMessage, BMessageBody, VCard};
//! use map::mas::PushMessageParams;
//! use map::types::MessageType;
//!
//! let message = BMessage::new(MessageType::SmsGsm, BMessageBody::from_text("On my way!"))
//!     .with_recipient(VCard::telephone("+15551234567"));
//!
//! client.set_folder_path("telecom/msg")?;
//! let handle = client.push_message("outbox", &message, &PushMessageParams::default())?;
//! # Ok(())
//! # }
//! ```
//!
#![warn(missing_docs)]

pub mod bmessage;
pub mod client;
pub mod conversation_listing;
pub mod error;
pub mod event_report;
pub mod features;
pub mod folder_listing;
pub mod mas;
pub mod messages_listing;
pub mod sdp;
pub mod server;
#[cfg(feature = "tokio")]
pub mod tokio;
pub mod types;

mod xml;

pub use crate::error::{Error, Result};
