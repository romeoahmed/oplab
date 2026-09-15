//! Partial-read-safe framed I/O with bounds checked before body allocation.

use super::{
    Command, MAX_OBJECT_BYTES, Reply, Request, Response,
    frame::{HEADER_BYTES, Header, HeaderError, Kind, MAX_BINARY_BYTES, MAX_CONTROL_BYTES},
    stream::{ObservationUpdate, StreamEvent},
};
use std::io::{self, Read, Write};
use thiserror::Error;

/// One complete frame. Operation-specific binary parsing is negotiated separately.
#[derive(Debug, PartialEq, Eq)]
pub struct Frame {
    /// Explicit payload format.
    pub kind: Kind,
    /// Complete body of a validated length.
    pub body: Vec<u8>,
}

/// A malformed or interrupted frame ends its connection.
#[derive(Debug, Error)]
pub enum TransportError {
    /// Host pipe failure or truncated header/body.
    #[error("worker pipe failed")]
    Io(#[from] io::Error),
    /// Header fields or body bounds failed validation.
    #[error(transparent)]
    Header(#[from] HeaderError),
    /// Request shape or scalar representation was invalid.
    #[error("invalid JSON protocol message")]
    Json,
    /// A frame kind was not negotiated for this operation.
    #[error("unexpected frame kind")]
    UnexpectedKind,
    /// Binary metadata, buffers, or chunk boundaries disagree with the operation.
    #[error("invalid binary payload length")]
    PayloadLength,
    /// Serializing a response would exceed its control budget.
    #[error("protocol output budget exceeded")]
    OutputLimit,
}

/// Read one complete frame or clean EOF before the first header byte.
///
/// # Errors
///
/// Rejects malformed/oversized frames and partial EOF without scanning for another header.
pub fn read_frame(reader: &mut impl Read) -> Result<Option<Frame>, TransportError> {
    let mut bytes = [0_u8; HEADER_BYTES];
    loop {
        match reader.read(&mut bytes[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error.into()),
        }
    }
    reader.read_exact(&mut bytes[1..])?;
    let header = Header::decode(bytes)?;
    let mut body = vec![0; header.length()];
    reader.read_exact(&mut body)?;
    Ok(Some(Frame {
        kind: header.kind(),
        body,
    }))
}

/// Write and flush a complete frame; the caller serializes access to the stream.
///
/// # Errors
///
/// Rejects empty or oversized bodies and propagates I/O failures.
pub fn write_frame(writer: &mut impl Write, kind: Kind, body: &[u8]) -> Result<(), TransportError> {
    let header = Header::new(kind, body.len())?;
    writer.write_all(&header.encode())?;
    writer.write_all(body)?;
    writer.flush()?;
    Ok(())
}

/// Serialize, write and flush a response without binary payloads.
///
/// # Errors
///
/// Returns [`TransportError::OutputLimit`] before writing if serialization exceeds
/// its budget. I/O failure may leave a partial frame and ends the connection.
pub fn write_json(writer: &mut impl Write, response: &Response) -> Result<(), TransportError> {
    let body = encode_json(response)?;
    write_frame(writer, Kind::Control, &body)
}

/// Serialize response metadata to bounded JSON bytes without a header or newline.
///
/// # Errors
///
/// Returns [`TransportError::OutputLimit`] if serialization fails or exceeds the control budget.
pub fn encode_json(response: &Response) -> Result<Vec<u8>, TransportError> {
    let mut buffer = LimitedBuffer(Vec::new());
    serde_json::to_writer(&mut buffer, response).map_err(|_| TransportError::OutputLimit)?;
    Ok(buffer.0)
}

struct LimitedBuffer(Vec<u8>);

impl Write for LimitedBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_CONTROL_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::other("output limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// One complete response and its ordered binary files. Success is observed only
/// after all declared payload bytes arrive; a control frame alone is incomplete.
#[derive(Debug)]
pub struct Message {
    /// Correlated control metadata.
    pub response: Response,
    /// Complete ELF files for assembly or one memory window for an observation.
    pub payloads: Vec<Vec<u8>>,
}

fn payload_lengths(response: &Response) -> Result<Vec<usize>, TransportError> {
    if let Reply::Observed(observation) = &response.result {
        return observation
            .memory
            .map(|window| checked_length(window.length, MAX_BINARY_BYTES))
            .transpose()
            .map(|length| length.into_iter().collect());
    }
    let Reply::Assembled(artifact) = &response.result else {
        return Ok(Vec::new());
    };
    [artifact.object_bytes, artifact.image_bytes]
        .into_iter()
        .map(|length| checked_length(length, MAX_OBJECT_BYTES))
        .collect()
}

/// Write metadata followed immediately by its ordered binary chunks.
///
/// # Errors
///
/// Rejects mismatched payload lengths or oversized JSON before writing.
/// I/O failure may leave a partial message and ends the connection.
pub fn write_message(writer: &mut impl Write, message: &Message) -> Result<(), TransportError> {
    let lengths = payload_lengths(&message.response)?;
    if lengths.len() != message.payloads.len()
        || lengths
            .iter()
            .zip(&message.payloads)
            .any(|(length, bytes)| *length != bytes.len())
    {
        return Err(TransportError::PayloadLength);
    }
    let metadata = encode_json(&message.response)?;
    write_frame(writer, Kind::Control, &metadata)?;
    for bytes in &message.payloads {
        for chunk in bytes.chunks(MAX_BINARY_BYTES) {
            write_frame(writer, Kind::Binary, chunk)?;
        }
    }
    Ok(())
}

/// Read a complete response, returning `None` only at EOF before its first header.
///
/// # Errors
///
/// Rejects malformed JSON, invalid lengths/kinds/chunks, truncated input and I/O failures.
pub fn read_message(reader: &mut impl Read) -> Result<Option<Message>, TransportError> {
    let Some(frame) = read_frame(reader)? else {
        return Ok(None);
    };
    if frame.kind != Kind::Control {
        return Err(TransportError::UnexpectedKind);
    }
    read_response(reader, &frame.body).map(Some)
}

fn read_response(reader: &mut impl Read, body: &[u8]) -> Result<Message, TransportError> {
    let response: Response = serde_json::from_slice(body).map_err(|_| TransportError::Json)?;
    let lengths = payload_lengths(&response)?;
    let mut payloads = Vec::with_capacity(lengths.len());
    for length in lengths {
        payloads.push(read_payload(reader, length)?);
    }
    Ok(Message { response, payloads })
}

/// Complete request with an optional standard ELF image. The machine cannot be
/// mutated until the declared image transfer has completed and been validated.
#[derive(Debug)]
pub struct RequestMessage {
    /// Correlation and operation metadata.
    pub request: Request,
    /// ELF image only for Load; never an implicit memory mapping.
    pub image: Option<Vec<u8>>,
}

impl From<Request> for RequestMessage {
    fn from(request: Request) -> Self {
        Self {
            request,
            image: None,
        }
    }
}

/// Read a control request and its ELF image, or `None` at EOF before the first header.
///
/// # Errors
///
/// Rejects malformed metadata, invalid lengths/kinds, partial transfers and I/O failures.
pub fn read_request(reader: &mut impl Read) -> Result<Option<RequestMessage>, TransportError> {
    let Some(frame) = read_frame(reader)? else {
        return Ok(None);
    };
    if frame.kind != Kind::Control {
        return Err(TransportError::UnexpectedKind);
    }
    let request: Request = serde_json::from_slice(&frame.body).map_err(|_| TransportError::Json)?;
    let image = request_length(&request)?
        .map(|length| read_payload(reader, length))
        .transpose()?;
    Ok(Some(RequestMessage { request, image }))
}

/// Write a complete request after checking its control and binary bounds.
///
/// # Errors
///
/// Rejects payload mismatches or oversized JSON before writing; propagates I/O failures.
pub fn write_request(
    writer: &mut impl Write,
    message: &RequestMessage,
) -> Result<(), TransportError> {
    validate_request(message)?;
    let mut body = LimitedBuffer(Vec::new());
    serde_json::to_writer(&mut body, &message.request).map_err(|_| TransportError::OutputLimit)?;
    write_frame(writer, Kind::Control, &body.0)?;
    if let Some(image) = &message.image {
        for chunk in image.chunks(MAX_BINARY_BYTES) {
            write_frame(writer, Kind::Binary, chunk)?;
        }
    }
    Ok(())
}

/// Check that a request owns exactly its declared binary image.
///
/// # Errors
///
/// Rejects missing, extra, empty, or oversized image data.
pub fn validate_request(message: &RequestMessage) -> Result<(), TransportError> {
    if request_length(&message.request)? != message.image.as_ref().map(Vec::len) {
        return Err(TransportError::PayloadLength);
    }
    Ok(())
}

fn request_length(request: &Request) -> Result<Option<usize>, TransportError> {
    match request.command {
        Command::Load { image_bytes, .. } => {
            checked_length(image_bytes, MAX_OBJECT_BYTES).map(Some)
        }
        _ => Ok(None),
    }
}

fn checked_length(length: u32, limit: usize) -> Result<usize, TransportError> {
    usize::try_from(length)
        .ok()
        .filter(|length| (1..=limit).contains(length))
        .ok_or(TransportError::PayloadLength)
}

/// Read one declared binary file or memory window in bounded ordered chunks.
///
/// # Errors
///
/// Rejects empty or oversized lengths, incorrect frames, partial transfers and I/O failures.
pub fn read_payload(reader: &mut impl Read, length: usize) -> Result<Vec<u8>, TransportError> {
    if !(1..=MAX_OBJECT_BYTES).contains(&length) {
        return Err(TransportError::PayloadLength);
    }
    let mut bytes = Vec::with_capacity(length);
    while bytes.len() < length {
        let frame =
            read_frame(reader)?.ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))?;
        if frame.kind != Kind::Binary {
            return Err(TransportError::UnexpectedKind);
        }
        if frame.body.len() != (length - bytes.len()).min(MAX_BINARY_BYTES) {
            return Err(TransportError::PayloadLength);
        }
        bytes.extend_from_slice(&frame.body);
    }
    Ok(bytes)
}

/// One complete worker output. Observation events never settle a request.
#[derive(Debug)]
pub enum Output {
    /// A correlated reply and its complete payloads.
    Response(Message),
    /// An unsolicited event and its complete optional memory window.
    Observation(StreamMessage),
}

/// An event becomes usable only after its complete binary payload arrives.
#[derive(Debug)]
pub struct StreamMessage {
    /// Explicit subscription and baseline metadata.
    pub event: StreamEvent,
    /// Full memory window when declared by the event, otherwise absent.
    pub memory: Option<Vec<u8>>,
}

fn stream_length(event: &StreamEvent) -> Result<Option<usize>, TransportError> {
    let length = match &event.update {
        ObservationUpdate::Full(observation) => observation.memory.map(|window| window.length),
        ObservationUpdate::Delta(delta) => (delta.memory_bytes != 0).then_some(delta.memory_bytes),
        ObservationUpdate::Ended(_) => None,
    };
    length
        .map(|length| checked_length(length, MAX_BINARY_BYTES))
        .transpose()
}

/// Write an observation event without interleaving its optional binary payload.
///
/// # Errors
///
/// Rejects payload mismatches or oversized JSON before writing; propagates I/O failures.
pub fn write_stream(
    writer: &mut impl Write,
    event: &StreamEvent,
    memory: Option<&[u8]>,
) -> Result<(), TransportError> {
    if stream_length(event)? != memory.map(<[u8]>::len) {
        return Err(TransportError::PayloadLength);
    }
    let mut body = LimitedBuffer(Vec::new());
    serde_json::to_writer(&mut body, event).map_err(|_| TransportError::OutputLimit)?;
    write_frame(writer, Kind::Observation, &body.0)?;
    if let Some(memory) = memory {
        write_frame(writer, Kind::Binary, memory)?;
    }
    Ok(())
}

/// Read a reply or subscription event with its binary payloads.
///
/// Returns `None` only at EOF before the first header.
///
/// # Errors
///
/// Rejects malformed metadata, invalid lengths/kinds, partial transfers and I/O failures.
pub fn read_output(reader: &mut impl Read) -> Result<Option<Output>, TransportError> {
    let Some(frame) = read_frame(reader)? else {
        return Ok(None);
    };
    match frame.kind {
        Kind::Control => read_response(reader, &frame.body)
            .map(Output::Response)
            .map(Some),
        Kind::Observation => {
            let event: StreamEvent =
                serde_json::from_slice(&frame.body).map_err(|_| TransportError::Json)?;
            let memory = stream_length(&event)?
                .map(|length| read_payload(reader, length))
                .transpose()?;
            Ok(Some(Output::Observation(StreamMessage { event, memory })))
        }
        Kind::Binary => Err(TransportError::UnexpectedKind),
    }
}
