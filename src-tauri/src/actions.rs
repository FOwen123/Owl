#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use crate::apple_intelligence;
use crate::audio_feedback::{play_feedback_sound, play_feedback_sound_blocking, SoundType};
use crate::audio_toolkit::VadPolicy;
use crate::managers::audio::{recording_error_type, AudioRecordingManager};
use crate::managers::history::HistoryManager;
use crate::managers::model::ModelManager;
use crate::managers::transcription::StreamWorkKind;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::{
    get_settings, AppSettings, CleanupLevel, OverlayStyle, PostProcessProvider,
    APPLE_INTELLIGENCE_PROVIDER_ID,
};
use crate::shortcut;
use crate::tray::{set_tray_state, TrayIconState};
use crate::utils::{
    self, show_processing_overlay, show_recording_overlay, show_transcribing_overlay,
};
use crate::TranscriptionCoordinator;
use log::{debug, error, warn};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::Manager;
use tauri::{AppHandle, Emitter};

const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Clone, serde::Serialize)]
struct RecordingErrorEvent {
    error_type: String,
    detail: Option<String>,
}

/// Drop guard that finishes the transcription pipeline, including immediate
/// model unloading on early exits.
struct FinishGuard(AppHandle, Arc<TranscriptionManager>);
impl Drop for FinishGuard {
    fn drop(&mut self) {
        self.1.maybe_unload_immediately("transcription session");
        if let Some(c) = self.0.try_state::<TranscriptionCoordinator>() {
            c.notify_processing_finished();
        }
        // The pipeline just freed its large transient buffers (captured PCM,
        // WAV copy, engine scratch); hand the cached pages back to the OS so
        // they don't sit in malloc arenas until they get swapped out (#1792).
        crate::memory::trim_freed_memory();
    }
}

// Shortcut Action Trait
pub trait ShortcutAction: Send + Sync {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
}

// Transcribe Action
struct TranscribeAction {
    post_process: bool,
    /// Owl: Edit by voice. What's said is an instruction for the Post
    /// Process AI, applied to the selected text or answered at the cursor.
    edit: bool,
}

/// Owl (Edit by voice): where the edit goes, as it was when the shortcut
/// went down: the app in front, and its text box, selected range and text.
/// The result is only pasted if all of that is unchanged.
struct EditTarget {
    app: Option<i32>,
    spot: (u64, (isize, isize), String),
}

impl EditTarget {
    /// What the AI changes; None when nothing is selected.
    fn text(&self) -> Option<String> {
        Some(self.spot.2.clone()).filter(|t| !t.trim().is_empty())
    }
}

static EDIT_TARGET: std::sync::Mutex<Option<EditTarget>> = std::sync::Mutex::new(None);

const EDIT_PROMPT: &str = "You edit text by voice. The user's spoken instruction is in <instruction>. \
If <selected_text> is given, rewrite it as the instruction asks and reply with only the rewritten text, in its language unless the instruction asks for another. \
If there is no selected text, reply with only the text the instruction asks for (an answer, a draft, a translation), ready to insert where the user is typing. \
Never add explanations, quotes or code fences. \
Keep the selected text's form: a sentence stays a sentence and a message never becomes a letter, so never add greetings, sign-offs or placeholders such as [Name] \
(\"gonna be late, traffic's bad\" made formal is \"I will be late because the traffic is heavy.\").";

/// Field name for structured output JSON schema
const TRANSCRIPTION_FIELD: &str = "transcription";

/// Strip invisible Unicode characters that some LLMs may insert
fn strip_invisible_chars(s: &str) -> String {
    s.replace(['\u{200B}', '\u{200C}', '\u{200D}', '\u{FEFF}'], "")
}

/// Strip a leading `<think>...</think>` block. Some endpoints can't disable
/// reasoning, and some local servers put the reasoning text into `content`
/// instead of a separate field — without this the user would get the model's
/// chain of thought pasted along with the cleaned transcription.
pub(crate) fn strip_think_block(s: &str) -> &str {
    if let Some(rest) = s.trim_start().strip_prefix("<think>") {
        if let Some(end) = rest.find("</think>") {
            return rest[end + "</think>".len()..].trim_start();
        }
    }
    s
}

/// Build a system prompt from the user's prompt template.
/// Removes `${output}` placeholder since the transcription is sent as the user message.
/// Handy's prompts wrap `${output}` in transcript tags.
static TRANSCRIPT_BLOCK: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
    regex::Regex::new(r"(?s)<transcript>\s*\$\{output\}\s*</transcript>").expect("valid regex")
});

fn tagged_transcript(transcript: &str) -> String {
    format!("<transcript>\n{transcript}\n</transcript>")
}

/// Owl: the prompt's instructions as the system message and the transcript,
/// in tags, as the user message. Removing `${output}` alone used to leave an
/// empty `<transcript></transcript>` in the instructions, telling the model
/// the transcript was empty.
fn prompt_messages(template: &str, transcript: &str) -> (String, String) {
    let system = TRANSCRIPT_BLOCK
        .replace_all(template, "")
        .replace("${output}", "")
        .trim()
        .to_string();
    (system, tagged_transcript(transcript))
}

/// Owl: one-message prompt for providers without system messages. A prompt
/// without `${output}` gets the transcript appended instead of dropped.
fn legacy_prompt(template: &str, transcript: &str) -> String {
    if template.contains("${output}") {
        template.replace("${output}", transcript)
    } else {
        format!("{}\n\n{}", template.trim(), tagged_transcript(transcript))
    }
}

/// Owl: the AI cleanup runs at Light and Medium, and only while Post Process
/// is on, so a cleanup level alone never starts sending dictations out.
fn ai_cleanup_on(settings: &AppSettings) -> bool {
    settings.post_process_enabled && settings.cleanup_level != CleanupLevel::None
}

/// Owl: spoken lists, as Wispr Flow formats them. The AI decides what is a
/// list; `cleanup_rules::format_lists` then puts each item on its own line.
const LIST_RULE: &str = "When the speaker lists items (\"one … two …\", \"first … second …\", 第一…第二…), number them: 1. … 2. …";

/// Owl: both levels format lists; Medium also lets the AI reword.
fn prompt_for_level(prompt: &str, level: CleanupLevel) -> String {
    match level {
        CleanupLevel::Medium => format!(
            "{prompt}\n\n{LIST_RULE}\nYou may reword for clarity and conciseness, keeping the meaning."
        ),
        _ => format!("{prompt}\n\n{LIST_RULE}"),
    }
}

/// Owl: with Post Process on, the normal shortcut cleans up every dictation
/// (as Felix and Muesli do) instead of needing a second shortcut.
fn wants_post_process(binding_asks: bool, post_process_enabled: bool) -> bool {
    binding_asks || post_process_enabled
}

/// Returns `true` when a transcription has no meaningful content to
/// post-process (empty or whitespace-only). Used to skip the post-processing
/// LLM call when nothing was actually transcribed, which would otherwise make
/// the model reply with an error message such as "you need to provide the
/// transcription".
fn is_blank_transcription(transcription: &str) -> bool {
    transcription.trim().is_empty()
}

async fn complete_unless_cancelled<F, C>(operation: F, is_cancelled: C) -> Option<F::Output>
where
    F: Future,
    C: Fn() -> bool,
{
    tokio::pin!(operation);

    loop {
        if is_cancelled() {
            return None;
        }

        if let Ok(result) =
            tokio::time::timeout(CANCELLATION_POLL_INTERVAL, operation.as_mut()).await
        {
            return Some(result);
        }
    }
}

fn should_use_streaming_overlay(style: OverlayStyle, is_streaming: bool) -> bool {
    style == OverlayStyle::Live && is_streaming
}

pub(crate) async fn post_process_transcription(
    settings: &AppSettings,
    transcription: &str,
) -> Option<String> {
    if is_blank_transcription(transcription) {
        debug!("Post-processing skipped because the transcription is empty");
        return None;
    }

    let provider = match settings.active_post_process_provider().cloned() {
        Some(provider) => provider,
        None => {
            debug!("Post-processing enabled but no provider is selected");
            return None;
        }
    };

    let model = settings
        .post_process_models
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();

    if model.trim().is_empty() {
        debug!(
            "Post-processing skipped because provider '{}' has no model configured",
            provider.id
        );
        return None;
    }

    let selected_prompt_id = match &settings.post_process_selected_prompt_id {
        Some(id) => id.clone(),
        None => {
            debug!("Post-processing skipped because no prompt is selected");
            return None;
        }
    };

    let prompt = match settings
        .post_process_prompts
        .iter()
        .find(|prompt| prompt.id == selected_prompt_id)
    {
        Some(prompt) => prompt.prompt.clone(),
        None => {
            debug!(
                "Post-processing skipped because prompt '{}' was not found",
                selected_prompt_id
            );
            return None;
        }
    };

    if prompt.trim().is_empty() {
        debug!("Post-processing skipped because the selected prompt is empty");
        return None;
    }
    let prompt = prompt_for_level(&prompt, settings.cleanup_level);

    debug!(
        "Starting LLM post-processing with provider '{}' (model: {})",
        provider.id, model
    );

    let api_key = settings
        .post_process_api_keys
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();

    // Ask these providers to skip reasoning/thinking — post-processing rarely
    // benefits from it and it adds seconds of latency. llm_client picks the
    // field the endpoint understands and retries without it if rejected.
    let disable_reasoning = matches!(provider.id.as_str(), "custom" | "openrouter");

    if provider.supports_structured_output {
        debug!("Using structured outputs for provider '{}'", provider.id);

        let (system_prompt, user_content) = prompt_messages(&prompt, transcription);

        // Handle Apple Intelligence separately since it uses native Swift APIs
        if provider.id == APPLE_INTELLIGENCE_PROVIDER_ID {
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            {
                if !apple_intelligence::check_apple_intelligence_availability() {
                    debug!(
                        "Apple Intelligence selected but not currently available on this device"
                    );
                    return None;
                }

                let token_limit = model.trim().parse::<i32>().unwrap_or(0);
                return match apple_intelligence::process_text_with_system_prompt(
                    &system_prompt,
                    &user_content,
                    token_limit,
                ) {
                    Ok(result) => {
                        if result.trim().is_empty() {
                            debug!("Apple Intelligence returned an empty response");
                            None
                        } else {
                            let result = strip_invisible_chars(&result);
                            debug!(
                                "Apple Intelligence post-processing succeeded. Output length: {} chars",
                                result.len()
                            );
                            Some(result)
                        }
                    }
                    Err(err) => {
                        error!("Apple Intelligence post-processing failed: {}", err);
                        None
                    }
                };
            }

            #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
            {
                debug!("Apple Intelligence provider selected on unsupported platform");
                return None;
            }
        }

        // Define JSON schema for transcription output
        let json_schema = serde_json::json!({
            "type": "object",
            "properties": {
                (TRANSCRIPTION_FIELD): {
                    "type": "string",
                    "description": "The cleaned and processed transcription text"
                }
            },
            "required": [TRANSCRIPTION_FIELD],
            "additionalProperties": false
        });

        match crate::llm_client::send_chat_completion_with_schema(
            &provider,
            api_key.clone(),
            &model,
            user_content,
            Some(system_prompt),
            Some(json_schema),
            disable_reasoning,
        )
        .await
        {
            Ok(Some(content)) => {
                // Parse the JSON response to extract the transcription field
                let content = strip_think_block(&content);
                match serde_json::from_str::<serde_json::Value>(content) {
                    Ok(json) => {
                        if let Some(transcription_value) =
                            json.get(TRANSCRIPTION_FIELD).and_then(|t| t.as_str())
                        {
                            let result = strip_invisible_chars(transcription_value);
                            debug!(
                                "Structured output post-processing succeeded for provider '{}'. Output length: {} chars",
                                provider.id,
                                result.len()
                            );
                            return Some(result);
                        } else {
                            // Owl: never paste the model's JSON; fall back to the rule-cleaned text.
                            error!("Structured output response missing 'transcription' field");
                            return None;
                        }
                    }
                    Err(e) => {
                        error!(
                            "Failed to parse structured output JSON: {}. Returning raw content.",
                            e
                        );
                        return Some(strip_invisible_chars(content));
                    }
                }
            }
            Ok(None) => {
                error!("LLM API response has no content");
                return None;
            }
            Err(e) => {
                warn!(
                    "Structured output failed for provider '{}': {}. Falling back to legacy mode.",
                    provider.id, e
                );
                // Fall through to legacy mode below
            }
        }
    }

    // Legacy mode: one message with the transcript in it
    let processed_prompt = legacy_prompt(&prompt, transcription);
    debug!("Processed prompt length: {} chars", processed_prompt.len());

    match crate::llm_client::send_chat_completion(
        &provider,
        api_key,
        &model,
        processed_prompt,
        disable_reasoning,
    )
    .await
    {
        Ok(Some(content)) => {
            let content = strip_invisible_chars(strip_think_block(&content));
            debug!(
                "LLM post-processing succeeded for provider '{}'. Output length: {} chars",
                provider.id,
                content.len()
            );
            Some(content)
        }
        Ok(None) => {
            error!("LLM API response has no content");
            None
        }
        Err(e) => {
            error!(
                "LLM post-processing failed for provider '{}': {}. Falling back to original transcription.",
                provider.id,
                e
            );
            None
        }
    }
}

/// Owl: the Post Process AI and its model, as Summarize uses them.
fn edit_ai(settings: &AppSettings) -> Option<(PostProcessProvider, String)> {
    let provider = settings.active_post_process_provider()?;
    let model = settings
        .post_process_models
        .get(&provider.id)
        .filter(|m| !m.trim().is_empty())?;
    Some((provider.clone(), model.clone()))
}

#[cfg(target_os = "macos")]
fn frontmost_app() -> Option<i32> {
    objc2_app_kit::NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|app| app.processIdentifier())
}

#[cfg(not(target_os = "macos"))]
fn frontmost_app() -> Option<i32> {
    None
}

/// Owl: where an edit would go, through Accessibility. None in apps that
/// hide their text, where Owl can't tell what's selected.
fn edit_target() -> Option<EditTarget> {
    let app = frontmost_app();
    if let Some(pid) = app {
        crate::correction_watch::expose_text(pid);
    }
    let spot = crate::correction_watch::selection_spot().or_else(|| {
        // An Electron app builds its text tree a moment after being asked.
        std::thread::sleep(Duration::from_millis(200));
        crate::correction_watch::selection_spot()
    })?;
    Some(EditTarget { app, spot })
}

/// Whether the same app is in front with the same text box and selection,
/// so the edit lands where it was asked for.
fn still_there(target: &EditTarget) -> bool {
    frontmost_app() == target.app
        && crate::correction_watch::selection_spot().as_ref() == Some(&target.spot)
}

/// Owl: the AI's rewrite of `selection`, or its answer when there's none.
async fn edit_text(
    settings: &AppSettings,
    instruction: &str,
    selection: Option<&str>,
) -> Result<String, String> {
    let (provider, model) = edit_ai(settings).ok_or("Choose an AI on the Post Process page")?;
    let mut user = format!("<instruction>\n{instruction}\n</instruction>");
    if let Some(text) = selection {
        user = format!("<selected_text>\n{text}\n</selected_text>\n{user}");
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    if provider.id == APPLE_INTELLIGENCE_PROVIDER_ID {
        let token_limit = model.trim().parse::<i32>().unwrap_or(0);
        return apple_intelligence::process_text_with_system_prompt(
            EDIT_PROMPT,
            &user,
            token_limit,
        )
        .map(|t| strip_invisible_chars(&t).trim().to_string());
    }
    let api_key = settings
        .post_process_api_keys
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();
    let disable_reasoning = matches!(provider.id.as_str(), "custom" | "openrouter");
    let reply = crate::llm_client::send_chat_completion_with_schema(
        &provider,
        api_key,
        &model,
        user,
        Some(EDIT_PROMPT.to_string()),
        None,
        disable_reasoning,
    )
    .await?
    .unwrap_or_default();
    Ok(strip_invisible_chars(strip_think_block(&reply))
        .trim()
        .to_string())
}

/// Owl: what Edit by voice pastes. A failure is shown and nothing is pasted.
async fn edited(
    app: &AppHandle,
    instruction: &str,
    selection: Option<String>,
) -> ProcessedTranscription {
    let text = match is_blank_transcription(instruction) {
        true => String::new(),
        false => edit_text(&get_settings(app), instruction, selection.as_deref())
            .await
            .unwrap_or_else(|e| {
                error!("Edit by voice failed: {e}");
                let _ = app.emit("transcription-error", e);
                String::new()
            }),
    };
    ProcessedTranscription {
        post_processed_text: Some(text.clone()).filter(|t| !t.is_empty()),
        final_text: text,
        post_process_prompt: None,
    }
}

pub(crate) struct ProcessedTranscription {
    pub final_text: String,
    pub post_processed_text: Option<String>,
    pub post_process_prompt: Option<String>,
}

pub(crate) async fn process_transcription_output(
    app: &AppHandle,
    transcription: &str,
    post_process: bool,
) -> ProcessedTranscription {
    let settings = get_settings(app);

    // Owl: a snippet trigger pastes its saved text as-is, skipping AI cleanup.
    // History keeps the spoken trigger and records the pasted text.
    if let Some(text) = crate::snippets::match_snippet(transcription, &settings.snippets) {
        return ProcessedTranscription {
            final_text: text.to_string(),
            post_processed_text: Some(text.to_string()),
            post_process_prompt: None,
        };
    }

    let mut final_text = transcription.to_string();
    let mut post_processed_text: Option<String> = None;
    let mut post_process_prompt: Option<String> = None;

    if post_process && crate::cleanup_guard::worth_cleaning(&final_text) {
        // Owl: a slow or off-topic AI reply falls back to the rule-cleaned text.
        let limit = crate::cleanup_guard::time_limit(&final_text);
        let reply = tokio::time::timeout(limit, post_process_transcription(&settings, &final_text))
            .await
            .unwrap_or_else(|_| {
                debug!("AI cleanup took longer than {limit:?}; pasting the rule-cleaned text");
                None
            })
            .and_then(|reply| {
                crate::cleanup_guard::check(&final_text, &reply)
                    .inspect_err(|why| debug!("AI cleanup reply rejected: {why}"))
                    .ok()
            });
        if let Some(processed_text) = reply {
            // Owl: the dictionary runs again in case the AI rewrote a fixed word.
            let processed_text =
                crate::dictionary::apply_replacements(&processed_text, &settings.replacements);
            let processed_text = crate::cleanup_rules::format_lists(&processed_text);
            post_processed_text = Some(processed_text.clone());
            final_text = processed_text;

            if let Some(prompt_id) = &settings.post_process_selected_prompt_id {
                if let Some(prompt) = settings
                    .post_process_prompts
                    .iter()
                    .find(|prompt| &prompt.id == prompt_id)
                {
                    post_process_prompt = Some(prompt.prompt.clone());
                }
            }
        }
    }

    ProcessedTranscription {
        final_text,
        post_processed_text,
        post_process_prompt,
    }
}

impl ShortcutAction for TranscribeAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let start_time = Instant::now();
        debug!("TranscribeAction::start called for binding: {}", binding_id);

        if self.edit {
            if edit_ai(&get_settings(app)).is_none() {
                crate::show_main_window(app);
                let _ = app.emit("edit-needs-ai", ());
                return;
            }
            let Some(target) = edit_target() else {
                crate::show_main_window(app);
                let _ = app.emit("edit-cant-see-text", ());
                return;
            };
            // The last dictation's watcher would take the edit for a fix.
            crate::correction_watch::stop();
            *EDIT_TARGET.lock().unwrap_or_else(|e| e.into_inner()) = Some(target);
        }

        // Load model in the background
        let tm = app.state::<Arc<TranscriptionManager>>();
        let rm = app.state::<Arc<AudioRecordingManager>>();

        // Load ASR model and VAD model in parallel
        let kickoff_started = Instant::now();
        tm.initiate_model_load();
        let rm_clone = Arc::clone(&rm);
        std::thread::spawn(move || {
            if let Err(e) = rm_clone.preload_vad() {
                debug!("VAD pre-load failed: {}", e);
            }
        });
        let kickoff_elapsed = kickoff_started.elapsed();

        // Don't open the mic if nothing can transcribe the recording; the load
        // kicked off above fails and reports why.
        if !tm.is_model_loaded() {
            let selected_model = get_settings(app).selected_model;
            if let Err(e) = app
                .state::<Arc<ModelManager>>()
                .get_model_path(&selected_model)
            {
                warn!("Not starting recording: no model can transcribe it ({})", e);
                return;
            }
        }

        let binding_id = binding_id.to_string();
        let tray_started = Instant::now();
        set_tray_state(app, TrayIconState::Recording);
        let tray_elapsed = tray_started.elapsed();

        // Get the microphone mode to determine audio feedback timing
        let plan_started = Instant::now();
        let settings = get_settings(app);
        let is_always_on = settings.always_on_microphone;

        let selected_model_info = app
            .state::<Arc<ModelManager>>()
            .get_model_info(&settings.selected_model);

        // Use the app-facing model capability as the single pre-recording source
        // for live streaming decisions. Unknown support is represented as false
        // until the model registry is updated by discovery or runtime load.
        let model_supports_streaming = selected_model_info
            .as_ref()
            .map(|m| m.supports_streaming)
            .unwrap_or(false);
        let vad_policy = if !settings.vad_enabled {
            VadPolicy::Disabled
        } else if model_supports_streaming {
            VadPolicy::Streaming
        } else {
            VadPolicy::Offline
        };
        if model_supports_streaming {
            tm.start_stream();
        }
        let plan_elapsed = plan_started.elapsed();

        // Sizing the overlay follows the same advertised capability. A model that
        // doesn't stream (or whose capability is not known yet) gets the compact
        // pill instead of an oversized transparent live window.
        let overlay_started = Instant::now();
        match settings.overlay_style {
            OverlayStyle::Live if model_supports_streaming => utils::show_streaming_overlay(app),
            OverlayStyle::Live | OverlayStyle::Minimal => show_recording_overlay(app),
            OverlayStyle::None => {} // show_overlay_state no-ops on None anyway
        }
        // Everything above runs before capture can begin, so each span here is
        // added keypress->capture latency.
        debug!(
            "start-path pre-recording steps: model_kickoff={:?} tray={:?} settings+stream_plan={:?} overlay={:?}",
            kickoff_elapsed,
            tray_elapsed,
            plan_elapsed,
            overlay_started.elapsed()
        );
        debug!("Microphone mode - always_on: {}", is_always_on);

        let mut recording_error: Option<anyhow::Error> = None;
        let recording_start_time = Instant::now();
        match rm.try_start_recording(&binding_id, vad_policy) {
            Ok(readiness) => {
                debug!(
                    "Recording request accepted in {:?}; waiting for first microphone samples",
                    recording_start_time.elapsed()
                );
                let generation = readiness.generation();
                let app_clone = app.clone();
                let rm_clone = Arc::clone(&rm);
                std::thread::spawn(move || {
                    if !readiness.wait() {
                        debug!("Microphone readiness wait ended without receiving samples");
                        return;
                    }

                    // Development-only preview hook for evaluating the brief
                    // arming animation on hardware that normally starts too fast
                    // to make it visible.
                    #[cfg(debug_assertions)]
                    if let Ok(delay_ms) = std::env::var("HANDY_DEBUG_MIC_READY_DELAY_MS")
                        .unwrap_or_default()
                        .parse::<u64>()
                    {
                        let delay_ms = delay_ms.min(10_000);
                        if delay_ms > 0 {
                            debug!("Delaying microphone-ready cue by {delay_ms}ms for UI preview");
                            std::thread::sleep(Duration::from_millis(delay_ms));
                        }
                    }

                    if !rm_clone.is_recording_readiness_current(generation) {
                        debug!("Microphone became ready for an inactive recording");
                        return;
                    }

                    debug!("Microphone is receiving samples; recording is ready");
                    utils::emit_recording_ready(&app_clone);

                    // The start chime is a readiness cue, so it must follow the
                    // first real input callback rather than Stream::play() or a
                    // fixed delay. The helper returns immediately when feedback
                    // is disabled; mute still follows the same readiness point.
                    if rm_clone.is_recording_readiness_current(generation) {
                        play_feedback_sound_blocking(&app_clone, SoundType::Start);
                    }
                    if rm_clone.is_recording_readiness_current(generation) {
                        rm_clone.apply_mute();
                    }
                });
            }
            Err(e) => {
                debug!("Failed to start recording: {}", e);
                recording_error = Some(e);
            }
        }

        if recording_error.is_none() {
            // Dynamically register the cancel shortcut in a separate task to avoid deadlock
            shortcut::register_cancel_shortcut(app);
        } else {
            // Starting failed (for example due to blocked microphone permissions).
            // Revert UI state so we don't stay stuck in the recording overlay.
            tm.cancel_stream();
            utils::hide_recording_overlay(app);
            set_tray_state(app, TrayIconState::Idle);
            if let Some(err) = recording_error {
                let _ = app.emit(
                    "recording-error",
                    RecordingErrorEvent {
                        error_type: recording_error_type(&err).to_string(),
                        detail: Some(format!("{err:#}")),
                    },
                );
            }
        }

        debug!(
            "TranscribeAction::start completed in {:?}",
            start_time.elapsed()
        );
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        // Prevent a slow microphone from emitting a ready event or start chime
        // after the user has already requested stop.
        app.state::<Arc<AudioRecordingManager>>()
            .invalidate_recording_readiness();

        // Unregister the cancel shortcut when transcription stops
        shortcut::unregister_cancel_shortcut(app);

        let stop_time = Instant::now();
        debug!("TranscribeAction::stop called for binding: {}", binding_id);

        let ah = app.clone();
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
        let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
        let hm = Arc::clone(&app.state::<Arc<HistoryManager>>());

        set_tray_state(app, TrayIconState::Transcribing);
        // Stop should give immediate visual feedback. Live streaming can keep
        // the larger panel, but it still switches from listening to a working
        // spinner while the stream finalizes. Non-streaming paths use the
        // compact transcribing pill (None no-ops in show_*).
        let style = get_settings(app).overlay_style;
        // Capture this before finalizing the stream so every later working state
        // targets the same overlay that was shown for this transcription.
        let use_streaming_overlay = should_use_streaming_overlay(style, tm.is_streaming());
        if use_streaming_overlay {
            tm.emit_stream_working(StreamWorkKind::Transcribing);
        } else {
            show_transcribing_overlay(app);
        }

        // Unmute before playing audio feedback so the stop sound is audible
        rm.remove_mute();

        // Play audio feedback for recording stop
        play_feedback_sound(app, SoundType::Stop);

        let binding_id = binding_id.to_string(); // Clone binding_id for the async task
        let post_process = wants_post_process(self.post_process, ai_cleanup_on(&get_settings(app)));
        let edit = self.edit;
        let cancel_generation = rm.cancel_generation();

        tauri::async_runtime::spawn(async move {
            let _guard = FinishGuard(ah.clone(), Arc::clone(&tm));
            debug!(
                "Starting async transcription task for binding: {}",
                binding_id
            );

            let stop_recording_time = Instant::now();
            if let Some(samples) = rm.stop_recording(&binding_id, cancel_generation) {
                debug!(
                    "Recording stopped and samples retrieved in {:?}, sample count: {}",
                    stop_recording_time.elapsed(),
                    samples.len()
                );

                if rm.was_cancelled_since(cancel_generation) {
                    debug!("Transcription operation cancelled after recording stop");
                    tm.cancel_stream();
                    utils::hide_recording_overlay(&ah);
                    set_tray_state(&ah, TrayIconState::Idle);
                    return;
                }

                if samples.is_empty() {
                    debug!("Recording produced no audio samples; skipping persistence");
                    // Tear down any streaming worker so its channel doesn't leak
                    // and block the next start_stream.
                    tm.cancel_stream();
                    utils::hide_recording_overlay(&ah);
                    set_tray_state(&ah, TrayIconState::Idle);
                } else {
                    // Save WAV concurrently with transcription
                    let sample_count = samples.len();
                    let file_name = format!("handy-{}.wav", chrono::Utc::now().timestamp());
                    let wav_path = hm.recordings_dir().join(&file_name);
                    let wav_path_for_verify = wav_path.clone();
                    let samples_for_wav = samples.clone();
                    let wav_handle = tauri::async_runtime::spawn_blocking(move || {
                        crate::audio_toolkit::save_wav_file(&wav_path, &samples_for_wav)
                    });

                    // Transcribe concurrently with WAV save. If a live stream was
                    // running, finalize it and use its text (all audio was already
                    // fed to the stream); otherwise batch-transcribe the samples.
                    let transcription_time = Instant::now();
                    let transcription_result = match tm.finalize_stream() {
                        // A finalized stream with usable text wins. An empty result
                        // (no active stream, produced nothing, or the stream failed
                        // or its worker crashed) falls back to a full batch
                        // transcription of the same audio. A cancelled finalize is
                        // surfaced instead, so a cancel never starts a batch run.
                        Ok(Some(text)) if !text.trim().is_empty() => Ok(text),
                        Ok(_) => tm.transcribe(samples),
                        Err(err) => Err(err),
                    };

                    // Await WAV save and verify
                    let wav_saved = match wav_handle.await {
                        Ok(Ok(())) => {
                            match crate::audio_toolkit::verify_wav_file(
                                &wav_path_for_verify,
                                sample_count,
                            ) {
                                Ok(()) => true,
                                Err(e) => {
                                    error!("WAV verification failed: {}", e);
                                    false
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            error!("Failed to save WAV file: {}", e);
                            false
                        }
                        Err(e) => {
                            error!("WAV save task panicked: {}", e);
                            false
                        }
                    };

                    if rm.was_cancelled_since(cancel_generation) {
                        debug!("Transcription operation cancelled before output handling");
                        utils::hide_recording_overlay(&ah);
                        set_tray_state(&ah, TrayIconState::Idle);
                        return;
                    }

                    match transcription_result {
                        Ok(transcription) => {
                            debug!(
                                "Transcription completed in {:?}: '{}'",
                                transcription_time.elapsed(),
                                utils::redact_text(&transcription)
                            );

                            if post_process || edit {
                                if use_streaming_overlay {
                                    tm.emit_stream_working(StreamWorkKind::Polishing);
                                } else {
                                    show_processing_overlay(&ah);
                                }
                            }
                            let target = edit
                                .then(|| {
                                    EDIT_TARGET.lock().unwrap_or_else(|e| e.into_inner()).take()
                                })
                                .flatten();
                            let output = async {
                                if let Some(target) = &target {
                                    edited(&ah, &transcription, target.text()).await
                                } else {
                                    process_transcription_output(&ah, &transcription, post_process)
                                        .await
                                }
                            };
                            let Some(processed) = complete_unless_cancelled(output, || {
                                rm.was_cancelled_since(cancel_generation)
                            })
                            .await
                            else {
                                debug!("Transcription operation cancelled during output handling");
                                utils::hide_recording_overlay(&ah);
                                set_tray_state(&ah, TrayIconState::Idle);
                                return;
                            };

                            if rm.was_cancelled_since(cancel_generation) {
                                debug!("Transcription operation cancelled before paste");
                                utils::hide_recording_overlay(&ah);
                                set_tray_state(&ah, TrayIconState::Idle);
                                return;
                            }

                            // Save to history if WAV was saved
                            if wav_saved {
                                if let Err(err) = hm.save_entry(
                                    file_name,
                                    transcription,
                                    post_process,
                                    processed.post_processed_text.clone(),
                                    processed.post_process_prompt.clone(),
                                ) {
                                    error!("Failed to save history entry: {}", err);
                                }
                            }

                            if processed.final_text.is_empty() {
                                utils::hide_recording_overlay(&ah);
                                set_tray_state(&ah, TrayIconState::Idle);
                            } else {
                                let ah_clone = ah.clone();
                                let paste_time = Instant::now();
                                let final_text = processed.final_text;
                                let rm_for_paste = Arc::clone(&rm);
                                // Owl: dropped when the paste is over, however it ends.
                                let (pasting, pasted) = std::sync::mpsc::channel::<()>();
                                ah.run_on_main_thread(move || {
                                    let _pasting = pasting;
                                    if rm_for_paste.was_cancelled_since(cancel_generation) {
                                        debug!("Transcription operation cancelled before paste");
                                        utils::hide_recording_overlay(&ah_clone);
                                        set_tray_state(&ah_clone, TrayIconState::Idle);
                                        return;
                                    }

                                    let check = target.as_ref().map(|t| move || still_there(t));
                                    let check = check.as_ref().map(|f| f as &dyn Fn() -> bool);
                                    match utils::paste(final_text.clone(), ah_clone.clone(), check)
                                    {
                                        Ok(()) => {
                                            debug!(
                                                "Text pasted successfully in {:?}",
                                                paste_time.elapsed()
                                            );
                                            // An edit's result isn't a dictation to learn from.
                                            if !edit {
                                                crate::correction_watch::start(
                                                    ah_clone.clone(),
                                                    final_text,
                                                );
                                            }
                                        }
                                        Err(e) if e == utils::TARGET_CHANGED => {
                                            let _ = ah_clone.emit("edit-not-pasted", ());
                                        }
                                        Err(e) => {
                                            error!("Failed to paste transcription: {}", e);
                                            let _ = ah_clone.emit("paste-error", ());
                                        }
                                    }
                                    utils::hide_recording_overlay(&ah_clone);
                                    set_tray_state(&ah_clone, TrayIconState::Idle);
                                })
                                .unwrap_or_else(|e| {
                                    error!("Failed to run paste on main thread: {:?}", e);
                                    utils::hide_recording_overlay(&ah);
                                    set_tray_state(&ah, TrayIconState::Idle);
                                });
                                // Owl: stay busy until the paste has happened, so an
                                // edit started meanwhile can't have its selection
                                // pasted over by this dictation.
                                let _ = tauri::async_runtime::spawn_blocking(move || {
                                    let _ = pasted.recv();
                                })
                                .await;
                            }
                        }
                        Err(err) => {
                            if rm.was_cancelled_since(cancel_generation) {
                                debug!(
                                    "Transcription operation cancelled after transcription error"
                                );
                                utils::hide_recording_overlay(&ah);
                                set_tray_state(&ah, TrayIconState::Idle);
                                return;
                            }

                            error!("Transcription failed: {}", err);
                            // Surface the failure to the UI (toast). The full
                            // message is also in handy.log via the line above.
                            let _ = ah.emit("transcription-error", err.to_string());
                            // Save entry with empty text so user can retry
                            if wav_saved {
                                if let Err(save_err) = hm.save_entry(
                                    file_name,
                                    String::new(),
                                    post_process,
                                    None,
                                    None,
                                ) {
                                    error!("Failed to save failed history entry: {}", save_err);
                                }
                            }
                            utils::hide_recording_overlay(&ah);
                            set_tray_state(&ah, TrayIconState::Idle);
                        }
                    }
                }
            } else {
                debug!("No samples retrieved from recording stop");
                // Tear down any streaming worker so its channel doesn't leak.
                tm.cancel_stream();
                utils::hide_recording_overlay(&ah);
                set_tray_state(&ah, TrayIconState::Idle);
            }
        });

        debug!(
            "TranscribeAction::stop completed in {:?}",
            stop_time.elapsed()
        );
    }
}

// Cancel Action
struct CancelAction;

impl ShortcutAction for CancelAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        utils::cancel_current_operation(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        // Nothing to do on stop for cancel
    }
}

// Test Action
struct TestAction;

impl ShortcutAction for TestAction {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str) {
        log::info!(
            "Shortcut ID '{}': Started - {} (App: {})", // Changed "Pressed" to "Started" for consistency
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str) {
        log::info!(
            "Shortcut ID '{}': Stopped - {} (App: {})", // Changed "Released" to "Stopped" for consistency
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }
}

// Static Action Map
pub static ACTION_MAP: Lazy<HashMap<String, Arc<dyn ShortcutAction>>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert(
        "transcribe".to_string(),
        Arc::new(TranscribeAction {
            post_process: false,
            edit: false,
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "transcribe_with_post_process".to_string(),
        Arc::new(TranscribeAction {
            post_process: true,
            edit: false,
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "edit_by_voice".to_string(),
        Arc::new(TranscribeAction {
            post_process: false,
            edit: true,
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "cancel".to_string(),
        Arc::new(CancelAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "test".to_string(),
        Arc::new(TestAction) as Arc<dyn ShortcutAction>,
    );
    map
});

#[cfg(test)]
mod tests {
    use super::{
        ai_cleanup_on, complete_unless_cancelled, edit_text, is_blank_transcription, legacy_prompt,
        prompt_for_level, prompt_messages, should_use_streaming_overlay, strip_think_block,
        wants_post_process, LIST_RULE,
    };
    use crate::settings::OverlayStyle;
    use crate::settings::{get_default_settings, AppSettings, CleanupLevel};
    use std::future;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    /// Calls the AI set up in a settings_store.json:
    /// `OWL_TEST_SETTINGS=… cargo test --lib edit_by_voice_real -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn edit_by_voice_real_ai_rewrites_translates_and_answers() {
        let path = std::env::var("OWL_TEST_SETTINGS").expect("OWL_TEST_SETTINGS");
        let store: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let settings: AppSettings = serde_json::from_value(store["settings"].clone()).unwrap();
        let edit = |instruction: &str, selection: Option<&str>| {
            let text = tauri::async_runtime::block_on(edit_text(&settings, instruction, selection))
                .unwrap();
            println!("{instruction} -> {text}");
            text
        };
        let formal = edit(
            "make it formal",
            Some("hey can u send me the file tmrw thx"),
        );
        assert!(formal.contains("file") && !formal.contains(" u "));
        let english = edit("翻成英文", Some("我們明天下午三點開會"));
        assert!(english.to_lowercase().contains("meet"));
        assert!(edit("what's the capital of France", None).contains("Paris"));
    }

    #[test]
    fn blank_transcription_is_detected() {
        assert!(is_blank_transcription(""));
        assert!(is_blank_transcription("   "));
        assert!(is_blank_transcription("\t\n  \r\n"));
    }

    #[test]
    fn non_blank_transcription_is_kept() {
        assert!(!is_blank_transcription("hello"));
        assert!(!is_blank_transcription("  hello  "));
    }

    #[test]
    fn completed_operation_returns_its_output() {
        let result = tauri::async_runtime::block_on(complete_unless_cancelled(
            future::ready("done"),
            || false,
        ));

        assert_eq!(result, Some("done"));
    }

    #[test]
    fn pending_operation_stops_after_cancellation() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_for_thread = Arc::clone(&cancelled);
        let cancel_thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            cancelled_for_thread.store(true, Ordering::Release);
        });

        let result = tauri::async_runtime::block_on(complete_unless_cancelled(
            future::pending::<()>(),
            || cancelled.load(Ordering::Acquire),
        ));

        cancel_thread.join().unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn leading_think_block_is_stripped() {
        assert_eq!(
            strip_think_block("<think>pondering...</think>Cleaned text."),
            "Cleaned text."
        );
        assert_eq!(
            strip_think_block("  \n<think>multi\nline</think>\n  Cleaned text."),
            "Cleaned text."
        );
    }

    #[test]
    fn content_without_think_block_is_unchanged() {
        assert_eq!(strip_think_block("Cleaned text."), "Cleaned text.");
        assert_eq!(
            strip_think_block("Mentions <think> mid-sentence."),
            "Mentions <think> mid-sentence."
        );
        // Unclosed block: leave untouched rather than guess
        assert_eq!(
            strip_think_block("<think>never closed"),
            "<think>never closed"
        );
    }

    #[test]
    fn live_overlay_uses_streaming_states_only_for_streaming_models() {
        assert!(should_use_streaming_overlay(OverlayStyle::Live, true));
        assert!(!should_use_streaming_overlay(OverlayStyle::Live, false));
        assert!(!should_use_streaming_overlay(OverlayStyle::Minimal, true));
        assert!(!should_use_streaming_overlay(OverlayStyle::None, true));
    }

    const HANDY_STYLE: &str =
        "<transcript>\n${output}\n</transcript>\n\nThe above is a transcript. Clean it.";

    #[test]
    fn instructions_go_in_the_system_message_and_the_transcript_in_tags() {
        let (system, user) = prompt_messages(HANDY_STYLE, "sorry, I mean Wednesday");
        // No empty <transcript></transcript> left behind to tell the model
        // the transcript is empty.
        assert!(!system.contains("<transcript>"), "{system}");
        assert!(!system.contains("${output}"), "{system}");
        assert!(system.contains("Clean it."));
        assert_eq!(user, "<transcript>\nsorry, I mean Wednesday\n</transcript>");

        let (system, user) = prompt_messages("Clean up dictated text.", "hi");
        assert_eq!(system, "Clean up dictated text.");
        assert_eq!(user, "<transcript>\nhi\n</transcript>");
    }

    #[test]
    fn single_message_prompt_always_contains_the_transcript() {
        assert_eq!(
            legacy_prompt("Fix: ${output}", "hi"),
            "Fix: hi",
            "Handy-style prompts keep their placeholder"
        );
        // A prompt without ${output} used to send no transcript at all.
        assert_eq!(
            legacy_prompt("Clean up dictated text.", "hi"),
            "Clean up dictated text.\n\n<transcript>\nhi\n</transcript>"
        );
    }

    #[test]
    fn the_normal_shortcut_cleans_up_when_post_process_is_on() {
        assert!(wants_post_process(false, true));
        assert!(wants_post_process(true, true));
        assert!(!wants_post_process(false, false));
        // The CLI's --toggle-post-process still asks for it explicitly.
        assert!(wants_post_process(true, false));
    }

    #[test]
    fn the_ai_runs_only_at_light_or_medium_with_post_process_on() {
        let mut settings = get_default_settings();
        for (level, post_process, expected) in [
            (CleanupLevel::None, true, false),
            (CleanupLevel::Light, false, false),
            (CleanupLevel::Light, true, true),
            (CleanupLevel::Medium, true, true),
        ] {
            settings.cleanup_level = level;
            settings.post_process_enabled = post_process;
            assert_eq!(ai_cleanup_on(&settings), expected, "{level:?}");
        }
    }

    #[test]
    fn medium_lets_the_ai_reword() {
        assert_eq!(
            prompt_for_level("Clean it.", CleanupLevel::Light),
            format!("Clean it.\n\n{LIST_RULE}")
        );
        assert_eq!(
            prompt_for_level("Clean it.", CleanupLevel::Medium),
            format!("Clean it.\n\n{LIST_RULE}\nYou may reword for clarity and conciseness, keeping the meaning.")
        );
    }
}
