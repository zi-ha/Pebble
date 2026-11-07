//! Volcengine Machine Translation (火山翻译, `TranslateText`).
//!
//! Signed with Volcengine's V4 scheme, the same key-derivation shape as AWS SigV4
//! but with its own header names: a canonical request is hashed, then signed with
//! `HMAC(secret → date → region → service → "request")`.
//!
//! Docs: <https://www.volcengine.com/docs/4640/65067>

use pebble_core::{PebbleError, Result};
use serde_json::{json, Value};

use crate::chunk::split_chunks;
use crate::deeplx::build_segments;
use crate::sign;
use crate::types::TranslateResult;

pub const VOLC_HOST: &str = "open.volcengineapi.com";
const SERVICE: &str = "translate";
/// Sorted query string for `TranslateText`, API version 2020-06-01. It is part of
/// the signed material, so the exact spelling matters.
const QUERY: &str = "Action=TranslateText&Version=2020-06-01";
const CONTENT_TYPE: &str = "application/json";
const SIGNED_HEADERS: &str = "content-type;host;x-content-sha256;x-date";
pub const DEFAULT_REGION: &str = "cn-north-1";
/// Volcengine caps a request at 5,000 characters / 16 text items.
const MAX_CHUNK_CHARS: usize = 4_000;

/// Map an app language code onto Volcengine's vocabulary (script-qualified Chinese).
pub fn to_volcengine_lang(code: &str) -> Option<String> {
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
        "zh-tw" | "zh-hk" | "zh-mo" | "zh-hant" | "zh-cht" => "zh-Hant".to_string(),
        "ja" | "jp" => "ja".to_string(),
        "ko" | "kr" => "ko".to_string(),
        other => other.to_string(),
    })
}

pub fn payload_for(text: &str, source: Option<&str>, target: &str) -> String {
    let mut body = json!({
        "TargetLanguage": target,
        "TextList": [text],
    });
    if let Some(source) = source {
        body["SourceLanguage"] = json!(source);
    }
    body.to_string()
}

pub fn canonical_request(payload_hash: &str, host: &str, amz_date: &str) -> String {
    format!(
        "POST\n/\n{QUERY}\ncontent-type:{CONTENT_TYPE}\nhost:{host}\nx-content-sha256:{payload_hash}\nx-date:{amz_date}\n\n{SIGNED_HEADERS}\n{payload_hash}"
    )
}

pub fn string_to_sign(amz_date: &str, date: &str, region: &str, canonical_request: &str) -> String {
    format!(
        "HMAC-SHA256\n{amz_date}\n{date}/{region}/{SERVICE}/request\n{}",
        sign::sha256_hex(canonical_request)
    )
}

pub fn signature(secret_key: &str, signing_scope: &SigningScope, string_to_sign: &str) -> String {
    let key_date = sign::hmac_sha256(secret_key.as_bytes(), signing_scope.date);
    let key_region = sign::hmac_sha256(&key_date, signing_scope.region);
    let key_service = sign::hmac_sha256(&key_region, SERVICE);
    let key_signing = sign::hmac_sha256(&key_service, "request");
    sign::hmac_sha256_hex(&key_signing, string_to_sign)
}

pub struct SigningScope<'a> {
    pub date: &'a str,
    pub region: &'a str,
}

pub fn authorization(
    access_key: &str,
    secret_key: &str,
    scope: &SigningScope<'_>,
    amz_date: &str,
    payload_hash: &str,
    host: &str,
) -> String {
    let canonical = canonical_request(payload_hash, host, amz_date);
    let to_sign = string_to_sign(amz_date, scope.date, scope.region, &canonical);
    let signature = signature(secret_key, scope, &to_sign);
    format!(
        "HMAC-SHA256 Credential={access_key}/{}/{}/{SERVICE}/request, \
         SignedHeaders={SIGNED_HEADERS}, Signature={signature}",
        scope.date, scope.region
    )
}

pub fn parse_response(value: &Value) -> Result<String> {
    let error = value
        .pointer("/ResponseMetadata/Error")
        .or_else(|| value.pointer("/Response/Error"));
    if let Some(error) = error {
        let code = error.get("Code").and_then(|code| {
            code.as_str()
                .map(str::to_string)
                .or_else(|| code.as_i64().map(|code| code.to_string()))
        });
        let message = error.get("Message").and_then(Value::as_str).unwrap_or("");
        return Err(PebbleError::Translate(format!(
            "Volcengine translate error {}: {message}",
            code.unwrap_or_default()
        )));
    }

    value
        .get("TranslationList")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("Translation"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            PebbleError::Translate(format!("Volcengine translate returned no text: {value}"))
        })
}

/// Everything the Volcengine endpoint needs besides the text itself.
pub struct VolcengineCredentials<'a> {
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub region: Option<&'a str>,
    pub endpoint: Option<&'a str>,
}

pub async fn translate(
    client: &reqwest::Client,
    credentials: &VolcengineCredentials<'_>,
    text: &str,
    from: &str,
    to: &str,
) -> Result<TranslateResult> {
    let host = credentials
        .endpoint
        .and_then(|url| reqwest::Url::parse(url).ok())
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| VOLC_HOST.to_string());
    let url = format!("https://{host}/?{QUERY}");
    let region = credentials
        .region
        .map(str::trim)
        .filter(|region| !region.is_empty())
        .unwrap_or(DEFAULT_REGION);
    let source = to_volcengine_lang(from);
    let target = to_volcengine_lang(to).unwrap_or_else(|| "zh".to_string());

    let mut translated = String::new();
    for chunk in split_chunks(text, MAX_CHUNK_CHARS) {
        if chunk.trim().is_empty() {
            translated.push_str(chunk);
            continue;
        }

        let payload = payload_for(chunk, source.as_deref(), &target);
        let payload_hash = sign::sha256_hex(&payload);
        let stamp = sign::utc_now();
        let scope = SigningScope {
            date: &stamp.date,
            region,
        };
        let authorization = authorization(
            credentials.access_key.trim(),
            credentials.secret_key.trim(),
            &scope,
            &stamp.amz_date,
            &payload_hash,
            &host,
        );

        let response = client
            .post(&url)
            .header(reqwest::header::AUTHORIZATION, authorization)
            .header(reqwest::header::CONTENT_TYPE, CONTENT_TYPE)
            .header("X-Date", stamp.amz_date.clone())
            .header("X-Content-Sha256", payload_hash.clone())
            .body(payload)
            .send()
            .await
            .map_err(|e| {
                PebbleError::Translate(format!("Volcengine translate request failed: {e}"))
            })?;

        let status = response.status();
        let value: Value = response.json().await.map_err(|e| {
            PebbleError::Translate(format!("Volcengine translate response parse failed: {e}"))
        })?;

        if !status.is_success() {
            return Err(PebbleError::Translate(format!(
                "Volcengine translate error {status}: {value}"
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

    const ACCESS_KEY: &str = "AKLTtest";
    const SECRET_KEY: &str = "c2VjcmV0";

    #[test]
    fn maps_volcengine_language_codes() {
        assert_eq!(to_volcengine_lang("zh").as_deref(), Some("zh"));
        assert_eq!(to_volcengine_lang("zh-TW").as_deref(), Some("zh-Hant"));
        assert_eq!(to_volcengine_lang("ja").as_deref(), Some("ja"));
        assert_eq!(to_volcengine_lang("auto"), None);
    }

    #[test]
    fn payload_omits_source_language_for_auto_detect() {
        let payload: Value = serde_json::from_str(&payload_for("hello", None, "zh")).unwrap();
        assert_eq!(payload["TargetLanguage"], "zh");
        assert_eq!(payload["TextList"][0], "hello");
        assert!(payload.get("SourceLanguage").is_none());

        let with_source: Value =
            serde_json::from_str(&payload_for("hello", Some("en"), "zh")).unwrap();
        assert_eq!(with_source["SourceLanguage"], "en");
    }

    #[test]
    fn canonical_request_lists_headers_in_signed_order() {
        let hash = sign::sha256_hex("{}");
        let canonical = canonical_request(&hash, VOLC_HOST, "2024-02-29T13:45:06Z");

        assert!(canonical.starts_with(
            "POST\n/\nAction=TranslateText&Version=2020-06-01\ncontent-type:application/json\nhost:open.volcengineapi.com\nx-content-sha256:"
        ));
        assert!(canonical.contains("\n\ncontent-type;host;x-content-sha256;x-date\n"));
        assert!(canonical.ends_with(&hash));
    }

    #[test]
    fn string_to_sign_carries_the_scope() {
        let hash = sign::sha256_hex("{}");
        let canonical = canonical_request(&hash, VOLC_HOST, "2024-02-29T13:45:06Z");
        let to_sign = string_to_sign(
            "2024-02-29T13:45:06Z",
            "2024-02-29",
            "cn-north-1",
            &canonical,
        );

        assert!(to_sign.starts_with(
            "HMAC-SHA256\n2024-02-29T13:45:06Z\n2024-02-29/cn-north-1/translate/request\n"
        ));
        assert!(to_sign.ends_with(&sign::sha256_hex(&canonical)));
    }

    #[test]
    fn authorization_header_has_the_v4_shape() {
        let hash = sign::sha256_hex("{}");
        let scope = SigningScope {
            date: "2024-02-29",
            region: "cn-north-1",
        };
        let header = authorization(
            ACCESS_KEY,
            SECRET_KEY,
            &scope,
            "2024-02-29T13:45:06Z",
            &hash,
            VOLC_HOST,
        );

        assert!(header.starts_with(
            "HMAC-SHA256 Credential=AKLTtest/2024-02-29/cn-north-1/translate/request, SignedHeaders=content-type;host;x-content-sha256;x-date, Signature="
        ));
        let signature = header.rsplit("Signature=").next().unwrap();
        assert_eq!(signature.len(), 64);
    }

    /// Ground-truth vector: the expected signature comes from an independent
    /// implementation of the V4 signing steps for the same request.
    #[test]
    fn signs_the_documented_v4_way() {
        let payload = r#"{"SourceLanguage":"en","TargetLanguage":"zh","TextList":["hello"]}"#;
        let payload_hash = sign::sha256_hex(payload);
        let scope = SigningScope {
            date: "2024-02-29",
            region: "cn-north-1",
        };
        let canonical = canonical_request(&payload_hash, VOLC_HOST, "2024-02-29T13:45:06Z");
        let to_sign = string_to_sign("2024-02-29T13:45:06Z", scope.date, scope.region, &canonical);

        assert_eq!(
            signature("test-secret", &scope, &to_sign),
            "700ad92d70473047d88719cc15e6f3a7d663569588b0d8e5ead8ca93bb8ead34"
        );
    }

    #[test]
    fn parses_translation_list() {
        let payload = json!({
            "ResponseMetadata": { "RequestId": "x", "Action": "TranslateText" },
            "TranslationList": [
                { "Translation": "你好", "DetectedSourceLanguage": "en" }
            ]
        });
        assert_eq!(parse_response(&payload).unwrap(), "你好");
    }

    #[test]
    fn surfaces_api_errors() {
        let payload = json!({
            "ResponseMetadata": {
                "RequestId": "x",
                "Error": { "Code": "InvalidAccessKey", "Message": "bad key" }
            }
        });
        let error = parse_response(&payload).unwrap_err().to_string();
        assert!(error.contains("InvalidAccessKey"));
        assert!(error.contains("bad key"));
    }

    #[test]
    fn rejects_empty_translation_list() {
        assert!(parse_response(&json!({ "TranslationList": [] }))
            .unwrap_err()
            .to_string()
            .contains("no text"));
    }
}
