//! ## Errors
//!
//! A single [`Error`] type is used throughout the crate. Failures of the underlying
//! OBEX session, including error responses from the remote device, are wrapped in
//! [`Error::Obex`]; [`Error::response_code`] gives the response code of a refused
//! request directly.
//!
use std::fmt;

use obex::response::ResponseCode;

/// The `Error` type. See [the module level documentation](index.html) for more.
#[derive(Debug)]
pub enum Error {
    /// The OBEX session failed, or the remote device refused the request.
    Obex(obex::Error),
    /// A MAP object or parameter received from the remote device could not be parsed.
    Malformed {
        /// What was being parsed, such as `"bMessage"` or `"MAP-msg-listing"`.
        object: &'static str,
        /// Why it could not be parsed.
        reason: String,
    },
    /// A response lacked a parameter or header that MAP requires.
    MissingParameter(&'static str),
    /// CONNECT was answered by a service other than the one requested.
    UnexpectedService,
    /// A value passed to this crate cannot be represented in a MAP request.
    InvalidArgument(&'static str),
}

impl Error {
    /// Returns the OBEX response code of a refused request, if this error was caused by one.
    ///
    /// MAP gives the codes specific meanings (MAP section 6.3.3), for example
    /// [`ResponseCode::NotFound`] for an unknown message handle or folder.
    ///
    /// # Examples
    /// ```
    /// # use bluetooth_map::obex::packet::Response;
    /// # use bluetooth_map::obex::response::ResponseCode;
    /// let error = bluetooth_map::Error::Obex(bluetooth_map::obex::Error::Response(Box::new(Response::new(
    ///     ResponseCode::NotFound,
    /// ))));
    ///
    /// assert_eq!(Some(ResponseCode::NotFound), error.response_code());
    /// ```
    pub fn response_code(&self) -> Option<ResponseCode> {
        match self {
            Error::Obex(error) => error.response_code(),
            _ => None,
        }
    }

    pub(crate) fn malformed(object: &'static str, reason: impl Into<String>) -> Error {
        Error::Malformed {
            object,
            reason: reason.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Obex(error) => write!(f, "OBEX error: {}", error),
            Error::Malformed { object, reason } => write!(f, "malformed {}: {}", object, reason),
            Error::MissingParameter(name) => write!(f, "response is missing {}", name),
            Error::UnexpectedService => {
                write!(f, "the remote device connected a different OBEX service")
            }
            Error::InvalidArgument(reason) => write!(f, "invalid argument: {}", reason),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Obex(error) => Some(error),
            _ => None,
        }
    }
}

impl From<obex::Error> for Error {
    fn from(error: obex::Error) -> Error {
        Error::Obex(error)
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Error {
        Error::Obex(obex::Error::Io(error))
    }
}

/// A `Result` alias where the `Err` case is a MAP [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
