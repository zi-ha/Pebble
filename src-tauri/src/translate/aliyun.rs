//! Alibaba Cloud Machine Translation (通用版 `TranslateGeneral`).
//!
//! This is an RPC-style API: all parameters travel as a sorted, percent-encoded
//! query string which is itself signed with HMAC-SHA1 and a key of
//! `access_key_secret + "&"`. The signature is base64 encoded and appended to the
//! same string, so the signed bytes and the submitted body can never drift apart.
//!
//! Docs: <https://help.aliyun.com/zh/machine-translation/developer-reference/api-reference-machine-translation-universal-version-call-guide>

use crate::core::{PebbleError, Result};
use serde_json::Value;

use crate::translate::chunk::split_chunks;
use crate::translate::deeplx::build_segments;
use crate::translate::sign;
use crate::translate::types::TranslateResult;

pub const ALIYUN_ENDPOINT: &str = "https://mt.cn-hangzhou.aliyuncs.com/";
const ACTION: &str = "TranslateGeneral";
const VERSION: &str = "2018-10-12";
pub const DEFAULT_REGION: &str = "cn-hangzhou";
/// The universal endpoint takes roughly 5,000 characters per request.
const MAX_CHUNK_CHARS: usize = 4_000;

/// Map an app language code onto Alibaba Cloud's vocabulary.
pub fn to_aliyun_lang(code: &str) -> Option<String> {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed.to_ascii_lowercase().replace('_', "-");
    if normalized == "auto" {
        return Some("auto".to_string());
    }
    Some(match normalized.as_str() {
        "zh" | "zh-cn" | "zh-sg" | "zh-hans" | "zh-chs" => "zh".to_string(),
        "zh-tw" | "zh-hk" | "zh-mo" | "zh-hant" | "zh-cht" => "zh-tw".to_string(),
        "ja" | "jp" => "ja".to_string(),
        "ko" | "kr" => "ko".to_string(),
        other => other.to_string(),
    })
}

/// Sorted `k=v` pairs, both sides percent-encoded with RFC 3986 rules.
pub fn canonical_query(params: &[(&str, String)]) -> String {
    let mut encoded: Vec<(String, String)> = params
        .iter()
        .map(|(key, value)| (sign::percent_encode(key), sign::percent_encode(value)))
        .collect();
    encoded.sort();
    encoded
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// `POST&%2F&<percent-encoded canonical query>`
pub fn string_to_sign(canonical_query: &str) -> String {
    format!(
        "POST&{}&{}",
        sign::percent_encode("/"),
        sign::percent_encode(canonical_query)
    )
}

/// base64(HMAC-SHA1(secret + "&", string_to_sign))
pub fn signature(access_key_secret: &str, canonical_query: &str) -> String {
    let to_sign = string_to_sign(canonical_query);
    sign::hmac_sha1_base64(format!("{access_key_secret}&").as_bytes(), &to_sign)
}

/// The response is either flat or wrapped in `TranslateGeneralResponse`, and
/// `Code` shows up as a number or a string depending on the endpoint.
pub fn parse_response(value: &Value) -> Result<String> {
    let root = value.get("TranslateGeneralResponse").unwrap_or(value);

    let code = root
        .get("Code")
        .or_else(|| value.get("Code"))
        .and_then(|code| {
            code.as_i64()
                .map(|code| code.to_string())
                .or_else(|| code.as_str().map(str::to_string))
        });
    if let Some(code) = code {
        if code != "200" {
            let message = root
                .get("Message")
                .or_else(|| value.get("Message"))
                .and_then(Value::as_str)
                .unwrap_or("");
            return Err(PebbleError::Translate(format!(
                "Alibaba translate error {code}: {message}"
            )));
        }
    }

    root.pointer("/Data/Translated")
        .or_else(|| root.pointer("/Data/TranslatedText"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            PebbleError::Translate(format!("Alibaba translate returned no text: {value}"))
        })
}

/// Everything the Alibaba Cloud endpoint needs besides the text itself.
pub struct AliyunCredentials<'a> {
    pub access_key_id: &'a str,
    pub access_key_secret: &'a str,
    pub region: Option<&'a str>,
    pub endpoint: Option<&'a str>,
}

pub async fn translate(
    client: &reqwest::Client,
    credentials: &AliyunCredentials<'_>,
    text: &str,
    from: &str,
    to: &str,
) -> Result<TranslateResult> {
    let url = credentials.endpoint.unwrap_or(ALIYUN_ENDPOINT);
    let region = credentials
        .region
        .map(str::trim)
        .filter(|region| !region.is_empty())
        .unwrap_or(DEFAULT_REGION);
    let source = to_aliyun_lang(from).unwrap_or_else(|| "auto".to_string());
    let target = to_aliyun_lang(to).unwrap_or_else(|| "zh".to_string());

    let mut translated = String::new();
    for chunk in split_chunks(text, MAX_CHUNK_CHARS) {
        if chunk.trim().is_empty() {
            translated.push_str(chunk);
            continue;
        }

        let stamp = sign::utc_now();
        let params: Vec<(&str, String)> = vec![
            ("AccessKeyId", credentials.access_key_id.trim().to_string()),
            ("Action", ACTION.to_string()),
            ("Format", "JSON".to_string()),
            ("FormatType", "text".to_string()),
            ("RegionId", region.to_string()),
            ("Scene", "general".to_string()),
            ("SignatureMethod", "HMAC-SHA1".to_string()),
            ("SignatureNonce", sign::nonce()),
            ("SignatureVersion", "1.0".to_string()),
            ("SourceLanguage", source.clone()),
            ("SourceText", chunk.to_string()),
            ("TargetLanguage", target.clone()),
            ("Timestamp", stamp.iso8601.clone()),
            ("Version", VERSION.to_string()),
        ];

        let canonical = canonical_query(&params);
        let signature = signature(credentials.access_key_secret.trim(), &canonical);
        let body = format!("{canonical}&Signature={}", sign::percent_encode(&signature));

        let response = client
            .post(url)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body)
            .send()
            .await
            .map_err(|e| {
                PebbleError::Translate(format!("Alibaba translate request failed: {e}"))
            })?;

        let status = response.status();
        let value: Value = response.json().await.map_err(|e| {
            PebbleError::Translate(format!("Alibaba translate response parse failed: {e}"))
        })?;

        if !status.is_success() {
            return Err(PebbleError::Translate(format!(
                "Alibaba translate error {status}: {value}"
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
    fn maps_alibaba_language_codes() {
        assert_eq!(to_aliyun_lang("zh").as_deref(), Some("zh"));
        assert_eq!(to_aliyun_lang("zh-TW").as_deref(), Some("zh-tw"));
        assert_eq!(to_aliyun_lang("ja").as_deref(), Some("ja"));
        assert_eq!(to_aliyun_lang("ko").as_deref(), Some("ko"));
        assert_eq!(to_aliyun_lang("auto").as_deref(), Some("auto"));
        assert_eq!(to_aliyun_lang(" "), None);
    }

    #[test]
    fn canonical_query_sorts_and_encodes() {
        let params = vec![
            ("TargetLanguage", "en".to_string()),
            ("Action", "TranslateGeneral".to_string()),
            ("SourceText", "你好 世界".to_string()),
        ];
        assert_eq!(
            canonical_query(&params),
            "Action=TranslateGeneral&SourceText=%E4%BD%A0%E5%A5%BD%20%E4%B8%96%E7%95%8C&TargetLanguage=en"
        );
    }

    #[test]
    fn string_to_sign_escapes_the_slash_and_query() {
        let to_sign = string_to_sign("Action=TranslateGeneral&Format=JSON");
        assert_eq!(
            to_sign,
            "POST&%2F&Action%3DTranslateGeneral%26Format%3DJSON"
        );
    }

    #[test]
    fn signature_is_base64_hmac_sha1() {
        let first = signature("secret", "Action=TranslateGeneral");
        // base64 of a 20-byte digest.
        assert_eq!(first.len(), 28);
        assert!(first.ends_with('='));
        // Deterministic for the same input, and different for another secret.
        assert_eq!(first, signature("secret", "Action=TranslateGeneral"));
        assert_ne!(first, signature("other", "Action=TranslateGeneral"));
    }

    /// Ground-truth vector: the expected signature was produced by an independent
    /// implementation of the Alibaba Cloud RPC signing steps for the same input.
    #[test]
    fn signs_the_documented_rpc_way() {
        let params = vec![
            ("AccessKeyId", "k".to_string()),
            ("Action", "TranslateGeneral".to_string()),
            ("SourceText", "你好 世界".to_string()),
            ("TargetLanguage", "en".to_string()),
        ];
        let canonical = canonical_query(&params);

        assert_eq!(
            canonical,
            "AccessKeyId=k&Action=TranslateGeneral&SourceText=%E4%BD%A0%E5%A5%BD%20%E4%B8%96%E7%95%8C&TargetLanguage=en"
        );
        assert_eq!(
            signature("secret", &canonical),
            "6Fc8//qxJdVt2L3tfRG6JbQCv7w="
        );
    }

    #[test]
    fn parses_wrapped_response() {
        let payload = json!({
            "TranslateGeneralResponse": {
                "RequestId": "14E447CA",
                "Data": { "Translated": "Hello" },
                "Code": 200
            }
        });
        assert_eq!(parse_response(&payload).unwrap(), "Hello");
    }

    #[test]
    fn parses_flat_response_with_string_code() {
        let payload = json!({
            "RequestId": "x",
            "Data": { "Translated": "Hello", "WordCount": "1" },
            "Code": "200"
        });
        assert_eq!(parse_response(&payload).unwrap(), "Hello");
    }

    #[test]
    fn surfaces_api_errors() {
        let payload = json!({
            "Code": "InvalidAccessKeyId.NotFound",
            "Message": "Specified access key is not found."
        });
        let error = parse_response(&payload).unwrap_err().to_string();
        assert!(error.contains("InvalidAccessKeyId.NotFound"));
        assert!(error.contains("not found"));
    }

    #[test]
    fn rejects_payload_without_text() {
        assert!(parse_response(&json!({ "Code": 200, "Data": {} }))
            .unwrap_err()
            .to_string()
            .contains("no text"));
    }
}
