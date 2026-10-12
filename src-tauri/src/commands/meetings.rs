//! Owl notetaker commands. Errors are plain strings shown to the user, except
//! `no_provider`, which the UI translates.

use crate::managers::meeting::{ExportFormat, Meeting, MeetingManager};
use crate::settings::{get_settings, write_settings, MeetingTemplate};
use serde::Serialize;
use specta::Type;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

#[tauri::command]
#[specta::specta]
pub fn start_meeting(meetings: State<Arc<MeetingManager>>) -> Result<i64, String> {
    meetings.start().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn stop_meeting(meetings: State<Arc<MeetingManager>>) -> Result<(), String> {
    meetings.stop().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
/// Async, so reading every meeting never blocks the main thread.
pub async fn get_meetings(
    meetings: State<'_, Arc<MeetingManager>>,
) -> Result<Vec<Meeting>, String> {
    meetings.list().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn delete_meeting(meetings: State<Arc<MeetingManager>>, id: i64) -> Result<(), String> {
    meetings.delete(id).map_err(|e| e.to_string())
}

/// Summarizes with the AI provider set up for post-processing.
#[tauri::command]
#[specta::specta]
pub async fn summarize_meeting(
    app: AppHandle,
    id: i64,
    template: Option<String>,
    provider: Option<String>,
) -> Result<String, String> {
    let meetings = app.state::<Arc<MeetingManager>>().inner().clone();
    meetings.summarize(id, template, provider).await
}

/// An AI that can write meeting notes: a Post Process provider with a model.
#[derive(Serialize, Type)]
pub struct SummaryProvider {
    pub id: String,
    pub label: String,
    pub model: String,
}

/// What the notes can be written with: the built-in template ids (named in
/// the UI) and the AIs that are set up.
#[derive(Serialize, Type)]
pub struct SummaryOptions {
    pub built_in_templates: Vec<String>,
    pub providers: Vec<SummaryProvider>,
}

#[tauri::command]
#[specta::specta]
pub fn get_summary_options(app: AppHandle) -> SummaryOptions {
    let settings = get_settings(&app);
    SummaryOptions {
        built_in_templates: crate::managers::meeting::BUILT_IN_TEMPLATES
            .iter()
            .map(|(id, _)| id.to_string())
            .collect(),
        providers: settings
            .post_process_providers
            .iter()
            .filter(|p| p.id != crate::settings::APPLE_INTELLIGENCE_PROVIDER_ID)
            .filter_map(|p| {
                let model = settings.post_process_models.get(&p.id)?.trim();
                (!model.is_empty()).then(|| SummaryProvider {
                    id: p.id.clone(),
                    label: p.label.clone(),
                    model: model.to_string(),
                })
            })
            .collect(),
    }
}

#[tauri::command]
#[specta::specta]
pub fn rename_meeting(
    meetings: State<Arc<MeetingManager>>,
    id: i64,
    title: String,
) -> Result<(), String> {
    meetings.rename(id, &title).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn retranscribe_meeting(meetings: State<Arc<MeetingManager>>, id: i64) -> Result<(), String> {
    meetings.retranscribe(id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn update_meeting_templates(
    app: AppHandle,
    templates: Vec<MeetingTemplate>,
) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.meeting_templates = templates;
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_meeting_template_setting(app: AppHandle, template: String) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.meeting_template = template;
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn rename_speaker(
    meetings: State<Arc<MeetingManager>>,
    id: i64,
    voice: u32,
    name: String,
) -> Result<(), String> {
    meetings
        .rename_speaker(id, voice, &name)
        .map_err(|e| e.to_string())
}

/// Whether the speaker model is installed; meetings work without it.
#[tauri::command]
#[specta::specta]
pub fn speaker_model_installed(app: AppHandle) -> bool {
    crate::diarization::is_installed(&app)
}

/// Downloads the speaker model, sending `speaker-model-progress` events.
#[tauri::command]
#[specta::specta]
pub async fn download_speaker_model(app: AppHandle) -> Result<(), String> {
    crate::diarization::download(&app)
        .await
        .map_err(|e| e.to_string())
}

/// Adds an audio or video file as a meeting; `unsupported_format` when Owl
/// can't read it (the UI translates that).
#[tauri::command]
#[specta::specta]
pub fn import_meeting(meetings: State<Arc<MeetingManager>>, path: String) -> Result<i64, String> {
    meetings
        .import(std::path::PathBuf::from(path))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn export_meeting(
    meetings: State<Arc<MeetingManager>>,
    id: i64,
    format: ExportFormat,
    path: String,
) -> Result<(), String> {
    meetings
        .export(id, format, std::path::Path::new(&path))
        .map_err(|e| e.to_string())
}
