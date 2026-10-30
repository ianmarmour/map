# obex-protocol-map
[![MIT licensed](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)
[![CI](https://github.com/ianmarmour/obex-protocol-map/workflows/CI/badge.svg)](https://github.com/ianmarmour/obex-protocol-map/actions/workflows/ci.yml)

An open source implementation of the Bluetooth Message Access Profile (MAP 1.4.2) in Rust, built on [`obex-protocol`](https://github.com/ianmarmour/obex-protocol).

## What is MAP

The Message Access Profile lets a device such as a car kit, a smart watch or a PC (the *MCE*, Message Client Equipment) browse, read, send and delete the messages stored on a phone (the *MSE*, Message Server Equipment): SMS, MMS, e-mail and instant messages. The phone can also notify the MCE when messages arrive, are sent or change.

MAP runs on OBEX and defines two services:

* the **Message Access Service** (MAS): the MCE is the OBEX client and the phone the server. It carries the MAP functions, such as GetMessagesListing, GetMessage and PushMessage.
* the **Message Notification Service** (MNS): the phone is the OBEX client and the MCE the server. It carries event reports, such as `NewMessage`.

## What is This Library

A building block for applications on either side of MAP. It covers both roles, blocking and async, on desktops, servers and embedded devices without `std`:

| Role | Message Access Service | Message Notification Service |
|------|------------------------|------------------------------|
| MCE (car kit, PC, watch) | `client::MasClient` | `server::MnsServer` + `server::NotificationHandler` |
| MSE (phone) | `server::MasServer` + `server::MessageStore` | `client::MnsClient` |

With the `tokio` feature, `obex_protocol_map::tokio` has async versions of all four with the same methods, and with the `embedded-io-async` feature `obex_protocol_map::embedded` has them for embedded devices.

```text
+----------------------------------------------------------------+
|  Your application                                              |
+----------------------------------------------------------------+
|  client / server                    (blocking)                  |  roles
|  tokio                              (async, `tokio` feature)    |
|  embedded                           (async, no_std)             |
|  mas                                functions and parameters    |
|  bmessage, *_listing, event_report  MAP objects (no I/O)        |  codec
|  types, features, sdp                                          |  (no_std)
+----------------------------------------------------------------+
|  obex-protocol (OBEX 1.5: packets, SRM, client and server)     |
+----------------------------------------------------------------+
|  RFCOMM (GOEP 1.1)  or  L2CAP ERTM (GOEP 2.0)                  |  any byte stream
+----------------------------------------------------------------+
```

* **Objects**: bMessage (1.0 and 1.1, with `LENGTH` computation, `END:MSG` escaping and quoted-printable vCards), Folder-Listing, Messages-Listing (1.0 and 1.1), Conversation-Listing and MAP-Event-Report (1.0, 1.1 and 1.2). Parsing is lenient where devices are known to deviate, such as bare `&` in XML attributes, LF line endings or wrong bMessage lengths.
* **Functions**: every MAP function, with typed parameters and results, encoded to and from OBEX headers and Application Parameters.
* **Clients and servers**: thin layers over the `obex-protocol` client and server (or their tokio and embedded versions) that take care of directed connections, MAS and MNS targets, `MapSupportedFeatures`, filler bytes and response codes.

## How Do I Use This Library

The crate is published as `obex-protocol-map`, part of the `obex-protocol` family, and imported as `obex_protocol_map`. It re-exports `obex-protocol` as `obex_protocol_map::obex_protocol`, so no separate OBEX dependency is needed:

```toml
[dependencies]
obex-protocol-map = "0.2"
# or, for the async API
obex-protocol-map = { version = "0.2", features = ["tokio"] }
```

Open a socket to the phone with your Bluetooth stack (BlueZ, `bluer`, or the platform APIs), using the RFCOMM channel or L2CAP PSM from its Message Access Server SDP record (`obex_protocol_map::sdp` has the UUIDs and attribute ids), and hand it to a client.

### Reading messages (MCE)

```rust
use obex_protocol_map::client::MasClient;
use obex_protocol_map::mas::{GetMessageParams, MessagesListingParams, MessageStatus};
use obex_protocol_map::types::ReadStatusFilter;

let mut client = MasClient::new(socket);
client.connect()?;
client.set_folder_path("telecom/msg")?;

let unread = client.get_messages_listing(
    "inbox",
    &MessagesListingParams {
        filter_read_status: Some(ReadStatusFilter::Unread),
        ..MessagesListingParams::default()
    },
)?;

for entry in unread.listing.unwrap_or_default().messages {
    let message = client.get_message(entry.handle, &GetMessageParams::default())?;
    println!("{:?}: {}", entry.sender_addressing, message.text());
    client.set_message_status(entry.handle, MessageStatus::Read(true))?;
}
```

### Sending a message (MCE)

```rust
use obex_protocol_map::bmessage::{BMessage, BMessageBody, VCard};
use obex_protocol_map::mas::PushMessageParams;
use obex_protocol_map::types::MessageType;

let sms = BMessage::new(MessageType::SmsGsm, BMessageBody::from_text("On my way!"))
    .with_recipient(VCard::telephone("+15551234567"));

client.set_folder_path("telecom/msg")?;
let handle = client.push_message("outbox", &sms, &PushMessageParams::default())?;
```

### Receiving notifications (MCE)

Register an MNS SDP record, accept the phone's connection and run a `MnsServer`. Any closure taking the MAS instance id and the report is a handler:

```rust
use obex_protocol_map::event_report::EventReport;
use obex_protocol_map::server::MnsServer;

std::thread::spawn(move || {
    let (socket, _) = mns_listener.accept()?;
    MnsServer::new(socket, |instance: u8, report: EventReport| {
        println!("{} {:?} in {:?}", report.event_type, report.handle, report.folder);
    })
    .run()
});

client.set_notification_registration(true)?;
```

### Serving messages (MSE)

Implement `MessageStore` and run a `MasServer`. Only folder navigation, the two listings and GetMessage are required; other functions answer `Not Implemented` until you implement them. `MessagesListingParams::apply` filters, sorts and pages a folder's messages for you.

```rust
use obex_protocol_map::bmessage::BMessage;
use obex_protocol_map::folder_listing::FolderListing;
use obex_protocol_map::mas::{GetMessageParams, MessagesListingParams, MessagesListingResponse, SetFolder};
use obex_protocol_map::server::{MasServer, MessageStore};
use obex_protocol_map::types::MessageHandle;
use obex_protocol_map::obex_protocol::response::ResponseCode;

struct Phone { /* ... */ }

impl MessageStore for Phone {
    fn set_folder(&mut self, step: SetFolder) -> Result<(), ResponseCode> { /* ... */ }
    fn get_folder_listing(&mut self) -> Result<FolderListing, ResponseCode> { /* ... */ }
    fn get_messages_listing(&mut self, folder: &str, params: &MessagesListingParams)
        -> Result<MessagesListingResponse, ResponseCode> {
        Ok(params.apply(self.messages_in(folder)))
    }
    fn get_message(&mut self, handle: MessageHandle, params: &GetMessageParams)
        -> Result<BMessage, ResponseCode> { /* ... */ }
}

MasServer::new(socket, Phone::new()).run()?;
```

### Async (tokio)

The async API mirrors the blocking one. Clients and servers work with any `AsyncRead + AsyncWrite` transport, such as `bluer`'s RFCOMM and L2CAP streams, and `MessageStore` and `NotificationHandler` are implemented with `async fn`:

```rust
use obex_protocol_map::tokio::{MasClient, MnsServer};

let mut client = MasClient::new(l2cap_stream)
    .with_max_packet_length(l2cap_mtu)
    .with_single_response_mode(true);
client.connect().await?;

tokio::spawn(async move {
    MnsServer::new(mns_stream, |_instance: u8, report: EventReport| async move {
        println!("{}", report.event_type);
    })
    .run()
    .await
});
```

### Embedded and `no_std`

The default `std` feature provides the blocking roles. Turn it off and the crate is `no_std`, needing only `alloc`. The objects, functions and values are always available, and the `embedded-io-async` feature adds async clients and servers for any transport implementing the [`embedded-io-async`](https://crates.io/crates/embedded-io-async) traits, such as the Bluetooth channels of Embassy based firmware:

```toml
[dependencies]
obex-protocol-map = { version = "0.2", default-features = false, features = ["embedded-io-async"] }
```

`obex_protocol_map::embedded` mirrors the tokio API, except that `MessageStore` and `NotificationHandler` futures need not be `Send`, so they can own peripherals and run on single threaded executors. A watch reading a phone's inbox:

```rust
use obex_protocol_map::embedded::{MasClient, MnsServer};
use obex_protocol_map::mas::MessagesListingParams;

let mut client = MasClient::new(mas_channel).with_max_packet_length(1024);
client.connect().await?;
client.set_folder_path("telecom/msg").await?;
let inbox = client.get_messages_listing("inbox", &MessagesListingParams::default()).await?;

client.set_notification_registration(true).await?;
MnsServer::new(mns_channel, |_instance: u8, report: EventReport| async move {
    // report.event_type, report.handle, ...
})
.run()
.await?;
```

On small devices, lower `with_max_packet_length` (it bounds the OBEX buffers) and, for an MSE, `with_max_object_size`. Without `std` there is no clock, so a `MessageStore` should set `MessagesListingResponse::mse_time` itself; with `std` the server fills in the current time.

### Example

`examples/loopback.rs` runs a pretend phone and a client over TCP on localhost, covering browsing, reading, sending and notifications:

```bash
cargo run --example loopback
```

## Bluetooth Notes

* **RFCOMM (GOEP 1.1)** is a stream socket. Leave Single Response Mode off.
* **L2CAP (GOEP 2.0)**: when the SDP record has a `GoepL2CapPsm`, connect over L2CAP in Enhanced Retransmission Mode, set `with_max_packet_length` to at most the channel MTU and enable `with_single_response_mode(true)`. SRM is then negotiated for every GET and PUT, as MAP requires.
* If the phone's `MapSupportedFeatures` has bit 19 set, send your own features in CONNECT with `MasClient::with_supported_features`.
* Pairing, SDP registration and queries, and opening sockets belong to the Bluetooth stack, not this crate. MAP requires an encrypted, bonded link; OBEX authentication is not used.

## Specification Coverage

| MAP 1.4.2 section | Status |
|-------------------|--------|
| 3.1.1 Handles, 3.1.10 to 3.1.16 timestamps and identifiers | Implemented |
| 3.1.3 bMessage, versions 1.0 and 1.1 | Implemented |
| 3.1.5 Folder-Listing | Implemented |
| 3.1.6 Messages-Listing, versions 1.0 and 1.1 | Implemented |
| 3.1.7 MAP-Event-Report, versions 1.0, 1.1 and 1.2 | Implemented |
| 3.1.9 Conversation-Listing | Implemented |
| 4 Features, `MapSupportedFeatures` | Implemented |
| 5.1 to 5.14 every MAP function | Implemented for clients and servers |
| 6.3.1 Application Parameters | Every tag |
| 6.3.3 Error codes | Exposed as OBEX response codes |
| 6.4 Session initialization and termination | Implemented |
| 6.4.6 Authentication, 6.5 Reliable sessions | Not used by MAP |
| 7.1 SDP records | UUIDs and attribute ids; registration is left to the Bluetooth stack |

The specification is in [`docs/MAP.pdf`](docs/MAP.pdf).

## Minimum Supported Rust Version

Rust 1.85, checked in CI with and without `std`.
