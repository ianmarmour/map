//! End to end tests of the `embedded-io-async` clients and servers, run on the host
//! with `cargo test --all-features`. Transports are tokio duplex streams adapted with
//! `embedded-io-adapters`, and both ends run concurrently on one task, so nothing here
//! needs to be `Send`.

#![cfg(all(feature = "embedded-io-async", feature = "tokio"))]

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::{long_text, Phone};
use embedded_io_adapters::tokio_1::FromTokio;
use obex_protocol::response::ResponseCode;
use obex_protocol_map::bmessage::{BMessage, BMessageBody, VCard};
use obex_protocol_map::embedded::{MasClient, MasServer, MessageStore, MnsClient, MnsServer};
use obex_protocol_map::event_report::{EventReport, EventType};
use obex_protocol_map::features::{MapSupportedFeatures, NotificationFilterMask};
use obex_protocol_map::folder_listing::FolderListing;
use obex_protocol_map::mas::{
    GetMessageParams, MessageStatus, MessagesListingParams, MessagesListingResponse,
    PushMessageParams, SetFolder,
};
use obex_protocol_map::server::MessageStore as _;
use obex_protocol_map::types::{MessageHandle, MessageType};
use tokio::io::{duplex, DuplexStream};

type Channel = FromTokio<DuplexStream>;

fn channels() -> (Channel, Channel) {
    let (client, server) = duplex(64 * 1024);
    (FromTokio::new(client), FromTokio::new(server))
}

/// The in-memory phone, shared through an `Rc` as a tokio store could not be.
struct EmbeddedPhone(Rc<RefCell<Phone>>);

impl MessageStore for EmbeddedPhone {
    async fn connect(
        &mut self,
        features: Option<MapSupportedFeatures>,
    ) -> Result<(), ResponseCode> {
        self.0.borrow_mut().connect(features)
    }

    async fn disconnect(&mut self) {
        self.0.borrow_mut().disconnect();
    }

    async fn set_folder(&mut self, step: SetFolder) -> Result<(), ResponseCode> {
        // Yield to show stores can await while the server waits for them.
        tokio::task::yield_now().await;
        self.0.borrow_mut().set_folder(step)
    }

    async fn get_folder_listing(&mut self) -> Result<FolderListing, ResponseCode> {
        self.0.borrow_mut().get_folder_listing()
    }

    async fn get_messages_listing(
        &mut self,
        folder: &str,
        params: &MessagesListingParams,
    ) -> Result<MessagesListingResponse, ResponseCode> {
        self.0.borrow_mut().get_messages_listing(folder, params)
    }

    async fn get_message(
        &mut self,
        handle: MessageHandle,
        params: &GetMessageParams,
    ) -> Result<BMessage, ResponseCode> {
        self.0.borrow_mut().get_message(handle, params)
    }

    async fn set_message_status(
        &mut self,
        handle: MessageHandle,
        status: MessageStatus,
    ) -> Result<(), ResponseCode> {
        self.0.borrow_mut().set_message_status(handle, status)
    }

    async fn push_message(
        &mut self,
        folder: &str,
        message: BMessage,
        params: &PushMessageParams,
    ) -> Result<MessageHandle, ResponseCode> {
        self.0.borrow_mut().push_message(folder, message, params)
    }

    async fn set_notification_registration(&mut self, enabled: bool) -> Result<(), ResponseCode> {
        self.0.borrow_mut().set_notification_registration(enabled)
    }

    async fn set_notification_filter(
        &mut self,
        mask: NotificationFilterMask,
    ) -> Result<(), ResponseCode> {
        self.0.borrow_mut().set_notification_filter(mask)
    }
}

#[tokio::test]
async fn message_access_client_and_server() {
    for srm in [false, true] {
        let (client_channel, server_channel) = channels();
        let phone = Rc::new(RefCell::new(Phone::new()));
        let mut server = MasServer::new(server_channel, EmbeddedPhone(phone.clone()))
            .with_single_response_mode(srm)
            .with_max_packet_length(512);

        let client = async move {
            let mut client = MasClient::new(client_channel)
                .with_single_response_mode(srm)
                .with_max_packet_length(512)
                .with_supported_features(MapSupportedFeatures::all());
            client.connect().await.unwrap();
            client.set_folder_path("telecom/msg").await.unwrap();

            let folders = client
                .get_folder_listing(&Default::default())
                .await
                .unwrap();
            assert_eq!(
                vec!["inbox", "outbox", "sent", "deleted", "draft"],
                folders.names()
            );

            let response = client
                .get_messages_listing("inbox", &MessagesListingParams::default())
                .await
                .unwrap();
            // Hosts have a clock, so the server fills in the time.
            assert!(response.mse_time.is_some());
            let listing = response.listing.unwrap();
            assert_eq!(4, listing.messages.len());

            let email = listing
                .messages
                .iter()
                .find(|entry| entry.message_type == Some(MessageType::Email))
                .unwrap()
                .handle;
            let message = client
                .get_message(email, &GetMessageParams::default())
                .await
                .unwrap();
            assert_eq!(long_text(), message.text());
            client
                .set_message_status(email, MessageStatus::Read(true))
                .await
                .unwrap();

            let reply = BMessage::new(MessageType::SmsGsm, BMessageBody::from_text("Sure!"))
                .with_recipient(VCard::telephone("+15550001"));
            let pushed = client
                .push_message("outbox", &reply, &PushMessageParams::default())
                .await
                .unwrap();

            client.set_notification_registration(true).await.unwrap();

            // Functions the store left unimplemented are refused.
            let update = client.update_inbox().await;
            assert_eq!(
                Some(ResponseCode::NotImplemented),
                update.unwrap_err().response_code()
            );
            let missing = client
                .get_message(MessageHandle(1), &GetMessageParams::default())
                .await;
            assert_eq!(
                Some(ResponseCode::NotFound),
                missing.unwrap_err().response_code()
            );

            client.disconnect().await.unwrap();
            (email, pushed)
        };

        let ((email, pushed), served) = tokio::join!(client, server.run());
        served.unwrap();

        let mut phone = phone.borrow_mut();
        assert_eq!(Some(true), phone.find(email).unwrap().entry.read);
        assert_eq!("telecom/msg/outbox", phone.find(pushed).unwrap().folder);
        assert_eq!(
            Some(MapSupportedFeatures::all()),
            phone.remote_features,
            "srm: {}",
            srm
        );
        assert_eq!(vec![true], phone.registrations);
        assert!(phone.disconnected);
    }
}

#[tokio::test]
async fn notifications_with_closure_handlers() {
    let (client_channel, server_channel) = channels();
    let received = Rc::new(RefCell::new(Vec::new()));
    let events = received.clone();

    let handler = move |instance: u8, report: EventReport| {
        events.borrow_mut().push((instance, report));
        async {}
    };
    let mut server = MnsServer::new(server_channel, handler);

    let report = EventReport {
        old_folder: Some("TELECOM/MSG/OUTBOX".to_string()),
        ..EventReport::message(
            EventType::MessageShift,
            MessageHandle(9),
            "TELECOM/MSG/SENT",
            MessageType::SmsGsm,
        )
    };
    let sent = report.clone();

    let client = async move {
        let mut client = MnsClient::new(client_channel);
        client.connect().await.unwrap();
        client.send_event(1, &sent).await.unwrap();
        client.disconnect().await.unwrap();
    };

    let ((), served) = tokio::join!(client, server.run());
    served.unwrap();
    assert_eq!(vec![(1, report)], *received.borrow());
}

#[tokio::test]
async fn interoperates_with_the_tokio_api() {
    // An embedded client against a tokio server.
    let (client_socket, server_socket) = duplex(64 * 1024);
    let server = tokio::spawn(async move {
        struct Store(Phone);
        impl obex_protocol_map::tokio::MessageStore for Store {
            async fn set_folder(&mut self, step: SetFolder) -> Result<(), ResponseCode> {
                self.0.set_folder(step)
            }

            async fn get_folder_listing(&mut self) -> Result<FolderListing, ResponseCode> {
                self.0.get_folder_listing()
            }

            async fn get_messages_listing(
                &mut self,
                folder: &str,
                params: &MessagesListingParams,
            ) -> Result<MessagesListingResponse, ResponseCode> {
                self.0.get_messages_listing(folder, params)
            }

            async fn get_message(
                &mut self,
                handle: MessageHandle,
                params: &GetMessageParams,
            ) -> Result<BMessage, ResponseCode> {
                self.0.get_message(handle, params)
            }
        }

        obex_protocol_map::tokio::MasServer::new(server_socket, Store(Phone::new()))
            .with_single_response_mode(true)
            .run()
            .await
            .unwrap();
    });

    let mut client = MasClient::new(FromTokio::new(client_socket)).with_single_response_mode(true);
    client.connect().await.unwrap();
    client.set_folder_path("telecom/msg/inbox").await.unwrap();
    let listing = client
        .get_messages_listing("", &MessagesListingParams::default())
        .await
        .unwrap()
        .listing
        .unwrap();
    assert_eq!(4, listing.messages.len());
    client.disconnect().await.unwrap();
    drop(client);
    server.await.unwrap();

    // A tokio client against an embedded server.
    let (client_socket, server_socket) = duplex(64 * 1024);
    let received = Rc::new(RefCell::new(Vec::new()));
    let events = received.clone();
    let handler = move |instance: u8, report: EventReport| {
        events.borrow_mut().push((instance, report.event_type));
        async {}
    };
    let mut server = MnsServer::new(FromTokio::new(server_socket), handler);

    let client = async move {
        let mut client = obex_protocol_map::tokio::MnsClient::new(client_socket);
        client.connect().await.unwrap();
        let report = EventReport::message(
            EventType::NewMessage,
            MessageHandle(3),
            "TELECOM/MSG/INBOX",
            MessageType::SmsGsm,
        );
        client.send_event(0, &report).await.unwrap();
        client.disconnect().await.unwrap();
    };

    let ((), served) = tokio::join!(client, server.run());
    served.unwrap();
    assert_eq!(vec![(0, EventType::NewMessage)], *received.borrow());
}
