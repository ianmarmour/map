//! ## Service Discovery
//!
//! Values for the SDP records MAP defines (MAP section 7.1): the MSE publishes one
//! Message Access Server record per MAS instance, the MCE one Message Notification
//! Server record. Registering records and running SDP queries belong to the
//! Bluetooth stack; these constants are what to register and look for.
//!
//! An MCE typically searches for [`MESSAGE_ACCESS_SERVER`] records, reads the RFCOMM
//! channel or [`ATTRIBUTE_GOEP_L2CAP_PSM`] from each, and picks a MAS instance by
//! [`ATTRIBUTE_MAS_INSTANCE_ID`], [`ATTRIBUTE_SUPPORTED_MESSAGE_TYPES`] and the
//! service name. If the record has a GOEP L2CAP PSM, connect over L2CAP and enable
//! Single Response Mode (GOEP 2.0); otherwise use RFCOMM (GOEP 1.1).
//!

/// 16-bit UUID of the Message Access Server service class.
pub const MESSAGE_ACCESS_SERVER_UUID16: u16 = 0x1132;

/// 16-bit UUID of the Message Notification Server service class.
pub const MESSAGE_NOTIFICATION_SERVER_UUID16: u16 = 0x1133;

/// 16-bit UUID of the Message Access Profile, used in the profile descriptor list.
pub const MESSAGE_ACCESS_PROFILE_UUID16: u16 = 0x1134;

/// The Message Access Server service class as a 128-bit UUID string.
pub const MESSAGE_ACCESS_SERVER: &str = "00001132-0000-1000-8000-00805f9b34fb";

/// The Message Notification Server service class as a 128-bit UUID string.
pub const MESSAGE_NOTIFICATION_SERVER: &str = "00001133-0000-1000-8000-00805f9b34fb";

/// The Message Access Profile as a 128-bit UUID string.
pub const MESSAGE_ACCESS_PROFILE: &str = "00001134-0000-1000-8000-00805f9b34fb";

/// The profile version this crate implements, MAP 1.4, for the profile descriptor list.
pub const PROFILE_VERSION: u16 = 0x0104;

/// Attribute id of `GoepL2CapPsm` (uint16), present when the service is reachable
/// over L2CAP (GOEP 2.0).
pub const ATTRIBUTE_GOEP_L2CAP_PSM: u16 = 0x0200;

/// Attribute id of `MASInstanceID` (uint8) in Message Access Server records.
pub const ATTRIBUTE_MAS_INSTANCE_ID: u16 = 0x0315;

/// Attribute id of `SupportedMessageTypes` (uint8, see
/// [`SupportedMessageTypes`](crate::features::SupportedMessageTypes)) in Message
/// Access Server records.
pub const ATTRIBUTE_SUPPORTED_MESSAGE_TYPES: u16 = 0x0316;

/// Attribute id of `MapSupportedFeatures` (uint32, see
/// [`MapSupportedFeatures`](crate::features::MapSupportedFeatures)).
pub const ATTRIBUTE_MAP_SUPPORTED_FEATURES: u16 = 0x0317;

/// Default service name of a Message Access Server record.
pub const DEFAULT_MAS_SERVICE_NAME: &str = "MAP MAS-name";

/// Default service name of a Message Notification Server record.
pub const DEFAULT_MNS_SERVICE_NAME: &str = "MAP MNS-name";
