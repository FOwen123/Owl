//! Owl: an OpenAI-compatible `POST /v1/audio/transcriptions` for other apps
//! on this Mac. Off by default, listens on 127.0.0.1 only, and needs the
//! token shown in Settings (`Authorization: Bearer <token>`). A small HTTP
//! server of its own: one connection at a time, with limits on header and
//! upload size and on how long a request may take to arrive.

use crate::managers::meeting::MeetingManager;
use crate::settings::{get_settings, write_settings, Secret};
use anyhow::{anyhow, Result};
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

pub const PORT: u16 = 8723;
/// Bigger uploads are refused before they're read (OpenAI's limit too).
const MAX_UPLOAD: usize = 25 * 1024 * 1024;
const MAX_HEADERS: usize = 16 * 1024;
/// A request must arrive within this, so a slow client can't hold the server.
const DEADLINE: Duration = Duration::from_secs(60);
/// Longer audio is refused while it's decoded.
const MAX_SECONDS: usize = 60 * 60;

/// The running server's stop flag and address.
static SERVER: Mutex<Option<(Arc<AtomicBool>, SocketAddr)>> = Mutex::new(None);

/// Starts or stops the server to match Settings. A token is made the first
/// time the API is turned on.
pub fn sync(app: &AppHandle) -> Result<()> {
    let mut settings = get_settings(app);
    let mut running = SERVER.lock().unwrap_or_else(|e| e.into_inner());
    if !settings.local_api_enabled {
        if let Some((stop, addr)) = running.take() {
            stop.store(true, Ordering::SeqCst);
            let _ = TcpStream::connect(addr); // wakes the waiting accept
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
    let listener = TcpListener::bind(("127.0.0.1", PORT))
        .map_err(|e| anyhow!("Port {PORT} is busy, so the local API can't start: {e}"))?;
    let stop = Arc::new(AtomicBool::new(false));
    *running = Some((stop.clone(), listener.local_addr()?));
    let app = app.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            let Ok(stream) = stream else { continue };
            let token = get_settings(&app).local_api_token.0;
            let transcribe = |path: &Path| {
                app.state::<Arc<MeetingManager>>()
                    .transcribe_file(path, MAX_SECONDS)
            };
            if catch_unwind(AssertUnwindSafe(|| serve(stream, &token, &transcribe))).is_err() {
                log::error!("Local API: a request crashed");
            }
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

/// Answers one connection; `transcribe` turns the uploaded file into text.
fn serve(mut stream: TcpStream, token: &str, transcribe: &dyn Fn(&Path) -> Result<String>) {
    let (status, body, kind) = match respond(&mut stream, token, transcribe) {
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
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        408 => "Request Timeout",
        411 => "Length Required",
        413 => "Content Too Large",
        431 => "Request Header Fields Too Large",
        _ => "Internal Server Error",
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    // Let the client read the reply before the connection closes: skip a
    // little of any body it's still sending, in small reads.
    let _ = stream.shutdown(Shutdown::Write);
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let _ = std::io::copy(&mut (&stream).take(1 << 20), &mut std::io::sink());
}

/// The text, and whether it was asked for as plain text.
fn respond(
    stream: &mut TcpStream,
    token: &str,
    transcribe: &dyn Fn(&Path) -> Result<String>,
) -> Result<(String, bool), (u16, String)> {
    let started = Instant::now();
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let too_slow = || (408, "The request took too long to arrive".to_string());
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let end = loop {
        if let Some(end) = find(&buf, b"\r\n\r\n") {
            break end;
        }
        if buf.len() > MAX_HEADERS {
            return Err((431, "The headers are too large".into()));
        }
        if started.elapsed() > DEADLINE {
            return Err(too_slow());
        }
        let n = stream.read(&mut chunk).map_err(|_| too_slow())?;
        if n == 0 {
            return Err((400, "The request ended early".into()));
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut request_line = lines.next().unwrap_or_default().split(' ');
    let (method, target) = (
        request_line.next().unwrap_or_default(),
        request_line.next().unwrap_or_default(),
    );
    let headers: Vec<(String, &str)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim()))
        .collect();
    let header = |name: &str| headers.iter().find(|(k, _)| k == name).map(|(_, v)| *v);

    if method != "POST" || target.split('?').next() != Some("/v1/audio/transcriptions") {
        return Err((404, "Only POST /v1/audio/transcriptions is served".into()));
    }
    let given = header("authorization").unwrap_or_default();
    if token.is_empty() || !same(given.strip_prefix("Bearer ").unwrap_or(""), token) {
        return Err((401, "Wrong or missing token".into()));
    }
    let boundary = header("content-type").and_then(boundary).ok_or((
        400,
        "Send multipart/form-data with a `file` field".to_string(),
    ))?;
    let length = header("content-length")
        .filter(|_| header("transfer-encoding").is_none())
        .and_then(|v| v.parse::<usize>().ok())
        .ok_or((411, "Send a Content-Length".to_string()))?;
    if length > MAX_UPLOAD {
        return Err((413, "The file is too big".into()));
    }
    if header("expect").is_some_and(|e| e.eq_ignore_ascii_case("100-continue")) {
        let _ = stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n");
    }
    let mut body = buf.split_off(end + 4);
    body.truncate(length);
    while body.len() < length {
        if started.elapsed() > DEADLINE {
            return Err(too_slow());
        }
        let n = stream.read(&mut chunk).map_err(|_| too_slow())?;
        if n == 0 {
            return Err((400, "The upload ended early".into()));
        }
        body.extend_from_slice(&chunk[..n.min(length - body.len())]);
    }

    let parts = parts(&body, &boundary);
    let file = parts
        .iter()
        .find(|p| p.name == "file")
        .ok_or((400, "No `file` field".to_string()))?;
    let format = parts
        .iter()
        .find(|p| p.name == "response_format")
        .map(|p| String::from_utf8_lossy(p.data).trim().to_string());
    let as_text = match format.as_deref() {
        None | Some("json") => false,
        Some("text") => true,
        Some(other) => {
            return Err((
                400,
                format!("response_format `{other}` isn't supported; use json or text"),
            ))
        }
    };

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
    let result = catch_unwind(AssertUnwindSafe(|| transcribe(&upload)))
        .unwrap_or_else(|_| Err(anyhow!("Transcribing crashed")));
    let _ = std::fs::remove_file(&upload);
    let text = result.map_err(|e| match e.to_string().as_str() {
        crate::media::UNSUPPORTED => (400, "This audio format isn't supported".to_string()),
        crate::media::TOO_LONG => (413, "Send at most an hour of audio".to_string()),
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
    let first = format!("--{boundary}");
    // Later delimiters start a line, so `--boundary` inside a file isn't one.
    let delimiter = format!("\r\n--{boundary}");
    let delimiter = delimiter.as_bytes();
    let mut out = Vec::new();
    let mut rest = match find(body, first.as_bytes()) {
        Some(i) => &body[i + first.len()..],
        None => return out,
    };
    // Each part: CRLF, headers, CRLF CRLF, data, then CRLF and the delimiter.
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
--XyZ\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.m4a\"\r\nContent-Type: audio/mp4\r\n\r\n\x00\x01\r\nab--XyZ\x02\r\n\
--XyZ--\r\n";
        let found = parts(body, "XyZ");
        assert_eq!(found.len(), 2);
        assert_eq!(
            (found[0].name.as_str(), found[0].data),
            ("model", &b"whisper-1"[..])
        );
        assert_eq!(found[1].filename.as_deref(), Some("a.m4a"));
        assert_eq!(found[1].data, b"\x00\x01\r\nab--XyZ\x02");
    }

    #[test]
    fn the_server_needs_the_token_and_answers_like_openai() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                serve(stream.unwrap(), "owl_t", &|path| {
                    Ok(format!("{} bytes", std::fs::read(path)?.len()))
                });
            }
        });
        let post = |auth: &str, format: &str| {
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
        assert!(post("Bearer owl_t", "srt").starts_with("HTTP/1.1 400"));

        // Clients that wait for "100 Continue" before the body get it.
        let body = "--b\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.wav\"\r\n\r\nabcd\r\n--b--\r\n";
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            stream,
            "POST /v1/audio/transcriptions HTTP/1.1\r\nAuthorization: Bearer owl_t\r\n\
Content-Type: multipart/form-data; boundary=b\r\nExpect: 100-continue\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .unwrap();
        let mut interim = [0u8; 25];
        stream.read_exact(&mut interim).unwrap();
        assert_eq!(&interim, b"HTTP/1.1 100 Continue\r\n\r\n");
        stream.write_all(body.as_bytes()).unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).unwrap();
        assert!(reply.ends_with(r#"{"text":"4 bytes"}"#), "{reply}");

        // A huge declared length is refused without waiting for or storing it.
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .write_all(
                b"POST /v1/audio/transcriptions HTTP/1.1\r\nAuthorization: Bearer owl_t\r\n\
Content-Type: multipart/form-data; boundary=b\r\nContent-Length: 999999999999\r\n\r\n",
            )
            .unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).unwrap();
        assert!(reply.starts_with("HTTP/1.1 413"), "{reply}");
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
