//! Owl: an OpenAI-compatible `POST /v1/audio/transcriptions` for other apps
//! on this Mac. Off by default, listens on 127.0.0.1 only, and needs the
//! token shown in Settings (`Authorization: Bearer <token>`).

use crate::managers::meeting::MeetingManager;
use crate::settings::{get_settings, write_settings, Secret};
use anyhow::{anyhow, Result};
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};
use tiny_http::{Header, Request, Response, Server};

pub const PORT: u16 = 8723;
/// Bigger uploads are refused before they're read (about 4 h of compressed audio).
const MAX_UPLOAD: usize = 500 * 1024 * 1024;

static SERVER: Mutex<Option<Arc<Server>>> = Mutex::new(None);

/// Starts or stops the server to match Settings. A token is made the first
/// time the API is turned on.
pub fn sync(app: &AppHandle) -> Result<()> {
    let mut settings = get_settings(app);
    let mut running = SERVER.lock().unwrap_or_else(|e| e.into_inner());
    if !settings.local_api_enabled {
        if let Some(server) = running.take() {
            server.unblock();
        }
        return Ok(());
    }
    if settings.local_api_token.0.is_empty() {
        settings.local_api_token = Secret(new_token()?);
        write_settings(app, settings);
    }
    if running.is_some() {
        return Ok(());
    }
    let server = Arc::new(
        Server::http(("127.0.0.1", PORT))
            .map_err(|e| anyhow!("Port {PORT} is busy, so the local API can't start: {e}"))?,
    );
    *running = Some(server.clone());
    let app = app.clone();
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let app = app.clone();
            // Transcribing takes a while; other requests needn't wait.
            std::thread::spawn(move || answer(&app, request));
        }
    });
    Ok(())
}

fn new_token() -> Result<String> {
    let mut bytes = [0u8; 24];
    getrandom::fill(&mut bytes).map_err(|e| anyhow!("No randomness for a token: {e}"))?;
    Ok(format!(
        "owl_{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}

fn answer(app: &AppHandle, request: Request) {
    let token = get_settings(app).local_api_token.0;
    serve(request, &token, &|path| {
        app.state::<Arc<MeetingManager>>().transcribe_file(path)
    });
}

/// Answers one request; `transcribe` turns the uploaded file into text.
fn serve(mut request: Request, token: &str, transcribe: &dyn Fn(&Path) -> Result<String>) {
    let (status, body, kind) = match respond(&mut request, token, transcribe) {
        Ok((text, true)) => (200, text, "text/plain; charset=utf-8"),
        Ok((text, false)) => (
            200,
            serde_json::json!({ "text": text }).to_string(),
            "application/json",
        ),
        Err((status, message)) => (
            status,
            serde_json::json!({ "error": { "message": message, "type": "invalid_request_error" } })
                .to_string(),
            "application/json",
        ),
    };
    let kind = Header::from_bytes("Content-Type", kind).expect("valid header");
    let _ = request.respond(
        Response::from_string(body)
            .with_status_code(status)
            .with_header(kind),
    );
}

/// The text, and whether it was asked for as plain text.
fn respond(
    request: &mut Request,
    token: &str,
    transcribe: &dyn Fn(&Path) -> Result<String>,
) -> Result<(String, bool), (u16, String)> {
    let path = request.url().split('?').next().unwrap_or("");
    if request.method() != &tiny_http::Method::Post || path != "/v1/audio/transcriptions" {
        return Err((404, "Only POST /v1/audio/transcriptions is served".into()));
    }
    let header = |name: &str| {
        request
            .headers()
            .iter()
            .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
            .map(|h| h.value.as_str().to_string())
    };
    let given = header("Authorization").unwrap_or_default();
    if token.is_empty() || !same(given.strip_prefix("Bearer ").unwrap_or(""), token) {
        return Err((401, "Wrong or missing token".into()));
    }
    let boundary = header("Content-Type")
        .as_deref()
        .and_then(boundary)
        .ok_or((
            400,
            "Send multipart/form-data with a `file` field".to_string(),
        ))?;
    if request.body_length().unwrap_or(0) > MAX_UPLOAD {
        return Err((413, "The file is too big".into()));
    }
    let mut body = Vec::new();
    request
        .as_reader()
        .take(MAX_UPLOAD as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|e| (400, e.to_string()))?;
    if body.len() > MAX_UPLOAD {
        return Err((413, "The file is too big".into()));
    }
    let parts = parts(&body, &boundary);
    let file = parts
        .iter()
        .find(|p| p.name == "file")
        .ok_or((400, "No `file` field".to_string()))?;
    let as_text = parts
        .iter()
        .any(|p| p.name == "response_format" && p.data == b"text");

    // Keep the extension: it's how the decoder recognises the format.
    let extension = file
        .filename
        .as_deref()
        .and_then(|f| Path::new(f).extension())
        .and_then(|e| e.to_str())
        .filter(|e| e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("wav");
    let name = new_token().map_err(|e| (500, e.to_string()))?;
    let upload = std::env::temp_dir().join(format!("owl-api-{name}.{extension}"));
    std::fs::write(&upload, file.data).map_err(|e| (500, e.to_string()))?;
    let result = transcribe(&upload);
    let _ = std::fs::remove_file(&upload);
    let text = result.map_err(|e| match e.to_string().as_str() {
        crate::media::UNSUPPORTED => (400, "This audio format isn't supported".to_string()),
        message => (500, message.to_string()),
    })?;
    Ok((text, as_text))
}

/// Compares in constant time, so the token can't be guessed byte by byte.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// The boundary from `multipart/form-data; boundary=…`.
fn boundary(content_type: &str) -> Option<String> {
    let (kind, rest) = content_type.split_once(';')?;
    if !kind.trim().eq_ignore_ascii_case("multipart/form-data") {
        return None;
    }
    rest.split(';').find_map(|p| {
        let (key, value) = p.split_once('=')?;
        (key.trim() == "boundary").then(|| value.trim().trim_matches('"').to_string())
    })
}

struct Part<'a> {
    name: String,
    filename: Option<String>,
    data: &'a [u8],
}

/// The fields of a multipart/form-data body.
fn parts<'a>(body: &'a [u8], boundary: &str) -> Vec<Part<'a>> {
    let delimiter = format!("--{boundary}");
    let delimiter = delimiter.as_bytes();
    let mut out = Vec::new();
    let mut rest = match find(body, delimiter) {
        Some(i) => &body[i + delimiter.len()..],
        None => return out,
    };
    // Each part: CRLF, headers, CRLF CRLF, data, CRLF, delimiter.
    while !rest.starts_with(b"--") {
        let Some(part) = rest.strip_prefix(b"\r\n") else {
            break;
        };
        let Some(end) = find(part, delimiter) else {
            break;
        };
        let chunk = &part[..end];
        rest = &part[end + delimiter.len()..];
        let Some(split) = find(chunk, b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&chunk[..split]);
        let disposition = headers
            .lines()
            .find(|l| l.to_ascii_lowercase().starts_with("content-disposition:"))
            .unwrap_or_default();
        let data = &chunk[split + 4..];
        let data = data.strip_suffix(b"\r\n").unwrap_or(data);
        let field = |key: &str| {
            disposition
                .split(';')
                .find_map(|p| p.trim().strip_prefix(&format!("{key}=")))
                .map(|v| v.trim().trim_matches('"').to_string())
        };
        if let Some(name) = field("name") {
            out.push(Part {
                name,
                filename: field("filename"),
                data,
            });
        }
    }
    out
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multipart_fields_and_files_are_read() {
        let body = b"--XyZ\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nwhisper-1\r\n\
--XyZ\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.m4a\"\r\nContent-Type: audio/mp4\r\n\r\n\x00\x01\r\n\x02\r\n\
--XyZ--\r\n";
        let found = parts(body, "XyZ");
        assert_eq!(found.len(), 2);
        assert_eq!(
            (found[0].name.as_str(), found[0].data),
            ("model", &b"whisper-1"[..])
        );
        assert_eq!(found[1].filename.as_deref(), Some("a.m4a"));
        assert_eq!(found[1].data, b"\x00\x01\r\n\x02");
    }

    #[test]
    fn the_server_needs_the_token_and_answers_like_openai() {
        let server = Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        std::thread::spawn(move || {
            for request in server.incoming_requests() {
                serve(request, "owl_t", &|path| {
                    Ok(format!("{} bytes", std::fs::read(path)?.len()))
                });
            }
        });
        let post = |auth: &str, format: &str| {
            use std::io::Write;
            let body = format!(
                "--b\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.wav\"\r\n\r\nabc\r\n\
--b\r\nContent-Disposition: form-data; name=\"response_format\"\r\n\r\n{format}\r\n--b--\r\n"
            );
            let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(
                stream,
                "POST /v1/audio/transcriptions HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\
Authorization: {auth}\r\nContent-Type: multipart/form-data; boundary=b\r\n\
Content-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            let mut reply = String::new();
            stream.read_to_string(&mut reply).unwrap();
            reply
        };
        assert!(post("Bearer nope", "json").starts_with("HTTP/1.1 401"));
        assert!(post("", "json").starts_with("HTTP/1.1 401"));
        let json = post("Bearer owl_t", "json");
        assert!(json.starts_with("HTTP/1.1 200"), "{json}");
        assert!(json.ends_with(r#"{"text":"3 bytes"}"#), "{json}");
        assert!(post("Bearer owl_t", "text").ends_with("\r\n\r\n3 bytes"));
    }

    #[test]
    fn boundary_and_token_checks() {
        assert_eq!(
            boundary("multipart/form-data; boundary=\"abc\"").as_deref(),
            Some("abc")
        );
        assert_eq!(boundary("application/json"), None);
        assert!(same("owl_1", "owl_1") && !same("owl_1", "owl_2") && !same("owl", "owl_1"));
        let token = new_token().unwrap();
        assert!(token.starts_with("owl_") && token.len() == 52);
    }
}
