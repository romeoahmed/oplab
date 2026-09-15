//! Pure frame-header validation, shared without importing native engine dependencies.

use thiserror::Error;

/// Fixed header: two magic bytes, kind, reserved flags, and a little-endian length.
pub const HEADER_BYTES: usize = 8;
/// Maximum JSON body size for control and observation frames, checked before allocation.
pub const MAX_CONTROL_BYTES: usize = 1024 * 1024;
/// Maximum binary body size; operation metadata travels in the preceding JSON frame.
pub const MAX_BINARY_BYTES: usize = 64 * 1024;

/// Body format declared by the header, independent of payload contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// UTF-8 JSON request or response envelope.
    Control,
    /// Exact binary payload, interpreted only by a negotiated operation.
    Binary,
    /// Unsolicited JSON observation event, distinct from a request outcome.
    Observation,
}

/// A validated frame length and kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    kind: Kind,
    length: usize,
}

/// Malformed headers are connection failures, never a reason to scan for new magic.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[error("invalid or oversized protocol frame")]
pub struct HeaderError;

impl Header {
    /// Validate a frame before reading or allocating its body.
    ///
    /// # Errors
    ///
    /// Rejects empty or oversized bodies.
    pub const fn new(kind: Kind, length: usize) -> Result<Self, HeaderError> {
        let maximum = match kind {
            Kind::Control | Kind::Observation => MAX_CONTROL_BYTES,
            Kind::Binary => MAX_BINARY_BYTES,
        };
        if length == 0 || length > maximum {
            return Err(HeaderError);
        }
        Ok(Self { kind, length })
    }

    /// Decode exactly one header. Protocol versions are negotiated inside control frames.
    ///
    /// # Errors
    ///
    /// Rejects unknown magic, kinds, flags, and invalid lengths.
    pub fn decode(bytes: [u8; HEADER_BYTES]) -> Result<Self, HeaderError> {
        if bytes[0..2] != *b"OP" || bytes[3] != 0 {
            return Err(HeaderError);
        }
        let kind = match bytes[2] {
            0 => Kind::Control,
            1 => Kind::Binary,
            2 => Kind::Observation,
            _ => return Err(HeaderError),
        };
        let length = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        Self::new(kind, usize::try_from(length).map_err(|_| HeaderError)?)
    }

    /// Encode a previously validated header without truncating its length.
    #[must_use]
    pub const fn encode(self) -> [u8; HEADER_BYTES] {
        // Both length limits fit in u32 by construction.
        let length = self.length.to_le_bytes();
        [
            b'O',
            b'P',
            match self.kind {
                Kind::Control => 0,
                Kind::Binary => 1,
                Kind::Observation => 2,
            },
            0,
            length[0],
            length[1],
            length[2],
            length[3],
        ]
    }

    /// Body interpretation.
    #[must_use]
    pub const fn kind(self) -> Kind {
        self.kind
    }

    /// Exact validated body length.
    #[must_use]
    pub const fn length(self) -> usize {
        self.length
    }
}
