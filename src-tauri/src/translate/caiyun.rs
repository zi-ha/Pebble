//! Caiyun Xiaoyi (彩云小译 · LingoCloud).
//!
//! The simplest vendor here: one token, no signature. The request names a
//! translation *direction* rather than a language pair (`auto2zh`), and Caiyun
//! only supports Chinese, English, and Japanese.
//!
//! Docs: <https://docs.caiyunapp.com/lingocloud-api/index.html>

use crate::core::{PebbleError, Result};
use serde_json::{json, Value};

use crate::translate::chunk::split_chunks;
use crate::translate::deeplx::build_segments;
use crate::translate::sign;
use crate::translate::types::TranslateResult;

pub const CAIYUN_ENDPOINT: &str = "https://api.interpreter.caiyunai.com/v1/translator";
/// The API batches whole sentences; keep each request well inside its limits.
const MAX_CHUNK_CHARS: usize = 2_000;

/// Caiyun's whole vocabulary: `zh`, `en`, `ja`. Anything else cannot be a source
/// or a target.
pub fn to_caiyun_lang(code: &str) -> Option<&'static str> {
    let normalized = code.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "zh" | "zh-cn" | "zh-sg" | "zh-hans" | "zh-chs" | "zh-tw" | "zh-hk" | "zh-mo"
        | "zh-hant" | "zh-cht" => Some("zh"),
        "en" | "en-us" | "en-gb" => Some("en"),
        "ja" | "jp" => Some("ja"),
        _ => None,
    }
}

/// Build the `trans_type` direction string, e.g. `auto2zh` or `en2ja`.
///
/// Caiyun requires a direction, so a target outside `zh/en/ja` is an error
/// rather than a silent downgrade to Chinese.
pub fn trans_type(from: &str, to: &str) -> Result<String> {
    let target = to_caiyun_lang(to).ok_or_else(|| {
        PebbleError::Translate(
            "Caiyun translate only supports translating into Chinese, English, or Japanese"
                .to_string(),
        )
    })?;
    Ok(match to_caiyun_lang(from) {
        Some(source) if source != target => format!("{source}2{target}"),
        _ => format!("auto2{target}"),
    })
}

pub fn payload_for(text: &str, trans_type: &str, request_id: &str) -> String {
    json!({
        "source": [text],
        "trans_type": trans_type,
        "request_id": request_id,
        "detect": trans_type.starts_with("auto"),
        "media": "text",
    })
    .to_string()
}

pub fn parse_response(value: &Value) -> Result<String> {
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .or_else(|| error.as_str())
            .or_else(|| value.get("message").and_then(Value::as_str))
            .unwrap_or("");
        return Err(PebbleError::Translate(format!(
            "Caiyun translate error: {message}"
        )));
    }

    let targets = value
        .get("target")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            PebbleError::Translate(format!("Caiyun translate returned no text: {value}"))
        })?;

    let mut out = Vec::with_capacity(targets.len());
    for target in targets {
        let text = target.as_str().ok_or_else(|| {
            PebbleError::Translate(format!("Caiyun translate returned no text: {target}"))
        })?;
        out.push(text.to_string());
    }
    if out.is_empty() {
        return Err(PebbleError::Translate(
            "Caiyun translate returned no text".to_string(),
        ));
    }
    Ok(out.join("\n"))
}

pub async fn translate(
    client: &reqwest::Client,
    token: &str,
    endpoint: Option<&str>,
    text: &str,
    from: &str,
    to: &str,
) -> Result<TranslateResult> {
    let url = endpoint.unwrap_or(CAIYUN_ENDPOINT);
    let direction = trans_type(from, to)?;

    let mut translated = String::new();
    for chunk in split_chunks(text, MAX_CHUNK_CHARS) {
        if chunk.trim().is_empty() {
            translated.push_str(chunk);
            continue;
        }

        let payload = payload_for(chunk, &direction, &sign::nonce());
        let response = client
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            // Caiyun wants the literal word `token` in front of the value.
            .header("x-authorization", format!("token {}", token.trim()))
            .body(payload)
            .send()
            .await
            .map_err(|e| PebbleError::Translate(format!("Caiyun translate request failed: {e}")))?;

        let status = response.status();
        let value: Value = response.json().await.map_err(|e| {
            PebbleError::Translate(format!("Caiyun translate response parse failed: {e}"))
        })?;

        if !status.is_success() {
            return Err(PebbleError::Translate(format!(
                "Caiyun translate error {status}: {value}"
            )));
        }
        translated.push_str(&parse_response(&value)?);
    }

    Ok(TranslateResult {
        segments: build_segments(text, &translated),
        translated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_only_three_languages() {
        assert_eq!(to_caiyun_lang("zh"), Some("zh"));
        assert_eq!(to_caiyun_lang("zh-TW"), Some("zh"));
        assert_eq!(to_caiyun_lang("en-US"), Some("en"));
        assert_eq!(to_caiyun_lang("ja"), Some("ja"));
        assert_eq!(to_caiyun_lang("de"), None);
        assert_eq!(to_caiyun_lang("auto"), None);
        assert_eq!(to_caiyun_lang(""), None);
    }

    #[test]
    fn builds_a_detecting_direction_when_the_source_is_unknown() {
        assert_eq!(trans_type("auto", "zh").unwrap(), "auto2zh");
        assert_eq!(trans_type("de", "zh").unwrap(), "auto2zh");
        // Same language on both sides: let Caiyun detect instead of sending zh2zh.
        assert_eq!(trans_type("zh", "zh").unwrap(), "auto2zh");
    }

    #[test]
    fn builds_an_explicit_direction_for_supported_pairs() {
        assert_eq!(trans_type("en", "zh").unwrap(), "en2zh");
        assert_eq!(trans_type("zh", "ja").unwrap(), "zh2ja");
        assert_eq!(trans_type("ja", "en").unwrap(), "ja2en");
    }

    #[test]
    fn rejects_unsupported_targets() {
        let error = trans_type("en", "fr").unwrap_err().to_string();
        assert!(error.contains("Chinese, English, or Japanese"));
    }

    #[test]
    fn payload_batches_one_sentence_and_flags_detection() {
        let payload: Value =
            serde_json::from_str(&payload_for("Hello", "auto2zh", "req-1")).unwrap();

        assert_eq!(payload["source"][0], "Hello");
        assert_eq!(payload["trans_type"], "auto2zh");
        assert_eq!(payload["request_id"], "req-1");
        assert_eq!(payload["detect"], true);
        assert_eq!(payload["media"], "text");

        let explicit: Value =
            serde_json::from_str(&payload_for("Hello", "en2zh", "req-2")).unwrap();
        assert_eq!(explicit["detect"], false);
    }

    #[test]
    fn parses_the_target_array() {
        let payload = json!({ "target": ["小译翻译是最好的翻译服务。"] });
        assert_eq!(
            parse_response(&payload).unwrap(),
            "小译翻译是最好的翻译服务。"
        );
    }

    #[test]
    fn surfaces_api_errors() {
        let payload = json!({ "error": { "message": "invalid token" } });
        assert!(parse_response(&payload)
            .unwrap_err()
            .to_string()
            .contains("invalid token"));

        let plain = json!({ "error": "rate limited" });
        assert!(parse_response(&plain)
            .unwrap_err()
            .to_string()
            .contains("rate limited"));
    }

    #[test]
    fn rejects_payloads_without_text() {
        assert!(parse_response(&json!({ "target": [] }))
            .unwrap_err()
            .to_string()
            .contains("no text"));
        assert!(parse_response(&json!({}))
            .unwrap_err()
            .to_string()
            .contains("no text"));
    }
}
