//! A writer for small synthetic GGUF files (tests, fixtures, and tools).
//!
//! It produces structurally valid little-endian GGUF v2/v3 files with arbitrary
//! metadata and zero-filled tensors; nothing here resembles real weights.

use std::{fs, io, path::Path};

use super::tables::tensor_type_layout;

/// ggml tensor type codes used by [`GgufBuilder::tensor`].
pub const GGML_TYPE_F32: u32 = 0;
pub const GGML_TYPE_F16: u32 = 1;
pub const GGML_TYPE_Q4_K: u32 = 12;

#[derive(Clone, Debug)]
pub struct GgufBuilder {
    version: u32,
    alignment: u64,
    kv: Vec<(String, u32, Vec<u8>)>,
    tensors: Vec<(String, Vec<u64>, u32)>,
}

fn string_bytes(text: &str) -> Vec<u8> {
    let mut bytes = (text.len() as u64).to_le_bytes().to_vec();
    bytes.extend_from_slice(text.as_bytes());
    bytes
}

impl GgufBuilder {
    /// A v3 file with `general.architecture` set.
    pub fn new(architecture: &str) -> Self {
        Self {
            version: 3,
            alignment: 32,
            kv: Vec::new(),
            tensors: Vec::new(),
        }
        .string("general.architecture", architecture)
    }

    /// A small chat model shape: 2 layers, 64-wide embeddings, grouped-query
    /// attention, a tool-aware chat template, a 16-token vocabulary, Q4_K_M
    /// file type, and one F32 tensor.
    pub fn chat_model(architecture: &str, name: &str) -> Self {
        let tokens: Vec<String> = (0..16).map(|index| format!("tok{index}")).collect();
        let tokens: Vec<&str> = tokens.iter().map(String::as_str).collect();
        Self::new(architecture)
            .string("general.name", name)
            .string("general.license", "apache-2.0")
            .u32("general.file_type", 15)
            .u32(&format!("{architecture}.context_length"), 4096)
            .u32(&format!("{architecture}.block_count"), 2)
            .u32(&format!("{architecture}.embedding_length"), 64)
            .u32(&format!("{architecture}.attention.head_count"), 4)
            .u32(&format!("{architecture}.attention.head_count_kv"), 2)
            .string(
                "tokenizer.chat_template",
                "{% for message in messages %}{{ message.content }}{% endfor %}{% if tools %}{{ tools }}{% endif %}",
            )
            .string_array("tokenizer.ggml.tokens", &tokens)
            .tensor("token_embd.weight", &[64, 16], GGML_TYPE_F32)
    }

    #[must_use]
    pub fn version(mut self, version: u32) -> Self {
        self.version = version;
        self
    }

    #[must_use]
    pub fn alignment(mut self, alignment: u32) -> Self {
        self.alignment = u64::from(alignment);
        self.u32("general.alignment", alignment)
    }

    fn push(mut self, key: &str, value_type: u32, bytes: Vec<u8>) -> Self {
        self.kv.retain(|(existing, ..)| existing != key);
        self.kv.push((key.to_owned(), value_type, bytes));
        self
    }

    #[must_use]
    pub fn string(self, key: &str, value: &str) -> Self {
        self.push(key, 8, string_bytes(value))
    }

    #[must_use]
    pub fn u32(self, key: &str, value: u32) -> Self {
        self.push(key, 4, value.to_le_bytes().to_vec())
    }

    #[must_use]
    pub fn u64(self, key: &str, value: u64) -> Self {
        self.push(key, 10, value.to_le_bytes().to_vec())
    }

    #[must_use]
    pub fn f32(self, key: &str, value: f32) -> Self {
        self.push(key, 6, value.to_le_bytes().to_vec())
    }

    #[must_use]
    pub fn bool(self, key: &str, value: bool) -> Self {
        self.push(key, 7, vec![u8::from(value)])
    }

    #[must_use]
    pub fn string_array(self, key: &str, values: &[&str]) -> Self {
        let mut bytes = 8u32.to_le_bytes().to_vec();
        bytes.extend_from_slice(&(values.len() as u64).to_le_bytes());
        for value in values {
            bytes.extend_from_slice(&string_bytes(value));
        }
        self.push(key, 9, bytes)
    }

    #[must_use]
    pub fn u32_array(self, key: &str, values: &[u32]) -> Self {
        let mut bytes = 4u32.to_le_bytes().to_vec();
        bytes.extend_from_slice(&(values.len() as u64).to_le_bytes());
        for value in values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        self.push(key, 9, bytes)
    }

    /// Drop a key (for example to build a file without `general.architecture`).
    #[must_use]
    pub fn without(mut self, key: &str) -> Self {
        self.kv.retain(|(existing, ..)| existing != key);
        self
    }

    /// A zero-filled tensor of ggml `tensor_type`.
    #[must_use]
    pub fn tensor(mut self, name: &str, dims: &[u64], tensor_type: u32) -> Self {
        self.tensors
            .push((name.to_owned(), dims.to_vec(), tensor_type));
        self
    }

    /// The header (metadata and tensor descriptors) only.
    fn header(&self) -> (Vec<u8>, Vec<u64>) {
        let mut out = b"GGUF".to_vec();
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&(self.tensors.len() as u64).to_le_bytes());
        out.extend_from_slice(&(self.kv.len() as u64).to_le_bytes());
        for (key, value_type, bytes) in &self.kv {
            out.extend_from_slice(&string_bytes(key));
            out.extend_from_slice(&value_type.to_le_bytes());
            out.extend_from_slice(bytes);
        }
        let mut sizes = Vec::new();
        let mut offset = 0u64;
        for (name, dims, tensor_type) in &self.tensors {
            out.extend_from_slice(&string_bytes(name));
            out.extend_from_slice(&(dims.len() as u32).to_le_bytes());
            for dim in dims {
                out.extend_from_slice(&dim.to_le_bytes());
            }
            out.extend_from_slice(&tensor_type.to_le_bytes());
            out.extend_from_slice(&offset.to_le_bytes());
            let elements: u64 = dims.iter().product();
            let (block, bytes) = tensor_type_layout(*tensor_type).unwrap_or((1, 1));
            let size = elements.div_ceil(block) * bytes;
            sizes.push(size);
            offset += size.div_ceil(self.alignment) * self.alignment;
        }
        (out, sizes)
    }

    /// The complete file: header, alignment padding, and zeroed tensor data.
    pub fn build(&self) -> Vec<u8> {
        let (mut out, sizes) = self.header();
        let padded = |len: u64| len.div_ceil(self.alignment) * self.alignment;
        let data_start = padded(out.len() as u64);
        out.resize(data_start as usize, 0);
        for size in sizes {
            let end = out.len() as u64 + padded(size);
            out.resize(end as usize, 0);
        }
        out
    }

    pub fn write_to(&self, path: &Path) -> io::Result<()> {
        fs::write(path, self.build())
    }
}
