//! GGUF header parsing without loading tensor data.
//!
//! Reads the magic, version (2 or 3, little-endian), metadata key/value pairs,
//! and tensor descriptors, with every count and length bounded before anything
//! is allocated. Large arrays (tokenizer vocabularies) are skipped by streaming;
//! only their element count is kept. When every tensor type is known the end of
//! the tensor data is computed, so a truncated download is detected without
//! hashing.

mod builder;
mod tables;

pub use builder::{GGML_TYPE_F16, GGML_TYPE_F32, GGML_TYPE_Q4_K, GgufBuilder};
pub(crate) use tables::is_known_architecture;

use std::{
    collections::BTreeMap,
    fmt,
    fs::File,
    io::{self, BufReader, Read},
    path::Path,
};

use serde::{Deserialize, Serialize};

/// `GGUF` in file order.
const MAGIC: [u8; 4] = *b"GGUF";
/// Metadata pairs accepted (real models carry tens to a few hundred).
const MAX_KV_COUNT: u64 = 65_536;
/// Tensors accepted (large MoE models carry tens of thousands).
const MAX_TENSOR_COUNT: u64 = 1 << 20;
/// Longest key (the GGUF specification limit).
const MAX_KEY_LEN: u64 = 65_535;
/// Longest string value read into memory.
const MAX_KEPT_STRING: u64 = 4096;
/// Longest chat template inspected.
const MAX_TEMPLATE: u64 = 1024 * 1024;
/// Longest string value accepted at all (skipped when not kept).
const MAX_STRING_LEN: u64 = 256 * 1024 * 1024;
/// Most elements accepted in one array.
const MAX_ARRAY_LEN: u64 = 1 << 26;
/// Deepest nesting of arrays.
const MAX_ARRAY_DEPTH: u32 = 3;
/// Longest tensor name.
const MAX_TENSOR_NAME: u64 = 4096;
/// `GGML_MAX_DIMS`.
const MAX_DIMS: u32 = 4;
/// Largest header (metadata + tensor descriptors) read.
const MAX_HEADER_BYTES: u64 = 1 << 30;
/// Values kept in [`GgufMetadata::values`].
const MAX_KEPT_VALUES: usize = 1024;
/// Default `general.alignment`.
const DEFAULT_ALIGNMENT: u64 = 32;

/// Why a file is not a readable GGUF model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GgufError {
    Io(String),
    NotGguf,
    /// The header is byte-swapped (a big-endian GGUF); the runtime cannot load it.
    BigEndian,
    UnsupportedVersion(u32),
    /// The file ended inside the header or before the tensor data ends.
    Truncated {
        offset: u64,
    },
    /// A count or length exceeds AiDEX's bounds.
    TooLarge {
        what: &'static str,
        value: u64,
    },
    InvalidUtf8 {
        offset: u64,
    },
    InvalidValueType {
        offset: u64,
        value: u32,
    },
    /// A required key is missing or has the wrong type.
    MissingKey(&'static str),
    Malformed(String),
}

impl fmt::Display for GgufError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => write!(f, "cannot read the file: {message}"),
            Self::NotGguf => f.write_str("the file does not start with the GGUF magic"),
            Self::BigEndian => f.write_str("big-endian GGUF files are not supported"),
            Self::UnsupportedVersion(version) => {
                write!(f, "GGUF version {version} is not supported (2 and 3 are)")
            }
            Self::Truncated { offset } => write!(f, "the file is truncated at byte {offset}"),
            Self::TooLarge { what, value } => write!(f, "{what} of {value} exceeds the limit"),
            Self::InvalidUtf8 { offset } => write!(f, "invalid UTF-8 string at byte {offset}"),
            Self::InvalidValueType { offset, value } => {
                write!(f, "unknown metadata value type {value} at byte {offset}")
            }
            Self::MissingKey(key) => write!(f, "required metadata `{key}` is missing"),
            Self::Malformed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for GgufError {}

/// A metadata value as kept by the parser.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum GgufValue {
    Unsigned(u64),
    Signed(i64),
    Float(f64),
    Bool(bool),
    Text(String),
    /// A string longer than the kept limit (its length).
    LongText(u64),
    /// An array (element type code and count); elements are not kept.
    Array {
        element_type: u32,
        len: u64,
    },
}

impl GgufValue {
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Unsigned(value) => Some(*value),
            Self::Signed(value) => u64::try_from(*value).ok(),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }
}

/// What AiDEX needs to know about a GGUF model, read from its header only.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GgufMetadata {
    pub version: u32,
    pub tensor_count: u64,
    pub kv_count: u64,
    pub architecture: String,
    pub name: Option<String>,
    pub basename: Option<String>,
    pub size_label: Option<String>,
    pub license: Option<String>,
    pub file_type: Option<u32>,
    /// Quantization label derived from `general.file_type` (for example `Q4_K_M`).
    pub quantization: Option<String>,
    /// `<arch>.context_length` (the training context).
    pub context_length: Option<u64>,
    pub block_count: Option<u64>,
    pub embedding_length: Option<u64>,
    pub head_count: Option<u64>,
    pub head_count_kv: Option<u64>,
    pub key_length: Option<u64>,
    pub value_length: Option<u64>,
    /// Number of tokens in `tokenizer.ggml.tokens`.
    pub vocab_size: Option<u64>,
    pub has_chat_template: bool,
    /// The chat template references `tools` (native tool-call templates).
    pub chat_template_mentions_tools: bool,
    pub alignment: u64,
    /// File offset where tensor data starts.
    pub data_offset: u64,
    /// File offset where the last tensor ends, when every tensor type is known.
    pub tensor_data_end: Option<u64>,
    /// Scalar and short string values by key (arrays are summarized).
    pub values: BTreeMap<String, GgufValue>,
}

impl GgufMetadata {
    /// The quantization label, else `F<file_type>`-style fallback.
    pub fn quantization_label(&self) -> Option<&str> {
        self.quantization.as_deref()
    }

    /// Bytes of K+V cache per context token at f16, from the attention shape.
    /// Falls back to `embedding_length` per head when grouped-query counts are absent.
    pub fn kv_bytes_per_token(&self) -> Option<u64> {
        let layers = self.block_count?;
        let embedding = self.embedding_length?;
        let heads = self.head_count.filter(|heads| *heads > 0).unwrap_or(1);
        let kv_heads = self
            .head_count_kv
            .filter(|heads| *heads > 0)
            .unwrap_or(heads);
        let head_dim = embedding / heads;
        let key = self.key_length.unwrap_or(head_dim) * kv_heads;
        let value = self.value_length.unwrap_or(head_dim) * kv_heads;
        layers.checked_mul(key.checked_add(value)?)?.checked_mul(2)
    }
}

/// Parse the header of the GGUF file at `path`.
pub fn read_metadata(path: &Path) -> Result<GgufMetadata, GgufError> {
    let file = File::open(path).map_err(|error| GgufError::Io(error.kind().to_string()))?;
    let file_len = file
        .metadata()
        .map_err(|error| GgufError::Io(error.kind().to_string()))?
        .len();
    parse(BufReader::with_capacity(256 * 1024, file), Some(file_len))
}

/// Parse a GGUF header from `reader`. With `file_len`, tensor descriptors are
/// checked against the file size.
pub fn parse(reader: impl Read, file_len: Option<u64>) -> Result<GgufMetadata, GgufError> {
    Parser {
        reader,
        offset: 0,
        kept: 0,
    }
    .run(file_len)
}

struct Parser<R> {
    reader: R,
    offset: u64,
    kept: usize,
}

/// Value type codes.
const T_U8: u32 = 0;
const T_I8: u32 = 1;
const T_U16: u32 = 2;
const T_I16: u32 = 3;
const T_U32: u32 = 4;
const T_I32: u32 = 5;
const T_F32: u32 = 6;
const T_BOOL: u32 = 7;
const T_STRING: u32 = 8;
const T_ARRAY: u32 = 9;
const T_U64: u32 = 10;
const T_I64: u32 = 11;
const T_F64: u32 = 12;

impl<R: Read> Parser<R> {
    fn run(mut self, file_len: Option<u64>) -> Result<GgufMetadata, GgufError> {
        let mut magic = [0u8; 4];
        self.exact(&mut magic)?;
        if magic != MAGIC {
            return Err(GgufError::NotGguf);
        }
        let version = self.u32()?;
        match version {
            2 | 3 => {}
            other if other.swap_bytes() == 2 || other.swap_bytes() == 3 => {
                return Err(GgufError::BigEndian);
            }
            other => return Err(GgufError::UnsupportedVersion(other)),
        }
        let tensor_count = self.u64()?;
        if tensor_count > MAX_TENSOR_COUNT {
            return Err(GgufError::TooLarge {
                what: "tensor count",
                value: tensor_count,
            });
        }
        let kv_count = self.u64()?;
        if kv_count > MAX_KV_COUNT {
            return Err(GgufError::TooLarge {
                what: "metadata count",
                value: kv_count,
            });
        }

        let mut values = BTreeMap::new();
        let mut has_chat_template = false;
        let mut template_tools = false;
        let mut vocab_size = None;
        for _ in 0..kv_count {
            let key = self.string(MAX_KEY_LEN, "metadata key")?;
            let value_type = self.u32()?;
            if key == "tokenizer.chat_template" && value_type == T_STRING {
                has_chat_template = true;
                let length = self.length(MAX_STRING_LEN, "chat template")?;
                if length <= MAX_TEMPLATE {
                    let text = self.utf8(length)?;
                    template_tools = text.contains("tools");
                } else {
                    self.skip(length)?;
                }
                continue;
            }
            let value = self.value(value_type, 0)?;
            if key == "tokenizer.ggml.tokens" {
                if let GgufValue::Array { len, .. } = value {
                    vocab_size = Some(len);
                }
            }
            if self.kept < MAX_KEPT_VALUES {
                self.kept += 1;
                values.insert(key, value);
            }
            self.check_header_size()?;
        }

        let text = |key: &str| {
            values
                .get(key)
                .and_then(GgufValue::as_str)
                .map(str::to_owned)
        };
        let architecture = text("general.architecture")
            .filter(|architecture| !architecture.is_empty())
            .ok_or(GgufError::MissingKey("general.architecture"))?;
        let number = |suffix: &str| {
            values
                .get(&format!("{architecture}.{suffix}"))
                .and_then(GgufValue::as_u64)
        };
        let alignment = match values.get("general.alignment") {
            None => DEFAULT_ALIGNMENT,
            Some(value) => value
                .as_u64()
                .filter(|alignment| {
                    *alignment > 0 && alignment.is_power_of_two() && *alignment <= 1 << 16
                })
                .ok_or_else(|| {
                    GgufError::Malformed("general.alignment must be a power of two".into())
                })?,
        };
        let file_type = values
            .get("general.file_type")
            .and_then(GgufValue::as_u64)
            .and_then(|value| u32::try_from(value).ok());

        let mut tensor_ends = Some(0u64);
        let mut tensors = Vec::with_capacity(usize::try_from(tensor_count.min(4096)).unwrap_or(0));
        for _ in 0..tensor_count {
            let _name = self.string(MAX_TENSOR_NAME, "tensor name")?;
            let dims = self.u32()?;
            if dims == 0 || dims > MAX_DIMS {
                return Err(GgufError::Malformed(format!(
                    "tensor has {dims} dimensions (1-{MAX_DIMS} allowed)"
                )));
            }
            let mut elements = 1u64;
            for _ in 0..dims {
                let dim = self.u64()?;
                elements = elements.checked_mul(dim).ok_or(GgufError::TooLarge {
                    what: "tensor element count",
                    value: dim,
                })?;
            }
            let tensor_type = self.u32()?;
            let offset = self.u64()?;
            if offset % alignment != 0 {
                return Err(GgufError::Malformed(format!(
                    "tensor data offset {offset} is not aligned to {alignment}"
                )));
            }
            tensors.push((elements, tensor_type, offset));
            self.check_header_size()?;
        }
        let data_offset = self.offset.div_ceil(alignment) * alignment;
        for (elements, tensor_type, offset) in tensors {
            let Some((block, bytes)) = tables::tensor_type_layout(tensor_type) else {
                tensor_ends = None;
                continue;
            };
            if elements % block != 0 {
                return Err(GgufError::Malformed(format!(
                    "tensor of {elements} elements does not fill blocks of {block}"
                )));
            }
            let size = (elements / block)
                .checked_mul(bytes)
                .ok_or(GgufError::TooLarge {
                    what: "tensor size",
                    value: elements,
                })?;
            let end = data_offset
                .checked_add(offset)
                .and_then(|start| start.checked_add(size))
                .ok_or(GgufError::TooLarge {
                    what: "tensor offset",
                    value: offset,
                })?;
            tensor_ends = tensor_ends.map(|current| current.max(end));
        }
        if let (Some(len), Some(end)) = (file_len, tensor_ends) {
            if end > len {
                return Err(GgufError::Truncated { offset: len });
            }
        }
        if let Some(len) = file_len {
            if data_offset > len && tensor_count > 0 {
                return Err(GgufError::Truncated { offset: len });
            }
        }

        Ok(GgufMetadata {
            version,
            tensor_count,
            kv_count,
            name: text("general.name"),
            basename: text("general.basename"),
            size_label: text("general.size_label"),
            license: text("general.license"),
            quantization: file_type
                .and_then(tables::file_type_label)
                .map(str::to_owned),
            file_type,
            context_length: number("context_length"),
            block_count: number("block_count"),
            embedding_length: number("embedding_length"),
            head_count: number("attention.head_count"),
            head_count_kv: number("attention.head_count_kv"),
            key_length: number("attention.key_length"),
            value_length: number("attention.value_length"),
            vocab_size,
            has_chat_template,
            chat_template_mentions_tools: template_tools,
            alignment,
            data_offset,
            tensor_data_end: tensor_ends.filter(|_| tensor_count > 0),
            architecture,
            values,
        })
    }

    fn check_header_size(&self) -> Result<(), GgufError> {
        if self.offset > MAX_HEADER_BYTES {
            return Err(GgufError::TooLarge {
                what: "header size",
                value: self.offset,
            });
        }
        Ok(())
    }

    fn exact(&mut self, buffer: &mut [u8]) -> Result<(), GgufError> {
        match self.reader.read_exact(buffer) {
            Ok(()) => {
                self.offset += buffer.len() as u64;
                Ok(())
            }
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                Err(GgufError::Truncated {
                    offset: self.offset,
                })
            }
            Err(error) => Err(GgufError::Io(error.kind().to_string())),
        }
    }

    fn skip(&mut self, length: u64) -> Result<(), GgufError> {
        let copied = io::copy(&mut (&mut self.reader).take(length), &mut io::sink())
            .map_err(|error| GgufError::Io(error.kind().to_string()))?;
        self.offset += copied;
        if copied < length {
            return Err(GgufError::Truncated {
                offset: self.offset,
            });
        }
        Ok(())
    }

    fn u8(&mut self) -> Result<u8, GgufError> {
        let mut bytes = [0u8; 1];
        self.exact(&mut bytes)?;
        Ok(bytes[0])
    }

    fn u32(&mut self) -> Result<u32, GgufError> {
        let mut bytes = [0u8; 4];
        self.exact(&mut bytes)?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, GgufError> {
        let mut bytes = [0u8; 8];
        self.exact(&mut bytes)?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn length(&mut self, max: u64, what: &'static str) -> Result<u64, GgufError> {
        let length = self.u64()?;
        if length > max {
            return Err(GgufError::TooLarge {
                what,
                value: length,
            });
        }
        Ok(length)
    }

    fn utf8(&mut self, length: u64) -> Result<String, GgufError> {
        let start = self.offset;
        let size = usize::try_from(length).map_err(|_| GgufError::TooLarge {
            what: "string length",
            value: length,
        })?;
        let mut bytes = vec![0u8; size];
        self.exact(&mut bytes)?;
        String::from_utf8(bytes).map_err(|_| GgufError::InvalidUtf8 { offset: start })
    }

    fn string(&mut self, max: u64, what: &'static str) -> Result<String, GgufError> {
        let length = self.length(max, what)?;
        self.utf8(length)
    }

    fn value(&mut self, value_type: u32, depth: u32) -> Result<GgufValue, GgufError> {
        let mut bytes4 = [0u8; 4];
        let mut bytes8 = [0u8; 8];
        let mut bytes2 = [0u8; 2];
        Ok(match value_type {
            T_U8 => GgufValue::Unsigned(u64::from(self.u8()?)),
            T_I8 => GgufValue::Signed(i64::from(self.u8()? as i8)),
            T_U16 => {
                self.exact(&mut bytes2)?;
                GgufValue::Unsigned(u64::from(u16::from_le_bytes(bytes2)))
            }
            T_I16 => {
                self.exact(&mut bytes2)?;
                GgufValue::Signed(i64::from(i16::from_le_bytes(bytes2)))
            }
            T_U32 => GgufValue::Unsigned(u64::from(self.u32()?)),
            T_I32 => {
                self.exact(&mut bytes4)?;
                GgufValue::Signed(i64::from(i32::from_le_bytes(bytes4)))
            }
            T_F32 => {
                self.exact(&mut bytes4)?;
                GgufValue::Float(f64::from(f32::from_le_bytes(bytes4)))
            }
            T_BOOL => match self.u8()? {
                0 => GgufValue::Bool(false),
                1 => GgufValue::Bool(true),
                other => {
                    return Err(GgufError::Malformed(format!(
                        "boolean value {other} at byte {}",
                        self.offset - 1
                    )));
                }
            },
            T_STRING => {
                let length = self.length(MAX_STRING_LEN, "string length")?;
                if length <= MAX_KEPT_STRING {
                    GgufValue::Text(self.utf8(length)?)
                } else {
                    self.skip(length)?;
                    GgufValue::LongText(length)
                }
            }
            T_ARRAY => {
                if depth >= MAX_ARRAY_DEPTH {
                    return Err(GgufError::TooLarge {
                        what: "array nesting",
                        value: u64::from(depth + 1),
                    });
                }
                let element_type = self.u32()?;
                let len = self.length(MAX_ARRAY_LEN, "array length")?;
                self.skip_elements(element_type, len, depth + 1)?;
                GgufValue::Array { element_type, len }
            }
            T_U64 => GgufValue::Unsigned(self.u64()?),
            T_I64 => {
                self.exact(&mut bytes8)?;
                GgufValue::Signed(i64::from_le_bytes(bytes8))
            }
            T_F64 => {
                self.exact(&mut bytes8)?;
                GgufValue::Float(f64::from_le_bytes(bytes8))
            }
            other => {
                return Err(GgufError::InvalidValueType {
                    offset: self.offset - 4,
                    value: other,
                });
            }
        })
    }

    fn skip_elements(&mut self, element_type: u32, len: u64, depth: u32) -> Result<(), GgufError> {
        let fixed = match element_type {
            T_U8 | T_I8 | T_BOOL => Some(1),
            T_U16 | T_I16 => Some(2),
            T_U32 | T_I32 | T_F32 => Some(4),
            T_U64 | T_I64 | T_F64 => Some(8),
            _ => None,
        };
        if let Some(size) = fixed {
            let total = len.checked_mul(size).ok_or(GgufError::TooLarge {
                what: "array size",
                value: len,
            })?;
            self.skip(total)?;
            return self.check_header_size();
        }
        match element_type {
            T_STRING => {
                for _ in 0..len {
                    let length = self.length(MAX_STRING_LEN, "string length")?;
                    self.skip(length)?;
                    self.check_header_size()?;
                }
                Ok(())
            }
            T_ARRAY => {
                for _ in 0..len {
                    self.value(T_ARRAY, depth)?;
                }
                Ok(())
            }
            other => Err(GgufError::InvalidValueType {
                offset: self.offset,
                value: other,
            }),
        }
    }
}

#[cfg(test)]
mod tests;
