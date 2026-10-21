//! Owl learning log: with Settings > Learning on, each dictation's audio and
//! text stay on this Mac (`learning/` in the app data folder) so Owl can test
//! and learn from them (`eval.rs`, `learner.rs`). What the user ended up
//! keeping is the answer key: their edit right after the paste
//! (`correction_watch.rs`) or a fix typed in History. Nothing leaves the Mac
//! except what the AI cleanup already sends.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use rusqlite_migration::{Migrations, M};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

static MIGRATIONS: &[M] = &[
    M::up(
        "CREATE TABLE dictations (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        created_at INTEGER NOT NULL,
        audio TEXT NOT NULL,
        history_file TEXT NOT NULL,
        model TEXT NOT NULL,
        heard TEXT NOT NULL,
        ai_reply TEXT,
        pasted TEXT NOT NULL,
        kept TEXT,
        kept_source TEXT
    );
    CREATE INDEX dictations_history_file ON dictations(history_file);
    CREATE TABLE learned (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        created_at INTEGER NOT NULL,
        change TEXT NOT NULL,
        score_before REAL NOT NULL,
        score_after REAL NOT NULL,
        undone INTEGER NOT NULL DEFAULT 0
    );",
    ),
    M::up(
        "CREATE TABLE runs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        at INTEGER NOT NULL,
        keyed INTEGER NOT NULL,
        report TEXT NOT NULL
    );",
    ),
];

/// A learned change: (id, when, change JSON, score before, score after, undone).
pub type LearnedRow = (i64, i64, String, f64, f64, bool);

/// Older dictations are deleted, and the oldest go first past the size cap.
const MAX_AGE_SECS: i64 = 90 * 24 * 60 * 60;
const MAX_AUDIO_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// One logged dictation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Dictation {
    pub id: i64,
    pub created_at: i64,
    pub audio: PathBuf,
    pub model: String,
    pub heard: String,
    pub ai_reply: Option<String>,
    pub pasted: String,
    pub kept: Option<String>,
    pub kept_source: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq)]
pub struct LearningStats {
    pub dictations: i64,
    /// Dictations with an answer key (edited after the paste, or fixed in History).
    pub keyed: i64,
    pub audio_bytes: i64,
}

pub struct Learning {
    dir: PathBuf,
}

impl Learning {
    pub fn new(app: &AppHandle) -> Result<Self> {
        Self::at(crate::portable::app_data_dir(app)?.join("learning"))
    }

    pub fn at(dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(dir.join("audio"))?;
        let this = Self { dir };
        let mut conn = this.conn()?;
        Migrations::new(MIGRATIONS.to_vec()).to_latest(&mut conn)?;
        Ok(this)
    }

    /// A file in the learning folder.
    pub fn file(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// The newest keyed dictation's id: a watermark that only grows, unlike
    /// the count, which tidying shrinks.
    pub fn last_keyed_id(&self) -> Result<i64> {
        Ok(self.conn()?.query_row(
            "SELECT COALESCE(MAX(id), 0) FROM dictations WHERE kept IS NOT NULL",
            [],
            |r| r.get(0),
        )?)
    }

    /// Keyed dictations newer than the `since` watermark.
    pub fn keyed_since(&self, since: i64) -> Result<i64> {
        Ok(self.conn()?.query_row(
            "SELECT COUNT(*) FROM dictations WHERE kept IS NOT NULL AND id > ?1",
            params![since],
            |r| r.get(0),
        )?)
    }

    /// Saves a learner run: the keyed watermark it saw and its report.
    pub fn add_run(&self, keyed: i64, report: &str) -> Result<()> {
        self.conn()?.execute(
            "INSERT INTO runs (at, keyed, report) VALUES (?1, ?2, ?3)",
            params![now(), keyed, report],
        )?;
        Ok(())
    }

    /// The last learner run: (when, keyed watermark then, report JSON).
    pub fn last_run(&self) -> Result<Option<(i64, i64, String)>> {
        Ok(self
            .conn()?
            .query_row(
                "SELECT at, keyed, report FROM runs ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?)
    }

    fn conn(&self) -> Result<Connection> {
        let conn = Connection::open(self.dir.join("learning.db"))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(conn)
    }

    /// Logs a pasted dictation and keeps a copy of its audio. Returns its id.
    pub fn record(
        &self,
        wav: &Path,
        history_file: &str,
        model: &str,
        heard: &str,
        ai_reply: Option<&str>,
        pasted: &str,
    ) -> Result<i64> {
        let name = wav
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("recording has no file name"))?
            .to_string();
        fs::copy(wav, self.dir.join("audio").join(&name))?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO dictations (created_at, audio, history_file, model, heard, ai_reply, pasted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![now(), name, history_file, model, heard, ai_reply, pasted],
        )?;
        let id = conn.last_insert_rowid();
        drop(conn);
        if let Err(e) = self.prune() {
            log::warn!("Learning: couldn't tidy old dictations: {e:#}");
        }
        Ok(id)
    }

    /// The text the user kept after editing the paste. A fix typed in History
    /// is never replaced by a later edit.
    pub fn set_edited(&self, id: i64, kept: &str) -> Result<()> {
        self.conn()?.execute(
            "UPDATE dictations SET kept = ?1, kept_source = 'edit'
             WHERE id = ?2 AND (kept_source IS NULL OR kept_source = 'edit')",
            params![kept, id],
        )?;
        Ok(())
    }

    /// A fix typed in History for the dictation saved as `history_file`.
    /// Returns whether that dictation is in the log.
    pub fn set_fixed(&self, history_file: &str, kept: &str) -> Result<bool> {
        let changed = self.conn()?.execute(
            "UPDATE dictations SET kept = ?1, kept_source = 'fixed' WHERE history_file = ?2",
            params![kept, history_file],
        )?;
        Ok(changed > 0)
    }

    /// Whether the dictation saved as `history_file` is in the log.
    pub fn has_history_file(&self, history_file: &str) -> Result<bool> {
        Ok(self
            .conn()?
            .query_row(
                "SELECT 1 FROM dictations WHERE history_file = ?1",
                params![history_file],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    /// Dictations with an answer key, oldest first.
    pub fn keyed(&self) -> Result<Vec<Dictation>> {
        self.query("WHERE kept IS NOT NULL ORDER BY created_at, id", [])
    }

    fn query<P: rusqlite::Params>(&self, filter: &str, args: P) -> Result<Vec<Dictation>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT id, created_at, audio, model, heard, ai_reply, pasted, kept, kept_source
             FROM dictations {filter}"
        ))?;
        let audio_dir = self.dir.join("audio");
        let rows = stmt.query_map(args, |r| {
            Ok(Dictation {
                id: r.get(0)?,
                created_at: r.get(1)?,
                audio: audio_dir.join(r.get::<_, String>(2)?),
                model: r.get(3)?,
                heard: r.get(4)?,
                ai_reply: r.get(5)?,
                pasted: r.get(6)?,
                kept: r.get(7)?,
                kept_source: r.get(8)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn stats(&self) -> Result<LearningStats> {
        let conn = self.conn()?;
        let (dictations, keyed) =
            conn.query_row("SELECT COUNT(*), COUNT(kept) FROM dictations", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        Ok(LearningStats {
            dictations,
            keyed,
            audio_bytes: audio_bytes(&self.dir.join("audio")) as i64,
        })
    }

    /// Up to `n` of the user's past fixes most like `text`, as (what Owl
    /// pasted, what the user kept), from dictations made before `before`.
    pub fn examples(&self, text: &str, n: usize, before: Option<i64>) -> Vec<(String, String)> {
        let rows = match self.query(
            "WHERE kept IS NOT NULL AND kept != pasted AND created_at < ?1 ORDER BY created_at DESC LIMIT 500",
            params![before.unwrap_or(i64::MAX)],
        ) {
            Ok(rows) => rows,
            Err(e) => {
                log::warn!("Learning: couldn't read examples: {e:#}");
                return Vec::new();
            }
        };
        let words = |t: &str| -> std::collections::HashSet<String> {
            crate::correction::tokens(t)
                .into_iter()
                .map(|w| w.to_lowercase())
                .collect()
        };
        let mine = words(text);
        let mut scored: Vec<(usize, Dictation)> = rows
            .into_iter()
            .map(|d| (words(&d.pasted).intersection(&mine).count(), d))
            .collect();
        // Most shared words first; ties go to the newest (the rows' order).
        scored.sort_by_key(|s| std::cmp::Reverse(s.0));
        scored
            .into_iter()
            .take(n)
            .filter_map(|(_, d)| Some((d.pasted, d.kept?)))
            .collect()
    }

    /// Saves a change the learner kept. Returns its id.
    pub fn add_learned(&self, change: &str, before: f64, after: f64) -> Result<i64> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO learned (created_at, change, score_before, score_after) VALUES (?1, ?2, ?3, ?4)",
            params![now(), change, before, after],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Changes the learner kept, newest first: (id, when, change JSON,
    /// score before, score after, undone).
    pub fn learned(&self) -> Result<Vec<LearnedRow>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, created_at, change, score_before, score_after, undone
             FROM learned ORDER BY id DESC LIMIT 200",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Marks a learned change as undone; returns its change JSON if it wasn't already.
    pub fn undo_learned(&self, id: i64) -> Result<Option<String>> {
        let conn = self.conn()?;
        let change: Option<String> = conn
            .query_row(
                "SELECT change FROM learned WHERE id = ?1 AND undone = 0",
                params![id],
                |r| r.get(0),
            )
            .optional()?;
        if change.is_some() {
            conn.execute("UPDATE learned SET undone = 1 WHERE id = ?1", params![id])?;
        }
        Ok(change)
    }

    /// Deletes dictations past 90 days, then the oldest while the audio is
    /// over 2 GB.
    fn prune(&self) -> Result<()> {
        let conn = self.conn()?;
        let audio_dir = self.dir.join("audio");
        let mut doomed: Vec<(i64, String)> = {
            let mut stmt =
                conn.prepare("SELECT id, audio FROM dictations WHERE created_at < ?1")?;
            let rows = stmt.query_map(params![now() - MAX_AGE_SECS], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut total = audio_bytes(&audio_dir);
        let size = |name: &str| fs::metadata(audio_dir.join(name)).map_or(0, |m| m.len());
        total -= doomed.iter().map(|(_, a)| size(a)).sum::<u64>().min(total);
        if total > MAX_AUDIO_BYTES {
            let mut stmt = conn.prepare(
                "SELECT id, audio FROM dictations WHERE created_at >= ?1 ORDER BY created_at, id",
            )?;
            let rows = stmt.query_map(params![now() - MAX_AGE_SECS], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in rows {
                if total <= MAX_AUDIO_BYTES {
                    break;
                }
                let (id, audio) = row?;
                total -= size(&audio).min(total);
                doomed.push((id, audio));
            }
        }
        for (id, audio) in doomed {
            conn.execute("DELETE FROM dictations WHERE id = ?1", params![id])?;
            let _ = fs::remove_file(audio_dir.join(audio));
        }
        Ok(())
    }
}

fn audio_bytes(dir: &Path) -> u64 {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.metadata().ok())
                .map(|m| m.len())
                .sum()
        })
        .unwrap_or(0)
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> (Learning, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "owl-learning-test-{}-{}",
            std::process::id(),
            now() + rand_suffix()
        ));
        let wav = dir.join("rec.wav");
        let learning = Learning::at(dir.join("learning")).unwrap();
        fs::write(&wav, b"RIFF").unwrap();
        (learning, wav)
    }

    fn rand_suffix() -> i64 {
        let mut b = [0u8; 4];
        getrandom::fill(&mut b).unwrap();
        i32::from_le_bytes(b) as i64
    }

    #[test]
    fn a_dictation_is_logged_keyed_and_fixed() {
        let (l, wav) = temp();
        let id = l
            .record(
                &wav,
                "rec.wav",
                "qwen",
                "send it to o",
                None,
                "Send it to O.",
            )
            .unwrap();
        assert_eq!(l.stats().unwrap().keyed, 0);
        l.set_edited(id, "Send it to Owl.").unwrap();
        assert_eq!(
            l.keyed().unwrap()[0].kept.as_deref(),
            Some("Send it to Owl.")
        );
        // A fix in History wins, and a later edit doesn't undo it.
        assert!(l.set_fixed("rec.wav", "Send it to Owl now.").unwrap());
        l.set_edited(id, "something else").unwrap();
        let d = &l.keyed().unwrap()[0];
        assert_eq!(
            (d.kept.as_deref(), d.kept_source.as_deref()),
            (Some("Send it to Owl now."), Some("fixed"))
        );
        assert!(d.audio.exists());
        assert!(!l.set_fixed("other.wav", "x").unwrap());
    }

    #[test]
    fn examples_are_the_most_alike_past_fixes() {
        let (l, wav) = temp();
        for (pasted, kept) in [
            ("ship the O beta", "ship the Owl beta"),
            ("lunch at noon", "lunch at 1"),
            ("the O app is fast", "the Owl app is fast"),
        ] {
            let id = l
                .record(&wav, "rec.wav", "m", pasted, None, pasted)
                .unwrap();
            l.set_edited(id, kept).unwrap();
        }
        let ex = l.examples("is the O app ready", 2, None);
        assert_eq!(ex[0].0, "the O app is fast");
        assert_eq!(ex.len(), 2);
        assert!(l.examples("anything", 2, Some(0)).is_empty());
    }

    #[test]
    fn learned_changes_can_be_undone_once() {
        let (l, _) = temp();
        let id = l.add_learned("{\"x\":1}", 0.2, 0.1).unwrap();
        assert_eq!(l.undo_learned(id).unwrap().as_deref(), Some("{\"x\":1}"));
        assert_eq!(l.undo_learned(id).unwrap(), None);
        assert!(l.learned().unwrap()[0].5);
    }
}
