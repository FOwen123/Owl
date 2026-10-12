//! `owl mcp`: a read-only MCP server over stdio, so AI agents (Claude Code,
//! Cursor, …) can list, read and search Owl's meetings and dictations. It can't
//! start, change or delete anything. `claude mcp add owl -- owl mcp`.

use crate::owl_cli::{meeting, meeting_list, meeting_text, meetings, open_db};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::io::{BufRead, Write};

const PROTOCOL: &str = "2025-06-18";

pub fn serve() -> i32 {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(request) => handle(&request),
            Err(e) => Some(error(Value::Null, -32700, &format!("Parse error: {e}"))),
        };
        if let Some(reply) = reply {
            let _ = writeln!(stdout, "{reply}");
            let _ = stdout.flush();
        }
    }
    0
}

/// One JSON-RPC message in, at most one out (notifications get none).
pub fn handle(request: &Value) -> Option<Value> {
    let id = request.get("id").cloned()?;
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    let result = match request.get("method").and_then(Value::as_str) {
        Some("initialize") => Ok(json!({
            "protocolVersion": params.get("protocolVersion").cloned().unwrap_or(json!(PROTOCOL)),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "owl", "version": env!("CARGO_PKG_VERSION") },
            "instructions": "Owl's meetings (notes and speaker-labelled transcripts) and dictations, read-only.",
        })),
        Some("ping") => Ok(json!({})),
        Some("tools/list") => Ok(json!({ "tools": tools() })),
        Some("tools/call") => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            Ok(match call(name, &args) {
                Ok(text) => json!({ "content": [{ "type": "text", "text": text }] }),
                Err(e) => json!({
                    "content": [{ "type": "text", "text": e.to_string() }],
                    "isError": true,
                }),
            })
        }
        Some(method) => Err((-32601, format!("Unknown method {method}"))),
        None => Err((-32600, "Missing method".to_string())),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => error(id, code, &message),
    })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tools() -> Value {
    let limit = json!({ "type": "integer", "description": "How many, newest first (default 20)." });
    let query = json!({ "type": "string", "description": "Words to look for (any case)." });
    json!([
        {
            "name": "list_meetings",
            "description": "List Owl meetings, newest first: id, date, length, title.",
            "inputSchema": { "type": "object", "properties": { "limit": limit } },
        },
        {
            "name": "read_meeting",
            "description": "Read one meeting: its notes, its speaker-labelled transcript, or both.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "integer" },
                    "part": { "type": "string", "enum": ["all", "notes", "transcript"] },
                },
                "required": ["id"],
            },
        },
        {
            "name": "search_meetings",
            "description": "Find meetings whose title, notes or transcript contain the words.",
            "inputSchema": { "type": "object", "properties": { "query": query }, "required": ["query"] },
        },
        {
            "name": "list_dictations",
            "description": "List recent dictations (time and text), newest first.",
            "inputSchema": { "type": "object", "properties": { "limit": limit } },
        },
        {
            "name": "search_dictations",
            "description": "Find dictations containing the words.",
            "inputSchema": {
                "type": "object",
                "properties": { "query": query, "limit": limit },
                "required": ["query"],
            },
        },
    ])
}

fn call(name: &str, args: &Value) -> Result<String> {
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(20)
        .clamp(1, 200) as usize;
    let query = || -> Result<String> {
        args.get("query")
            .and_then(Value::as_str)
            .map(str::to_lowercase)
            .filter(|q| !q.trim().is_empty())
            .ok_or_else(|| anyhow!("Give a query"))
    };
    match name {
        "list_meetings" => Ok(meeting_list(
            &meetings()?.into_iter().take(limit).collect::<Vec<_>>(),
        )),
        "read_meeting" => {
            let id = args
                .get("id")
                .and_then(Value::as_i64)
                .ok_or_else(|| anyhow!("Give an id"))?;
            let part = args.get("part").and_then(Value::as_str).unwrap_or("all");
            Ok(meeting_text(
                &meeting(id)?,
                part == "notes",
                part == "transcript",
            ))
        }
        "search_meetings" => {
            let query = query()?;
            let found: Vec<_> = meetings()?
                .into_iter()
                .filter(|m| {
                    meeting_text(m, false, false)
                        .to_lowercase()
                        .contains(&query)
                })
                .collect();
            Ok(match found.is_empty() {
                true => "No meetings match.".to_string(),
                false => meeting_list(&found),
            })
        }
        "list_dictations" => dictations(None, limit),
        "search_dictations" => dictations(Some(query()?), limit),
        _ => Err(anyhow!("Unknown tool {name}")),
    }
}

/// Recent dictations (the cleaned-up text when there is one), optionally
/// only those containing `query`.
fn dictations(query: Option<String>, limit: usize) -> Result<String> {
    let conn = open_db("history.db")?;
    let mut stmt = conn.prepare(
        "SELECT timestamp, COALESCE(post_processed_text, transcription_text)
         FROM transcription_history
         WHERE ?1 IS NULL OR lower(COALESCE(post_processed_text, transcription_text)) LIKE '%' || ?1 || '%'
         ORDER BY timestamp DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![query, limit as i64], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let lines: Vec<String> = rows
        .filter_map(|r| r.ok())
        .map(|(ts, text)| {
            let when = chrono::DateTime::from_timestamp(ts, 0)
                .map(|t| {
                    t.with_timezone(&chrono::Local)
                        .format("%Y-%m-%d %H:%M")
                        .to_string()
                })
                .unwrap_or_default();
            format!("{when}  {text}")
        })
        .collect();
    Ok(match lines.is_empty() {
        true => "No dictations match.".to_string(),
        false => lines.join("\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_and_tool_list_follow_the_protocol() {
        let init = handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2025-03-26"}}))
        .unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert!(init["result"]["capabilities"]["tools"].is_object());
        let list = handle(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).unwrap();
        let names: Vec<&str> = list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "list_meetings",
                "read_meeting",
                "search_meetings",
                "list_dictations",
                "search_dictations"
            ]
        );
    }

    #[test]
    fn notifications_get_no_reply_and_unknown_methods_an_error() {
        assert!(
            handle(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).is_none()
        );
        let reply =
            handle(&json!({"jsonrpc": "2.0", "id": 3, "method": "resources/write"})).unwrap();
        assert_eq!(reply["error"]["code"], -32601);
        let bad = handle(&json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
            "params": {"name": "delete_meeting", "arguments": {}}}))
        .unwrap();
        assert_eq!(bad["result"]["isError"], true);
    }
}
