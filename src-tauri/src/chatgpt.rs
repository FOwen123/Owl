//! Owl: Sign in with ChatGPT, so the AI steps (cleanup, Edit by voice,
//! meeting notes, the learner) can run on the user's ChatGPT plan instead of
//! an API key. OpenAI's flow for open-source apps
//! (developers.openai.com/siwc/token-sharing-open-source): Owl registers
//! itself on the first sign-in with `dynamic_agent_client`, keeps the client
//! ID it's issued, and calls the Responses API with the access token.
//!
//! Credentials live in `chatgpt/` in the app data folder, owner-only (0600),
//! written atomically, and are never logged.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const PROVIDER_ID: &str = "chatgpt";
const AUTHORIZE: &str = "https://auth.openai.com/api/accounts/authorize";
const TOKEN: &str = "https://auth.openai.com/api/accounts/oauth/token";
const DISCOVERY: &str = "https://auth.openai.com/.well-known/openid-configuration";
const ISSUER: &str = "https://auth.openai.com";
const API: &str = "https://api.openai.com/v1";
const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
/// Without this scope a sign-in is identity only and can't fund requests.
const PLAN_SCOPE: &str = "chatgpt.tokens.use.direct";
const FIRST_CLIENT: &str = "dynamic_agent_client";
const PORT: u16 = 1455;
const SIGN_IN_WAIT: Duration = Duration::from_secs(300);
/// Refresh this long before the access token (1 hour) runs out.
const REFRESH_EARLY_SECS: i64 = 300;
const USAGE_URL: &str = "https://chatgpt.com/settings/usage";

static APP: OnceLock<AppHandle> = OnceLock::new();
static DIR: OnceLock<PathBuf> = OnceLock::new();
/// One refresh at a time: refresh tokens rotate, so two at once would race.
static REFRESH: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub fn init(app: &AppHandle, app_data_dir: &Path) {
    let _ = APP.set(app.clone());
    let _ = DIR.set(app_data_dir.join("chatgpt"));
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq)]
pub struct ChatGptStatus {
    pub signed_in: bool,
    pub email: Option<String>,
}

/// This install's registration with OpenAI; kept across sign-outs.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Registration {
    /// `urn:uuid:…`, made once per Mac.
    host_id: String,
    /// The client ID OpenAI issued on the first sign-in.
    client_id: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Credentials {
    client_id: String,
    access_token: String,
    refresh_token: String,
    expires_at: i64,
    email: Option<String>,
}

fn dir() -> Result<&'static PathBuf> {
    DIR.get()
        .ok_or_else(|| anyhow!("ChatGPT sign-in isn't set up"))
}

fn read<T: for<'de> Deserialize<'de>>(name: &str) -> Option<T> {
    let text = std::fs::read_to_string(dir().ok()?.join(name)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Writes `value` owner-only, replacing the old file in one step.
fn write<T: Serialize>(name: &str, value: &T) -> Result<()> {
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    let dir = dir()?;
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{name}.tmp"));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&tmp)?;
    file.write_all(&serde_json::to_vec(value)?)?;
    file.sync_all()?;
    std::fs::rename(&tmp, dir.join(name))?;
    Ok(())
}

fn forget_credentials() {
    if let Ok(dir) = dir() {
        let _ = std::fs::remove_file(dir.join("credentials.json"));
    }
}

pub fn status() -> ChatGptStatus {
    let creds: Option<Credentials> = read("credentials.json");
    ChatGptStatus {
        signed_in: creds.is_some(),
        email: creds.and_then(|c| c.email),
    }
}

fn registration() -> Result<Registration> {
    if let Some(r) = read::<Registration>("registration.json").filter(|r| !r.host_id.is_empty()) {
        return Ok(r);
    }
    let r = Registration {
        host_id: format!("urn:uuid:{}", uuid_v4()?),
        client_id: None,
    };
    write("registration.json", &r)?;
    Ok(r)
}

// --- Sign in ------------------------------------------------------------

/// Opens the ChatGPT sign-in page with `open` and waits for the browser to
/// come back to Owl.
pub async fn sign_in(open: impl FnOnce(&str) -> Result<()>) -> Result<ChatGptStatus> {
    let mut reg = registration()?;
    let listener = match tokio::net::TcpListener::bind(("127.0.0.1", PORT)).await {
        Ok(l) => l,
        // Only the port may vary between sign-ins.
        Err(_) => tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?,
    };
    let port = listener.local_addr()?.port();
    let redirect = format!("http://127.0.0.1:{port}/auth/callback");
    let verifier = base64url(&random::<32>()?);
    let challenge = base64url(&Sha256::digest(verifier.as_bytes()));
    let state = base64url(&random::<16>()?);
    let nonce = base64url(&random::<16>()?);
    let first = reg.client_id.is_none();
    let client = reg
        .client_id
        .clone()
        .unwrap_or_else(|| FIRST_CLIENT.to_string());

    let mut params = vec![
        ("client_id", client.as_str()),
        ("ext_agent_host_id", reg.host_id.as_str()),
        ("response_type", "code"),
        ("redirect_uri", redirect.as_str()),
        ("scope", SCOPES),
        ("resource", API),
        ("state", state.as_str()),
        ("nonce", nonce.as_str()),
        ("code_challenge", challenge.as_str()),
        ("code_challenge_method", "S256"),
    ];
    if first {
        params.push(("agent_name_hint", "Owl"));
    }
    let url = reqwest::Url::parse_with_params(AUTHORIZE, &params)?;
    open(url.as_str())?;

    let query = tokio::time::timeout(SIGN_IN_WAIT, callback(&listener))
        .await
        .map_err(|_| anyhow!("Sign-in timed out"))??;
    let get = |k: &str| {
        query
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
    };
    if get("state").as_deref() != Some(state.as_str()) {
        return Err(anyhow!("Sign-in answer didn't match this request"));
    }
    if let Some(error) = get("error") {
        return Err(anyhow!("Sign-in was cancelled ({error})"));
    }
    let code = get("code").context("Sign-in answer had no code")?;
    let issued = match (first, get("client_id")) {
        (true, Some(id)) => id,
        (true, None) => return Err(anyhow!("OpenAI didn't finish registering Owl")),
        (false, Some(id)) if Some(&id) != reg.client_id.as_ref() => {
            return Err(anyhow!("Sign-in came back for a different app"))
        }
        (false, _) => client.clone(),
    };

    let tokens = token_request(&[
        ("grant_type", "authorization_code"),
        ("client_id", &issued),
        ("code", &code),
        ("code_verifier", &verifier),
        ("redirect_uri", &redirect),
        ("resource", API),
    ])
    .await?;
    let email = check_id_token(tokens.id_token.as_deref(), &issued, &nonce)?;
    if !tokens.scope.split_whitespace().any(|s| s == PLAN_SCOPE) {
        return Err(anyhow!(
            "ChatGPT didn't allow Owl to use your plan. Sign in again and allow plan usage."
        ));
    }
    if first {
        reg.client_id = Some(issued.clone());
        write("registration.json", &reg)?;
    }
    let creds = Credentials {
        client_id: issued,
        refresh_token: tokens.refresh_token.context("no refresh token")?,
        access_token: tokens.access_token,
        expires_at: now() + tokens.expires_in.unwrap_or(3600),
        email,
    };
    write("credentials.json", &creds)?;
    Ok(status())
}

/// Waits for the browser's request to `/auth/callback` and returns its query.
async fn callback(listener: &tokio::net::TcpListener) -> Result<Vec<(String, String)>> {
    loop {
        let (mut stream, _) = listener.accept().await?;
        let mut buf = vec![0u8; 8192];
        let n = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut buf))
            .await
            .unwrap_or(Ok(0))?;
        let request = String::from_utf8_lossy(&buf[..n]);
        let target = request
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or("");
        if !target.starts_with("/auth/callback") {
            let _ = stream
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await;
            continue; // e.g. the browser asking for a favicon
        }
        let page = "<!doctype html><meta charset=utf-8><title>Owl</title>\
            <p style=\"font:16px -apple-system,sans-serif;margin:3em\">You can close this tab and go back to Owl.</p>";
        let _ = stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
                    page.len()
                )
                .as_bytes(),
            )
            .await;
        let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}"))?;
        return Ok(url.query_pairs().into_owned().collect());
    }
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    expires_in: Option<i64>,
    #[serde(default)]
    scope: String,
}

#[derive(Debug)]
struct TokenError {
    code: String,
    message: String,
}

async fn token_request(form: &[(&str, &str)]) -> Result<TokenReply> {
    token_request_raw(form)
        .await
        .map_err(|e| anyhow!("{}", e.message))
}

async fn token_request_raw(form: &[(&str, &str)]) -> Result<TokenReply, TokenError> {
    let reply = reqwest::Client::new()
        .post(TOKEN)
        .form(form)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| TokenError {
            code: "network".into(),
            message: format!("Couldn't reach OpenAI: {e}"),
        })?;
    let ok = reply.status().is_success();
    let body = reply.text().await.unwrap_or_default();
    if ok {
        return serde_json::from_str(&body).map_err(|e| TokenError {
            code: "bad_reply".into(),
            message: format!("Unexpected answer from OpenAI: {e}"),
        });
    }
    let json: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
    let code = json["error"]
        .as_str()
        .or_else(|| json["error"]["code"].as_str())
        .unwrap_or("unknown")
        .to_string();
    Err(TokenError {
        message: format!("OpenAI refused the sign-in ({code})"),
        code,
    })
}

/// Checks the ID token's claims and returns the account's email. The token
/// comes straight from OpenAI's token endpoint over TLS, which OpenID
/// Connect allows in place of a signature check (Core 1.0, 3.1.3.7); Owl
/// uses it only to show who is signed in.
fn check_id_token(id_token: Option<&str>, client_id: &str, nonce: &str) -> Result<Option<String>> {
    let token = id_token.context("OpenAI sent no ID token")?;
    let payload = token.split('.').nth(1).context("malformed ID token")?;
    let claims: serde_json::Value = serde_json::from_slice(&base64url_decode(payload)?)?;
    let aud_ok = match &claims["aud"] {
        serde_json::Value::String(a) => a == client_id,
        serde_json::Value::Array(a) => a.iter().any(|v| v.as_str() == Some(client_id)),
        _ => false,
    };
    if claims["iss"].as_str() != Some(ISSUER)
        || !aud_ok
        || claims["nonce"].as_str() != Some(nonce)
        || claims["exp"].as_i64().unwrap_or(0) <= now()
    {
        return Err(anyhow!("OpenAI's sign-in answer didn't check out"));
    }
    Ok(claims["email"].as_str().map(str::to_string))
}

/// Refresh codes that mean the user has to sign in again.
const SIGNED_OUT: &[&str] = &[
    "invalid_grant",
    "invalid_refresh_token",
    "token_expired",
    "refresh_token_expired",
    "refresh_token_invalidated",
    "refresh_token_reused",
];

/// A current access token, refreshed when it's about to run out.
async fn access_token() -> Result<String> {
    let _one = REFRESH.lock().await;
    let mut creds: Credentials =
        read("credentials.json").context("Sign in with ChatGPT on the Post Process page")?;
    if creds.expires_at - REFRESH_EARLY_SECS > now() {
        return Ok(creds.access_token);
    }
    let reply = token_request_raw(&[
        ("grant_type", "refresh_token"),
        ("client_id", &creds.client_id),
        ("refresh_token", &creds.refresh_token),
        ("resource", API),
    ])
    .await;
    match reply {
        Ok(tokens) => {
            creds.access_token = tokens.access_token;
            if let Some(refresh) = tokens.refresh_token {
                creds.refresh_token = refresh;
            }
            creds.expires_at = now() + tokens.expires_in.unwrap_or(3600);
            write("credentials.json", &creds)?;
            Ok(creds.access_token)
        }
        Err(e) if SIGNED_OUT.contains(&e.code.as_str()) => {
            forget_credentials();
            changed();
            Err(anyhow!(
                "ChatGPT signed Owl out. Sign in again on the Post Process page."
            ))
        }
        Err(e) => Err(anyhow!("{}", e.message)),
    }
}

pub async fn sign_out() -> Result<()> {
    // A refresh still running would write the credentials back afterwards.
    let _one = REFRESH.lock().await;
    if let Some(creds) = read::<Credentials>("credentials.json") {
        // Best effort: the local sign-out happens either way.
        if let Ok(endpoint) = revocation_endpoint().await {
            let _ = reqwest::Client::new()
                .post(endpoint)
                .form(&[
                    ("token", creds.refresh_token.as_str()),
                    ("token_type_hint", "refresh_token"),
                    ("client_id", creds.client_id.as_str()),
                ])
                .timeout(Duration::from_secs(10))
                .send()
                .await;
        }
    }
    forget_credentials();
    Ok(())
}

async fn revocation_endpoint() -> Result<String> {
    let doc: serde_json::Value = reqwest::Client::new()
        .get(DISCOVERY)
        .timeout(Duration::from_secs(10))
        .send()
        .await?
        .json()
        .await?;
    doc["revocation_endpoint"]
        .as_str()
        .map(str::to_string)
        .context("no revocation endpoint")
}

fn changed() {
    if let Some(app) = APP.get() {
        let _ = app.emit("chatgpt-changed", ());
    }
}

// --- Models and requests ------------------------------------------------

/// The models this account may use.
pub async fn models() -> Result<Vec<String>, String> {
    let token = access_token().await.map_err(|e| e.to_string())?;
    let reply = reqwest::Client::new()
        .get(format!("{API}/models"))
        .bearer_auth(token)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("Couldn't reach OpenAI: {e}"))?;
    let status = reply.status();
    let json: serde_json::Value = reply.json().await.unwrap_or_default();
    if !status.is_success() {
        return Err(api_error(status.as_u16(), &json));
    }
    let list = json["models"]
        .as_array()
        .or_else(|| json["data"].as_array());
    Ok(list
        .into_iter()
        .flatten()
        .filter(|m| m["visibility"].as_str().is_none_or(|v| v == "list"))
        .filter_map(|m| m["slug"].as_str().or_else(|| m["id"].as_str()))
        .map(str::to_string)
        .collect())
}

/// One reply from `model` on the user's plan.
pub async fn complete(
    model: &str,
    system: Option<String>,
    user: String,
) -> Result<Option<String>, String> {
    let token = access_token().await.map_err(|e| e.to_string())?;
    let mut body = serde_json::json!({
        "model": model,
        "input": [{
            "type": "message",
            "role": "user",
            "content": [{ "type": "input_text", "text": user }],
        }],
        "store": false,
        "stream": true,
    });
    if let Some(system) = system {
        body["instructions"] = serde_json::Value::String(system);
    }
    let reply = reqwest::Client::new()
        .post(format!("{API}/responses"))
        .bearer_auth(token)
        .json(&body)
        .timeout(Duration::from_secs(120))
        .send()
        .await
        // A timeout is said as such: it isn't worth trying again.
        .map_err(|e| match e.is_timeout() {
            true => "OpenAI timed out".to_string(),
            false => format!("Couldn't reach OpenAI: {e}"),
        })?;
    let status = reply.status();
    let text = reply.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        let json: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        return Err(api_error(status.as_u16(), &json));
    }
    read_stream(&text)
}

/// The text of a Responses API event stream, or why it failed.
fn read_stream(stream: &str) -> Result<Option<String>, String> {
    let mut out = String::new();
    for data in stream.lines().filter_map(|l| l.strip_prefix("data:")) {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(data.trim()) else {
            continue;
        };
        match event["type"].as_str().unwrap_or("") {
            "response.output_text.delta" => out.push_str(event["delta"].as_str().unwrap_or("")),
            "response.completed" => return Ok(Some(out).filter(|t| !t.trim().is_empty())),
            "response.failed" | "error" => {
                let error = if event["response"]["error"].is_object() {
                    &event["response"]["error"]
                } else {
                    &event["error"]
                };
                return Err(api_error(0, &serde_json::json!({ "error": error })));
            }
            "response.incomplete" => return Err("ChatGPT stopped before finishing".into()),
            _ => {}
        }
    }
    Err("ChatGPT's answer ended early".into())
}

/// A readable error for an API failure. Hitting the plan's limit also tells
/// the user, since it explains why cleanup stopped.
fn api_error(status: u16, json: &serde_json::Value) -> String {
    let code = json["error"]["code"].as_str().unwrap_or("");
    match code {
        "subscription_sharing_usage_limit_exceeded" => {
            if let Some(app) = APP.get() {
                let _ = app.emit("chatgpt-limit", USAGE_URL);
            }
            "Owl has used its ChatGPT allowance for now".to_string()
        }
        "subscription_sharing_invalid_user" => {
            forget_credentials();
            changed();
            "ChatGPT signed Owl out. Sign in again on the Post Process page.".to_string()
        }
        "subscription_sharing_user_not_eligible" => {
            "This ChatGPT plan can't be used in other apps".to_string()
        }
        _ => {
            let message = json["error"]["message"]
                .as_str()
                .or_else(|| json["detail"].as_str())
                .unwrap_or("request failed");
            format!("ChatGPT error {status} {code}: {message}")
        }
    }
}

// --- Helpers ------------------------------------------------------------

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn random<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|e| anyhow!("no randomness: {e}"))?;
    Ok(bytes)
}

fn uuid_v4() -> Result<String> {
    let mut b = random::<16>()?;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Base64url without padding, as PKCE and JWTs use it.
fn base64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 4 / 3 + 3);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..=chunk.len() {
            out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

fn base64url_decode(text: &str) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in text.trim_end_matches('=').bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return Err(anyhow!("bad base64")),
        } as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_matches_the_pkce_example() {
        // Checked against Python's base64.urlsafe_b64encode(sha256(...)).
        let verifier = "dBjftJeZ4CVP-mJ92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            base64url(&Sha256::digest(verifier.as_bytes())),
            "ngF5GsXcbwljx6u133FFr3Xht9xooA_DuaX_3QwODtc"
        );
        assert_eq!(base64url(b"hello world"), "aGVsbG8gd29ybGQ");
        assert_eq!(
            base64url_decode(&base64url(b"hello world")).unwrap(),
            b"hello world"
        );
        assert_eq!(base64url(b"ab"), "YWI");
    }

    #[test]
    fn a_host_id_is_a_version_4_uuid() {
        let id = uuid_v4().unwrap();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
        assert!("89ab".contains(&id[19..20]));
    }

    #[test]
    fn the_id_token_must_be_for_this_sign_in() {
        let token = |claims: serde_json::Value| {
            format!("x.{}.sig", base64url(claims.to_string().as_bytes()))
        };
        let good = serde_json::json!({
            "iss": ISSUER, "aud": "app_1", "nonce": "n", "exp": now() + 60, "email": "a@b.c"
        });
        assert_eq!(
            check_id_token(Some(&token(good.clone())), "app_1", "n").unwrap(),
            Some("a@b.c".to_string())
        );
        assert!(check_id_token(Some(&token(good.clone())), "app_2", "n").is_err());
        assert!(check_id_token(Some(&token(good)), "app_1", "other").is_err());
        let expired = serde_json::json!({ "iss": ISSUER, "aud": "app_1", "nonce": "n", "exp": 1 });
        assert!(check_id_token(Some(&token(expired)), "app_1", "n").is_err());
    }

    #[test]
    fn a_stream_gives_its_text_or_its_error() {
        let ok = "event: x\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"Hi \"}\n\n\
                  data: {\"type\":\"response.output_text.delta\",\"delta\":\"there\"}\n\n\
                  data: {\"type\":\"response.completed\"}\n";
        assert_eq!(read_stream(ok), Ok(Some("Hi there".to_string())));
        let failed = "data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"subscription_sharing_user_not_eligible\"}}}\n";
        assert!(read_stream(failed).unwrap_err().contains("can't be used"));
        assert!(
            read_stream("data: {\"type\":\"response.output_text.delta\",\"delta\":\"x\"}").is_err()
        );
    }
}
