use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_shell::ShellExt;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TranscribeResult {
    pub task_id: String,
    pub text: String,
    pub language: String,
    pub duration_secs: f64,
    pub output_path: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WhisperModelInfo {
    pub name: String,
    pub size_bytes: u64,
    pub downloaded: bool,
    pub path: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct TranscribeProgress {
    pub task_id: String,
    pub phase: String,
    pub percent: f32,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ScrapeResult {
    pub title: String,
    pub markdown: String,
    pub url: String,
}

// ---------------------------------------------------------------------------
// Available whisper models
// ---------------------------------------------------------------------------

struct ModelEntry {
    filename: &'static str,
    size_bytes: u64,
}

const WHISPER_MODELS: &[ModelEntry] = &[
    ModelEntry { filename: "ggml-small-q5_1.bin", size_bytes: 199_229_440 },
    ModelEntry { filename: "ggml-medium-q5_0.bin", size_bytes: 565_190_656 },
    ModelEntry { filename: "ggml-large-v3-turbo-q5_0.bin", size_bytes: 601_968_640 },
];

const HF_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct TranscribeState {
    cancel_flags: Arc<Mutex<HashMap<String, bool>>>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn whisper_models_dir(app: &AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .expect("app_data_dir")
        .join("whisper-models");
    fs::create_dir_all(&dir).ok();
    dir
}

fn emit_progress(app: &AppHandle, p: &TranscribeProgress) {
    let _ = app.emit("transcribe://progress", p);
}

fn is_cancelled(state: &TranscribeState, task_id: &str) -> bool {
    state
        .cancel_flags
        .lock()
        .map(|m| m.get(task_id).copied().unwrap_or(false))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Audio extraction via symphonia (blocking)
// ---------------------------------------------------------------------------

fn extract_audio_pcm(file_path: &str) -> Result<(Vec<f32>, u32), String> {
    use symphonia::core::audio::Audio;
    use symphonia::core::codecs::audio::AudioDecoderOptions;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;

    let src = fs::File::open(file_path).map_err(|e| format!("Open failed: {e}"))?;
    let mss = MediaSourceStream::new(Box::new(src), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
    {
        hint.with_extension(ext);
    }

    let probe = symphonia::default::get_probe();
    let mut format = probe
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| format!("Probe failed: {e}"))?;

    // Find the first audio track
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.as_ref().map_or(false, |p| p.is_audio()))
        .ok_or("No audio track found")?
        .clone();

    let codec_params = track
        .codec_params
        .ok_or("Track has no codec params")?;

    let audio_params = codec_params
        .audio()
        .ok_or("Track is not audio")?
        .clone();

    let sample_rate = audio_params.sample_rate.ok_or("Unknown sample rate")?;

    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&audio_params, &AudioDecoderOptions::default())
        .map_err(|e| format!("Decoder creation failed: {e}"))?;

    let mut samples: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(symphonia::core::errors::Error::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(e) => return Err(format!("Packet read error: {e}")),
        };

        if packet.track_id != track.id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let frames = decoded.frames();
                let channels = decoded.spec().channels().count();

                // Access channel data via plane() method
                match decoded {
                    symphonia::core::audio::GenericAudioBufferRef::F32(buf) => {
                        for i in 0..frames {
                            let sum: f32 = (0..channels)
                                .map(|ch| buf.plane(ch).map(|p| p[i]).unwrap_or(0.0))
                                .sum();
                            samples.push(sum / channels as f32);
                        }
                    }
                    symphonia::core::audio::GenericAudioBufferRef::S16(buf) => {
                        for i in 0..frames {
                            let sum: f32 = (0..channels)
                                .map(|ch| buf.plane(ch).map(|p| p[i] as f32 / 32768.0).unwrap_or(0.0))
                                .sum();
                            samples.push(sum / channels as f32);
                        }
                    }
                    symphonia::core::audio::GenericAudioBufferRef::S32(buf) => {
                        for i in 0..frames {
                            let sum: f32 = (0..channels)
                                .map(|ch| buf.plane(ch).map(|p| p[i] as f32 / 2147483648.0).unwrap_or(0.0))
                                .sum();
                            samples.push(sum / channels as f32);
                        }
                    }
                    symphonia::core::audio::GenericAudioBufferRef::U8(buf) => {
                        for i in 0..frames {
                            let sum: f32 = (0..channels)
                                .map(|ch| buf.plane(ch).map(|p| (p[i] as f32 - 128.0) / 128.0).unwrap_or(0.0))
                                .sum();
                            samples.push(sum / channels as f32);
                        }
                    }
                    _ => return Err("Unsupported audio sample format".into()),
                }
            }
            Err(symphonia::core::errors::Error::IoError(_)) => continue,
            Err(e) => return Err(format!("Decode error: {e}")),
        }
    }

    if samples.is_empty() {
        return Err("No audio samples decoded".into());
    }
    Ok((samples, sample_rate))
}

// ---------------------------------------------------------------------------
// Resample to 16kHz mono (blocking)
// ---------------------------------------------------------------------------

fn resample_to_16k(samples: &[f32], src_rate: u32) -> Result<Vec<f32>, String> {
    if src_rate == 16000 {
        return Ok(samples.to_vec());
    }
    use rubato::{Resampler, SincFixedIn};
    let mut resampler = SincFixedIn::<f32>::new(
        16000.0 / src_rate as f64,
        2.0,
        rubato::SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: rubato::SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: rubato::WindowFunction::BlackmanHarris2,
        },
        samples.len(),
        1,
    )
    .map_err(|e| format!("Resampler creation failed: {e}"))?;

    let input = vec![samples.to_vec()];
    let output = resampler
        .process(&input, None)
        .map_err(|e| format!("Resampling failed: {e}"))?;
    Ok(output.into_iter().next().unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Whisper transcription (blocking)
// ---------------------------------------------------------------------------

fn whisper_transcribe(
    app: &AppHandle,
    audio: &[f32],
    model_path: &str,
    language: Option<&str>,
    task_id: &str,
) -> Result<(String, String), String> {
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    emit_progress(app, &TranscribeProgress {
        task_id: task_id.into(),
        phase: "transcribing".into(),
        percent: 0.0,
        message: "Loading model...".into(),
    });

    let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
        .map_err(|e| format!("Model load failed: {e}"))?;

    let mut ws = ctx.create_state().map_err(|e| format!("State creation failed: {e}"))?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    if let Some(lang) = language {
        params.set_language(Some(lang));
    }
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_print_special(false);

    ws.full(params, audio).map_err(|e| format!("Transcription failed: {e}"))?;

    // whisper-rs 0.16: full_n_segments() returns c_int directly
    let n = ws.full_n_segments();
    let mut parts = Vec::new();
    let detected_lang = language.unwrap_or("unknown").to_string();

    for i in 0..n {
        if let Some(seg) = ws.get_segment(i) {
            if let Ok(text) = seg.to_str() {
                parts.push(text.to_string());
            }
        }
    }

    let text = parts.join("").trim().to_string();
    Ok((text, detected_lang))
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Transcribe a local video/audio file.
#[tauri::command]
pub async fn transcribe_file(
    app: AppHandle,
    state: State<'_, TranscribeState>,
    source_path: String,
    model_name: String,
    language: Option<String>,
    output_dir: Option<String>,
) -> Result<TranscribeResult, String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    state.cancel_flags.lock().unwrap().insert(task_id.clone(), false);

    let model_path = whisper_models_dir(&app).join(&model_name);
    if !model_path.exists() {
        return Err(format!("Model '{model_name}' not found. Download it in Settings → Transcription."));
    }

    // Extract audio (blocking)
    emit_progress(&app, &TranscribeProgress {
        task_id: task_id.clone(),
        phase: "extracting_audio".into(),
        percent: 10.0,
        message: "Extracting audio...".into(),
    });

    let sp = source_path.clone();
    let (pcm, rate) = tokio::task::spawn_blocking(move || extract_audio_pcm(&sp))
        .await
        .map_err(|e| format!("Join error: {e}"))??;

    // Resample (blocking)
    emit_progress(&app, &TranscribeProgress {
        task_id: task_id.clone(),
        phase: "extracting_audio".into(),
        percent: 30.0,
        message: "Resampling to 16kHz...".into(),
    });

    let audio_16k = tokio::task::spawn_blocking(move || resample_to_16k(&pcm, rate))
        .await
        .map_err(|e| format!("Join error: {e}"))??;

    // Transcribe (blocking)
    if is_cancelled(&state, &task_id) {
        state.cancel_flags.lock().unwrap().remove(&task_id);
        return Err("Cancelled".into());
    }

    let app2 = app.clone();
    let tid2 = task_id.clone();
    let mp = model_path.to_string_lossy().to_string();
    let lang = language.clone();
    let audio_for_whisper = audio_16k.clone();

    let (text, detected_lang) = tokio::task::spawn_blocking(move || {
        whisper_transcribe(&app2, &audio_for_whisper, &mp, lang.as_deref(), &tid2)
    })
    .await
    .map_err(|e| format!("Join error: {e}"))??;

    if is_cancelled(&state, &task_id) {
        state.cancel_flags.lock().unwrap().remove(&task_id);
        return Err("Cancelled".into());
    }

    // Save
    emit_progress(&app, &TranscribeProgress {
        task_id: task_id.clone(),
        phase: "saving".into(),
        percent: 95.0,
        message: "Saving transcript...".into(),
    });

    let stem = std::path::Path::new(&source_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("transcript");

    let out_dir = output_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(&source_path)
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf()
        });

    let output_path = out_dir.join(format!("{stem}.txt"));
    fs::write(&output_path, &text).map_err(|e| format!("Write failed: {e}"))?;

    let duration = audio_16k.len() as f64 / 16000.0;

    emit_progress(&app, &TranscribeProgress {
        task_id: task_id.clone(),
        phase: "saving".into(),
        percent: 100.0,
        message: "Done".into(),
    });

    state.cancel_flags.lock().unwrap().remove(&task_id);

    Ok(TranscribeResult {
        task_id,
        text,
        language: detected_lang,
        duration_secs: duration,
        output_path: output_path.to_string_lossy().to_string(),
    })
}

/// Find yt-dlp binary - try system PATH, then bundled binary
fn find_ytdlp_binary(app: &AppHandle) -> Result<String, String> {
    // Try system PATH
    if let Ok(path) = which::which("yt-dlp") {
        return Ok(path.to_string_lossy().to_string());
    }

    // Try common locations
    let common_paths = [
        "/usr/local/bin/yt-dlp",
        "/opt/homebrew/bin/yt-dlp",
        "/usr/bin/yt-dlp",
    ];
    for path in &common_paths {
        if std::path::Path::new(path).exists() {
            return Ok(path.to_string());
        }
    }

    // Try bundled binary (dev mode: in project dir, production: in resource dir)
    let bundled = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(format!("yt-dlp-{}", std::env::consts::ARCH));
    if bundled.exists() {
        return Ok(bundled.to_string_lossy().to_string());
    }

    // Try resource dir
    if let Ok(dir) = app.path().resource_dir() {
        let p = dir.join("yt-dlp");
        if p.exists() {
            return Ok(p.to_string_lossy().to_string());
        }
    }

    Err("yt-dlp not found. Install it with: brew install yt-dlp or pip install yt-dlp".into())
}

/// Transcribe a video from URL via yt-dlp.
#[tauri::command]
pub async fn transcribe_url(
    app: AppHandle,
    state: State<'_, TranscribeState>,
    url: String,
    model_name: String,
    language: Option<String>,
    output_dir: String,
) -> Result<TranscribeResult, String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    state.cancel_flags.lock().unwrap().insert(task_id.clone(), false);

    let model_path = whisper_models_dir(&app).join(&model_name);
    if !model_path.exists() {
        state.cancel_flags.lock().unwrap().remove(&task_id);
        return Err(format!("Model '{model_name}' not found. Download it in Settings → Transcription."));
    }

    // Step 1: Download audio with yt-dlp
    emit_progress(&app, &TranscribeProgress {
        task_id: task_id.clone(),
        phase: "downloading_video".into(),
        percent: 0.0,
        message: "Downloading audio...".into(),
    });

    let temp_dir = std::env::temp_dir().join("llm-wiki-transcribe");
    fs::create_dir_all(&temp_dir).ok();
    let out_template = temp_dir
        .join(format!("{}.%(ext)s", task_id))
        .to_string_lossy()
        .to_string();

    // Find yt-dlp binary - try bundled sidecar first, then system PATH
    let ytdlp_path = find_ytdlp_binary(&app)?;

    // Use tokio::process::Command for better control
    let mut cmd = tokio::process::Command::new(&ytdlp_path);
    cmd.args([
        "--extract-audio",
        "--audio-format",
        "wav",
        "--no-playlist",
        "--cookies-from-browser",
        "chrome",
        "--output",
        &out_template,
        "--newline",
        &url,
    ]);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("yt-dlp spawn failed: {e}"))?;

    // Read stdout for progress, collect stderr for error messages
    use tokio::io::{AsyncBufReadExt, BufReader};
    let stderr_handle = child.stderr.take();
    let stdout_handle = child.stdout.take();

    // Spawn stderr reader
    let stderr_task = tokio::spawn(async move {
        let mut stderr_lines = Vec::new();
        if let Some(stderr) = stderr_handle {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                stderr_lines.push(line);
            }
        }
        stderr_lines
    });

    // Read stdout for progress
    if let Some(stdout) = stdout_handle {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if is_cancelled(&state, &task_id) {
                child.kill().await.ok();
                state.cancel_flags.lock().unwrap().remove(&task_id);
                return Err("Cancelled".into());
            }
            if line.contains("[download]") && line.contains('%') {
                if let Some(pct_str) = line.split_whitespace().find(|s| s.ends_with('%')) {
                    if let Ok(pct) = pct_str.trim_end_matches('%').parse::<f32>() {
                        emit_progress(&app, &TranscribeProgress {
                            task_id: task_id.clone(),
                            phase: "downloading_video".into(),
                            percent: pct,
                            message: format!("Downloading... {:.0}%", pct),
                        });
                    }
                }
            }
        }
    }

    let status = child.wait().await.map_err(|e| format!("yt-dlp error: {e}"))?;
    let stderr_output = stderr_task.await.unwrap_or_default();

    if !status.success() {
        state.cancel_flags.lock().unwrap().remove(&task_id);
        let stderr_text = stderr_output.join("\n");

        // Provide user-friendly error messages for common issues
        if stderr_text.contains("412") || stderr_text.contains("Precondition Failed") {
            if url.contains("bilibili.com") || url.contains("b23.tv") {
                return Err("BILIBILI_LOGIN_REQUIRED".into());
            }
            return Err("LOGIN_REQUIRED".into());
        }
        if stderr_text.contains("Sign in to confirm") || stderr_text.contains("login") {
            return Err("LOGIN_REQUIRED".into());
        }

        return Err(format!("yt-dlp exited with code {:?}: {}", status.code(), stderr_text));
    }

    // Find downloaded file
    let audio_path = find_downloaded_file(&temp_dir, &task_id)?;

    // Step 2: Extract + resample + transcribe
    emit_progress(&app, &TranscribeProgress {
        task_id: task_id.clone(),
        phase: "extracting_audio".into(),
        percent: 10.0,
        message: "Extracting audio...".into(),
    });

    let ap = audio_path.to_string_lossy().to_string();
    let (pcm, rate) = tokio::task::spawn_blocking(move || extract_audio_pcm(&ap))
        .await
        .map_err(|e| format!("Join error: {e}"))??;

    emit_progress(&app, &TranscribeProgress {
        task_id: task_id.clone(),
        phase: "extracting_audio".into(),
        percent: 30.0,
        message: "Resampling...".into(),
    });

    let audio_16k = tokio::task::spawn_blocking(move || resample_to_16k(&pcm, rate))
        .await
        .map_err(|e| format!("Join error: {e}"))??;

    if is_cancelled(&state, &task_id) {
        fs::remove_file(&audio_path).ok();
        state.cancel_flags.lock().unwrap().remove(&task_id);
        return Err("Cancelled".into());
    }

    let app2 = app.clone();
    let tid2 = task_id.clone();
    let mp = model_path.to_string_lossy().to_string();
    let lang = language.clone();
    let audio_for_whisper = audio_16k.clone();

    let (text, detected_lang) = tokio::task::spawn_blocking(move || {
        whisper_transcribe(&app2, &audio_for_whisper, &mp, lang.as_deref(), &tid2)
    })
    .await
    .map_err(|e| format!("Join error: {e}"))??;

    // Save
    emit_progress(&app, &TranscribeProgress {
        task_id: task_id.clone(),
        phase: "saving".into(),
        percent: 95.0,
        message: "Saving...".into(),
    });

    let slug = url_to_slug(&url);
    let output_path = PathBuf::from(&output_dir).join(format!("{slug}.txt"));
    fs::write(&output_path, &text).map_err(|e| format!("Write failed: {e}"))?;

    let duration = audio_16k.len() as f64 / 16000.0;

    fs::remove_file(&audio_path).ok();
    state.cancel_flags.lock().unwrap().remove(&task_id);

    emit_progress(&app, &TranscribeProgress {
        task_id: task_id.clone(),
        phase: "saving".into(),
        percent: 100.0,
        message: "Done".into(),
    });

    Ok(TranscribeResult {
        task_id,
        text,
        language: detected_lang,
        duration_secs: duration,
        output_path: output_path.to_string_lossy().to_string(),
    })
}

fn url_to_slug(url: &str) -> String {
    let cleaned = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.");
    let slug: String = cleaned
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let mut s = slug.chars().take(80).collect::<String>();
    while s.ends_with('-') { s.pop(); }
    if s.is_empty() { "video".into() } else { s }
}

fn find_downloaded_file(dir: &std::path::Path, task_id: &str) -> Result<PathBuf, String> {
    fs::read_dir(dir)
        .map_err(|e| format!("Read dir failed: {e}"))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(task_id))
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
        .ok_or("Downloaded audio not found".into())
}

/// Download a whisper model.
#[tauri::command]
pub async fn download_whisper_model(app: AppHandle, model_name: String) -> Result<(), String> {
    let dest = whisper_models_dir(&app).join(&model_name);
    if dest.exists() { return Ok(()); }

    let url = format!("{HF_BASE_URL}/{model_name}");
    let client = reqwest::Client::new();
    let resp = client.get(&url).send().await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let total = resp.content_length().unwrap_or(0);
    let temp = dest.with_extension("bin.downloading");
    let mut file = fs::File::create(&temp).map_err(|e| format!("Create failed: {e}"))?;

    let mut downloaded: u64 = 0;
    use futures::StreamExt;
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download error: {e}"))?;
        std::io::Write::write_all(&mut file, &chunk).map_err(|e| format!("Write error: {e}"))?;
        downloaded += chunk.len() as u64;

        if total > 0 {
            let pct = (downloaded as f32 / total as f32) * 100.0;
            emit_progress(&app, &TranscribeProgress {
                task_id: "model-download".into(),
                phase: "downloading_model".into(),
                percent: pct,
                message: format!("{}: {:.0}% ({} MB / {} MB)", model_name, pct, downloaded / 1_000_000, total / 1_000_000),
            });
        }
    }

    fs::rename(&temp, &dest).map_err(|e| format!("Rename failed: {e}"))?;
    Ok(())
}

/// List whisper models with download status.
#[tauri::command]
pub async fn list_whisper_models(app: AppHandle) -> Result<Vec<WhisperModelInfo>, String> {
    let dir = whisper_models_dir(&app);
    Ok(WHISPER_MODELS.iter().map(|m| {
        let path = dir.join(m.filename);
        let downloaded = path.exists();
        WhisperModelInfo {
            name: m.filename.into(),
            size_bytes: m.size_bytes,
            downloaded,
            path: if downloaded { Some(path.to_string_lossy().into()) } else { None },
        }
    }).collect())
}

/// Delete a downloaded whisper model.
#[tauri::command]
pub async fn delete_whisper_model(app: AppHandle, model_name: String) -> Result<(), String> {
    let path = whisper_models_dir(&app).join(&model_name);
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("Delete failed: {e}"))?;
    }
    Ok(())
}

/// Cancel an in-progress transcription.
#[tauri::command]
pub async fn cancel_transcribe(state: State<'_, TranscribeState>, task_id: String) -> Result<(), String> {
    state.cancel_flags.lock().unwrap().insert(task_id, true);
    Ok(())
}

/// Detect yt-dlp installation and return version info.
#[tauri::command]
pub async fn detect_ytdlp(app: AppHandle) -> Result<YtdlpInfo, String> {
    // Try to find yt-dlp
    let path = which::which("yt-dlp")
        .or_else(|_| which::which("yt-dlp_macos"))
        .map(|p| p.to_string_lossy().to_string())
        .ok();

    // Try common locations if which fails
    let final_path = path.or_else(|| {
        let common = ["/usr/local/bin/yt-dlp", "/opt/homebrew/bin/yt-dlp", "/usr/bin/yt-dlp"];
        common.iter().find(|p| std::path::Path::new(p).exists()).map(|p| p.to_string())
    });

    // Also check bundled binary (for dev mode)
    let final_path = final_path.or_else(|| {
        // Check the binaries directory relative to the project
        let bundled = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(format!("yt-dlp-{}", std::env::consts::ARCH));
        if bundled.exists() {
            Some(bundled.to_string_lossy().to_string())
        } else {
            // Try resource dir for production builds
            app.path().resource_dir().ok().and_then(|dir| {
                let p = dir.join("yt-dlp");
                if p.exists() { Some(p.to_string_lossy().to_string()) } else { None }
            })
        }
    });

    match final_path {
        Some(p) => {
            // Get version
            let output = tokio::process::Command::new(&p)
                .arg("--version")
                .output()
                .await;
            let version = output
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|v| v.trim().to_string())
                .unwrap_or_else(|| "unknown".into());

            Ok(YtdlpInfo {
                installed: true,
                path: Some(p),
                version: Some(version),
                error: None,
            })
        }
        None => Ok(YtdlpInfo {
            installed: false,
            path: None,
            version: None,
            error: Some("yt-dlp not found".into()),
        }),
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct YtdlpInfo {
    pub installed: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub error: Option<String>,
}

/// Scrape a web article URL via Firecrawl.
#[tauri::command]
pub async fn scrape_url(url: String) -> Result<ScrapeResult, String> {
    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.firecrawl.dev/v1/scrape")
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "url": url,
            "formats": ["markdown"],
            "onlyMainContent": true,
        }))
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {s}: {b}"));
    }

    let data: serde_json::Value = resp.json().await
        .map_err(|e| format!("Parse error: {e}"))?;

    let title = data["data"]["metadata"]["title"].as_str().unwrap_or("Untitled").into();
    let markdown = data["data"]["markdown"].as_str()
        .ok_or("No markdown in response")?
        .into();

    Ok(ScrapeResult { title, markdown, url })
}
