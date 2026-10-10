use std::collections::HashSet;

use super::{CROCKFORD, EntityId, IdError, encode_ulid};

fn decode_crockford(text: &str) -> u128 {
    text.bytes().fold(0, |value, byte| {
        let digit = CROCKFORD
            .iter()
            .position(|&candidate| candidate == byte)
            .expect("crockford digit");
        (value << 5) | digit as u128
    })
}

#[test]
fn ulid_encoding_places_timestamp_then_randomness_in_crockford_base32() {
    assert_eq!(encode_ulid(0, [0; 10]), "00000000000000000000000000");
    assert_eq!(
        encode_ulid((1 << 48) - 1, [0xff; 10]),
        "7ZZZZZZZZZZZZZZZZZZZZZZZZZ"
    );
    let random = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let encoded = encode_ulid(1_469_918_176_385, random);
    assert_eq!(encoded.len(), 26);
    let value = decode_crockford(&encoded);
    assert_eq!(value >> 80, 1_469_918_176_385);
    let mut low = [0_u8; 10];
    low.copy_from_slice(&value.to_be_bytes()[6..]);
    assert_eq!(low, random);
    assert_eq!(&encoded[..10], "01ARYZ6S41");
}

#[test]
fn ulid_text_order_follows_timestamp_order() {
    let earlier = encode_ulid(1_700_000_000_000, [0xff; 10]);
    let later = encode_ulid(1_700_000_000_001, [0; 10]);
    assert!(earlier < later);
}

#[test]
fn generated_ids_are_prefixed_valid_and_unique() {
    let mut seen = HashSet::new();
    for _ in 0..10_000 {
        let id = EntityId::generate("ses").unwrap();
        let (prefix, ulid) = id.as_str().split_once('_').unwrap();
        assert_eq!(prefix, "ses");
        assert_eq!(ulid.len(), 26);
        assert!(ulid.bytes().all(|byte| CROCKFORD.contains(&byte)));
        assert_eq!(EntityId::new(id.as_str()).unwrap(), id);
        assert!(seen.insert(id), "duplicate generated identifier");
    }
}

#[test]
fn generated_ids_sort_by_creation_millisecond() {
    let first = EntityId::generate("evt").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(3));
    let second = EntityId::generate("evt").unwrap();
    assert!(first < second, "{first} should sort before {second}");
    let decode_time = |id: &EntityId| decode_crockford(&id.as_str()[4..]) >> 80;
    assert!(decode_time(&first) < decode_time(&second));
}

#[test]
fn generation_rejects_invalid_prefixes() {
    for prefix in ["", "Ses", "se_s", "s1", "abcdefghijklmnopq", "sé", "a b"] {
        assert_eq!(
            EntityId::generate(prefix),
            Err(IdError::InvalidPrefix),
            "accepted prefix {prefix:?}"
        );
    }
    assert!(EntityId::generate("abcdefghijklmnop").is_ok());
    assert_eq!(
        IdError::InvalidPrefix.to_string(),
        "identifier prefix must be 1-16 ASCII lowercase letters"
    );
    assert!(
        IdError::Randomness("unavailable".into())
            .to_string()
            .contains("randomness")
    );
}
