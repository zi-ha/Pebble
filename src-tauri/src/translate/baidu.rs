//! Baidu Translate (百度翻译开放平台 / 通用文本翻译).
//!
//! Auth is the simplest of the four vendors: every request carries `appid` and a
//! `sign` that is `MD5(appid + q + salt + secret_key)`. There is no token to
//! refresh and no header signing.
//!
//! Docs: <https://fanyi-api.baidu.com/doc/21>

use crate::core::{PebbleError, Result};
use serde_json::Value;

use crate::translate::chunk::split_chunks;
use crate::translate::deeplx::build_segments;
use crate::translate::sign;
use crate::translate::types::TranslateResult;

pub const BAIDU_ENDPOINT: &str = "https://fanyi-api.baidu.com/api/trans/vip/translate";
/// Baidu counts bytes and rejects requests over 6 KB; keep a margin.
const MAX_CHUNK_CHARS: usize = 3_500;

/// Map an app language code onto Baidu's vocabulary.
///
/// Baidu is the odd one out: Japanese is `jp`, Korean is `kor`, French `fra`,
/// Spanish `spa` and Traditional Chinese is `cht`. `None` means "detect".
pub fn to_baidu_lang(code: &str) -> Option<String> {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed.to_ascii_lowercase().replace('_', "-");
    if normalized == "auto" {
        return None;
    }
    Some(match normalized.as_str() {
        "zh" | "zh-cn" | "zh-sg" | "zh-hans" | "zh-chs" => "zh".to_string(),
        "zh-tw" | "zh-hk" | "zh-mo" | "zh-hant" | "zh-cht" => "cht".to_string(),
        "ja" | "jp" => "jp".to_string(),
        "ko" | "kr" => "kor".to_string(),
        "fr" => "fra".to_string(),
        "es" => "spa".to_string(),
        "ar" => "ara".to_string(),
        "vi" => "vie".to_string(),
        "ms" => "may".to_string(),
        "ro" => "rom".to_string(),
        "sv" => "swe".to_string(),
        "da" => "dan".to_string(),
        "fi" => "fin".to_string(),
        "uk" => "ukr".to_string(),
        "fa" => "per".to_string(),
        other => other.to_string(),
    })
}

/// `sign = MD5(appid + q + salt + secret_key)`, lower-case hex.
pub fn sign_for(app_id: &str, text: &str, salt: &str, secret_key: &str) -> String {
    sign::md5_hex(&format!("{app_id}{text}{salt}{secret_key}"))
}

/// Baidu splits multi-line input itself and returns one entry per line.
pub fn parse_response(value: &Value) -> Result<String> {
    if let Some(code) = value.get("error_code").and_then(|code| {
        code.as_str()
            .map(str::to_string)
            .or_else(|| code.as_i64().map(|c| c.to_string()))
    }) {
        let message = value.get("error_msg").and_then(Value::as_str).unwrap_or("");
        return Err(PebbleError::Translate(format!(
            "Baidu translate error {code}: {message}"
        )));
    }

    let results = value
        .get("trans_result")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            PebbleError::Translate(format!(
                "Baidu translate returned an unexpected response: {value}"
            ))
        })?;

    let mut out = Vec::with_capacity(results.len());
    for item in results {
        let text = item.get("dst").and_then(Value::as_str).ok_or_else(|| {
            PebbleError::Translate(format!("Baidu translate returned no text: {item}"))
        })?;
        out.push(text.to_string());
    }
    Ok(out.join("\n"))
}

pub async fn translate(
    client: &reqwest::Client,
    app_id: &str,
    secret_key: &str,
    endpoint: Option<&str>,
    text: &str,
    from: &str,
    to: &str,
) -> Result<TranslateResult> {
    let url = endpoint.unwrap_or(BAIDU_ENDPOINT);
    let source = to_baidu_lang(from).unwrap_or_else(|| "auto".to_string());
    let target = to_baidu_lang(to).unwrap_or_else(|| "zh".to_string());

    let mut translated = String::new();
    for chunk in split_chunks(text, MAX_CHUNK_CHARS) {
        if chunk.trim().is_empty() {
            translated.push_str(chunk);
            continue;
        }

        let salt = sign::nonce();
        let form = [
            ("q", chunk.to_string()),
            ("from", source.clone()),
            ("to", target.clone()),
            ("appid", app_id.trim().to_string()),
            ("salt", salt.clone()),
            (
                "sign",
                sign_for(app_id.trim(), chunk, &salt, secret_key.trim()),
            ),
        ];

        let response =
            client.post(url).form(&form).send().await.map_err(|e| {
                PebbleError::Translate(format!("Baidu translate request failed: {e}"))
            })?;

        let status = response.status();
        let value: Value = response.json().await.map_err(|e| {
            PebbleError::Translate(format!("Baidu translate response parse failed: {e}"))
        })?;

        if !status.is_success() {
            return Err(PebbleError::Translate(format!(
                "Baidu translate error {status}: {value}"
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
    use serde_json::json;

    #[test]
    fn maps_baidu_language_codes() {
        assert_eq!(to_baidu_lang("zh").as_deref(), Some("zh"));
        assert_eq!(to_baidu_lang("zh-TW").as_deref(), Some("cht"));
        assert_eq!(to_baidu_lang("ja").as_deref(), Some("jp"));
        assert_eq!(to_baidu_lang("ko").as_deref(), Some("kor"));
        assert_eq!(to_baidu_lang("fr").as_deref(), Some("fra"));
        assert_eq!(to_baidu_lang("es").as_deref(), Some("spa"));
        assert_eq!(to_baidu_lang("en").as_deref(), Some("en"));
        assert_eq!(to_baidu_lang("auto"), None);
    }

    #[test]
    fn sign_is_md5_of_appid_text_salt_and_secret() {
        // md5("2015063000000001apple143566028812345678")
        assert_eq!(
            sign_for("2015063000000001", "apple", "1435660288", "12345678"),
            sign::md5_hex("2015063000000001apple143566028812345678")
        );
    }

    #[test]
    fn joins_translated_lines() {
        let payload = json!({
            "from": "en",
            "to": "zh",
            "trans_result": [
                { "src": "Hello", "dst": "你好" },
                { "src": "World", "dst": "世界" }
            ]
        });
        assert_eq!(parse_response(&payload).unwrap(), "你好\n世界");
    }

    #[test]
    fn surfaces_error_codes_as_numbers_or_strings() {
        let numeric = json!({ "error_code": 54003, "error_msg": "Invalid Access Limit" });
        assert!(parse_response(&numeric)
            .unwrap_err()
            .to_string()
            .contains("54003"));

        let text = json!({ "error_code": "52001", "error_msg": "TIMEOUT" });
        assert!(parse_response(&text)
            .unwrap_err()
            .to_string()
            .contains("52001"));
    }

    #[test]
    fn rejects_unexpected_payload() {
        assert!(parse_response(&json!({ "from": "en" }))
            .unwrap_err()
            .to_string()
            .contains("unexpected response"));
    }
}
