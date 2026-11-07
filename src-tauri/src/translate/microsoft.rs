//! Microsoft Translator.
//!
//! One config covers two ways of reaching Microsoft's translation service:
//!
//! * **Keyless, built in.** The public `ttranslatev3` backend behind
//!   `bing.com/translator`. It needs no account and no key, but each request is
//!   authorised with material (`IG`, `IID`, a key/token pair) scraped from the
//!   translator page, so the values are cached for as long as the page says they
//!   are valid. Microsoft retired the old `edge.microsoft.com/translate/auth`
//!   endpoint in mid-2026, this is the surviving tokenless route.
//! * **Azure subscription.** When the user pastes a key we talk to
//!   `api.cognitive.microsofttranslator.com` with `Ocp-Apim-Subscription-Key`
//!   (the free F0 tier is plenty for email). A region is forwarded when supplied.
//!
//! Both paths return the same `[{ translations: [{ text }] }]` shape, so only the
//! request/auth differs.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::core::{PebbleError, Result};
use serde_json::{json, Value};

use crate::translate::deeplx::build_segments;
use crate::translate::types::TranslateResult;

pub const AZURE_API_BASE: &str = "https://api.cognitive.microsofttranslator.com";
/// Page that hands out the keyless credentials.
const BING_TRANSLATOR_PAGE: &str = "https://www.bing.com/translator";
/// Bing serves a stripped-down page (without the credentials) to unknown agents.
const BING_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
(KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

const API_VERSION: &str = "3.0";
/// Keep requests small enough that a single POST stays comfortable.
const MAX_CHUNK_CHARS: usize = 4_000;
/// Fallback lifetime when the page does not advertise one.
const DEFAULT_SESSION_TTL: Duration = Duration::from_secs(20 * 60);

#[derive(Debug, Clone)]
struct BingSession {
    /// Origin that served the page, e.g. `https://cn.bing.com`.
    host: String,
    ig: String,
    iid: String,
    key: String,
    token: String,
    expires_at: Instant,
}

fn session_cache() -> &'static Mutex<Option<BingSession>> {
    static CACHE: OnceLock<Mutex<Option<BingSession>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn cached_session() -> Option<BingSession> {
    let guard = session_cache().lock().ok()?;
    guard
        .as_ref()
        .filter(|session| session.expires_at > Instant::now())
        .cloned()
}

fn store_session(session: BingSession) {
    if let Ok(mut guard) = session_cache().lock() {
        *guard = Some(session);
    }
}

fn invalidate_session() {
    if let Ok(mut guard) = session_cache().lock() {
        *guard = None;
    }
}

/// Map an app language code onto the code Microsoft expects.
///
/// The rest of Pebble speaks in short codes (`zh`, `en`, `ja`) while Microsoft
/// wants script-qualified Chinese (`zh-Hans` / `zh-Hant`). Unknown codes are
/// passed through lower-cased, which is what the API expects for simple tags.
/// `None` means "let the service detect the source language".
pub fn to_microsoft_lang(code: &str) -> Option<String> {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed.to_ascii_lowercase().replace('_', "-");
    if normalized == "auto" {
        return None;
    }
    Some(match normalized.as_str() {
        "zh" | "zh-cn" | "zh-sg" | "zh-hans" | "zh-chs" => "zh-Hans".to_string(),
        "zh-tw" | "zh-hk" | "zh-mo" | "zh-hant" | "zh-cht" => "zh-Hant".to_string(),
        other => other.to_string(),
    })
}

/// Split long text into request-sized chunks, preferring line boundaries so the
/// translated output still lines up with the source.
pub fn split_chunks(text: &str) -> Vec<&str> {
    crate::translate::chunk::split_chunks(text, MAX_CHUNK_CHARS)
}

// ─── page parsing ────────────────────────────────────────────────────────────

fn value_after<'a>(haystack: &'a str, marker: &str) -> Option<&'a str> {
    let start = haystack.find(marker)? + marker.len();
    let rest = &haystack[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

pub fn parse_ig(html: &str) -> Option<String> {
    value_after(html, "IG:\"").map(str::to_string)
}

pub fn parse_iid(html: &str) -> Option<String> {
    value_after(html, "data-iid=\"translator.").map(|value| format!("translator.{value}"))
}

/// `params_AbusePreventionHelper = [<key>,"<token>",<ttl_ms>]`
pub fn parse_abuse_params(html: &str) -> Option<(String, String, u64)> {
    let marker = "params_AbusePreventionHelper = [";
    let start = html.find(marker)? + marker.len();
    let rest = &html[start..];
    let key_end = rest.find(',')?;
    let key = rest[..key_end].trim().to_string();
    let rest = &rest[key_end + 1..];
    let token_start = rest.find('"')? + 1;
    let token_end = rest[token_start..].find('"')? + token_start;
    let token = rest[token_start..token_end].to_string();
    let tail = &rest[token_end + 1..];
    let ttl_start = tail.find(',')? + 1;
    let ttl: String = tail[ttl_start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();

    if key.is_empty() || token.is_empty() {
        return None;
    }
    Some((key, token, ttl.parse().unwrap_or(0)))
}

fn origin_of(url: &reqwest::Url) -> String {
    let mut origin = format!(
        "{}://{}",
        url.scheme(),
        url.host_str().unwrap_or("www.bing.com")
    );
    if let Some(port) = url.port() {
        origin.push_str(&format!(":{port}"));
    }
    origin
}

async fn fetch_session(client: &reqwest::Client, force_refresh: bool) -> Result<BingSession> {
    if !force_refresh {
        if let Some(session) = cached_session() {
            return Ok(session);
        }
    }

    let response = client
        .get(BING_TRANSLATOR_PAGE)
        .header(reqwest::header::USER_AGENT, BING_USER_AGENT)
        .header(reqwest::header::ACCEPT_LANGUAGE, "en-US,en;q=0.9")
        .send()
        .await
        .map_err(|e| PebbleError::Translate(format!("Microsoft translator page failed: {e}")))?;

    let status = response.status();
    let host = origin_of(response.url());
    let html = response.text().await.map_err(|e| {
        PebbleError::Translate(format!("Microsoft translator page read failed: {e}"))
    })?;

    if !status.is_success() {
        return Err(PebbleError::Translate(format!(
            "Microsoft translator page error {status}"
        )));
    }

    let (ig, iid, (key, token, ttl_ms)) =
        match (parse_ig(&html), parse_iid(&html), parse_abuse_params(&html)) {
            (Some(ig), Some(iid), Some(params)) => (ig, iid, params),
            _ => {
                return Err(PebbleError::Translate(
                    "Microsoft translator credentials not found on the page".to_string(),
                ))
            }
        };

    let ttl = if ttl_ms > 0 {
        Duration::from_millis(ttl_ms.saturating_sub(60_000))
    } else {
        DEFAULT_SESSION_TTL
    };

    let session = BingSession {
        host,
        ig,
        iid,
        key,
        token,
        expires_at: Instant::now() + ttl,
    };
    store_session(session.clone());
    Ok(session)
}

// ─── request/response ────────────────────────────────────────────────────────

/// Pull the translated text out of either Microsoft response shape.
pub fn parse_response(value: &Value) -> Result<String> {
    if let Some(error) = value.get("statusCode") {
        let message = value
            .get("errorMessage")
            .and_then(Value::as_str)
            .unwrap_or("");
        return Err(PebbleError::Translate(format!(
            "Microsoft translate error {error}: {message}"
        )));
    }

    let items = value.as_array().ok_or_else(|| {
        PebbleError::Translate(format!(
            "Microsoft translate returned an unexpected response: {value}"
        ))
    })?;

    let mut out = String::new();
    for item in items {
        let text = item
            .get("translations")
            .and_then(Value::as_array)
            .and_then(|translations| translations.first())
            .and_then(|translation| translation.get("text"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                PebbleError::Translate(format!(
                    "Microsoft translate returned no translation: {item}"
                ))
            })?;
        out.push_str(text);
    }
    Ok(out)
}

fn is_retryable(response: &Value) -> bool {
    matches!(
        response.get("statusCode").and_then(Value::as_i64),
        Some(205 | 400 | 401)
    )
}

pub async fn translate(
    client: &reqwest::Client,
    api_key: Option<&str>,
    region: Option<&str>,
    endpoint: Option<&str>,
    text: &str,
    from: &str,
    to: &str,
) -> Result<TranslateResult> {
    let key = api_key.map(str::trim).filter(|value| !value.is_empty());
    let endpoint = endpoint.map(str::trim).filter(|value| !value.is_empty());
    let target = to_microsoft_lang(to).unwrap_or_else(|| "en".to_string());
    let source = to_microsoft_lang(from);

    let mut translated = String::new();
    for chunk in split_chunks(text) {
        if chunk.trim().is_empty() {
            translated.push_str(chunk);
            continue;
        }
        let piece = match key {
            Some(key) => {
                azure_request(
                    client,
                    endpoint,
                    key,
                    region,
                    chunk,
                    source.as_deref(),
                    &target,
                )
                .await?
            }
            None => bing_request(client, endpoint, chunk, source.as_deref(), &target).await?,
        };
        translated.push_str(&piece);
    }

    Ok(TranslateResult {
        segments: build_segments(text, &translated),
        translated,
    })
}

async fn bing_request(
    client: &reqwest::Client,
    endpoint: Option<&str>,
    text: &str,
    from: Option<&str>,
    to: &str,
) -> Result<String> {
    let mut attempt = 0;
    loop {
        let session = fetch_session(client, attempt > 0).await?;
        let host = endpoint.unwrap_or(&session.host);
        // Bing wants a fresh IID suffix per request; the value itself only has to
        // be unique, so any monotonically growing number works.
        let sequence = request_sequence();
        let url = format!(
            "{}/ttranslatev3?isVertical=1&&IG={}&IID={}.{}",
            host.trim_end_matches('/'),
            session.ig,
            session.iid,
            sequence
        );

        let form = [
            ("fromLang", from.unwrap_or("auto-detect").to_string()),
            ("to", to.to_string()),
            ("text", text.to_string()),
            ("token", session.token.clone()),
            ("key", session.key.clone()),
        ];

        let response = client
            .post(&url)
            .header(reqwest::header::USER_AGENT, BING_USER_AGENT)
            .header(reqwest::header::REFERER, format!("{host}/translator"))
            .form(&form)
            .send()
            .await
            .map_err(|e| {
                PebbleError::Translate(format!("Microsoft translate request failed: {e}"))
            })?;

        let status = response.status();
        let value: Value = response.json().await.map_err(|e| {
            PebbleError::Translate(format!("Microsoft translate response parse failed: {e}"))
        })?;

        if status.is_success() {
            if is_retryable(&value) && attempt == 0 {
                invalidate_session();
                attempt += 1;
                continue;
            }
            return parse_response(&value);
        }

        if status.is_server_error() && attempt == 0 {
            invalidate_session();
            attempt += 1;
            continue;
        }

        return Err(PebbleError::Translate(format!(
            "Microsoft translate error {status}: {value}"
        )));
    }
}

fn request_sequence() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(1);
    SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

async fn azure_request(
    client: &reqwest::Client,
    endpoint: Option<&str>,
    key: &str,
    region: Option<&str>,
    text: &str,
    from: Option<&str>,
    to: &str,
) -> Result<String> {
    let base = endpoint.unwrap_or(AZURE_API_BASE).trim_end_matches('/');
    let mut url = format!("{base}/translate?api-version={API_VERSION}&to={to}");
    if let Some(from) = from {
        url.push_str("&from=");
        url.push_str(from);
    }

    let mut request = client
        .post(&url)
        .header("Ocp-Apim-Subscription-Key", key)
        .json(&json!([{ "Text": text }]));
    if let Some(region) = region.map(str::trim).filter(|value| !value.is_empty()) {
        request = request.header("Ocp-Apim-Subscription-Region", region);
    }

    let response = request
        .send()
        .await
        .map_err(|e| PebbleError::Translate(format!("Microsoft translate request failed: {e}")))?;
    let status = response.status();
    let value: Value = response.json().await.map_err(|e| {
        PebbleError::Translate(format!("Microsoft translate response parse failed: {e}"))
    })?;

    if !status.is_success() {
        return Err(PebbleError::Translate(format!(
            "Microsoft translate error {status}: {value}"
        )));
    }
    parse_response(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE_FRAGMENT: &str = r#"
        <script>window.__cfg={IG:"A2A3F62CF6DC4C9AAF232D16CFBCB3D4"};</script>
        <div id="rich_tta" data-iid="translator.5023"></div>
        <script>var params_AbusePreventionHelper = [1794056738758,"jeiFskzZH7PUiLfK34fnf3kgbauf7hPX",3600000];</script>
    "#;

    #[test]
    fn maps_chinese_variants() {
        assert_eq!(to_microsoft_lang("zh").as_deref(), Some("zh-Hans"));
        assert_eq!(to_microsoft_lang("zh-CN").as_deref(), Some("zh-Hans"));
        assert_eq!(to_microsoft_lang("zh_hans").as_deref(), Some("zh-Hans"));
        assert_eq!(to_microsoft_lang("zh-TW").as_deref(), Some("zh-Hant"));
        assert_eq!(to_microsoft_lang("zh-HK").as_deref(), Some("zh-Hant"));
    }

    #[test]
    fn passes_through_simple_languages_and_drops_auto() {
        assert_eq!(to_microsoft_lang("en").as_deref(), Some("en"));
        assert_eq!(to_microsoft_lang(" JA ").as_deref(), Some("ja"));
        assert_eq!(to_microsoft_lang("auto"), None);
        assert_eq!(to_microsoft_lang(""), None);
    }

    #[test]
    fn parses_credentials_from_the_translator_page() {
        assert_eq!(
            parse_ig(PAGE_FRAGMENT).as_deref(),
            Some("A2A3F62CF6DC4C9AAF232D16CFBCB3D4")
        );
        assert_eq!(parse_iid(PAGE_FRAGMENT).as_deref(), Some("translator.5023"));
        assert_eq!(
            parse_abuse_params(PAGE_FRAGMENT),
            Some((
                "1794056738758".to_string(),
                "jeiFskzZH7PUiLfK34fnf3kgbauf7hPX".to_string(),
                3_600_000
            ))
        );
    }

    #[test]
    fn missing_credentials_are_reported_as_none() {
        assert_eq!(parse_ig("<html></html>"), None);
        assert_eq!(parse_iid("<html></html>"), None);
        assert_eq!(parse_abuse_params("<html></html>"), None);
    }

    #[test]
    fn parses_translation_payload() {
        let payload = json!([{
            "detectedLanguage": { "language": "en", "score": 1.0 },
            "translations": [{ "text": "你好", "to": "zh-Hans" }]
        }]);
        assert_eq!(parse_response(&payload).unwrap(), "你好");
    }

    #[test]
    fn surfaces_bing_error_status() {
        let err = parse_response(&json!({ "statusCode": 205, "errorMessage": "" })).unwrap_err();
        assert!(err.to_string().contains("205"));
        assert!(is_retryable(&json!({ "statusCode": 205 })));
        assert!(!is_retryable(
            &json!([{ "translations": [{ "text": "hi" }] }])
        ));
    }

    #[test]
    fn rejects_unexpected_payload() {
        let err = parse_response(&json!("nope")).unwrap_err();
        assert!(err.to_string().contains("unexpected response"));
    }

    #[test]
    fn keeps_short_text_in_one_chunk() {
        assert_eq!(split_chunks("Hello\nWorld"), vec!["Hello\nWorld"]);
    }

    #[test]
    fn splits_long_text_on_line_boundaries_and_preserves_content() {
        let line = "x".repeat(400);
        let text = std::iter::repeat_n(line.as_str(), 60)
            .collect::<Vec<_>>()
            .join("\n");

        let chunks = split_chunks(&text);

        assert!(chunks.len() > 1, "long text must be split");
        for chunk in &chunks {
            assert!(chunk.len() <= MAX_CHUNK_CHARS, "chunk exceeded the limit");
        }
        assert_eq!(chunks.concat(), text);
    }
}
