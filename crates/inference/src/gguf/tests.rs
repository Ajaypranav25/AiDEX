use super::*;

fn parse_bytes(bytes: &[u8]) -> Result<GgufMetadata, GgufError> {
    parse(bytes, Some(bytes.len() as u64))
}

#[test]
fn synthetic_chat_model_metadata_is_extracted() {
    let bytes = GgufBuilder::chat_model("qwen2", "Tiny Coder").build();
    let metadata = parse_bytes(&bytes).unwrap();
    assert_eq!(metadata.version, 3);
    assert_eq!(metadata.architecture, "qwen2");
    assert_eq!(metadata.name.as_deref(), Some("Tiny Coder"));
    assert_eq!(metadata.license.as_deref(), Some("apache-2.0"));
    assert_eq!(metadata.quantization.as_deref(), Some("Q4_K_M"));
    assert_eq!(metadata.context_length, Some(4096));
    assert_eq!(metadata.block_count, Some(2));
    assert_eq!(metadata.embedding_length, Some(64));
    assert_eq!(metadata.vocab_size, Some(16));
    assert!(metadata.has_chat_template);
    assert!(metadata.chat_template_mentions_tools);
    assert_eq!(metadata.tensor_count, 1);
    // 2 layers * (2 kv heads * 16 dims) * (K + V) * 2 bytes.
    assert_eq!(metadata.kv_bytes_per_token(), Some(2 * 32 * 2 * 2));
    assert_eq!(
        metadata.tensor_data_end,
        Some(metadata.data_offset + 64 * 16 * 4)
    );
    assert!(!metadata.values.contains_key("tokenizer.chat_template"));
    assert_eq!(
        metadata.values["tokenizer.ggml.tokens"],
        GgufValue::Array {
            element_type: 8,
            len: 16
        }
    );
}

#[test]
fn version_two_is_accepted_and_others_are_refused() {
    let v2 = GgufBuilder::chat_model("llama", "v2").version(2).build();
    assert_eq!(parse_bytes(&v2).unwrap().version, 2);
    for version in [0, 1, 4, 99] {
        let bytes = GgufBuilder::chat_model("llama", "x")
            .version(version)
            .build();
        assert_eq!(
            parse_bytes(&bytes).unwrap_err(),
            GgufError::UnsupportedVersion(version)
        );
    }
    let swapped = GgufBuilder::chat_model("llama", "x")
        .version(3u32.swap_bytes())
        .build();
    assert_eq!(parse_bytes(&swapped).unwrap_err(), GgufError::BigEndian);
}

#[test]
fn non_gguf_and_missing_architecture_are_refused() {
    assert_eq!(parse_bytes(b"GGML1234").unwrap_err(), GgufError::NotGguf);
    assert_eq!(
        parse_bytes(b"GG").unwrap_err(),
        GgufError::Truncated { offset: 0 }
    );
    let bytes = GgufBuilder::chat_model("llama", "x")
        .without("general.architecture")
        .build();
    assert_eq!(
        parse_bytes(&bytes).unwrap_err(),
        GgufError::MissingKey("general.architecture")
    );
}

#[test]
fn every_truncation_of_a_header_is_an_error_not_a_panic() {
    let bytes = GgufBuilder::chat_model("qwen2", "truncate me").build();
    let header_end = parse_bytes(&bytes).unwrap().data_offset as usize;
    for cut in 0..bytes.len() {
        let result = parse_bytes(&bytes[..cut]);
        assert!(
            result.is_err(),
            "a file cut at {cut} of {} parsed",
            bytes.len()
        );
        if cut < header_end.saturating_sub(32) {
            assert!(
                matches!(result, Err(GgufError::Truncated { .. })),
                "cut at {cut}: {result:?}"
            );
        }
    }
}

#[test]
fn corrupted_bytes_never_panic_or_allocate_unboundedly() {
    let bytes = GgufBuilder::chat_model("qwen2", "fuzz").build();
    // Deterministic xorshift so failures are reproducible.
    let mut state = 0x2545_f491_4f6c_dd1du64;
    for _ in 0..2000 {
        let mut mutated = bytes.clone();
        for _ in 0..4 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let index = (state as usize) % mutated.len();
            mutated[index] = (state >> 32) as u8;
        }
        let _ = parse_bytes(&mutated);
    }
}

#[test]
fn absurd_counts_and_lengths_are_rejected_before_allocation() {
    let mut header = b"GGUF".to_vec();
    header.extend_from_slice(&3u32.to_le_bytes());
    header.extend_from_slice(&u64::MAX.to_le_bytes());
    header.extend_from_slice(&1u64.to_le_bytes());
    assert!(matches!(
        parse_bytes(&header).unwrap_err(),
        GgufError::TooLarge {
            what: "tensor count",
            ..
        }
    ));

    let mut header = b"GGUF".to_vec();
    header.extend_from_slice(&3u32.to_le_bytes());
    header.extend_from_slice(&0u64.to_le_bytes());
    header.extend_from_slice(&1u64.to_le_bytes());
    header.extend_from_slice(&(1u64 << 40).to_le_bytes()); // key length
    assert!(matches!(
        parse_bytes(&header).unwrap_err(),
        GgufError::TooLarge {
            what: "metadata key",
            ..
        }
    ));

    let mut header = b"GGUF".to_vec();
    header.extend_from_slice(&3u32.to_le_bytes());
    header.extend_from_slice(&0u64.to_le_bytes());
    header.extend_from_slice(&1u64.to_le_bytes());
    header.extend_from_slice(&3u64.to_le_bytes());
    header.extend_from_slice(b"key");
    header.extend_from_slice(&9u32.to_le_bytes()); // array
    header.extend_from_slice(&8u32.to_le_bytes()); // of strings
    header.extend_from_slice(&u64::MAX.to_le_bytes());
    assert!(matches!(
        parse_bytes(&header).unwrap_err(),
        GgufError::TooLarge {
            what: "array length",
            ..
        }
    ));
}

#[test]
fn tensor_data_beyond_the_file_is_detected_as_truncation() {
    let bytes = GgufBuilder::chat_model("llama", "big")
        .tensor("blk.0.attn_q.weight", &[256, 64], GGML_TYPE_Q4_K)
        .build();
    let metadata = parse_bytes(&bytes).unwrap();
    assert_eq!(
        metadata.tensor_data_end.unwrap() as usize,
        bytes.len(),
        "the last tensor ends at the end of the file"
    );
    let cut = &bytes[..bytes.len() - 100];
    assert!(matches!(
        parse(cut, Some(cut.len() as u64)).unwrap_err(),
        GgufError::Truncated { .. }
    ));
    // Without a file length only the header is checked.
    assert!(parse(cut, None).is_ok());
}

#[test]
fn unknown_tensor_types_skip_the_size_check() {
    let bytes = GgufBuilder::chat_model("llama", "odd")
        .tensor("odd", &[32], 250)
        .build();
    assert_eq!(parse_bytes(&bytes).unwrap().tensor_data_end, None);
}

#[test]
fn invalid_value_types_and_bad_alignment_are_refused() {
    let mut bytes = b"GGUF".to_vec();
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&0u64.to_le_bytes());
    bytes.extend_from_slice(&1u64.to_le_bytes());
    bytes.extend_from_slice(&1u64.to_le_bytes());
    bytes.extend_from_slice(b"k");
    bytes.extend_from_slice(&42u32.to_le_bytes());
    assert!(matches!(
        parse_bytes(&bytes).unwrap_err(),
        GgufError::InvalidValueType { value: 42, .. }
    ));
    let misaligned = GgufBuilder::chat_model("llama", "x")
        .u32("general.alignment", 3)
        .build();
    assert!(matches!(
        parse_bytes(&misaligned).unwrap_err(),
        GgufError::Malformed(_)
    ));
}

#[test]
fn custom_keys_and_long_strings_are_summarized() {
    let long = "x".repeat(10_000);
    let bytes = GgufBuilder::chat_model("llama", "x")
        .u64("aidex.fake.token_delay_ms", 25)
        .bool("aidex.fake.flag", true)
        .string("general.description", &long)
        .build();
    let metadata = parse_bytes(&bytes).unwrap();
    assert_eq!(
        metadata.values["aidex.fake.token_delay_ms"].as_u64(),
        Some(25)
    );
    assert_eq!(metadata.values["aidex.fake.flag"], GgufValue::Bool(true));
    assert_eq!(
        metadata.values["general.description"],
        GgufValue::LongText(10_000)
    );
}

/// Real-model check (run with `AIDEX_TEST_GGUF=<file> cargo test -p aidex-inference -- --ignored`).
#[test]
#[ignore = "needs AIDEX_TEST_GGUF (a real GGUF model)"]
fn staged_real_model_header_parses_and_tensor_sizes_match_the_file() {
    let Some(path) = std::env::var_os("AIDEX_TEST_GGUF") else {
        panic!("set AIDEX_TEST_GGUF to a GGUF model file");
    };
    let path = std::path::PathBuf::from(path);
    let started = std::time::Instant::now();
    let metadata = read_metadata(&path).unwrap();
    let file_len = std::fs::metadata(&path).unwrap().len();
    println!(
        "parsed {} in {:?}: arch {} name {:?} quant {:?} ctx {:?} layers {:?} emb {:?} heads {:?}/{:?} vocab {:?} template {} tools {} kv/token {:?} data {}..{:?} of {file_len}",
        path.display(),
        started.elapsed(),
        metadata.architecture,
        metadata.name,
        metadata.quantization,
        metadata.context_length,
        metadata.block_count,
        metadata.embedding_length,
        metadata.head_count,
        metadata.head_count_kv,
        metadata.vocab_size,
        metadata.has_chat_template,
        metadata.chat_template_mentions_tools,
        metadata.kv_bytes_per_token(),
        metadata.data_offset,
        metadata.tensor_data_end,
    );
    assert!(is_known_architecture(&metadata.architecture));
    let end = metadata
        .tensor_data_end
        .expect("every tensor type of the staged model is known");
    assert!(end <= file_len && file_len - end < metadata.alignment.max(64));
}
