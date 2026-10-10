//! Bounded GGUF v2/v3 metadata and tensor-directory validation.
use crate::{InferenceError, files};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
pub(crate) struct Header {
    pub version: u32,
    pub context: Option<u32>,
}
struct Reader {
    file: File,
    size: u64,
}
impl Reader {
    fn bytes<const N: usize>(&mut self) -> Result<[u8; N], InferenceError> {
        let mut b = [0; N];
        self.file
            .read_exact(&mut b)
            .map_err(|_| bad("truncated GGUF"))?;
        Ok(b)
    }
    fn u32(&mut self) -> Result<u32, InferenceError> {
        Ok(u32::from_le_bytes(self.bytes()?))
    }
    fn u64(&mut self) -> Result<u64, InferenceError> {
        Ok(u64::from_le_bytes(self.bytes()?))
    }
    fn skip(&mut self, n: u64) -> Result<(), InferenceError> {
        let end = self
            .file
            .stream_position()?
            .checked_add(n)
            .filter(|n| *n <= self.size)
            .ok_or_else(|| bad("GGUF value exceeds file"))?;
        self.file.seek(SeekFrom::Start(end))?;
        Ok(())
    }
    fn string(&mut self) -> Result<String, InferenceError> {
        let n = self.u64()?;
        if n > 1024 * 1024 {
            return Err(bad("GGUF key too large"));
        }
        let mut b = vec![0; n as usize];
        self.file
            .read_exact(&mut b)
            .map_err(|_| bad("truncated GGUF string"))?;
        String::from_utf8(b).map_err(|_| bad("invalid GGUF string"))
    }
    fn value(&mut self, t: u32, depth: u8) -> Result<Option<u64>, InferenceError> {
        match t {
            0 | 1 | 7 => {
                self.skip(1)?;
                Ok(None)
            }
            2 | 3 => {
                self.skip(2)?;
                Ok(None)
            }
            4 => Ok(Some(self.u32()? as u64)),
            5 | 6 => {
                self.skip(4)?;
                Ok(None)
            }
            10 => Ok(Some(self.u64()?)),
            11 | 12 => {
                self.skip(8)?;
                Ok(None)
            }
            8 => {
                let n = self.u64()?;
                self.skip(n)?;
                Ok(None)
            }
            9 => {
                if depth != 0 {
                    return Err(bad("nested GGUF arrays refused"));
                }
                let element = self.u32()?;
                let count = self.u64()?;
                if count > 100_000_000 {
                    return Err(bad("GGUF array too large"));
                }
                let width = match element {
                    0 | 1 | 7 => Some(1),
                    2 | 3 => Some(2),
                    4..=6 => Some(4),
                    10..=12 => Some(8),
                    8 => None,
                    _ => return Err(bad("invalid GGUF array type")),
                };
                if let Some(width) = width {
                    self.skip(
                        count
                            .checked_mul(width)
                            .ok_or_else(|| bad("GGUF array overflow"))?,
                    )?;
                } else {
                    for _ in 0..count {
                        self.value(element, 1)?;
                    }
                }
                Ok(None)
            }
            _ => Err(bad("unknown GGUF metadata type")),
        }
    }
}
fn bad(message: &str) -> InferenceError {
    InferenceError::Integrity(message.into())
}
// GGML serialized block layouts, pinned to llama.cpp b11205. Reject unknown
// layouts rather than claiming a truncated or unsupported tensor is installed.
fn tensor_size(kind: u32, elements: u64) -> Result<u64, InferenceError> {
    let (block, bytes) = match kind {
        0 => (1, 4),
        1 | 30 => (1, 2),
        2 | 20 => (32, 18),
        3 => (32, 20),
        6 => (32, 22),
        7 => (32, 24),
        8 => (32, 34),
        10 => (256, 84),
        11 => (256, 110),
        12 => (256, 144),
        13 => (256, 176),
        14 => (256, 210),
        15 => (256, 292),
        16 => (256, 66),
        17 => (256, 74),
        18 => (256, 98),
        19 => (256, 50),
        21 => (256, 110),
        22 => (256, 82),
        23 => (256, 136),
        24 => (1, 1),
        25 => (1, 2),
        26 => (1, 4),
        27 | 28 => (1, 8),
        29 => (256, 56),
        _ => return Err(bad("unsupported GGUF tensor layout")),
    };
    if elements.checked_rem(block) != Some(0) {
        return Err(bad("GGUF tensor shape does not fit quantization block"));
    }
    elements
        .checked_div(block)
        .and_then(|n| n.checked_mul(bytes))
        .ok_or_else(|| bad("GGUF tensor size overflow"))
}
pub(crate) fn inspect(path: &Path) -> Result<Header, InferenceError> {
    let size = files::regular(path)?.len();
    let mut r = Reader {
        file: File::open(path)?,
        size,
    };
    if r.bytes::<4>()? != *b"GGUF" {
        return Err(bad("file is not GGUF"));
    }
    let version = r.u32()?;
    if !matches!(version, 2 | 3) {
        return Err(bad("unsupported GGUF version"));
    }
    let tensors = r.u64()?;
    let metadata = r.u64()?;
    if tensors == 0 || tensors > 1_000_000 || metadata > 1_000_000 {
        return Err(bad("invalid GGUF counts"));
    }
    let mut context = None;
    let mut alignment = 32u64;
    for _ in 0..metadata {
        let key = r.string()?;
        let t = r.u32()?;
        let value = r.value(t, 0)?;
        if key.ends_with(".context_length") {
            context = value.and_then(|v| u32::try_from(v).ok()).filter(|v| *v > 0);
        }
        if key == "general.alignment" {
            alignment = value.ok_or_else(|| bad("invalid GGUF alignment"))?;
        }
    }
    if alignment == 0 || alignment > 4096 || !alignment.is_power_of_two() {
        return Err(bad("invalid GGUF alignment"));
    }
    let mut offsets = Vec::new();
    for _ in 0..tensors {
        let name = r.string()?;
        if name.is_empty() {
            return Err(bad("empty GGUF tensor name"));
        }
        let dims = r.u32()?;
        if !(1..=4).contains(&dims) {
            return Err(bad("invalid GGUF tensor dimensions"));
        }
        let mut elements = 1u64;
        for _ in 0..dims {
            let dim = r.u64()?;
            elements = elements
                .checked_mul(dim)
                .filter(|v| *v > 0)
                .ok_or_else(|| bad("invalid GGUF tensor shape"))?;
        }
        let kind = r.u32()?;
        let length = tensor_size(kind, elements)?;
        let offset = r.u64()?;
        if offset % alignment != 0 {
            return Err(bad("unaligned GGUF tensor offset"));
        }
        offsets.push((offset, length));
    }
    let pos = r.file.stream_position()?;
    let data = pos
        .checked_add(alignment - 1)
        .ok_or_else(|| bad("GGUF offset overflow"))?
        / alignment
        * alignment;
    if offsets.iter().any(|(off, len)| {
        data.checked_add(*off)
            .and_then(|v| v.checked_add(*len))
            .is_none_or(|v| v > size)
    }) {
        return Err(bad("GGUF tensor data is missing or truncated"));
    }
    offsets.sort_unstable();
    for pair in offsets.windows(2) {
        if pair[0]
            .0
            .checked_add(pair[0].1)
            .is_none_or(|end| end > pair[1].0)
        {
            return Err(bad("GGUF tensor payloads overlap"));
        }
    }
    Ok(Header { version, context })
}
