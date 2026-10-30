//! End to end tests of the async clients and servers, and of their interoperation
//! with the blocking ones.

#![cfg(feature = "tokio")]

mod common;

use common::{long_text, Phone};
use map::bmessage::{BMessage, BMessageBody, VCard};
use map::event_report::{EventReport, EventType};
use map::features::{MapSupportedFeatures, NotificationFilterMask};
use map::folder_listing::FolderListing;
use map::mas::{
    GetMessageParams, MessageStatus, MessagesListingParams, MessagesListingResponse, OwnerStatus,
    PushMessageParams, SetFolder,
};
use map::server::MessageStore as _;
use map::tokio::{MasClient, MasServer, MessageStore, MnsClient, MnsServer};
use map::types::{ConversationId, MessageHandle, MessageType};
use obex::response::ResponseCode;
use tokio::io::duplex;
use tokio::sync::mpsc;

/// The in-memory phone behind the async server.
struct AsyncPhone(Phone);

impl MessageStore for AsyncPhone {
    async fn connect(
        &mut self,
        features: Option<MapSupportedFeatures>,
    ) -> Result<(), ResponseCode> {
        self.0.connect(features)
    }

    async fn disconnect(&mut self) {
        self.0.disconnect();
    }

    async fn set_folder(&mut self, step: SetFolder) -> Result<(), ResponseCode> {
        // Yield to show handlers can await while the server waits for them.
        tokio::task::yield_now().await;
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

    async fn set_message_status(
        &mut self,
        handle: MessageHandle,
        status: MessageStatus,
    ) -> Result<(), ResponseCode> {
        self.0.set_message_status(handle, status)
    }

    async fn push_message(
        &mut self,
        folder: &str,
        message: BMessage,
        params: &PushMessageParams,
    ) -> Result<MessageHandle, ResponseCode> {
        self.0.push_message(folder, message, params)
    }

    async fn set_notification_registration(&mut self, enabled: bool) -> Result<(), ResponseCode> {
        self.0.set_notification_registration(enabled)
    }

    async fn set_notification_filter(
        &mut self,
        mask: NotificationFilterMask,
    ) -> Result<(), ResponseCode> {
        self.0.set_notification_filter(mask)
    }

    async fn set_owner_status(&mut self, status: OwnerStatus) -> Result<(), ResponseCode> {
        self.0.set_owner_status(status)
    }

    async fn get_owner_status(
        &mut self,
        conversation_id: Option<ConversationId>,
    ) -> Result<OwnerStatus, ResponseCode> {
        self.0.get_owner_status(conversation_id)
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn spawned_client_and_server() {
    for srm in [false, true] {
        let (client_socket, server_socket) = duplex(64 * 1024);

        let server = tokio::spawn(async move {
            let mut server = MasServer::new(server_socket, AsyncPhone(Phone::new()))
                .with_single_response_mode(srm)
                .with_max_packet_length(1024);
            server.run().await.unwrap();
            server.into_parts().1 .0
        });

        let mut client = MasClient::new(client_socket)
            .with_single_response_mode(srm)
            .with_max_packet_length(1024);
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

        let listing = client
            .get_messages_listing("inbox", &MessagesListingParams::default())
            .await
            .unwrap()
            .listing
            .unwrap();
        assert_eq!(4, listing.messages.len());

        let email = listing
            .messages
            .iter()
            .find(|entry| entry.message_type == Some(MessageType::Email))
            .unwrap();
        let message = client
            .get_message(email.handle, &GetMessageParams::default())
            .await
            .unwrap();
        assert_eq!(long_text(), message.text());
        client
            .set_message_status(email.handle, MessageStatus::Read(true))
            .await
            .unwrap();

        let reply = BMessage::new(MessageType::SmsGsm, BMessageBody::from_text("Sure!"))
            .with_recipient(VCard::telephone("+15550001"));
        let handle = client
            .push_message("outbox", &reply, &PushMessageParams::default())
            .await
            .unwrap();

        client.set_notification_registration(true).await.unwrap();
        client
            .set_notification_filter(NotificationFilterMask::all())
            .await
            .unwrap();

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
        drop(client);

        let mut phone = server.await.unwrap();
        assert_eq!(Some(true), phone.find(email.handle).unwrap().entry.read);
        assert_eq!("telecom/msg/outbox", phone.find(handle).unwrap().folder);
        assert_eq!(vec![true], phone.registrations);
        assert!(phone.disconnected);
    }
}

#[tokio::test]
async fn async_client_with_blocking_server() {
    let (client_socket, server_socket) = std::os::unix::net::UnixStream::pair().unwrap();
    client_socket.set_nonblocking(true).unwrap();

    let server = std::thread::spawn(move || {
        let mut server = map::server::MasServer::new(server_socket, Phone::new());
        server.run().unwrap();
        server.into_parts().1
    });

    let socket = tokio::net::UnixStream::from_std(client_socket).unwrap();
    let mut client = MasClient::new(socket);
    client.connect().await.unwrap();

    let status = OwnerStatus {
        presence_availability: Some(3),
        presence_text: Some("Away".to_string()),
        ..OwnerStatus::default()
    };
    client.set_owner_status(&status).await.unwrap();
    assert_eq!(status, client.get_owner_status(None).await.unwrap());

    let information = client.get_mas_instance_information(0).await.unwrap();
    assert_eq!("SMS/MMS on SIM 1", information.information);

    client.disconnect().await.unwrap();
    drop(client);
    assert_eq!(status, server.join().unwrap().owner);
}

#[tokio::test(flavor = "multi_thread")]
async fn blocking_client_with_async_server() {
    let (client_socket, server_socket) = std::os::unix::net::UnixStream::pair().unwrap();
    server_socket.set_nonblocking(true).unwrap();
    let socket = tokio::net::UnixStream::from_std(server_socket).unwrap();

    let server = tokio::spawn(async move {
        let mut server = MasServer::new(socket, AsyncPhone(Phone::new()));
        server.run().await.unwrap();
        server.into_parts().1 .0
    });

    let phone = tokio::task::spawn_blocking(move || {
        let mut client = map::client::MasClient::new(client_socket);
        client.connect().unwrap();
        client.set_folder_path("telecom/msg/inbox").unwrap();
        let size = client
            .get_messages_listing(
                "",
                &MessagesListingParams {
                    max_list_count: Some(0),
                    ..MessagesListingParams::default()
                },
            )
            .unwrap();
        assert_eq!(Some(4), size.listing_size);
        client.disconnect().unwrap();
    });

    phone.await.unwrap();
    assert!(server.await.unwrap().disconnected);
}

#[tokio::test(flavor = "multi_thread")]
async fn notifications_with_closure_handlers() {
    let (client_socket, server_socket) = duplex(4096);
    let (events, mut received) = mpsc::unbounded_channel();

    let server = tokio::spawn(async move {
        let handler = move |instance: u8, report: EventReport| {
            let events = events.clone();
            async move {
                events.send((instance, report)).unwrap();
            }
        };
        MnsServer::new(server_socket, handler).run().await
    });

    let mut client = MnsClient::new(client_socket);
    client.connect().await.unwrap();

    let report = EventReport::message(
        EventType::MessageShift,
        MessageHandle(9),
        "TELECOM/MSG/SENT",
        MessageType::SmsGsm,
    );
    let report = EventReport {
        old_folder: Some("TELECOM/MSG/OUTBOX".to_string()),
        ..report
    };
    client.send_event(1, &report).await.unwrap();
    client.disconnect().await.unwrap();
    drop(client);

    server.await.unwrap().unwrap();
    assert_eq!(Some((1, report)), received.recv().await);
}
