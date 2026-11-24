//! Frozen outputs from registry `uuid` 1.29.0 and 1.29.3 (identical), `serde_json`
//! 1.0.154 and `bmux_codec` 0.0.1-alpha.4 stable mode. Do not regenerate from Switchy.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use switchy_uuid::Uuid;

#[test]
fn frozen_parse_and_wire_contract() {
    for row in include_str!("contract.tsv").lines() {
        let mut fields = row.split('\t');
        let input = fields.next().unwrap();
        let raw = fields.next().unwrap();
        let result = input.parse::<Uuid>();
        if raw == "!" {
            assert!(result.is_err(), "accepted {input:?}");
            continue;
        }
        let id = result.unwrap();
        let bits = u128::from_str_radix(raw, 16).unwrap();
        assert_eq!(*id.as_bytes(), bits.to_be_bytes());
        assert_eq!(id.as_u128_le(), bits.swap_bytes());
        assert_eq!(Uuid::from_u128_le(bits.swap_bytes()), id);
        let json = fields.next().unwrap();
        let canonical = json.trim_matches('"');
        assert_eq!(id.to_string(), canonical);
        assert_eq!(format!("{id:?}"), canonical);
        assert_eq!(id.simple(), raw);
        assert_eq!(id.braced(), format!("{{{canonical}}}"));
        assert_eq!(id.urn(), format!("urn:uuid:{canonical}"));
        #[cfg(feature = "serde")]
        {
            let binary = fields.next().unwrap();
            let bytes: Vec<_> = (0..binary.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&binary[i..i + 2], 16).unwrap())
                .collect();
            assert_eq!(bmux_codec::to_vec(&id).unwrap(), bytes);
            assert_eq!(bmux_codec::from_bytes::<Uuid>(&bytes).unwrap(), id);
            assert_eq!(serde_json::to_string(&id).unwrap(), json);
            assert_eq!(serde_json::from_str::<Uuid>(json).unwrap(), id);
        }
    }
}

#[cfg(feature = "serde")]
#[test]
fn rejects_wrong_binary_lengths_and_invalid_json() {
    for length in [0u8, 15, 17] {
        let mut bytes = vec![0; usize::from(length) + 1];
        bytes[0] = length;
        assert!(bmux_codec::from_bytes::<Uuid>(&bytes).is_err());
    }
    assert!(serde_json::from_str::<Uuid>(r#""{bad}""#).is_err());
}
