//! Framing is independent of native engines and tolerates arbitrary pipe fragmentation.

use oplab_core::{
    address::Address,
    protocol::{
        BuildIdentity, Command, Reply, Request, Response,
        frame::{Header, Kind},
        scalar::{Counter, HexAddress},
        transport,
    },
    target::Target,
};
use proptest::prelude::*;
use std::io::{self, Cursor, Read, Write};

struct Fragmented<R>(R, usize);
impl<W: Write> Write for Fragmented<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(&bytes[..bytes.len().min(self.1)])
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
impl<R: Read> Read for Fragmented<R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let length = bytes.len().min(self.1);
        self.0.read(&mut bytes[..length])
    }
}

proptest! {
    #[test]
    fn pipe_fragmentation_preserves_complete_frames(
        bodies in prop::collection::vec(prop::collection::vec(any::<u8>(), 1..512), 0..12),
        write_chunk in 1_usize..32,
        read_chunk in 1_usize..32,
    ) {
        let mut bytes = Vec::new();
        for body in &bodies {
            transport::write_frame(&mut Fragmented(&mut bytes, write_chunk), Kind::Binary, body)?;
        }
        let mut reader = Fragmented(Cursor::new(&bytes), read_chunk);
        for body in bodies {
            let frame = transport::read_frame(&mut reader)?.ok_or_else(|| TestCaseError::fail("missing frame"))?;
            prop_assert_eq!(frame.kind, Kind::Binary);
            prop_assert_eq!(frame.body, body);
        }
        prop_assert!(transport::read_frame(&mut reader)?.is_none());
    }
}

#[test]
fn truncated_and_oversized_frames_fail_before_use() -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    transport::write_frame(&mut bytes, Kind::Control, b"{}")?;
    for length in 1..bytes.len() {
        assert!(transport::read_frame(&mut Cursor::new(&bytes[..length])).is_err());
    }
    let malicious = [b'O', b'P', 0, 0, 255, 255, 255, 255];
    assert!(transport::read_frame(&mut Cursor::new(malicious)).is_err());
    assert!(Header::new(Kind::Binary, 65_537).is_err());
    Ok(())
}

#[test]
fn oversized_replies_fail_before_any_frame_is_written() {
    use oplab_core::{
        address::Address,
        protocol::{DecodedInstruction, frame::MAX_CONTROL_BYTES, scalar::HexAddress},
    };
    let response = Response {
        id: Counter::new(1),
        result: Reply::Decoded(vec![DecodedInstruction {
            address: HexAddress::new(Address::new(0)),
            bytes: vec![0x90],
            text: "x".repeat(MAX_CONTROL_BYTES),
        }]),
    };
    let mut output = Vec::new();
    assert!(matches!(
        transport::write_json(&mut output, &response),
        Err(transport::TransportError::OutputLimit)
    ));
    assert!(output.is_empty());
}

#[test]
fn binary_artifacts_require_complete_bounded_ordered_transfers()
-> Result<(), Box<dyn std::error::Error>> {
    use oplab_core::protocol::{Artifact, MAX_OBJECT_BYTES};
    let mut message = transport::Message {
        response: Response {
            id: Counter::new(2),
            result: Reply::Assembled(Artifact {
                identity: BuildIdentity {
                    document: "binary".into(),
                    revision: Counter::new(0),
                    target: Target::X86_64,
                    base: HexAddress::new(Address::new(0x1000)),
                    assembler: oplab_core::protocol::AssemblerIdentity {
                        name: "fixture".into(),
                        version: "1".into(),
                    },
                },
                object_bytes: 65_537,
                image_bytes: 1,
                image: oplab_core::protocol::image::ImageInfo {
                    entry: HexAddress::new(Address::new(0x1000)),
                    segments: Vec::new(),
                    symbols: Vec::new(),
                    symbols_truncated: false,
                },
            }),
        },
        payloads: vec![vec![0xa5; 65_537], vec![0x5a]],
    };
    let mut bytes = Vec::new();
    transport::write_message(&mut Fragmented(&mut bytes, 1), &message)?;
    let parsed = transport::read_message(&mut Fragmented(Cursor::new(&bytes), 1))?
        .ok_or("missing message")?;
    assert_eq!(parsed.response, message.response);
    assert_eq!(parsed.payloads, message.payloads);
    for missing in [1, 9, 18] {
        assert!(
            transport::read_message(&mut Cursor::new(&bytes[..bytes.len() - missing])).is_err()
        );
    }
    for kind in [Kind::Control, Kind::Binary] {
        let mut malformed = Vec::new();
        transport::write_json(&mut malformed, &message.response)?;
        transport::write_frame(&mut malformed, kind, b"{}")?;
        assert!(transport::read_message(&mut Cursor::new(malformed)).is_err());
    }
    message.payloads[0].pop();
    let mut output = Vec::new();
    assert!(transport::write_message(&mut output, &message).is_err());
    assert!(output.is_empty());
    if let Reply::Assembled(artifact) = &mut message.response.result {
        artifact.object_bytes = u32::try_from(MAX_OBJECT_BYTES + 1)?;
    }
    let mut oversized = Vec::new();
    transport::write_json(&mut oversized, &message.response)?;
    assert!(transport::read_message(&mut Cursor::new(oversized)).is_err());
    Ok(())
}

#[test]
fn image_requests_require_complete_bounded_binary_transfers()
-> Result<(), Box<dyn std::error::Error>> {
    use oplab_core::address::Address;
    let mut message = transport::RequestMessage {
        request: Request {
            id: Counter::new(2),
            command: Command::Load {
                replace: None,
                target: Target::X86_64,
                completion: HexAddress::new(Address::new(0x2000)),
                instruction_budget: Counter::new(100),
                image_bytes: 65_537,
            },
        },
        image: Some(vec![0xa5; 65_537]),
    };
    let mut bytes = Vec::new();
    transport::write_request(&mut Fragmented(&mut bytes, 1), &message)?;
    let parsed = transport::read_request(&mut Fragmented(Cursor::new(&bytes), 1))?
        .ok_or("missing request")?;
    assert_eq!(parsed.request, message.request);
    assert_eq!(parsed.image, message.image);
    assert!(transport::read_request(&mut Cursor::new(&bytes[..bytes.len() - 1])).is_err());
    let mut wrong_kind = Vec::new();
    transport::write_frame(
        &mut wrong_kind,
        Kind::Control,
        &serde_json::to_vec(&message.request)?,
    )?;
    transport::write_frame(&mut wrong_kind, Kind::Control, b"{}")?;
    assert!(transport::read_request(&mut Cursor::new(wrong_kind)).is_err());
    message.image = Some(vec![0]);
    let mut output = Vec::new();
    assert!(transport::write_request(&mut output, &message).is_err());
    assert!(output.is_empty());
    if let Command::Load { image_bytes, .. } = &mut message.request.command {
        *image_bytes = 1_048_577;
    }
    let mut oversized = Vec::new();
    transport::write_frame(
        &mut oversized,
        Kind::Control,
        &serde_json::to_vec(&message.request)?,
    )?;
    assert!(transport::read_request(&mut Cursor::new(oversized)).is_err());
    Ok(())
}
