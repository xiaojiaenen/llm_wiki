import { useState, useEffect, useCallback } from "react"
import { useTranslation } from "react-i18next"
import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"
import { openUrl } from "@tauri-apps/plugin-opener"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { X, Globe, Video, Loader2, CheckCircle, AlertCircle, ExternalLink, LogIn } from "lucide-react"
import {
  isUrl,
  isVideoUrl,
  transcribeFromUrl,
  scrapeUrl,
  type TranscribeProgress,
} from "@/lib/video-transcribe"
import type { WhisperModelSize } from "@/stores/wiki-store"
import { useWikiStore } from "@/stores/wiki-store"
import { writeFile } from "@/commands/fs"

interface Props {
  open: boolean
  onClose: () => void
  outputDir: string // raw/sources/ path
  onImported?: () => void
}

type Phase = "input" | "processing" | "done" | "error"

const MODEL_OPTIONS: { value: WhisperModelSize; label: string }[] = [
  { value: "ggml-small-q5_1.bin", label: "Small (190 MB)" },
  { value: "ggml-medium-q5_0.bin", label: "Medium (539 MB)" },
  { value: "ggml-large-v3-turbo-q5_0.bin", label: "Large v3 Turbo (574 MB)" },
]

export function UrlImportDialog({ open, onClose, outputDir, onImported }: Props) {
  const { t } = useTranslation()
  const transcriptionConfig = useWikiStore((s) => s.transcriptionConfig)

  const [url, setUrl] = useState("")
  const [phase, setPhase] = useState<Phase>("input")
  const [progress, setProgress] = useState<TranscribeProgress | null>(null)
  const [error, setError] = useState("")
  const [resultPath, setResultPath] = useState("")

  // Reset on open
  useEffect(() => {
    if (open) {
      setUrl("")
      setPhase("input")
      setProgress(null)
      setError("")
      setResultPath("")
    }
  }, [open])

  const detectedType = url.trim() && isUrl(url.trim())
    ? isVideoUrl(url.trim())
      ? "video"
      : "web"
    : null

  const handleImport = useCallback(async () => {
    const trimmed = url.trim()
    if (!trimmed || !isUrl(trimmed)) return

    setPhase("processing")
    setError("")
    setProgress(null)

    try {
      if (isVideoUrl(trimmed)) {
        // Video transcription
        const result = await transcribeFromUrl(trimmed, outputDir, setProgress)
        setResultPath(result.output_path)
      } else {
        // Web article scrape
        setProgress({
          task_id: "scrape",
          phase: "saving",
          percent: 50,
          message: t("urlImport.scraping", { defaultValue: "Fetching article..." }),
        })
        const result = await scrapeUrl(trimmed)
        // Save as markdown with frontmatter
        const slug = result.title
          .toLowerCase()
          .replace(/[^a-z0-9一-鿿]+/g, "-")
          .replace(/^-|-$/g, "")
          .slice(0, 80) || "imported-article"

        const now = new Date().toISOString().split("T")[0]
        const content = `---
type: clip
title: "${result.title.replace(/"/g, '\\"')}"
url: "${trimmed}"
clipped: ${now}
origin: web-import
---

${result.markdown}`

        const filePath = `${outputDir}/${slug}-${now}.md`
        await invoke("write_file", { path: filePath, content })
        setResultPath(filePath)
      }

      setPhase("done")
      onImported?.()
    } catch (e) {
      setPhase("error")
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [url, outputDir, t, onImported])

  if (!open) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-background rounded-lg shadow-lg w-full max-w-md p-6 space-y-4">
        {/* Header */}
        <div className="flex items-center justify-between">
          <h3 className="text-lg font-semibold">
            {t("urlImport.title", { defaultValue: "Import from URL" })}
          </h3>
          <Button variant="ghost" size="sm" onClick={onClose}>
            <X className="h-4 w-4" />
          </Button>
        </div>

        {phase === "input" && (
          <>
            {/* URL input */}
            <div className="space-y-2">
              <Label htmlFor="url-input">URL</Label>
              <Input
                id="url-input"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="https://..."
                onKeyDown={(e) => e.key === "Enter" && handleImport()}
                autoFocus
              />
            </div>

            {/* Detection hint */}
            {detectedType && (
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                {detectedType === "video" ? (
                  <>
                    <Video className="h-4 w-4 text-blue-500" />
                    <span>
                      {t("urlImport.detectedVideo", {
                        defaultValue: "Video detected — will download and transcribe audio",
                      })}
                    </span>
                  </>
                ) : (
                  <>
                    <Globe className="h-4 w-4 text-green-500" />
                    <span>
                      {t("urlImport.detectedWeb", {
                        defaultValue: "Web article detected — will extract content",
                      })}
                    </span>
                  </>
                )}
              </div>
            )}

            {/* Model override for video */}
            {detectedType === "video" && (
              <div className="space-y-2">
                <Label>
                  {t("urlImport.transcriptionModel", { defaultValue: "Transcription Model" })}
                </Label>
                <select
                  className="w-full rounded-md border bg-background px-3 py-2 text-sm"
                  value={transcriptionConfig.model}
                  onChange={(e) => {
                    useWikiStore.getState().setTranscriptionConfig({
                      ...transcriptionConfig,
                      model: e.target.value as WhisperModelSize,
                    })
                  }}
                >
                  {MODEL_OPTIONS.map((m) => (
                    <option key={m.value} value={m.value}>
                      {m.label}
                    </option>
                  ))}
                </select>
              </div>
            )}

            {/* Actions */}
            <div className="flex justify-end gap-2">
              <Button variant="outline" onClick={onClose}>
                {t("common.cancel", { defaultValue: "Cancel" })}
              </Button>
              <Button
                onClick={handleImport}
                disabled={!detectedType}
              >
                {detectedType === "video"
                  ? t("urlImport.transcribe", { defaultValue: "Transcribe" })
                  : t("urlImport.import", { defaultValue: "Import" })}
              </Button>
            </div>
          </>
        )}

        {phase === "processing" && (
          <div className="space-y-4 py-4">
            <div className="flex items-center gap-3">
              <Loader2 className="h-5 w-5 animate-spin text-primary" />
              <span className="text-sm">{progress?.message ?? "Processing..."}</span>
            </div>
            {progress && progress.percent > 0 && (
              <div className="space-y-1">
                <div className="h-2 rounded-full bg-muted overflow-hidden">
                  <div
                    className="h-full bg-primary transition-all duration-300"
                    style={{ width: `${progress.percent}%` }}
                  />
                </div>
                <p className="text-xs text-muted-foreground text-right">
                  {progress.percent.toFixed(0)}%
                </p>
              </div>
            )}
          </div>
        )}

        {phase === "done" && (
          <div className="space-y-4 py-4">
            <div className="flex items-center gap-3">
              <CheckCircle className="h-5 w-5 text-green-500" />
              <span className="text-sm">
                {t("urlImport.success", { defaultValue: "Import complete!" })}
              </span>
            </div>
            {resultPath && (
              <p className="text-xs text-muted-foreground break-all">
                {resultPath}
              </p>
            )}
            <div className="flex justify-end">
              <Button onClick={onClose}>
                {t("common.done", { defaultValue: "Done" })}
              </Button>
            </div>
          </div>
        )}

        {phase === "error" && (
          <div className="space-y-4 py-4">
            {error === "BILIBILI_LOGIN_REQUIRED" ? (
              <>
                <div className="flex items-center gap-3">
                  <LogIn className="h-5 w-5 text-amber-500" />
                  <span className="text-sm font-medium">
                    {t("urlImport.bilibiliLoginTitle", { defaultValue: "需要登录 BiliBili" })}
                  </span>
                </div>
                <p className="text-xs text-muted-foreground">
                  {t("urlImport.bilibiliLoginDesc", {
                    defaultValue: "BiliBili 需要登录才能下载视频。请先在 Chrome 浏览器中登录 BiliBili，然后重试。",
                  })}
                </p>
                <div className="flex gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => openUrl("https://www.bilibili.com")}
                  >
                    <ExternalLink className="h-3 w-3 mr-1" />
                    {t("urlImport.openBilibili", { defaultValue: "打开 BiliBili 登录" })}
                  </Button>
                </div>
              </>
            ) : error === "LOGIN_REQUIRED" ? (
              <>
                <div className="flex items-center gap-3">
                  <LogIn className="h-5 w-5 text-amber-500" />
                  <span className="text-sm font-medium">
                    {t("urlImport.loginRequired", { defaultValue: "需要登录" })}
                  </span>
                </div>
                <p className="text-xs text-muted-foreground">
                  {t("urlImport.loginRequiredDesc", {
                    defaultValue: "该网站需要登录才能下载内容。请先在 Chrome 浏览器中登录该网站，然后重试。",
                  })}
                </p>
              </>
            ) : (
              <>
                <div className="flex items-center gap-3">
                  <AlertCircle className="h-5 w-5 text-red-500" />
                  <span className="text-sm text-red-600">
                    {t("urlImport.error", { defaultValue: "Import failed" })}
                  </span>
                </div>
                <p className="text-xs text-muted-foreground break-all">{error}</p>
              </>
            )}
            <div className="flex justify-end gap-2">
              <Button variant="outline" onClick={() => setPhase("input")}>
                {t("common.retry", { defaultValue: "Retry" })}
              </Button>
              <Button variant="outline" onClick={onClose}>
                {t("common.close", { defaultValue: "Close" })}
              </Button>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
