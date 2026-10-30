//! End to end tests running the blocking clients against the blocking servers over
//! socket pairs.

mod common;

use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};

use bluetooth_map::bmessage::{BMessage, BMessageBody, VCard};
use bluetooth_map::client::{MasClient, MnsClient};
use bluetooth_map::event_report::{EventReport, EventType};
use bluetooth_map::features::{
    MapSupportedFeatures, MessageTypeFilter, NotificationFilterMask, ParameterMask,
};
use bluetooth_map::folder_listing::FolderListing;
use bluetooth_map::mas::{
    ConversationListingParams, FolderListingParams, Forward, FractionContent, GetMessageParams,
    MessageStatus, MessagesListingParams, OwnerStatus, PushMessageParams, SetFolder,
};
use bluetooth_map::server::{MasServer, MnsServer};
use bluetooth_map::types::{
    Charset, ConversationId, DatabaseIdentifier, FractionDeliver, FractionRequest, MessageHandle,
    MessageType, ModifyText, ReadStatusFilter, VersionCounter,
};
use common::{long_text, Phone, CONVERSATION};
use obex::response::ResponseCode;

/// A socket counting the packets written to it.
struct Counting {
    socket: UnixStream,
    writes: Arc<AtomicUsize>,
}

impl Read for Counting {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.socket.read(buf)
    }
}

impl Write for Counting {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.socket.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.socket.flush()
    }
}

/// Serves `phone` on a thread and returns a connected client. Joining the thread
/// after the client is dropped returns the phone for inspection.
fn start(
    phone: Phone,
    srm: bool,
    max_packet_length: u16,
) -> (MasClient<Counting>, JoinHandle<Phone>) {
    let (client_socket, server_socket) = UnixStream::pair().unwrap();

    let server = thread::spawn(move || {
        let mut server = MasServer::new(server_socket, phone)
            .with_single_response_mode(srm)
            .with_max_packet_length(max_packet_length);
        server.run().unwrap();
        server.into_parts().1
    });

    let socket = Counting {
        socket: client_socket,
        writes: Arc::new(AtomicUsize::new(0)),
    };
    let mut client = MasClient::new(socket)
        .with_single_response_mode(srm)
        .with_max_packet_length(max_packet_length)
        .with_supported_features(
            MapSupportedFeatures::DEFAULT | MapSupportedFeatures::MESSAGES_LISTING_FORMAT_1_1,
        );
    client.connect().unwrap();

    (client, server)
}

fn finish(mut client: MasClient<Counting>, server: JoinHandle<Phone>) -> Phone {
    client.disconnect().unwrap();
    drop(client);
    server.join().unwrap()
}

#[test]
fn browses_folders() {
    let (mut client, server) = start(Phone::new(), false, u16::MAX);

    let root = client
        .get_folder_listing(&FolderListingParams::default())
        .unwrap();
    assert_eq!(vec!["telecom"], root.names());

    client.set_folder_path("telecom/msg").unwrap();
    assert_eq!(5, client.get_folder_listing_size().unwrap());

    let page = client
        .get_folder_listing(&FolderListingParams {
            max_list_count: Some(2),
            list_start_offset: Some(1),
        })
        .unwrap();
    assert_eq!(vec!["outbox", "sent"], page.names());

    client
        .set_folder(SetFolder::Child("inbox".to_string()))
        .unwrap();
    client
        .set_folder(SetFolder::Sibling("sent".to_string()))
        .unwrap();
    assert_eq!(
        FolderListing::default(),
        client
            .get_folder_listing(&FolderListingParams::default())
            .unwrap()
    );

    let missing = client.set_folder(SetFolder::Child("nowhere".to_string()));
    assert_eq!(
        Some(ResponseCode::NotFound),
        missing.unwrap_err().response_code()
    );

    client.set_folder(SetFolder::Parent).unwrap();
    client.set_folder(SetFolder::Root).unwrap();
    assert_eq!(
        vec!["telecom"],
        client
            .get_folder_listing(&FolderListingParams::default())
            .unwrap()
            .names()
    );

    let phone = finish(client, server);
    assert!(phone.disconnected);
    assert!(phone
        .remote_features
        .unwrap()
        .contains(MapSupportedFeatures::MESSAGES_LISTING_FORMAT_1_1));
}

#[test]
fn lists_and_filters_messages() {
    let (mut client, server) = start(Phone::new(), false, u16::MAX);
    client.set_folder_path("telecom/msg").unwrap();

    let all = client
        .get_messages_listing("inbox", &MessagesListingParams::default())
        .unwrap();
    let listing = all.listing.unwrap();
    assert_eq!(4, listing.messages.len());
    assert_eq!(
        Some("20240302T080000"),
        listing.messages[0].datetime.as_deref()
    );
    assert_eq!(Some(4), all.listing_size);
    assert_eq!(Some(true), all.new_message);
    assert_eq!(Some("20240310T120000+0100"), all.mse_time.as_deref());
    assert_eq!(Some(DatabaseIdentifier(1)), all.database_identifier);
    assert_eq!(Some(VersionCounter(7)), all.folder_version_counter);

    let unread_sms = client
        .get_messages_listing(
            "inbox",
            &MessagesListingParams {
                filter_read_status: Some(ReadStatusFilter::Unread),
                filter_message_type: Some(MessageTypeFilter::only(&[MessageType::SmsGsm])),
                parameter_mask: Some(ParameterMask::SENDER_NAME | ParameterMask::READ),
                ..MessagesListingParams::default()
            },
        )
        .unwrap()
        .listing
        .unwrap();
    assert_eq!(1, unread_sms.messages.len());
    assert_eq!(Some("Alice"), unread_sms.messages[0].sender_name.as_deref());
    assert_eq!(None, unread_sms.messages[0].subject);

    let size_only = client
        .get_messages_listing(
            "inbox",
            &MessagesListingParams {
                max_list_count: Some(0),
                filter_originator: Some("alice".to_string()),
                ..MessagesListingParams::default()
            },
        )
        .unwrap();
    assert_eq!(None, size_only.listing);
    assert_eq!(Some(2), size_only.listing_size);

    let conversation = client
        .get_messages_listing(
            "",
            &MessagesListingParams {
                conversation_id: Some(CONVERSATION),
                ..MessagesListingParams::default()
            },
        )
        .unwrap();
    assert_eq!(Some(2), conversation.listing_size);

    let missing = client.get_messages_listing("nowhere", &MessagesListingParams::default());
    assert_eq!(
        Some(ResponseCode::NotFound),
        missing.unwrap_err().response_code()
    );

    finish(client, server);
}

#[test]
fn downloads_and_updates_messages() {
    let (mut client, server) = start(Phone::new(), false, u16::MAX);
    client.set_folder_path("telecom/msg/inbox").unwrap();

    let listing = client
        .get_messages_listing("", &MessagesListingParams::default())
        .unwrap()
        .listing
        .unwrap();
    let alice = listing
        .messages
        .iter()
        .find(|entry| entry.read == Some(false) && entry.sender_name.as_deref() == Some("Alice"))
        .unwrap();

    let message = client
        .get_message(alice.handle, &GetMessageParams::default())
        .unwrap();
    assert_eq!("See you at 10?", message.text());
    assert_eq!(
        vec!["+15550001"],
        message.originator.as_ref().unwrap().telephones
    );
    assert_eq!("TELECOM/MSG/INBOX", message.folder);

    client
        .set_message_status(alice.handle, MessageStatus::Read(true))
        .unwrap();
    client
        .set_message_status(
            alice.handle,
            MessageStatus::ExtendedData("0:54;".to_string()),
        )
        .unwrap();
    client
        .set_message_status(alice.handle, MessageStatus::Deleted(true))
        .unwrap();

    let unknown = client.get_message(MessageHandle(1), &GetMessageParams::default());
    assert_eq!(
        Some(ResponseCode::NotFound),
        unknown.unwrap_err().response_code()
    );

    let first = client
        .get_message_fraction(
            alice.handle,
            &GetMessageParams::default(),
            FractionRequest::First,
        )
        .unwrap();
    assert!(matches!(first.content, FractionContent::Message(_)));
    assert_eq!(Some(FractionDeliver::More), first.deliver);

    let next = client
        .get_message_fraction(
            alice.handle,
            &GetMessageParams::default(),
            FractionRequest::Next,
        )
        .unwrap();
    let FractionContent::Body(body) = next.content else {
        panic!("expected a lone body");
    };
    assert_eq!("second fraction", body.text());
    assert_eq!(Some(1), body.part_id);
    assert_eq!(Some(FractionDeliver::Last), next.deliver);

    let mut phone = finish(client, server);
    let stored = phone.find(alice.handle).unwrap();
    assert_eq!(Some(true), stored.entry.read);
    assert_eq!("telecom/msg/deleted", stored.folder);
    assert_eq!(
        vec![(alice.handle, "0:54;".to_string())],
        phone.extended_data
    );
}

#[test]
fn pushes_messages_to_the_outbox() {
    let (mut client, server) = start(Phone::new(), false, u16::MAX);
    client.set_folder_path("telecom/msg").unwrap();

    let message = BMessage::new(
        MessageType::SmsGsm,
        BMessageBody::from_text("On my way\r\nEND:MSG"),
    )
    .with_recipient(VCard::telephone("+15550002"));
    let handle = client
        .push_message(
            "outbox",
            &message,
            &PushMessageParams {
                retry: Some(false),
                ..PushMessageParams::default()
            },
        )
        .unwrap();

    let forward = PushMessageParams {
        charset: Charset::Utf8,
        forward: Some(Forward {
            message_handle: MessageHandle(0x20000100000),
            attachment: true,
            modify_text: ModifyText::Prepend,
        }),
        ..PushMessageParams::default()
    };
    let forwarded = client.push_message("outbox", &message, &forward).unwrap();
    assert_ne!(handle, forwarded);

    let mut phone = finish(client, server);
    let stored = phone.find(handle).unwrap();
    assert_eq!("telecom/msg/outbox", stored.folder);
    assert_eq!("On my way\r\nEND:MSG", stored.message.text());
    assert_eq!(vec!["+15550002"], stored.message.recipients()[0].telephones);
    assert_eq!(Some(false), phone.pushed[0].retry);
    assert_eq!(forward, phone.pushed[1]);
}

#[test]
fn notification_registration_and_other_functions() {
    let (mut client, server) = start(Phone::new(), false, u16::MAX);

    client.set_notification_registration(true).unwrap();
    client
        .set_notification_filter(
            NotificationFilterMask::NEW_MESSAGE | NotificationFilterMask::SENDING_SUCCESS,
        )
        .unwrap();
    client.set_notification_registration(false).unwrap();

    // The phone keeps the default for UpdateInbox, Not Implemented.
    let update = client.update_inbox();
    assert_eq!(
        Some(ResponseCode::NotImplemented),
        update.unwrap_err().response_code()
    );

    let information = client.get_mas_instance_information(0).unwrap();
    assert_eq!("SMS/MMS on SIM 1", information.information);
    assert_eq!(Some("tel:+15559999"), information.owner_uci.as_deref());
    assert!(client.get_mas_instance_information(3).is_err());

    let conversations = client
        .get_conversation_listing(&ConversationListingParams::default())
        .unwrap();
    let listing = conversations.listing.unwrap();
    assert_eq!(CONVERSATION, listing.conversations[0].id);
    assert_eq!(Some(2), listing.conversations[0].participants[0].chat_state);
    assert_eq!(
        Some(DatabaseIdentifier(1)),
        conversations.database_identifier
    );

    let size = client
        .get_conversation_listing(&ConversationListingParams {
            max_list_count: Some(0),
            ..ConversationListingParams::default()
        })
        .unwrap();
    assert_eq!(
        (None, Some(1), Some(VersionCounter(3))),
        (size.listing, size.listing_size, size.version_counter)
    );

    let status = OwnerStatus {
        presence_availability: Some(2),
        presence_text: Some("Driving".to_string()),
        chat_state: Some(1),
        last_activity: Some("20240310T115900+0100".to_string()),
        conversation_id: Some(CONVERSATION),
    };
    client.set_owner_status(&status).unwrap();
    let read_back = client.get_owner_status(Some(CONVERSATION)).unwrap();
    assert_eq!(
        OwnerStatus {
            conversation_id: None,
            ..status
        },
        read_back
    );

    let unknown = client.get_owner_status(Some(ConversationId(5)));
    assert_eq!(
        Some(ResponseCode::NotFound),
        unknown.unwrap_err().response_code()
    );
    assert!(client.set_owner_status(&OwnerStatus::default()).is_err());

    let phone = finish(client, server);
    assert_eq!(vec![true, false], phone.registrations);
    assert_eq!(1, phone.filters.len());
}

#[test]
fn transfers_large_messages_in_small_packets_with_single_response_mode() {
    for srm in [false, true] {
        let (mut client, server) = start(Phone::new(), srm, 512);
        client.set_folder_path("telecom/msg/inbox").unwrap();

        let listing = client
            .get_messages_listing(
                "",
                &MessagesListingParams {
                    filter_message_type: Some(MessageTypeFilter::only(&[MessageType::Email])),
                    ..MessagesListingParams::default()
                },
            )
            .unwrap()
            .listing
            .unwrap();
        let email = listing.messages[0].handle;

        let writes = Arc::clone(&client.obex().get_ref().writes);
        let before = writes.load(Ordering::SeqCst);
        let message = client
            .get_message(
                email,
                &GetMessageParams {
                    attachment: true,
                    charset: Charset::Utf8,
                },
            )
            .unwrap();
        assert_eq!(long_text(), message.text());

        // Without SRM every response packet is requested; with it, one request suffices.
        let requests = writes.load(Ordering::SeqCst) - before;
        if srm {
            assert_eq!(1, requests);
        } else {
            assert!(requests > 100, "only {} GET requests", requests);
        }

        let upload = BMessage::new(MessageType::Email, BMessageBody::from_text(long_text()))
            .with_recipient(VCard::email("dave@example.com"));
        let handle = client
            .push_message("", &upload, &PushMessageParams::default())
            .unwrap();

        let mut phone = finish(client, server);
        assert_eq!(long_text(), phone.find(handle).unwrap().message.text());
    }
}

#[test]
fn refuses_connections_to_the_wrong_service() {
    let (client_socket, server_socket) = UnixStream::pair().unwrap();
    let server = thread::spawn(move || MasServer::new(server_socket, Phone::new()).run());

    // An MNS client asks for the Message Notification Service, which a MAS lacks.
    let mut client = MnsClient::new(client_socket);
    let refused = client.connect().unwrap_err();
    assert_eq!(
        Some(ResponseCode::ServiceUnavailable),
        refused.response_code()
    );

    drop(client);
    server.join().unwrap().unwrap();
}

#[test]
fn delivers_event_reports_to_the_notification_server() {
    let (client_socket, server_socket) = UnixStream::pair().unwrap();
    let (sender, received) = mpsc::channel();

    let server = thread::spawn(move || {
        MnsServer::new(server_socket, move |instance: u8, report: EventReport| {
            sender.send((instance, report)).unwrap();
        })
        .run()
    });

    let mut client = MnsClient::new(client_socket);
    client.connect().unwrap();

    let new_message = EventReport {
        version: "1.1".to_string(),
        subject: Some("Hello & welcome".to_string()),
        sender_name: Some("Alice".to_string()),
        datetime: Some("20240310T120000".to_string()),
        priority: Some(false),
        ..EventReport::message(
            EventType::NewMessage,
            MessageHandle(0x20000100005),
            "TELECOM/MSG/INBOX",
            MessageType::SmsGsm,
        )
    };
    client.send_event(0, &new_message).unwrap();
    client
        .send_event(2, &EventReport::new(EventType::MemoryFull))
        .unwrap();
    client.disconnect().unwrap();
    drop(client);
    server.join().unwrap().unwrap();

    assert_eq!((0, new_message), received.recv().unwrap());
    assert_eq!(
        (2, EventReport::new(EventType::MemoryFull)),
        received.recv().unwrap()
    );
}

#[test]
fn notification_handlers_can_refuse_events() {
    struct Busy;

    impl bluetooth_map::server::NotificationHandler for Busy {
        fn event(&mut self, _instance: u8, _report: EventReport) -> Result<(), ResponseCode> {
            Err(ResponseCode::ServiceUnavailable)
        }
    }

    let (client_socket, server_socket) = UnixStream::pair().unwrap();
    let server = thread::spawn(move || MnsServer::new(server_socket, Busy).run());

    let mut client = MnsClient::new(client_socket);
    client.connect().unwrap();
    let refused = client
        .send_event(0, &EventReport::new(EventType::MemoryAvailable))
        .unwrap_err();
    assert_eq!(
        Some(ResponseCode::ServiceUnavailable),
        refused.response_code()
    );

    drop(client);
    server.join().unwrap().unwrap();
}
