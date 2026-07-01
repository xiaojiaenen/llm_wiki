import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"
import { useWikiStore } from "@/stores/wiki-store"

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface TranscribeResult {
  task_id: string
  text: string
  language: string
  duration_secs: number
  output_path: string
}

export interface TranscribeProgress {
  task_id: string
  phase: "downloading_video" | "extracting_audio" | "transcribing" | "saving" | "downloading_model"
  percent: number
  message: string
}

export interface ScrapeResult {
  title: string
  markdown: string
  url: string
}

export type ProgressCallback = (progress: TranscribeProgress) => void

// ---------------------------------------------------------------------------
// URL detection
// ---------------------------------------------------------------------------

const VIDEO_URL_PATTERNS = [
  /youtube\.com\/watch/i,
  /youtu\.be\//i,
  /bilibili\.com\/video/i,
  /b23\.tv\//i,
  /douyin\.com\//i,
  /tiktok\.com\//i,
]

export function isVideoUrl(url: string): boolean {
  return VIDEO_URL_PATTERNS.some((p) => p.test(url))
}

export function isUrl(input: string): boolean {
  return /^https?:\/\//i.test(input.trim())
}

// ---------------------------------------------------------------------------
// Transcribe from URL (video)
// ---------------------------------------------------------------------------

export async function transcribeFromUrl(
  url: string,
  outputDir: string,
  onProgress?: ProgressCallback,
): Promise<TranscribeResult> {
  const config = useWikiStore.getState().transcriptionConfig
  const taskId = crypto.randomUUID()

  const unlisten = await listen<TranscribeProgress>("transcribe://progress", (e) => {
    if (onProgress) onProgress(e.payload)
  })

  try {
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

// ---------------------------------------------------------------------------
// Transcribe local file
// ---------------------------------------------------------------------------

export async function transcribeFromFile(
  filePath: string,
  outputDir?: string,
  onProgress?: ProgressCallback,
): Promise<TranscribeResult> {
  const config = useWikiStore.getState().transcriptionConfig

  const unlisten = await listen<TranscribeProgress>("transcribe://progress", (e) => {
    if (onProgress) onProgress(e.payload)
  })

  try {
    const result = await invoke<TranscribeResult>("transcribe_file", {
      sourcePath: filePath,
      modelName: config.model,
      language: config.language === "auto" ? null : config.language,
      outputDir: outputDir ?? null,
    })
    return result
  } finally {
    unlisten()
  }
}

// ---------------------------------------------------------------------------
// Scrape web article URL
// ---------------------------------------------------------------------------

export async function scrapeUrl(url: string): Promise<ScrapeResult> {
  return invoke<ScrapeResult>("scrape_url", { url })
}

// ---------------------------------------------------------------------------
// Cancel transcription
// ---------------------------------------------------------------------------

export async function cancelTranscribe(taskId: string): Promise<void> {
  await invoke("cancel_transcribe", { taskId })
}

// ---------------------------------------------------------------------------
// Model management
// ---------------------------------------------------------------------------

export interface WhisperModelInfo {
  name: string
  size_bytes: number
  downloaded: boolean
  path: string | null
}

export async function listWhisperModels(): Promise<WhisperModelInfo[]> {
  return invoke<WhisperModelInfo[]>("list_whisper_models")
}

export async function downloadWhisperModel(modelName: string): Promise<void> {
  await invoke("download_whisper_model", { modelName })
}

export async function deleteWhisperModel(modelName: string): Promise<void> {
  await invoke("delete_whisper_model", { modelName })
}

// ---------------------------------------------------------------------------
// yt-dlp detection
// ---------------------------------------------------------------------------

export interface YtdlpInfo {
  installed: boolean
  path: string | null
  version: string | null
  error: string | null
}

export async function detectYtdlp(): Promise<YtdlpInfo> {
  return invoke<YtdlpInfo>("detect_ytdlp")
}
