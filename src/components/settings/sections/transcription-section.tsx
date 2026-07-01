import { useState, useEffect, useCallback } from "react"
import { useTranslation } from "react-i18next"
import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"
import { Label } from "@/components/ui/label"
import { Button } from "@/components/ui/button"
import { Trash2, Download, CheckCircle, Loader2, AlertCircle, ExternalLink } from "lucide-react"
import type { SettingsDraft, DraftSetter } from "../settings-types"
import type { WhisperModelSize } from "@/stores/wiki-store"
import { detectYtdlp, type YtdlpInfo } from "@/lib/video-transcribe"

interface Props {
  draft: SettingsDraft
  setDraft: DraftSetter
}

interface WhisperModelInfo {
  name: string
  size_bytes: number
  downloaded: boolean
  path: string | null
}

interface TranscribeProgress {
  task_id: string
  phase: string
  percent: number
  message: string
}

function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(0)} MB`
  return `${(bytes / 1_000).toFixed(0)} KB`
}

const MODEL_OPTIONS: { value: WhisperModelSize; label: string; size: string }[] = [
  { value: "ggml-small-q5_1.bin", label: "Small (q5_1)", size: "190 MB" },
  { value: "ggml-medium-q5_0.bin", label: "Medium (q5_0)", size: "539 MB" },
  { value: "ggml-large-v3-turbo-q5_0.bin", label: "Large v3 Turbo (q5_0)", size: "574 MB" },
]

const LANGUAGE_OPTIONS = [
  { value: "auto", label: "Auto-detect" },
  { value: "en", label: "English" },
  { value: "zh", label: "Chinese" },
  { value: "ja", label: "Japanese" },
  { value: "ko", label: "Korean" },
  { value: "fr", label: "French" },
  { value: "de", label: "German" },
  { value: "es", label: "Spanish" },
  { value: "pt", label: "Portuguese" },
  { value: "ru", label: "Russian" },
  { value: "ar", label: "Arabic" },
]

export function TranscriptionSection({ draft, setDraft }: Props) {
  const { t } = useTranslation()
  const [models, setModels] = useState<WhisperModelInfo[]>([])
  const [downloading, setDownloading] = useState<string | null>(null)
  const [downloadProgress, setDownloadProgress] = useState(0)
  const [downloadMessage, setDownloadMessage] = useState("")
  const [ytdlpInfo, setYtdlpInfo] = useState<YtdlpInfo | null>(null)
  const [checkingYtdlp, setCheckingYtdlp] = useState(false)

  const refreshModels = useCallback(async () => {
    try {
      const list = await invoke<WhisperModelInfo[]>("list_whisper_models")
      setModels(list)
    } catch (e) {
      console.error("Failed to list models:", e)
    }
  }, [])

  useEffect(() => {
    refreshModels()
  }, [refreshModels])

  const checkYtdlp = useCallback(async () => {
    setCheckingYtdlp(true)
    try {
      const info = await detectYtdlp()
      setYtdlpInfo(info)
    } catch (e) {
      setYtdlpInfo({ installed: false, path: null, version: null, error: String(e) })
    } finally {
      setCheckingYtdlp(false)
    }
  }, [])

  useEffect(() => {
    checkYtdlp()
  }, [checkYtdlp])

  useEffect(() => {
    const unlisten = listen<TranscribeProgress>("transcribe://progress", (e) => {
      if (e.payload.task_id === "model-download") {
        setDownloadProgress(e.payload.percent)
        setDownloadMessage(e.payload.message)
      }
    })
    return () => { unlisten.then(fn => fn()) }
  }, [])

  const handleDownload = async (modelName: string) => {
    setDownloading(modelName)
    setDownloadProgress(0)
    setDownloadMessage("Starting download...")
    try {
      await invoke("download_whisper_model", { modelName })
      await refreshModels()
    } catch (e) {
      console.error("Download failed:", e)
    } finally {
      setDownloading(null)
      setDownloadProgress(0)
      setDownloadMessage("")
    }
  }

  const handleDelete = async (modelName: string) => {
    try {
      await invoke("delete_whisper_model", { modelName })
      await refreshModels()
    } catch (e) {
      console.error("Delete failed:", e)
    }
  }

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-xl font-semibold">
          {t("settings.sections.transcription.title", { defaultValue: "Video Transcription" })}
        </h2>
        <p className="text-sm text-muted-foreground mt-1">
          {t("settings.sections.transcription.description", {
            defaultValue:
              "Transcribe video and audio files to text using local Whisper models. No internet required for transcription.",
          })}
        </p>
      </div>

      <label className="flex items-center gap-2">
        <input
          type="checkbox"
          checked={draft.transcriptionEnabled}
          onChange={(e) => setDraft("transcriptionEnabled", e.target.checked)}
          className="h-4 w-4"
        />
        <span className="text-sm">
          {t("settings.sections.transcription.enabled", { defaultValue: "Enable Transcription" })}
        </span>
      </label>

      {draft.transcriptionEnabled && (
        <div className="space-y-4 pl-1">
          {/* yt-dlp status */}
          <div className="rounded-md border p-3 space-y-2">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                {ytdlpInfo?.installed ? (
                  <CheckCircle className="h-4 w-4 text-green-500" />
                ) : (
                  <AlertCircle className="h-4 w-4 text-amber-500" />
                )}
                <span className="text-sm font-medium">yt-dlp</span>
                {ytdlpInfo?.version && (
                  <span className="text-xs text-muted-foreground">v{ytdlpInfo.version}</span>
                )}
              </div>
              <Button variant="ghost" size="sm" onClick={checkYtdlp} disabled={checkingYtdlp}>
                {checkingYtdlp ? (
                  <Loader2 className="h-3 w-3 animate-spin" />
                ) : (
                  t("settings.sections.transcription.checkYtdlp", { defaultValue: "Refresh" })
                )}
              </Button>
            </div>
            {ytdlpInfo?.installed ? (
              <p className="text-xs text-muted-foreground">
                {t("settings.sections.transcription.ytdlpInstalled", {
                  defaultValue: "yt-dlp is installed and ready for video downloads.",
                })}
                {ytdlpInfo.path && (
                  <span className="block mt-1 text-[11px] opacity-60">{ytdlpInfo.path}</span>
                )}
              </p>
            ) : (
              <div className="space-y-2">
                <p className="text-xs text-amber-600 dark:text-amber-400">
                  {t("settings.sections.transcription.ytdlpNotInstalled", {
                    defaultValue: "yt-dlp is required to download videos from URLs (YouTube, Bilibili, etc.)",
                  })}
                </p>
                <div className="flex items-center gap-2">
                  <code className="text-xs bg-muted px-2 py-1 rounded">
                    brew install yt-dlp
                  </code>
                  <span className="text-xs text-muted-foreground">
                    {t("settings.sections.transcription.or", { defaultValue: "or" })}
                  </span>
                  <code className="text-xs bg-muted px-2 py-1 rounded">
                    pip install yt-dlp
                  </code>
                </div>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    window.open("https://github.com/yt-dlp/yt-dlp#installation", "_blank")
                  }}
                >
                  <ExternalLink className="h-3 w-3 mr-1" />
                  {t("settings.sections.transcription.installGuide", { defaultValue: "Installation Guide" })}
                </Button>
              </div>
            )}
          </div>

          {/* Model selector */}
          <div className="space-y-2">
            <Label>
              {t("settings.sections.transcription.model", { defaultValue: "Model" })}
            </Label>
            <select
              className="w-full rounded-md border bg-background px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring"
              value={draft.transcriptionModel}
              onChange={(e) => setDraft("transcriptionModel", e.target.value as WhisperModelSize)}
            >
              {MODEL_OPTIONS.map((m) => (
                <option key={m.value} value={m.value}>
                  {m.label} — {m.size}
                </option>
              ))}
            </select>
            <p className="text-xs text-muted-foreground">
              {t("settings.sections.transcription.modelHint", {
                defaultValue:
                  "Small is fast and good for most uses. Medium is more accurate. Large v3 Turbo offers the best quality.",
              })}
            </p>
          </div>

          {/* Language selector */}
          <div className="space-y-2">
            <Label>
              {t("settings.sections.transcription.language", { defaultValue: "Language" })}
            </Label>
            <select
              className="w-full rounded-md border bg-background px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring"
              value={draft.transcriptionLanguage}
              onChange={(e) => setDraft("transcriptionLanguage", e.target.value)}
            >
              {LANGUAGE_OPTIONS.map((l) => (
                <option key={l.value} value={l.value}>
                  {l.label}
                </option>
              ))}
            </select>
            <p className="text-xs text-muted-foreground">
              {t("settings.sections.transcription.languageHint", {
                defaultValue:
                  "Auto-detect works well for most languages. Specify a language for better accuracy.",
              })}
            </p>
          </div>

          {/* Model management */}
          <div className="space-y-2">
            <Label>
              {t("settings.sections.transcription.downloadedModels", {
                defaultValue: "Downloaded Models",
              })}
            </Label>
            <div className="rounded-md border divide-y">
              {MODEL_OPTIONS.map((opt) => {
                const info = models.find((m) => m.name === opt.value)
                const isDownloaded = info?.downloaded ?? false
                const isDownloadingThis = downloading === opt.value

                return (
                  <div
                    key={opt.value}
                    className="flex items-center justify-between px-3 py-2"
                  >
                    <div className="flex items-center gap-2">
                      {isDownloaded ? (
                        <CheckCircle className="h-4 w-4 text-green-500" />
                      ) : (
                        <div className="h-4 w-4 rounded-full border-2 border-muted-foreground/30" />
                      )}
                      <span className="text-sm font-medium">{opt.label}</span>
                      <span className="text-xs text-muted-foreground">{opt.size}</span>
                    </div>
                    <div className="flex items-center gap-2">
                      {!isDownloaded && (
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => handleDownload(opt.value)}
                          disabled={downloading !== null}
                        >
                          {isDownloadingThis ? (
                            <Loader2 className="h-3 w-3 animate-spin" />
                          ) : (
                            <Download className="h-3 w-3" />
                          )}
                          <span className="ml-1">
                            {isDownloadingThis
                              ? t("settings.sections.transcription.downloading", {
                                  defaultValue: "Downloading...",
                                })
                              : t("settings.sections.transcription.download", {
                                  defaultValue: "Download",
                                })}
                          </span>
                        </Button>
                      )}
                      {isDownloaded && (
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleDelete(opt.value)}
                          disabled={downloading !== null}
                        >
                          <Trash2 className="h-3 w-3" />
                        </Button>
                      )}
                    </div>
                  </div>
                )
              })}
            </div>
          </div>

          {/* Download progress */}
          {downloading && (
            <div className="rounded-md border p-3 space-y-2">
              <div className="flex items-center justify-between text-sm">
                <span>{downloadMessage}</span>
                <span className="text-muted-foreground">{downloadProgress.toFixed(0)}%</span>
              </div>
              <div className="h-2 rounded-full bg-muted overflow-hidden">
                <div
                  className="h-full bg-primary transition-all duration-300"
                  style={{ width: `${downloadProgress}%` }}
                />
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  )
}
