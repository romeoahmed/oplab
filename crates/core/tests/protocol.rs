//! Wire serialization is checked independently of TypeScript annotations.

use oplab_core::{
    address::Address,
    protocol::{
        Command, Diagnostic, DiagnosticCode, Reply, Request, Response,
        frame::{Header, Kind, MAX_CONTROL_BYTES},
        scalar::{Counter, HexAddress},
    },
};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]
    #[test]
    fn integers_round_trip_without_json_numbers(value in any::<u64>()) {
        let counter = Counter::new(value);
        let address = HexAddress::new(Address::new(value));
        let decimal_json = serde_json::to_string(&counter)?;
        let address_json = serde_json::to_string(&address)?;
        prop_assert_eq!(&decimal_json, &format!("\"{value}\""));
        prop_assert_eq!(&address_json, &format!("\"0x{value:016x}\""));
        prop_assert_eq!(serde_json::from_str::<Counter>(&decimal_json)?, counter);
        prop_assert_eq!(serde_json::from_str::<HexAddress>(&address_json)?, address);
    }

    #[test]
    fn headers_match_the_published_little_endian_layout(length in 1..=MAX_CONTROL_BYTES) {
        let header = Header::new(Kind::Control, length)?;
        let mut expected = [b'O', b'P', 0, 0, 0, 0, 0, 0];
        expected[4..].copy_from_slice(&u32::try_from(length)?.to_le_bytes());
        prop_assert_eq!(header.encode(), expected);
        prop_assert_eq!(Header::decode(expected)?, header);
    }
}

#[test]
fn wire_schema_rejects_ambiguous_scalars_and_unknown_commands()
-> Result<(), Box<dyn std::error::Error>> {
    for input in [
        "1",
        "\"01\"",
        "\"-1\"",
        "\"+1\"",
        "\"1.0\"",
        "\"18446744073709551616\"",
        "null",
    ] {
        assert!(
            serde_json::from_str::<Counter>(input).is_err(),
            "accepted {input}"
        );
    }
    for input in [
        "1",
        "\"0x1\"",
        "\"0X0000000000000001\"",
        "\"0x00000000000000FF\"",
    ] {
        assert!(serde_json::from_str::<HexAddress>(input).is_err());
    }
    for input in [
        r#"{"id":"1","command":{"type":"shutdown","extra":true}}"#,
        r#"{"id":"1","command":{"type":"hello","data":{"version":1,"extra":true}}}"#,
        r#"{"id":"1","command":{"type":"run"}}"#,
    ] {
        assert!(
            serde_json::from_str::<Request>(input).is_err(),
            "accepted {input}"
        );
    }
    let request = Request {
        id: Counter::new(u64::MAX),
        command: Command::Hello { version: 1 },
    };
    let json = serde_json::to_value(&request)?;
    assert_eq!(
        json,
        serde_json::json!({"id":"18446744073709551615", "command":{"type":"hello", "data":{"version":1}}})
    );
    let response = Response {
        id: request.id,
        result: Reply::Error(Diagnostic::new(DiagnosticCode::InvalidInput)),
    };
    assert_eq!(
        serde_json::to_value(&response)?,
        serde_json::json!({"id":"18446744073709551615", "result":{"type":"error", "data":{"code":"invalid_input", "address":null, "source_offset":null}}})
    );
    Ok(())
}

#[test]
fn execution_observations_preserve_wide_registers_and_strict_session_keys()
-> Result<(), Box<dyn std::error::Error>> {
    use oplab_core::protocol::execution::{Registers, SessionKey};
    let registers = Registers::X86_64 {
        gpr: [Counter::new(u64::MAX); 16],
        rip: HexAddress::new(Address::new(u64::MAX)),
        rflags: Counter::new(2),
    };
    let mut json = serde_json::to_value(&registers)?;
    assert_eq!(json["data"]["gpr"][0], "18446744073709551615");
    assert_eq!(json["data"]["rip"], "0xffffffffffffffff");
    assert_eq!(
        serde_json::from_value::<Registers>(json.clone())?,
        registers
    );
    json["data"]["gpr"][0] = serde_json::json!(42);
    assert!(serde_json::from_value::<Registers>(json).is_err());
    for value in [
        serde_json::json!({"session": "1", "generation": 0}),
        serde_json::json!({"session": "1", "generation": "0", "extra": true}),
    ] {
        assert!(serde_json::from_value::<SessionKey>(value).is_err());
    }
    Ok(())
}
