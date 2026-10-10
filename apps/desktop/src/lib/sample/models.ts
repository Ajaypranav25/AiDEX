// Sample model choices for pickers until Core reports the models on the drive.
export const modelOptions = [
  { value: "gemma-4", label: "Gemma 4 · Local" },
  { value: "qwen-coder-7b", label: "Qwen2.5 Coder 7B · Local" },
  { value: "llama-3.2-3b", label: "Llama 3.2 3B · Local" },
  { value: "provider-default", label: "Provider model · Cloud" },
] as const;
