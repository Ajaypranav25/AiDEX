//! AWS event-stream decoding with prelude/message CRC and allocation bounds.
use std::io::{self, Read};
pub(super) fn read_frame(
    reader: &mut impl Read,
    max: usize,
) -> io::Result<Option<(String, String)>> {
    let mut prelude = [0u8; 12];
    match reader.read(&mut prelude[..1])? {
        0 => return Ok(None),
        _ => reader.read_exact(&mut prelude[1..])?,
    }
    let total = u32::from_be_bytes(prelude[..4].try_into().map_err(|_| invalid())?) as usize;
    let headers_len = u32::from_be_bytes(prelude[4..8].try_into().map_err(|_| invalid())?) as usize;
    let expected = u32::from_be_bytes(prelude[8..12].try_into().map_err(|_| invalid())?);
    if crc32fast::hash(&prelude[..8]) != expected
        || total < 16
        || total > max
        || headers_len > total - 16
    {
        return Err(invalid());
    }
    let mut bytes = vec![0u8; total - 12];
    reader.read_exact(&mut bytes)?;
    let checksum = u32::from_be_bytes(bytes[bytes.len() - 4..].try_into().map_err(|_| invalid())?);
    let mut hash = crc32fast::Hasher::new();
    hash.update(&prelude);
    hash.update(&bytes[..bytes.len() - 4]);
    if hash.finalize() != checksum {
        return Err(invalid());
    }
    let mut rest = &bytes[..headers_len];
    let mut event = None;
    let mut message_type = None;
    while !rest.is_empty() {
        let key_len = usize::from(rest[0]);
        rest = &rest[1..];
        if rest.len() < key_len + 1 {
            return Err(invalid());
        }
        let key = std::str::from_utf8(&rest[..key_len]).map_err(|_| invalid())?;
        let kind = rest[key_len];
        rest = &rest[key_len + 1..];
        if kind != 7 || rest.len() < 2 {
            return Err(invalid());
        }
        let len = u16::from_be_bytes([rest[0], rest[1]]) as usize;
        rest = &rest[2..];
        if rest.len() < len {
            return Err(invalid());
        }
        let value = std::str::from_utf8(&rest[..len]).map_err(|_| invalid())?;
        rest = &rest[len..];
        match key {
            ":event-type" | ":exception-type" => event = Some(value.to_owned()),
            ":message-type" => message_type = Some(value.to_owned()),
            _ => {}
        }
    }
    let name = if message_type.as_deref() == Some("event") {
        event.ok_or_else(invalid)?
    } else {
        format!("exception:{}", event.unwrap_or_default())
    };
    let payload =
        String::from_utf8(bytes[headers_len..bytes.len() - 4].to_vec()).map_err(|_| invalid())?;
    Ok(Some((name, payload)))
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid AWS event stream frame")
}
