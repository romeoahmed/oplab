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
        let mut expected = Vec::new();
        for body in &bodies {
            expected.extend_from_slice(&[b'O', b'P', 1, 0]);
            expected.extend_from_slice(&u32::try_from(body.len())?.to_le_bytes());
            expected.extend_from_slice(body);
        }
        prop_assert_eq!(&bytes, &expected);
        let mut reader = Fragmented(Cursor::new(&expected), read_chunk);
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
    // A declared payload must reject an incorrect header without waiting for its body.
    let control = Header::new(Kind::Control, 1)?.encode();
    assert!(matches!(
        transport::read_payload(&mut control.as_slice(), 1),
        Err(transport::TransportError::UnexpectedKind)
    ));
    let oversized = Header::new(Kind::Binary, 2)?.encode();
    assert!(matches!(
        transport::read_payload(&mut oversized.as_slice(), 1),
        Err(transport::TransportError::PayloadLength)
    ));
    Ok(())
}

#[test]
fn declared_payloads_preserve_chunk_boundaries_and_leave_the_next_frame_unread()
-> Result<(), Box<dyn std::error::Error>> {
    for length in [1, 65_535, 65_536, 65_537, 1_048_576] {
        let payload = (0..length)
            .map(|index| u8::try_from(index % 251))
            .collect::<Result<Vec<_>, _>>()?;
        let mut wire = Vec::new();
        for chunk in payload.chunks(65_536) {
            wire.extend_from_slice(&[b'O', b'P', 1, 0]);
            wire.extend_from_slice(&u32::try_from(chunk.len())?.to_le_bytes());
            wire.extend_from_slice(chunk);
        }
        let end = wire.len();
        wire.extend_from_slice(&[b'O', b'P', 0, 0, 2, 0, 0, 0, b'{', b'}']);
        let mut reader = Cursor::new(&wire);
        assert_eq!(
            transport::read_payload(&mut Fragmented(&mut reader, 7), length)?,
            payload
        );
        assert_eq!(reader.position(), u64::try_from(end)?);
        assert!(transport::read_payload(&mut Cursor::new(&wire[..end - 1]), length).is_err());
    }
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
    let Reply::Assembled(artifact) = &mut message.response.result else {
        return Err("wrong fixture reply".into());
    };
    artifact.object_bytes = u32::try_from(MAX_OBJECT_BYTES + 1)?;
    let mut oversized = Vec::new();
    transport::write_json(&mut oversized, &message.response)?;
    assert!(transport::read_message(&mut Cursor::new(oversized)).is_err());
    Ok(())
}

#[test]
fn image_requests_require_complete_bounded_binary_transfers()
-> Result<(), Box<dyn std::error::Error>> {
    use oplab_core::protocol::execution::LoadImage;
    for image in [
        LoadImage::Elf,
        LoadImage::Raw {
            base: HexAddress::new(Address::new(0x1000)),
            entry: HexAddress::new(Address::new(0x1002)),
        },
    ] {
        let mut message = transport::RequestMessage {
            request: Request {
                id: Counter::new(2),
                command: Command::Load {
                    image,
                    initial: oplab_core::protocol::execution::InitialState::default(),
                    replace: None,
                    target: Target::X86_64,
                    completion: HexAddress::new(Address::new(0x2000)),
                    instruction_budget: Counter::new(100),
                    image_bytes: 65_537,
                },
            },
            payload: Some(vec![0xa5; 65_537]),
        };
        let mut bytes = Vec::new();
        transport::write_request(&mut Fragmented(&mut bytes, 1), &message)?;
        let parsed = transport::read_request(&mut Fragmented(Cursor::new(&bytes), 1))?
            .ok_or("missing request")?;
        assert_eq!(parsed.request, message.request);
        assert_eq!(parsed.payload, message.payload);
        assert!(transport::read_request(&mut Cursor::new(&bytes[..bytes.len() - 1])).is_err());
        let mut wrong_kind = Vec::new();
        transport::write_frame(
            &mut wrong_kind,
            Kind::Control,
            &serde_json::to_vec(&message.request)?,
        )?;
        transport::write_frame(&mut wrong_kind, Kind::Control, b"{}")?;
        assert!(transport::read_request(&mut Cursor::new(wrong_kind)).is_err());
        message.payload = Some(vec![0]);
        let mut output = Vec::new();
        assert!(transport::write_request(&mut output, &message).is_err());
        assert!(output.is_empty());
        let Command::Load { image_bytes, .. } = &mut message.request.command else {
            return Err("wrong fixture command".into());
        };
        *image_bytes = 1_048_577;
        let mut oversized = Vec::new();
        transport::write_frame(
            &mut oversized,
            Kind::Control,
            &serde_json::to_vec(&message.request)?,
        )?;
        assert!(transport::read_request(&mut Cursor::new(oversized)).is_err());
    }
    Ok(())
}

#[test]
fn memory_patches_require_exact_bounded_payloads() -> Result<(), Box<dyn std::error::Error>> {
    use oplab_core::protocol::execution::{MemoryWindow, SessionAction, SessionKey};
    for length in [0, 1, 65_536, 65_537, u32::MAX] {
        let request = Request {
            id: Counter::new(3),
            command: Command::Execute {
                session: SessionKey {
                    session: Counter::new(2),
                    generation: Counter::new(0),
                },
                action: SessionAction::WriteMemory(MemoryWindow {
                    address: HexAddress::new(Address::new(0x1000)),
                    length,
                }),
            },
        };
        let metadata = serde_json::to_vec(&request)?;
        let mut wire = vec![b'O', b'P', 0, 0];
        wire.extend_from_slice(&u32::try_from(metadata.len())?.to_le_bytes());
        wire.extend_from_slice(&metadata);
        if !(1..=65_536).contains(&length) {
            // Reject metadata before attempting to read or allocate its declared body.
            assert!(matches!(
                transport::read_request(&mut wire.as_slice()),
                Err(transport::TransportError::PayloadLength)
            ));
            let mut output = Vec::new();
            assert!(transport::write_request(&mut output, &request.into()).is_err());
            assert!(output.is_empty());
            continue;
        }
        let payload = (0..length)
            .map(|index| u8::try_from(index % 251))
            .collect::<Result<Vec<_>, _>>()?;
        wire.extend_from_slice(&[b'O', b'P', 1, 0]);
        wire.extend_from_slice(&length.to_le_bytes());
        wire.extend_from_slice(&payload);
        let parsed = transport::read_request(&mut Fragmented(wire.as_slice(), 3))?
            .ok_or("missing request")?;
        assert_eq!(parsed.request, request);
        assert_eq!(parsed.payload.as_ref(), Some(&payload));
        let mut output = Vec::new();
        transport::write_request(&mut output, &parsed)?;
        assert_eq!(output, wire);
        assert!(transport::read_request(&mut &wire[..wire.len() - 1]).is_err());
        for wrong in [
            None,
            Some(payload[..payload.len() - 1].to_vec()),
            Some(vec![0; payload.len() + 1]),
        ] {
            let message = transport::RequestMessage {
                request: request.clone(),
                payload: wrong,
            };
            let mut output = Vec::new();
            assert!(transport::write_request(&mut output, &message).is_err());
            assert!(output.is_empty());
        }
    }
    Ok(())
}
