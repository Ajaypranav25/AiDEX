# Model manager feature

Model Center and model details screens, plus the Models sidebar. Everything is composed from `@aidex/ui`; the data is sample data in `sample.ts` until AiDEX Core reports the models on the drive, runtime detection, benchmarks, and provider accounts.

| File | Shows |
| --- | --- |
| `ModelCenterPage.tsx` | Route `models`. Header with a tab-dependent primary action and underline tabs bound to the route tab (`local`, `providers`). |
| `LocalModelsPanel.tsx` | Current model card (from `usePreferences().modelId`), GGUF models on the drive (rows open `model`), downloadable models, runtime facts, and storage meter. |
| `ProvidersPanel.tsx` | Cloud provider accounts with connection status and Connect / Manage actions. |
| `ModelDetailsPage.tsx` | Route `model` (`id` = model id). Badges, "Use this model" (sets the shared model), "Run benchmark", stats, charts, quantizations, and details. |
| `ModelBenchmarks.tsx` | Stat tiles, bar charts by quantization and hardware, and the quantization table. |
| `ModelsSidebar.tsx` | On this drive (active by model id) and Manage links. |
| `format.ts` | Display helpers for sizes, rates, compatibility, and the current model name. |

Chat model ids match `lib/sample/models.ts` so choosing a model here updates the top bar picker. Actions that need Core (import, download, connect, benchmark) show a "Preview" toast.
