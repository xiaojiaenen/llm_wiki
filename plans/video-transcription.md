# Video Transcription & URL Import — Implementation Plan

## Overview

Add video/audio transcription (whisper-rs + symphonia + yt-dlp sidecar) and URL direct import (Firecrawl Scrape) to LLM Wiki. Zero Python dependency — all Rust-compiled or bundled binaries.

---

## Architecture

```
User Input (URL or local file)
    ↓
┌─────────────────────────────────────────────┐
│  URL Detection Router                        │
│  ├─ Video URL (YouTube/Bilibili/Douyin)      │
│  │   → yt-dlp sidecar → download video       │
│  │   → symphonia extract audio → PCM         │
│  │   → whisper-rs transcribe → text          │
│  │   → save to raw/sources/*.txt              │
│  ├─ Web Article URL                          │
│  │   → Firecrawl v1/scrape → markdown        │
│  │   → save to raw/sources/*.md               │
│  └─ Local video/audio file                   │
│      → symphonia extract audio → PCM         │
│      → whisper-rs transcribe → text          │
│      → save to raw/sources/*.txt              │
└─────────────────────────────────────────────┘
    ↓
autoIngest → LLM parse → wiki pages
```

---

## Phase 1: Rust Backend — whisper-rs + symphonia

### 1.1 Add Dependencies to Cargo.toml

```toml
# Video transcription
whisper-rs = { version = "0.16", features = ["metal"] }
symphonia = { version = "0.5", features = ["isomp4", "aac", "mkv", "mp3", "pcm"] }
rubato = "0.16"  # resampling to 16kHz
```

Note: Use symphonia 0.5 (stable, Rust 1.53+) rather than 0.6 (requires Rust 1.85+).

### 1.2 New Rust Module: `src-tauri/src/commands/transcribe.rs`

**Tauri Commands:**

```rust
/// Transcribe a local video/audio file to text
#[tauri::command]
async fn transcribe_file(
    app: AppHandle,
    source_path: String,       // absolute path to video/audio file
    model_name: String,        // "small" | "medium" | "large-v3-turbo"
    language: Option<String>,  // "en" | "zh" | "ja" | ... or None for auto
) -> Result<TranscribeResult, String>

/// Transcribe a video from URL (YouTube/Bilibili/etc)
#[tauri::command]
async fn transcribe_url(
    app: AppHandle,
    url: String,
    model_name: String,
    language: Option<String>,
    output_dir: String,        // where to save the transcript
) -> Result<TranscribeResult, String>

/// Download a whisper model
#[tauri::command]
async fn download_whisper_model(
    app: AppHandle,
    model_name: String,        // "ggml-small-q5_1.bin" etc.
) -> Result<(), String>

/// List downloaded whisper models
#[tauri::command]
async fn list_whisper_models() -> Result<Vec<WhisperModelInfo>, String>

/// Delete a whisper model
#[tauri::command]
async fn delete_whisper_model(model_name: String) -> Result<(), String>

/// Cancel an in-progress transcription
#[tauri::command]
async fn cancel_transcribe(task_id: String) -> Result<(), String>
```

**Types:**

```rust
#[derive(Serialize, Deserialize)]
struct TranscribeResult {
    task_id: String,
    text: String,
    language: String,
    duration_secs: f64,
    output_path: String,  // path to saved .txt file
}

#[derive(Serialize, Deserialize)]
struct WhisperModelInfo {
    name: String,           // "ggml-small-q5_1.bin"
    size_bytes: u64,
    downloaded: bool,
    path: Option<String>,
}
```

### 1.3 Audio Extraction Pipeline (symphonia)

```rust
/// Extract audio from video file, return f32 PCM samples at 16kHz mono
fn extract_audio(video_path: &str) -> Result<Vec<f32>, String> {
    // 1. Open file, probe format with Hint::extension
    // 2. Find audio track (skip video tracks)
    // 3. Create decoder for audio codec
    // 4. Decode all packets → collect f32 samples
    // 5. Resample to 16kHz mono using rubato
    // 6. Return Vec<f32>
}
```

Handles: MP4 (AAC), WebM (Vorbis/Opus), MKV, MP3, WAV, FLAC, OGG.

### 1.4 Whisper Transcription (whisper-rs)

```rust
/// Transcribe f32 PCM audio at 16kHz using whisper-rs
fn whisper_transcribe(
    app: &AppHandle,
    audio: &[f32],
    model_path: &str,
    language: Option<&str>,
    task_id: &str,  // for progress events
) -> Result<(String, String), String> {
    // 1. Load WhisperContext from model file
    // 2. Create state, set params
    //    - SamplingStrategy::Greedy { best_of: 1 }
    //    - Set language if provided
    //    - Enable timestamps
    // 3. Run state.full() with progress callback
    //    - Emit "transcribe://progress" events
    // 4. Collect segments into text
    // 5. Return (text, detected_language)
}
```

### 1.5 yt-dlp Sidecar Integration

Bundle yt-dlp as a Tauri sidecar binary:

- `src-tauri/binaries/yt-dlp-aarch64-apple-darwin`
- `src-tauri/binaries/yt-dlp-x86_64-apple-darwin`
- `src-tauri/binaries/yt-dlp-x86_64-pc-windows-msvc.exe`
- `src-tauri/binaries/yt-dlp-x86_64-unknown-linux-gnu`

```rust
/// Download video from URL using yt-dlp sidecar
async fn download_video_with_ytdlp(
    app: &AppHandle,
    url: &str,
    output_dir: &str,
    task_id: &str,
) -> Result<String, String> {
    // 1. Use tauri_plugin_shell to run yt-dlp sidecar
    // 2. Args: --extract-audio --audio-format wav --output <path>
    // 3. Emit progress events from stdout parsing
    // 4. Return path to downloaded audio file
}
```

### 1.6 Model Management

Models stored in: `{app_data_dir}/whisper-models/`

Download URLs:
```
https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin   (190 MB)
https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium-q5_0.bin  (539 MB)
https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin (574 MB)
```

Download with progress via Tauri HTTP plugin + streaming write.

### 1.7 Progress Events

```rust
// Event: transcribe://progress
#[derive(Serialize, Clone)]
struct TranscribeProgress {
    task_id: String,
    phase: String,        // "downloading_video" | "extracting_audio" | "transcribing" | "saving"
    percent: f32,         // 0.0 - 100.0
    message: String,      // human-readable status
}
```

### 1.8 Register Commands in lib.rs

```rust
// In commands/mod.rs:
pub mod transcribe;

// In lib.rs invoke_handler:
commands::transcribe::transcribe_file,
commands::transcribe::transcribe_url,
commands::transcribe::download_whisper_model,
commands::transcribe::list_whisper_models,
commands::transcribe::delete_whisper_model,
commands::transcribe::cancel_transcribe,
```

---

## Phase 2: URL Direct Import (Firecrawl Scrape)

### 2.1 Extend web-search.ts

Add `scrapeUrl()` function:

```typescript
// src/lib/web-fetch.ts (new file)
export async function scrapeUrl(url: string): Promise<{
  title: string
  markdown: string
  metadata: { url: string; description?: string; author?: string }
}> {
  const httpFetch = getHttpFetch()
  const resp = await httpFetch("https://api.firecrawl.dev/v1/scrape", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      url,
      formats: ["markdown"],
      onlyMainContent: true,
    }),
  })
  const data = await resp.json()
  return {
    title: data.data.metadata.title,
    markdown: data.data.markdown,
    metadata: data.data.metadata,
  }
}
```

### 2.2 URL Router in Transcription Module

```typescript
// src/lib/video-transcribe.ts
export function isVideoUrl(url: string): boolean {
  return /youtube\.com|youtu\.be|bilibili\.com|douyin\.com|tiktok\.com/i.test(url)
}

export function isUrl(input: string): boolean {
  return /^https?:\/\//i.test(input)
}
```

### 2.3 Frontend Import Dialog

Add URL import to Sources view:

```tsx
// In sources-view.tsx, add alongside existing import buttons:
<Button onClick={handleUrlImport}>🌐 Import URL</Button>
<Button onClick={handleVideoTranscribe}>🎥 Transcribe Video</Button>
```

URL import dialog:
- Text input for URL
- Auto-detect: video URL → transcription flow, web URL → Firecrawl scrape
- Show progress during fetch/transcription

---

## Phase 3: Settings UI — Transcription Section

### 3.1 Add TranscriptionConfig to wiki-store.ts

```typescript
export type WhisperModelSize = "small-q5_1" | "medium-q5_0" | "large-v3-turbo-q5_0"

export interface TranscriptionConfig {
  enabled: boolean
  model: WhisperModelSize
  language: string  // "auto" | "en" | "zh" | "ja" | ...
}
```

Add to WikiState:
```typescript
transcriptionConfig: TranscriptionConfig
setTranscriptionConfig: (config: TranscriptionConfig) => void
```

Default:
```typescript
transcriptionConfig: {
  enabled: false,
  model: "small-q5_1",
  language: "auto",
}
```

### 3.2 Add to SettingsDraft (settings-types.ts)

```typescript
transcriptionEnabled: boolean
transcriptionModel: WhisperModelSize
transcriptionLanguage: string
```

### 3.3 New Settings Section: TranscriptionSection

File: `src/components/settings/sections/transcription-section.tsx`

Layout:
```
┌─────────────────────────────────────────────┐
│  🎥 Video Transcription                     │
│                                              │
│  Transcribe video and audio files to text    │
│  using local Whisper models.                 │
│                                              │
│  ☑ Enable Transcription                     │
│                                              │
│  Model: [small-q5_1 (190MB) ▼]              │
│  Language: [Auto-detect ▼]                   │
│                                              │
│  ┌─ Downloaded Models ──────────────────┐    │
│  │  ✅ ggml-small-q5_1.bin    190 MB    │    │
│  │  ❌ ggml-medium-q5_0.bin   539 MB    │    │
│  │  ❌ ggml-large-v3-turbo... 574 MB    │    │
│  └──────────────────────────────────────┘    │
│                                              │
│  [Download Model]  [Delete Selected]         │
│                                              │
│  ┌─ Progress ───────────────────────────┐    │
│  │  Downloading: ggml-medium-q5_0.bin   │    │
│  │  ████████████░░░░░░░░  45% (243 MB)  │    │
│  └──────────────────────────────────────┘    │
└─────────────────────────────────────────────┘
```

### 3.4 Wire into Settings View

- Add `"transcription"` to CategoryId type
- Add to CATEGORIES array with `Video` icon from lucide-react
- Add draft fields in initialDraft()
- Add save logic in handleSave()
- Add i18n keys

### 3.5 Persist Config (project-store.ts)

```typescript
const TRANSCRIPTION_KEY = "transcriptionConfig"

export async function saveTranscriptionConfig(config: TranscriptionConfig): Promise<void> {
  const store = await getStore()
  await store.set(TRANSCRIPTION_KEY, config)
}

export async function loadTranscriptionConfig(): Promise<TranscriptionConfig | null> {
  const store = await getStore()
  return (await store.get<TranscriptionConfig>(TRANSCRIPTION_KEY)) ?? null
}
```

---

## Phase 4: Frontend Transcription Flow

### 4.1 Transcription Client (src/lib/video-transcribe.ts)

```typescript
export interface TranscribeProgress {
  taskId: string
  phase: "downloading_video" | "extracting_audio" | "transcribing" | "saving"
  percent: number
  message: string
}

export async function transcribeFromUrl(
  url: string,
  outputDir: string,
  onProgress: (p: TranscribeProgress) => void,
): Promise<TranscribeResult> {
  const taskId = crypto.randomUUID()

  // Listen for progress events
  const unlisten = await listen<TranscribeProgress>("transcribe://progress", (e) => {
    if (e.payload.taskId === taskId) onProgress(e.payload)
  })

  try {
    const config = useWikiStore.getState().transcriptionConfig
    const result = await invoke<TranscribeResult>("transcribe_url", {
      url,
      modelName: config.model,
      language: config.language === "auto" ? null : config.language,
      outputDir,
    })
    return result
  } finally {
    unlisten()
  }
}

export async function transcribeFromFile(
  filePath: string,
  onProgress: (p: TranscribeProgress) => void,
): Promise<TranscribeResult> {
  // Similar pattern with invoke("transcribe_file", ...)
}
```

### 4.2 URL Import Dialog Component

File: `src/components/sources/url-import-dialog.tsx`

```
┌─────────────────────────────────────────────┐
│  Import from URL                             │
│                                              │
│  URL: [https://youtube.com/watch?v=...    ]  │
│                                              │
│  Detected: 🎥 YouTube Video                  │
│  → Will download and transcribe audio        │
│                                              │
│  Model: [small-q5_1 ▼]                       │
│                                              │
│  [Cancel]  [Start Transcription]             │
└─────────────────────────────────────────────┘
```

Auto-detection:
- Video URL → show transcription options, invoke `transcribe_url`
- Web article URL → show scrape preview, invoke `scrape_url` → save to raw/sources/

### 4.3 Integration with Sources View

Add two new buttons in the sources view toolbar:
- "Import URL" — opens URL import dialog
- "Transcribe Video" — opens file picker for local video files

After transcription completes, the .txt file lands in `raw/sources/` and the existing file watcher + autoIngest pipeline takes over.

---

## Phase 5: Tauri Configuration

### 5.1 Add shell plugin for yt-dlp sidecar

In `Cargo.toml`:
```toml
tauri-plugin-shell = "2"
```

In `lib.rs`:
```rust
.plugin(tauri_plugin_shell::init())
```

### 5.2 Bundle yt-dlp binaries

In `tauri.conf.json`:
```json
{
  "bundle": {
    "externalBin": ["binaries/yt-dlp"]
  }
}
```

Download yt-dlp binaries during build:
- `src-tauri/binaries/yt-dlp-aarch64-apple-darwin`
- `src-tauri/binaries/yt-dlp-x86_64-apple-darwin`
- `src-tauri/binaries/yt-dlp-x86_64-pc-windows-msvc.exe`
- `src-tauri/binaries/yt-dlp-x86_64-unknown-linux-gnu`

### 5.3 Shell permissions

In `src-tauri/capabilities/default.json`:
```json
{
  "permissions": [
    {
      "identifier": "shell:allow-execute",
      "allow": [{
        "name": "binaries/yt-dlp",
        "sidecar": true,
        "args": true
      }]
    }
  ]
}
```

---

## File Summary

### New Files
| File | Purpose |
|---|---|
| `src-tauri/src/commands/transcribe.rs` | Rust: whisper-rs + symphonia + yt-dlp commands |
| `src/lib/video-transcribe.ts` | TS: transcription client, URL detection |
| `src/lib/web-fetch.ts` | TS: Firecrawl scrape wrapper |
| `src/components/settings/sections/transcription-section.tsx` | Settings UI |
| `src/components/sources/url-import-dialog.tsx` | URL import dialog |

### Modified Files
| File | Change |
|---|---|
| `src-tauri/Cargo.toml` | Add whisper-rs, symphonia, rubato, tauri-plugin-shell |
| `src-tauri/src/commands/mod.rs` | Add `pub mod transcribe;` |
| `src-tauri/src/lib.rs` | Register transcribe commands, add shell plugin |
| `src-tauri/tauri.conf.json` | Add externalBin for yt-dlp |
| `src-tauri/capabilities/default.json` | Add shell permissions |
| `src/stores/wiki-store.ts` | Add TranscriptionConfig |
| `src/components/settings/settings-types.ts` | Add transcription draft fields |
| `src/components/settings/settings-view.tsx` | Add transcription category |
| `src/lib/project-store.ts` | Add save/load transcription config |
| `src/components/sources/sources-view.tsx` | Add URL import + transcribe buttons |
| `src/i18n/en.json` | Add transcription i18n keys |
| `src/i18n/zh.json` | Add transcription i18n keys |

---

## Model Recommendations

| Model | Size | Quality | Speed | Recommended For |
|---|---|---|---|---|
| `ggml-small-q5_1` | 190 MB | Good | Fast | Default, most users |
| `ggml-medium-q5_0` | 539 MB | Better | Medium | Better accuracy needed |
| `ggml-large-v3-turbo-q5_0` | 574 MB | Best | Slower | Max quality, turbo variant |

Default: `ggml-small-q5_1` — good balance of size and quality.

---

## Implementation Order

1. **Rust: transcribe.rs** — core transcription pipeline (symphonia + whisper-rs)
2. **Rust: model download** — model management commands
3. **Rust: yt-dlp sidecar** — URL video download
4. **TS: video-transcribe.ts** — frontend transcription client
5. **TS: web-fetch.ts** — Firecrawl scrape
6. **Settings: TranscriptionConfig** — store + UI
7. **Sources: URL import dialog** — UI integration
8. **i18n** — translation keys
9. **Build: yt-dlp binaries** — sidecar bundling
