//! Owl's command line: `owl meetings`, `owl meeting <id>`, `owl transcribe
//! <file>` and `owl mcp`. The read commands open Owl's databases directly, so
//! the app needn't be running; `transcribe` runs headless (see `lib.rs`).

use crate::managers::meeting::{fill_speakers, read_meetings, render, ExportFormat, Meeting};
use anyhow::{anyhow, Result};
use clap::Subcommand;
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};

#[derive(Subcommand, Debug, Clone)]
pub enum OwlCommand {
    /// List meetings, newest first.
    Meetings {
        /// Print JSON.
        #[arg(long)]
        json: bool,
    },
    /// Print a meeting as Markdown (or only its notes or transcript).
    Meeting {
        id: i64,
        #[arg(long, conflicts_with = "transcript")]
        notes: bool,
        #[arg(long)]
        transcript: bool,
    },
    /// Transcribe an audio or video file as a new meeting and print it.
    Transcribe { file: PathBuf },
    /// Serve Owl's meetings and dictations to AI agents (MCP over stdio), read-only.
    Mcp,
}

/// Owl's data folder (`app_data_dir` without an app handle).
pub fn data_dir() -> PathBuf {
    crate::portable::init();
    match crate::portable::data_dir() {
        Some(dir) => dir.clone(),
        None => dirs::data_dir()
            .unwrap_or_default()
            .join(crate::OWL_IDENTIFIER),
    }
}

/// Opens one of Owl's databases read-only.
pub fn open_db(name: &str) -> Result<Connection> {
    let path = data_dir().join(name);
    if !path.exists() {
        return Err(anyhow!("No {name} yet; open Owl once first"));
    }
    Ok(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?)
}

pub fn meetings() -> Result<Vec<Meeting>> {
    read_meetings(&open_db("meetings.db")?)
        .map_err(|e| anyhow!("{e}; open Owl once so it can update its meetings"))
}

pub fn meeting(id: i64) -> Result<Meeting> {
    meetings()?
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| anyhow!("No meeting {id}"))
}

/// One line per meeting: id, date, length and title.
pub fn meeting_list(meetings: &[Meeting]) -> String {
    meetings
        .iter()
        .map(|m| {
            let when = chrono::DateTime::from_timestamp_millis(m.started_at)
                .map(|t| {
                    t.with_timezone(&chrono::Local)
                        .format("%Y-%m-%d %H:%M")
                        .to_string()
                })
                .unwrap_or_default();
            let title = m.title.as_deref().unwrap_or("(untitled)");
            format!("{:>4}  {when}  {:>8}  {title}", m.id, clock(m.duration_ms))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A meeting's notes, transcript, or both, as plain text.
pub fn meeting_text(m: &Meeting, notes: bool, transcript: bool) -> String {
    if notes {
        return m
            .summary
            .as_deref()
            .map(|s| fill_speakers(s, &m.speaker_names))
            .unwrap_or_default();
    }
    if transcript {
        // Without notes, so a "Transcript" heading in them can't be mistaken for it.
        let bare = Meeting {
            summary: None,
            ..m.clone()
        };
        return render(&bare, ExportFormat::Text)
            .split_once("Transcript\n\n")
            .map(|(_, t)| t.to_string())
            .unwrap_or_default();
    }
    render(m, ExportFormat::Markdown)
}

fn clock(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// Runs the commands that need no app. `None` for `transcribe`, which runs
/// headless inside the app.
pub fn run_offline(command: &OwlCommand) -> Option<i32> {
    let result = match command {
        OwlCommand::Meetings { json } => meetings().map(|all| {
            if *json {
                serde_json::to_string_pretty(&all).unwrap_or_default()
            } else {
                meeting_list(&all)
            }
        }),
        OwlCommand::Meeting {
            id,
            notes,
            transcript,
        } => meeting(*id).map(|m| meeting_text(&m, *notes, *transcript)),
        OwlCommand::Mcp => return Some(crate::mcp::serve()),
        OwlCommand::Transcribe { .. } => return None,
    };
    Some(match result {
        Ok(text) => {
            println!("{text}");
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    })
}

/// Links `owl` to this app's binary, in `/usr/local/bin` when it's writable,
/// else `~/.local/bin`. Returns where the link is.
pub fn install() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let user_bin = dirs::home_dir()
        .ok_or_else(|| anyhow!("No home folder"))?
        .join(".local/bin");
    let candidates = [PathBuf::from("/usr/local/bin"), user_bin];
    let mut last_error = None;
    for dir in &candidates {
        let link = dir.join("owl");
        match link_to(&exe, dir, &link) {
            Ok(()) => return Ok(link),
            Err(e) => last_error = Some(e),
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow!("Nowhere to install")))
}

/// The `.app` a binary is in. Owl and Handy's binaries share a name, so this
/// is what tells their links apart.
fn app_of(path: &Path) -> Option<&std::ffi::OsStr> {
    path.ancestors()
        .filter_map(Path::file_name)
        .find(|name| name.to_string_lossy().ends_with(".app"))
}

fn link_to(exe: &Path, dir: &Path, link: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    // Replace an old link to Owl (e.g. to a moved app), never a real file or
    // another program's link.
    if let Ok(target) = std::fs::read_link(link) {
        let ours = target == exe || app_of(&target).is_some_and(|app| Some(app) == app_of(exe));
        if !ours {
            return Err(anyhow!("{} belongs to another program", link.display()));
        }
        std::fs::remove_file(link)?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(exe, link)?;
    #[cfg(not(unix))]
    return Err(anyhow!("Installing the command line is macOS-only for now"));
    #[cfg(unix)]
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_keeps_other_programs_links_and_replaces_dead_ones() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("owl");
        let exe = Path::new("/Applications/Owl.app/Contents/MacOS/handy");

        std::os::unix::fs::symlink("/bin/ls", &link).unwrap();
        assert!(link_to(exe, dir.path(), &link).is_err());
        assert_eq!(std::fs::read_link(&link).unwrap(), Path::new("/bin/ls"));

        // Handy's binary has the same name as Owl's.
        let handy = dir.path().join("Handy.app/Contents/MacOS/handy");
        std::fs::create_dir_all(handy.parent().unwrap()).unwrap();
        std::fs::write(&handy, "").unwrap();
        std::fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(&handy, &link).unwrap();
        assert!(link_to(exe, dir.path(), &link).is_err());

        // A link to a missing program isn't Owl's either.
        std::fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink("/gone/handy", &link).unwrap();
        assert!(link_to(exe, dir.path(), &link).is_err());

        // A moved Owl is replaced, even once it's gone.
        let moved = dir.path().join("Old/Owl.app/Contents/MacOS/handy");
        std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
        std::fs::write(&moved, "").unwrap();
        std::fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(&moved, &link).unwrap();
        link_to(exe, dir.path(), &link).unwrap();
        assert_eq!(std::fs::read_link(&link).unwrap(), exe);
        std::fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink("/gone/Owl.app/Contents/MacOS/handy", &link).unwrap();
        link_to(exe, dir.path(), &link).unwrap();
        assert_eq!(std::fs::read_link(&link).unwrap(), exe);
    }
}
