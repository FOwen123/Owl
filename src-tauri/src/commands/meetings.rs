//! Owl notetaker commands. Errors are plain strings shown to the user, except
//! `no_provider`, which the UI translates.

use crate::managers::meeting::{Meeting, MeetingManager};
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
pub fn get_meetings(meetings: State<Arc<MeetingManager>>) -> Result<Vec<Meeting>, String> {
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
pub async fn summarize_meeting(app: AppHandle, id: i64) -> Result<String, String> {
    let meetings = app.state::<Arc<MeetingManager>>().inner().clone();
    meetings.summarize(id).await
}
