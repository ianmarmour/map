//! ## Identifiers and Values
//!
//! The small value types MAP objects and functions are made of: message handles
//! (MAP section 3.1.1), 128-bit identifiers written as hexadecimal strings (sections
//! 3.1.14 to 3.1.16), message types (section 2.3) and timestamps (section 3.1.10).
//!
use core::fmt;
use core::str::FromStr;

use crate::error::Error;
use crate::prelude::*;

/// Parses up to `max_digits` hexadecimal digits, tolerating surrounding whitespace
/// and null terminators.
fn parse_hex(text: &str, max_digits: usize, object: &'static str) -> Result<u128, Error> {
    let digits = text.trim_matches(|c: char| c.is_whitespace() || c == '\0');

    if digits.is_empty() || digits.len() > max_digits {
        return Err(Error::malformed(
            object,
            format!("expected 1 to {} hexadecimal digits", max_digits),
        ));
    }

    u128::from_str_radix(digits, 16).map_err(|error| Error::malformed(object, error.to_string()))
}

/// A message handle, the 64-bit identifier the MSE assigns to every message (MAP
/// section 3.1.1).
///
/// Handles are written as hexadecimal digits. MAP requires 16 digits in Name
/// headers, which is how [`Display`](fmt::Display) formats them; parsing accepts
/// handles with or without leading zeros.
///
/// # Examples
/// ```
/// # use obex_protocol_map::types::MessageHandle;
/// let handle: MessageHandle = "20000100001".parse().unwrap();
///
/// assert_eq!(MessageHandle(0x20000100001), handle);
/// assert_eq!("0000020000100001", handle.to_string());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct MessageHandle(pub u64);

impl fmt::Display for MessageHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016X}", self.0)
    }
}

impl FromStr for MessageHandle {
    type Err = Error;

    fn from_str(text: &str) -> Result<MessageHandle, Error> {
        parse_hex(text, 16, "message handle").map(|value| MessageHandle(value as u64))
    }
}

impl From<u64> for MessageHandle {
    fn from(value: u64) -> MessageHandle {
        MessageHandle(value)
    }
}

macro_rules! hex_identifier {
    ($(#[$meta:meta])* $name:ident, $object:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct $name(pub u128);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{:032X}", self.0)
            }
        }

        impl FromStr for $name {
            type Err = Error;

            fn from_str(text: &str) -> Result<$name, Error> {
                parse_hex(text, 32, $object).map($name)
            }
        }

        impl From<u128> for $name {
            fn from(value: u128) -> $name {
                $name(value)
            }
        }
    };
}

hex_identifier!(
    /// Identifies a conversation within a MAS instance (MAP section 3.1.16), written
    /// as a 32 digit hexadecimal string.
    ///
    /// # Examples
    /// ```
    /// # use obex_protocol_map::types::ConversationId;
    /// let id: ConversationId = "E1E2E3E4F1F2F3F4A1A2A3A4B1B2B3B4".parse().unwrap();
    ///
    /// assert_eq!("E1E2E3E4F1F2F3F4A1A2A3A4B1B2B3B4", id.to_string());
    /// ```
    ConversationId,
    "conversation id"
);

hex_identifier!(
    /// Identifies the MSE's message database (MAP section 3.1.14). It changes when
    /// handles or version counters from earlier sessions can no longer be trusted; 0
    /// means the version counters are not persistent.
    DatabaseIdentifier,
    "database identifier"
);

hex_identifier!(
    /// A 128-bit version counter (MAP section 3.1.15), used to detect changes to a
    /// folder, a conversation or the conversation listing.
    VersionCounter,
    "version counter"
);

/// The type of a message (MAP section 2.3).
///
/// # Examples
/// ```
/// # use obex_protocol_map::types::MessageType;
/// assert_eq!(MessageType::SmsGsm, "SMS_GSM".parse().unwrap());
/// assert_eq!("EMAIL", MessageType::Email.as_str());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageType {
    /// E-mail on RFC 2822 or MIME basis, `EMAIL`.
    Email,
    /// GSM short message, `SMS_GSM`.
    SmsGsm,
    /// CDMA short message, `SMS_CDMA`.
    SmsCdma,
    /// 3GPP MMS message, `MMS`.
    Mms,
    /// Instant message on MIME basis, `IM` (message format 1.1).
    Im,
}

impl MessageType {
    /// The value used for this type in MAP objects.
    pub fn as_str(self) -> &'static str {
        match self {
            MessageType::Email => "EMAIL",
            MessageType::SmsGsm => "SMS_GSM",
            MessageType::SmsCdma => "SMS_CDMA",
            MessageType::Mms => "MMS",
            MessageType::Im => "IM",
        }
    }
}

impl fmt::Display for MessageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MessageType {
    type Err = Error;

    fn from_str(text: &str) -> Result<MessageType, Error> {
        let text = text.trim();

        [
            MessageType::Email,
            MessageType::SmsGsm,
            MessageType::SmsCdma,
            MessageType::Mms,
            MessageType::Im,
        ]
        .into_iter()
        .find(|kind| kind.as_str().eq_ignore_ascii_case(text))
        .ok_or_else(|| Error::malformed("message type", format!("unknown type {:?}", text)))
    }
}

/// The time zone of a [`Timestamp`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum UtcOffset {
    /// Local time without a known offset, `YYYYMMDDTHHMMSS`.
    #[default]
    Local,
    /// UTC, `YYYYMMDDTHHMMSSZ`.
    Utc,
    /// Local time with its offset from UTC in minutes, `YYYYMMDDTHHMMSS±HHMM`.
    Minutes(i16),
}

/// A MAP timestamp (MAP section 3.1.10).
///
/// MAP uses the OBEX ISO 8601 basic format, `YYYYMMDDTHHMMSS` in local time, or with
/// the 'UTC Offset Timestamp Format' feature `YYYYMMDDTHHMMSS±HHMM`. Timestamps in
/// received objects are kept as the strings the device sent; parse them with
/// [`str::parse`] when needed.
///
/// # Examples
/// ```
/// # use obex_protocol_map::types::{Timestamp, UtcOffset};
/// let time: Timestamp = "20140706T095000-0400".parse().unwrap();
///
/// assert_eq!(2014, time.year);
/// assert_eq!(UtcOffset::Minutes(-240), time.offset);
/// assert_eq!("20140706T095000-0400", time.to_string());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Timestamp {
    /// Four digit year.
    pub year: u16,
    /// Month, 1 to 12.
    pub month: u8,
    /// Day of the month, 1 to 31.
    pub day: u8,
    /// Hour, 0 to 23.
    pub hour: u8,
    /// Minute, 0 to 59.
    pub minute: u8,
    /// Second, 0 to 60.
    pub second: u8,
    /// The time zone.
    pub offset: UtcOffset,
}

impl Timestamp {
    /// Creates a local time timestamp.
    pub fn new(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> Timestamp {
        Timestamp {
            year,
            month,
            day,
            hour,
            minute,
            second,
            offset: UtcOffset::Local,
        }
    }

    /// Returns this timestamp with a different time zone.
    pub fn with_offset(self, offset: UtcOffset) -> Timestamp {
        Timestamp { offset, ..self }
    }

    /// The UTC time `seconds` after the Unix epoch.
    ///
    /// # Examples
    /// ```
    /// # use obex_protocol_map::types::Timestamp;
    /// assert_eq!("20000229T120000Z", Timestamp::from_unix_time(951_825_600).to_string());
    /// ```
    pub fn from_unix_time(seconds: i64) -> Timestamp {
        let days = seconds.div_euclid(86_400);
        let time = seconds.rem_euclid(86_400);

        // Converts days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let day_of_era = z.rem_euclid(146_097);
        let year_of_era =
            (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let month_index = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * month_index + 2) / 5 + 1;
        let month = if month_index < 10 {
            month_index + 3
        } else {
            month_index - 9
        };
        let year = year_of_era + era * 400 + i64::from(month <= 2);

        Timestamp {
            year: year.clamp(0, 9999) as u16,
            month: month as u8,
            day: day as u8,
            hour: (time / 3_600) as u8,
            minute: (time % 3_600 / 60) as u8,
            second: (time % 60) as u8,
            offset: UtcOffset::Utc,
        }
    }

    /// The current UTC time.
    #[cfg(feature = "std")]
    pub fn now() -> Timestamp {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs() as i64);

        Timestamp::from_unix_time(seconds)
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}{:02}{:02}T{:02}{:02}{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )?;

        match self.offset {
            UtcOffset::Local => Ok(()),
            UtcOffset::Utc => f.write_str("Z"),
            UtcOffset::Minutes(minutes) => {
                let sign = if minutes < 0 { '-' } else { '+' };
                let minutes = minutes.unsigned_abs();
                write!(f, "{}{:02}{:02}", sign, minutes / 60, minutes % 60)
            }
        }
    }
}

impl FromStr for Timestamp {
    type Err = Error;

    fn from_str(text: &str) -> Result<Timestamp, Error> {
        let malformed = || Error::malformed("timestamp", format!("invalid timestamp {:?}", text));
        let text = text.trim_matches(|c: char| c.is_whitespace() || c == '\0');
        let bytes = text.as_bytes();

        if bytes.len() < 15 || !bytes[8].eq_ignore_ascii_case(&b'T') {
            return Err(malformed());
        }

        let number = |range: core::ops::Range<usize>| -> Result<u16, Error> {
            let digits = text.get(range).ok_or_else(malformed)?;
            if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(malformed());
            }
            digits.parse().map_err(|_| malformed())
        };

        let timestamp = Timestamp {
            year: number(0..4)?,
            month: number(4..6)? as u8,
            day: number(6..8)? as u8,
            hour: number(9..11)? as u8,
            minute: number(11..13)? as u8,
            second: number(13..15)? as u8,
            offset: match &text[15..] {
                "" => UtcOffset::Local,
                "Z" | "z" => UtcOffset::Utc,
                zone => {
                    let sign = match zone.as_bytes()[0] {
                        b'+' => 1,
                        b'-' => -1,
                        _ => return Err(malformed()),
                    };
                    let zone = zone[1..].replace(':', "");
                    if zone.len() != 4 || !zone.bytes().all(|byte| byte.is_ascii_digit()) {
                        return Err(malformed());
                    }
                    let hours: i16 = zone[..2].parse().map_err(|_| malformed())?;
                    let minutes: i16 = zone[2..].parse().map_err(|_| malformed())?;
                    UtcOffset::Minutes(sign * (hours * 60 + minutes))
                }
            },
        };

        let valid = (1..=12).contains(&timestamp.month)
            && (1..=31).contains(&timestamp.day)
            && timestamp.hour < 24
            && timestamp.minute < 60
            && timestamp.second <= 60;

        if valid {
            Ok(timestamp)
        } else {
            Err(malformed())
        }
    }
}

/// How the textual content of a message is transcoded (`Charset`, MAP sections 5.6.4.1
/// and 5.8.4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Charset {
    /// No transcoding: SMS are exchanged as PDUs.
    Native,
    /// Text transcoded to UTF-8. The only value allowed for e-mail, MMS and IM.
    #[default]
    Utf8,
}

/// Which fraction of a fractioned e-mail to deliver (`FractionRequest`, MAP section 5.6.4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FractionRequest {
    /// The first fraction, as a complete bMessage.
    First,
    /// The fraction following the previously delivered one, as a lone bBody.
    Next,
}

/// Whether more fractions of an e-mail follow (`FractionDeliver`, MAP section 5.6.4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FractionDeliver {
    /// More fractions follow.
    More,
    /// This was the last fraction.
    Last,
}

/// Selects messages or conversations by read status (`FilterReadStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReadStatusFilter {
    /// Only unread entries.
    Unread,
    /// Only read entries.
    Read,
}

/// Selects messages by priority (`FilterPriority`, MAP section 5.5.4.10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PriorityFilter {
    /// Only high priority messages.
    High,
    /// Only messages that are not high priority.
    NonHigh,
}

/// How a forwarded message's text is combined with the original (`ModifyText`, MAP
/// section 5.8.4.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModifyText {
    /// Replace the original text with the pushed text.
    Replace,
    /// Put the pushed text before the original text.
    Prepend,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_accept_short_and_padded_forms() {
        assert_eq!(MessageHandle(0x12345678AB), "12345678AB".parse().unwrap());
        assert_eq!(
            MessageHandle(0x12345678AB),
            "00000012345678AB\0".parse().unwrap()
        );
        assert!("".parse::<MessageHandle>().is_err());
        assert!("10000000000000000".parse::<MessageHandle>().is_err());
        assert!("XYZ".parse::<MessageHandle>().is_err());
    }

    #[test]
    fn timestamps_round_trip_every_zone() {
        for text in [
            "20071213T130510",
            "20071213T130510Z",
            "20140612T105430+0100",
        ] {
            assert_eq!(text, text.parse::<Timestamp>().unwrap().to_string());
        }
        assert_eq!(
            UtcOffset::Minutes(330),
            "20200101T000000+05:30".parse::<Timestamp>().unwrap().offset
        );
        assert!("20071313T130510".parse::<Timestamp>().is_err());
        assert!("2007-12-13".parse::<Timestamp>().is_err());
        assert!("20071213T130510+1".parse::<Timestamp>().is_err());
    }
}
