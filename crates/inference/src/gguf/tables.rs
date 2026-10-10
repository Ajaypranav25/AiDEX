//! Static GGUF/ggml facts: quantization names, tensor type sizes, and the
//! architectures the pinned llama.cpp runtime family loads.

/// `general.file_type` (llama.cpp `LLAMA_FTYPE_*`) to its quantization label.
pub(crate) fn file_type_label(file_type: u32) -> Option<&'static str> {
    Some(match file_type {
        0 => "F32",
        1 => "F16",
        2 => "Q4_0",
        3 => "Q4_1",
        7 => "Q8_0",
        8 => "Q5_0",
        9 => "Q5_1",
        10 => "Q2_K",
        11 => "Q3_K_S",
        12 => "Q3_K_M",
        13 => "Q3_K_L",
        14 => "Q4_K_S",
        15 => "Q4_K_M",
        16 => "Q5_K_S",
        17 => "Q5_K_M",
        18 => "Q6_K",
        19 => "IQ2_XXS",
        20 => "IQ2_XS",
        21 => "Q2_K_S",
        22 => "IQ3_XS",
        23 => "IQ3_XXS",
        24 => "IQ1_S",
        25 => "IQ4_NL",
        26 => "IQ3_S",
        27 => "IQ3_M",
        28 => "IQ2_S",
        29 => "IQ2_M",
        30 => "IQ4_XS",
        31 => "IQ1_M",
        32 => "BF16",
        36 => "TQ1_0",
        37 => "TQ2_0",
        38 => "MXFP4_MOE",
        _ => return None,
    })
}

/// `(elements per block, bytes per block)` of a ggml tensor type.
pub(crate) fn tensor_type_layout(tensor_type: u32) -> Option<(u64, u64)> {
    Some(match tensor_type {
        0 => (1, 4),      // F32
        1 => (1, 2),      // F16
        2 => (32, 18),    // Q4_0
        3 => (32, 20),    // Q4_1
        6 => (32, 22),    // Q5_0
        7 => (32, 24),    // Q5_1
        8 => (32, 34),    // Q8_0
        9 => (32, 36),    // Q8_1
        10 => (256, 84),  // Q2_K
        11 => (256, 110), // Q3_K
        12 => (256, 144), // Q4_K
        13 => (256, 176), // Q5_K
        14 => (256, 210), // Q6_K
        15 => (256, 292), // Q8_K
        16 => (256, 66),  // IQ2_XXS
        17 => (256, 74),  // IQ2_XS
        18 => (256, 98),  // IQ3_XXS
        19 => (256, 50),  // IQ1_S
        20 => (32, 18),   // IQ4_NL
        21 => (256, 110), // IQ3_S
        22 => (256, 82),  // IQ2_S
        23 => (256, 136), // IQ4_XS
        24 => (1, 1),     // I8
        25 => (1, 2),     // I16
        26 => (1, 4),     // I32
        27 => (1, 8),     // I64
        28 => (1, 8),     // F64
        29 => (256, 56),  // IQ1_M
        30 => (1, 2),     // BF16
        34 => (256, 54),  // TQ1_0
        35 => (256, 66),  // TQ2_0
        39 => (32, 17),   // MXFP4
        _ => return None,
    })
}

/// Text-generation architectures (`general.architecture`) known to the llama.cpp
/// release family AiDEX pins. Anything else (including vision projectors such as
/// `clip`) is refused at import rather than failing at load time.
pub(crate) const KNOWN_ARCHITECTURES: &[&str] = &[
    "apertus",
    "arcee",
    "arctic",
    "baichuan",
    "bailingmoe",
    "bailingmoe2",
    "bitnet",
    "bloom",
    "chameleon",
    "chatglm",
    "codeshell",
    "cohere2",
    "command-r",
    "dbrx",
    "deci",
    "deepseek",
    "deepseek2",
    "dots1",
    "dream",
    "ernie4_5",
    "ernie4_5-moe",
    "exaone",
    "exaone4",
    "falcon",
    "falcon-h1",
    "gemma",
    "gemma2",
    "gemma3",
    "gemma3n",
    "gemma4",
    "glm4",
    "glm4moe",
    "gpt-oss",
    "gpt2",
    "gptj",
    "gptneox",
    "granite",
    "granitehybrid",
    "granitemoe",
    "grok",
    "grovemoe",
    "hunyuan-dense",
    "hunyuan-moe",
    "internlm2",
    "jais",
    "jamba",
    "lfm2",
    "lfm2moe",
    "llada",
    "llada-moe",
    "llama",
    "llama4",
    "mamba",
    "mamba2",
    "minicpm",
    "minicpm3",
    "minimax-m2",
    "mistral3",
    "mpt",
    "nemotron",
    "nemotron-h",
    "olmo",
    "olmo2",
    "olmoe",
    "openelm",
    "orion",
    "phi2",
    "phi3",
    "phimoe",
    "plamo",
    "plamo2",
    "plm",
    "qwen",
    "qwen2",
    "qwen2moe",
    "qwen2vl",
    "qwen3",
    "qwen3moe",
    "qwen3next",
    "qwen3vl",
    "qwen3vlmoe",
    "refact",
    "rwkv6",
    "rwkv6qwen2",
    "rwkv7",
    "seed_oss",
    "smallthinker",
    "smollm3",
    "stablelm",
    "starcoder",
    "starcoder2",
    "xverse",
];

pub(crate) fn is_known_architecture(architecture: &str) -> bool {
    KNOWN_ARCHITECTURES.binary_search(&architecture).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn architecture_list_is_sorted_for_binary_search() {
        let mut sorted = KNOWN_ARCHITECTURES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, KNOWN_ARCHITECTURES);
        assert!(is_known_architecture("qwen2"));
        assert!(!is_known_architecture("clip"));
    }

    #[test]
    fn common_quantizations_have_labels_and_layouts() {
        assert_eq!(file_type_label(15), Some("Q4_K_M"));
        assert_eq!(file_type_label(1), Some("F16"));
        assert_eq!(file_type_label(4), None, "removed ftype");
        assert_eq!(tensor_type_layout(12), Some((256, 144)));
        assert_eq!(tensor_type_layout(1000), None);
    }
}
