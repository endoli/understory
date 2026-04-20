// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Transcript payload bodies.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

/// Payload body stored by a transcript entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryBody {
    /// UTF-8 text content.
    Text(String),
    /// Opaque bytes.
    Bytes(Vec<u8>),
    /// No payload.
    Empty,
}

impl EntryBody {
    /// Returns the body as text when it is stored as UTF-8 text.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text.as_str()),
            Self::Bytes(_) | Self::Empty => None,
        }
    }

    /// Returns the body as bytes when it is stored as raw bytes.
    #[must_use]
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Bytes(bytes) => Some(bytes.as_slice()),
            Self::Text(_) | Self::Empty => None,
        }
    }

    pub(crate) fn append(&mut self, chunk: Self) -> Result<(), BodyAppendError> {
        match (self, chunk) {
            (body @ Self::Empty, Self::Text(text)) => {
                *body = Self::Text(text);
                Ok(())
            }
            (body @ Self::Empty, Self::Bytes(bytes)) => {
                *body = Self::Bytes(bytes);
                Ok(())
            }
            (Self::Text(text), Self::Text(chunk)) => {
                text.push_str(&chunk);
                Ok(())
            }
            (Self::Bytes(bytes), Self::Bytes(chunk)) => {
                bytes.extend_from_slice(&chunk);
                Ok(())
            }
            (Self::Text(_), Self::Bytes(_)) | (Self::Bytes(_), Self::Text(_)) => {
                Err(BodyAppendError::KindMismatch)
            }
            (_, Self::Empty) => Ok(()),
        }
    }
}

impl From<&str> for EntryBody {
    fn from(value: &str) -> Self {
        Self::Text(value.into())
    }
}

impl From<String> for EntryBody {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<Vec<u8>> for EntryBody {
    fn from(value: Vec<u8>) -> Self {
        Self::Bytes(value)
    }
}

/// Error returned when appending chunk payloads to an existing body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyAppendError {
    /// The existing body and the new chunk use incompatible storage kinds.
    KindMismatch,
}
