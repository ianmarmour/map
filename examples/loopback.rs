//! Runs a pretend phone (MSE) and a MAP client (MCE) over TCP on localhost.
//!
//! The client browses the phone's inbox, reads a message, registers for
//! notifications and sends an SMS; the phone reports the sent message to the
//! client's Message Notification Server. On a real device the sockets would be
//! Bluetooth RFCOMM or L2CAP connections instead.
//!
//! ```text
//! cargo run --example loopback
//! ```

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread;

use map::bmessage::{BMessage, BMessageBody, VCard};
use map::client::{MasClient, MnsClient};
use map::event_report::{EventReport, EventType};
use map::folder_listing::{Folder, FolderListing};
use map::mas::{
    GetMessageParams, MessagesListingParams, MessagesListingResponse, PushMessageParams, SetFolder,
};
use map::messages_listing::MessageEntry;
use map::server::{MasServer, MessageStore, MnsServer};
use map::types::{MessageHandle, MessageType};
use obex::response::ResponseCode;

/// A phone with a few messages, connecting back to the client for notifications.
struct Phone {
    path: Vec<String>,
    messages: Vec<(String, MessageEntry, BMessage)>,
    next_handle: u64,
    client_mns: SocketAddr,
    notifications: Option<MnsClient<TcpStream>>,
}

impl Phone {
    fn new(client_mns: SocketAddr) -> Phone {
        let mut phone = Phone {
            path: Vec::new(),
            messages: Vec::new(),
            next_handle: 1,
            client_mns,
            notifications: None,
        };
        phone.store("inbox", "Alice", "+15550001", "Are we still on for lunch?");
        phone.store("inbox", "Bob", "+15550002", "Running 10 minutes late");
        phone
    }

    fn store(&mut self, folder: &str, name: &str, number: &str, text: &str) -> MessageHandle {
        let handle = MessageHandle(self.next_handle);
        self.next_handle += 1;

        let entry = MessageEntry {
            subject: Some(text.to_string()),
            datetime: Some(format!("20240310T12{:02}00", handle.0)),
            sender_name: Some(name.to_string()),
            sender_addressing: Some(number.to_string()),
            message_type: Some(MessageType::SmsGsm),
            read: Some(false),
            ..MessageEntry::new(handle)
        };
        let message = BMessage::new(MessageType::SmsGsm, BMessageBody::from_text(text))
            .with_originator(VCard::telephone(number).with_name(name));

        self.messages.push((folder.to_string(), entry, message));
        handle
    }

    fn notify(&mut self, report: EventReport) {
        if let Some(notifications) = self.notifications.as_mut() {
            notifications
                .send_event(0, &report)
                .expect("event delivered");
        }
    }
}

impl MessageStore for Phone {
    fn set_folder(&mut self, step: SetFolder) -> Result<(), ResponseCode> {
        match step {
            SetFolder::Root => self.path.clear(),
            SetFolder::Parent => drop(self.path.pop()),
            SetFolder::Child(name) => self.path.push(name),
            SetFolder::Sibling(name) => {
                self.path.pop();
                self.path.push(name);
            }
        }
        Ok(())
    }

    fn get_folder_listing(&mut self) -> Result<FolderListing, ResponseCode> {
        let names: &[&str] = match self.path.join("/").as_str() {
            "" => &["telecom"],
            "telecom" => &["msg"],
            "telecom/msg" => &["inbox", "outbox", "sent", "deleted"],
            _ => &[],
        };
        Ok(FolderListing::new(
            names.iter().map(|name| Folder::new(*name)),
        ))
    }

    fn get_messages_listing(
        &mut self,
        folder: &str,
        params: &MessagesListingParams,
    ) -> Result<MessagesListingResponse, ResponseCode> {
        let entries = self
            .messages
            .iter()
            .filter(|(stored, _, _)| stored == folder)
            .map(|(_, entry, _)| entry.clone());
        Ok(params.apply(entries))
    }

    fn get_message(
        &mut self,
        handle: MessageHandle,
        _params: &GetMessageParams,
    ) -> Result<BMessage, ResponseCode> {
        self.messages
            .iter()
            .find(|(_, entry, _)| entry.handle == handle)
            .map(|(_, _, message)| message.clone())
            .ok_or(ResponseCode::NotFound)
    }

    fn push_message(
        &mut self,
        folder: &str,
        message: BMessage,
        _params: &PushMessageParams,
    ) -> Result<MessageHandle, ResponseCode> {
        let number = message
            .recipients()
            .first()
            .and_then(|card| card.telephones.first())
            .cloned()
            .unwrap_or_default();
        println!("phone: sending {:?} to {}", message.text(), number);

        let handle = self.store(folder, "Me", &number, &message.text());
        self.notify(EventReport::message(
            EventType::SendingSuccess,
            handle,
            "TELECOM/MSG/SENT",
            MessageType::SmsGsm,
        ));
        Ok(handle)
    }

    fn set_notification_registration(&mut self, enabled: bool) -> Result<(), ResponseCode> {
        if enabled {
            let socket = TcpStream::connect(self.client_mns)
                .map_err(|_| ResponseCode::ServiceUnavailable)?;
            let mut notifications = MnsClient::new(socket);
            notifications
                .connect()
                .map_err(|_| ResponseCode::ServiceUnavailable)?;
            self.notifications = Some(notifications);
        } else if let Some(mut notifications) = self.notifications.take() {
            let _ = notifications.disconnect();
        }
        Ok(())
    }
}

fn main() -> map::Result<()> {
    let mas_listener = TcpListener::bind("127.0.0.1:0")?;
    let mns_listener = TcpListener::bind("127.0.0.1:0")?;
    let mas_address = mas_listener.local_addr()?;
    let mns_address = mns_listener.local_addr()?;

    // The phone serves the Message Access Service.
    let phone = thread::spawn(move || -> map::Result<()> {
        let (socket, _) = mas_listener.accept()?;
        MasServer::new(socket, Phone::new(mns_address)).run()
    });

    // The client serves the Message Notification Service.
    let notifications = thread::spawn(move || -> map::Result<()> {
        let (socket, _) = mns_listener.accept()?;
        MnsServer::new(socket, |instance: u8, report: EventReport| {
            println!(
                "client: event from MAS {}: {} {:?} in {:?}",
                instance, report.event_type, report.handle, report.folder
            );
        })
        .run()
    });

    let mut client = MasClient::new(TcpStream::connect(mas_address)?);
    client.connect()?;

    let folders = client.get_folder_listing(&Default::default())?;
    println!("client: root folders {:?}", folders.names());

    client.set_folder_path("telecom/msg")?;
    println!(
        "client: message folders {:?}",
        client.get_folder_listing(&Default::default())?.names()
    );

    let inbox = client.get_messages_listing("inbox", &MessagesListingParams::default())?;
    println!(
        "client: {} messages in the inbox",
        inbox.listing_size.unwrap_or(0)
    );

    for entry in inbox.listing.unwrap_or_default().messages {
        let message = client.get_message(entry.handle, &GetMessageParams::default())?;
        println!(
            "client: {} from {}: {:?}",
            entry.handle,
            entry.sender_name.unwrap_or_default(),
            message.text()
        );
    }

    client.set_notification_registration(true)?;

    let reply = BMessage::new(
        MessageType::SmsGsm,
        BMessageBody::from_text("Yes, see you at noon!"),
    )
    .with_recipient(VCard::telephone("+15550001"));
    let handle = client.push_message("outbox", &reply, &PushMessageParams::default())?;
    println!("client: pushed message {}", handle);

    client.set_notification_registration(false)?;
    client.disconnect()?;
    drop(client);

    phone.join().expect("phone thread")?;
    notifications.join().expect("notification thread")?;
    Ok(())
}
