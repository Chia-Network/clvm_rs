use std::io::{Error, ErrorKind};

pub fn bad_encoding() -> Error {
    Error::new(ErrorKind::InvalidInput, "bad encoding")
}

pub fn internal_error() -> Error {
    Error::new(ErrorKind::InvalidInput, "internal error")
}
