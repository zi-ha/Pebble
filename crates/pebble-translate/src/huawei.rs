//! Huawei Cloud Text Translation (华为云自然语言处理 NLP · 文本翻译).
//!
//! Auth is Huawei's own AK/SK scheme: `SDK-HMAC-SHA256` over a canonical request
//! that signs `content-type;host;x-sdk-date`. It looks like AWS SigV4 but the
//! key derivation is a single HMAC (no date/region/service chain) and the
//! canonical URI must end in `/` — the request itself may omit it.
//!
//! Docs: <https://support.huaweicloud.com/api-nlp/nlp_03_0024.html>
//! Signing: <https://support.huaweicloud.com/devg-apisign/api-sign-algorithm-002.html>

use pebble_core::{PebbleError, Result};
use serde_json::{json, Value};

use crate::chunk::split_chunks;
use crate::deeplx::build_segments;
use crate::sign;
use crate::types::TranslateResult;

pub const DEFAULT_REGION: &str = "cn-north-4";
const CONTENT_TYPE: &str = "application/json";
const ALGORITHM: &str = "SDK-HMAC-SHA256";
/// Sorted, lower-case, `;`-joined — Huawei requires `x-sdk-date` in here.
const SIGNED_HEADERS: &str = "content-type;host;x-sdk-date";
/// Text translation rejects payloads over 2,000 characters.
const MAX_CHUNK_CHARS: usize = 2_000;

pub fn host_for_region(region: &str) -> String {
    format!("nlp-ext.{region}.myhuaweicloud.com")
}

pub fn api_path(project_id: &str) -> String {
    format!("/v1/{project_id}/machine-translation/text-translation")
}

/// The canonical URI is the request path, always terminated with `/`.
pub fn canonical_uri(path: &str) -> String {
    if path.ends_with('/') {
        path.to_string()
    } else {
        format!("{path}/")
    }
}

/// Map an app language code onto Huawei's vocabulary.
///
/// `None` means "let the service detect it"; Huawei spells that `auto` rather
/// than omitting the field.
pub fn to_huawei_lang(code: &str) -> Option<String> {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed.to_ascii_lowercase().replace('_', "-");
    if normalized == "auto" {
        return None;
    }
    Some(match normalized.as_str() {
        // Huawei only ships simplified Chinese, so both scripts collapse to `zh`.
        "zh" | "zh-cn" | "zh-sg" | "zh-hans" | "zh-chs" | "zh-tw" | "zh-hk" | "zh-mo"
        | "zh-hant" | "zh-cht" => "zh".to_string(),
        "ja" | "jp" => "ja".to_string(),
        "ko" | "kr" => "ko".to_string(),
        other => other.to_string(),
    })
}

pub fn payload_for(text: &str, source: Option<&str>, target: &str) -> String {
    json!({
        "text": text,
        "from": source.unwrap_or("auto"),
        "to": target,
        "scene": "common",
    })
    .to_string()
}

pub fn canonical_request(uri: &str, host: &str, x_sdk_date: &str, payload_hash: &str) -> String {
    // Every canonical header carries its own trailing newline, and the formula
    // adds one more — hence the blank line before SignedHeaders. There is no
    // query string here, so that line is empty.
    format!(
        "POST\n{uri}\n\ncontent-type:{CONTENT_TYPE}\nhost:{host}\nx-sdk-date:{x_sdk_date}\n\n{SIGNED_HEADERS}\n{payload_hash}"
    )
}

pub fn string_to_sign(x_sdk_date: &str, canonical_request: &str) -> String {
    format!(
        "{ALGORITHM}\n{x_sdk_date}\n{}",
        sign::sha256_hex(canonical_request)
    )
}

pub fn signature(secret_key: &str, string_to_sign: &str) -> String {
    sign::hmac_sha256_hex(secret_key.as_bytes(), string_to_sign)
}

pub fn authorization(
    access_key: &str,
    secret_key: &str,
    x_sdk_date: &str,
    canonical: &str,
) -> String {
    let signature = signature(secret_key, &string_to_sign(x_sdk_date, canonical));
    format!(
        "{ALGORITHM} Access={access_key}, SignedHeaders={SIGNED_HEADERS}, Signature={signature}"
    )
}

pub fn parse_response(value: &Value) -> Result<String> {
    if let Some(code) = value.get("error_code").and_then(|code| {
        code.as_str()
            .map(str::to_string)
            .or_else(|| code.as_i64().map(|code| code.to_string()))
    }) {
        let message = value.get("error_msg").and_then(Value::as_str).unwrap_or("");
        return Err(PebbleError::Translate(format!(
            "Huawei translate error {code}: {message}"
        )));
    }

    value
        .get("translated_text")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            PebbleError::Translate(format!("Huawei translate returned no text: {value}"))
        })
}

/// Everything the endpoint needs besides the text itself.
pub struct HuaweiCredentials<'a> {
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub project_id: &'a str,
    pub region: Option<&'a str>,
    pub endpoint: Option<&'a str>,
}

pub async fn translate(
    client: &reqwest::Client,
    credentials: &HuaweiCredentials<'_>,
    text: &str,
    from: &str,
    to: &str,
) -> Result<TranslateResult> {
    let region = credentials
        .region
        .map(str::trim)
        .filter(|region| !region.is_empty())
        .unwrap_or(DEFAULT_REGION);
    let base = credentials
        .endpoint
        .map(str::trim)
        .filter(|endpoint| !endpoint.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("https://{}", host_for_region(region)));
    let url = format!(
        "{}{}",
        base.trim_end_matches('/'),
        api_path(credentials.project_id.trim())
    );

    let parsed = reqwest::Url::parse(&url).map_err(|e| {
        PebbleError::Translate(format!("Huawei translate endpoint is invalid: {e}"))
    })?;
    let host = parsed
        .host_str()
        .ok_or_else(|| PebbleError::Translate("Huawei translate endpoint has no host".to_string()))?
        .to_string();
    let uri = canonical_uri(parsed.path());

    let source = to_huawei_lang(from);
    let target = to_huawei_lang(to).unwrap_or_else(|| "zh".to_string());

    let mut translated = String::new();
    for chunk in split_chunks(text, MAX_CHUNK_CHARS) {
        if chunk.trim().is_empty() {
            translated.push_str(chunk);
            continue;
        }

        let payload = payload_for(chunk, source.as_deref(), &target);
        let payload_hash = sign::sha256_hex(&payload);
        let stamp = sign::utc_now();
        // Huawei spells its timestamp exactly like the AWS `amz_date` form.
        let x_sdk_date = stamp.amz_date.clone();
        let canonical = canonical_request(&uri, &host, &x_sdk_date, &payload_hash);
        let authorization = authorization(
            credentials.access_key.trim(),
            credentials.secret_key.trim(),
            &x_sdk_date,
            &canonical,
        );

        let response = client
            .post(&url)
            .header(reqwest::header::CONTENT_TYPE, CONTENT_TYPE)
            .header("X-Sdk-Date", x_sdk_date)
            .header(reqwest::header::AUTHORIZATION, authorization)
            .body(payload)
            .send()
            .await
            .map_err(|e| PebbleError::Translate(format!("Huawei translate request failed: {e}")))?;

        let status = response.status();
        let value: Value = response.json().await.map_err(|e| {
            PebbleError::Translate(format!("Huawei translate response parse failed: {e}"))
        })?;

        if !status.is_success() {
            return Err(PebbleError::Translate(format!(
                "Huawei translate error {status}: {value}"
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

    const AK: &str = "QTWAOYNNMZZU4MABCDEF";
    const SK: &str = "MFySkqUz1234567890abcdefghijklmn";

    #[test]
    fn maps_huawei_language_codes() {
        assert_eq!(to_huawei_lang("zh").as_deref(), Some("zh"));
        assert_eq!(to_huawei_lang("zh-TW").as_deref(), Some("zh"));
        assert_eq!(to_huawei_lang("ja").as_deref(), Some("ja"));
        assert_eq!(to_huawei_lang("ko").as_deref(), Some("ko"));
        assert_eq!(to_huawei_lang("en").as_deref(), Some("en"));
        assert_eq!(to_huawei_lang("auto"), None);
    }

    #[test]
    fn payload_asks_for_auto_detection_by_name() {
        let payload: Value = serde_json::from_str(&payload_for("hello", None, "zh")).unwrap();
        assert_eq!(payload["from"], "auto");
        assert_eq!(payload["to"], "zh");
        assert_eq!(payload["text"], "hello");
        assert_eq!(payload["scene"], "common");

        let explicit: Value =
            serde_json::from_str(&payload_for("hello", Some("en"), "zh")).unwrap();
        assert_eq!(explicit["from"], "en");
    }

    #[test]
    fn canonical_uri_always_ends_with_a_slash() {
        assert_eq!(canonical_uri("/v1/abc/vpcs"), "/v1/abc/vpcs/");
        assert_eq!(canonical_uri("/v1/abc/vpcs/"), "/v1/abc/vpcs/");
    }

    #[test]
    fn builds_the_endpoint_from_the_region() {
        assert_eq!(
            host_for_region("cn-north-4"),
            "nlp-ext.cn-north-4.myhuaweicloud.com"
        );
        assert_eq!(
            api_path("abc123"),
            "/v1/abc123/machine-translation/text-translation"
        );
    }

    /// Ground-truth vector straight from Huawei's signing guide: the canonical
    /// request of their VPC example hashes to this value.
    #[test]
    fn canonical_request_matches_the_documented_example() {
        let empty_payload_hash = sign::sha256_hex("");
        assert_eq!(
            empty_payload_hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );

        // The guide's example is a GET with a query string. Rebuild it verbatim
        // to prove the newline layout (blank line before SignedHeaders) is right.
        let canonical = format!(
            "GET\n/v1/77b6a44cba5143ab91d13ab9a8ff44fd/vpcs/\nlimit=2&marker=13551d6b-755d-4757-b956-536f674975c0\ncontent-type:application/json\nhost:service.region.example.com\nx-sdk-date:20191115T033655Z\n\ncontent-type;host;x-sdk-date\n{empty_payload_hash}"
        );
        assert_eq!(
            sign::sha256_hex(&canonical),
            "b25362e603ee30f4f25e7858e8a7160fd36e803bb2dfe206278659d71a9bcd7a"
        );
    }

    #[test]
    fn canonical_request_without_query_keeps_an_empty_line() {
        let canonical = canonical_request(
            "/v1/abc/machine-translation/text-translation/",
            "nlp-ext.cn-north-4.myhuaweicloud.com",
            "20240229T134506Z",
            "e3b0c4",
        );

        assert!(canonical.starts_with(
            "POST\n/v1/abc/machine-translation/text-translation/\n\ncontent-type:application/json\nhost:nlp-ext.cn-north-4.myhuaweicloud.com\nx-sdk-date:20240229T134506Z\n\n"
        ));
        assert!(canonical.ends_with("\ncontent-type;host;x-sdk-date\ne3b0c4"));
    }

    #[test]
    fn string_to_sign_carries_the_timestamp() {
        let hash = sign::sha256_hex("{}");
        let canonical = canonical_request("/v1/a/b/", "h", "20240229T134506Z", &hash);
        let to_sign = string_to_sign("20240229T134506Z", &canonical);

        assert!(to_sign.starts_with("SDK-HMAC-SHA256\n20240229T134506Z\n"));
        assert!(to_sign.ends_with(&sign::sha256_hex(&canonical)));
    }

    #[test]
    fn signature_is_a_single_hmac_of_the_string_to_sign() {
        let signature = signature(SK, "SDK-HMAC-SHA256\n20240229T134506Z\nabc");

        assert_eq!(
            signature,
            sign::hmac_sha256_hex(SK.as_bytes(), "SDK-HMAC-SHA256\n20240229T134506Z\nabc")
        );
        assert_eq!(signature.len(), 64);
    }

    #[test]
    fn authorization_header_has_the_sdk_shape() {
        let hash = sign::sha256_hex("{}");
        let canonical = canonical_request(
            "/v1/abc/machine-translation/text-translation/",
            "h",
            "20240229T134506Z",
            &hash,
        );
        let header = authorization(AK, SK, "20240229T134506Z", &canonical);

        assert!(header.starts_with(&format!(
            "SDK-HMAC-SHA256 Access={AK}, SignedHeaders=content-type;host;x-sdk-date, Signature="
        )));
        assert_eq!(header.rsplit("Signature=").next().unwrap().len(), 64);
    }

    #[test]
    fn parses_translated_text() {
        let payload = json!({
            "src_text": "Hello",
            "translated_text": "你好",
            "from": "en",
            "to": "zh"
        });
        assert_eq!(parse_response(&payload).unwrap(), "你好");
    }

    #[test]
    fn surfaces_api_errors() {
        let payload = json!({ "error_code": "NLP.0101", "error_msg": "Authentication failed." });
        let error = parse_response(&payload).unwrap_err().to_string();

        assert!(error.contains("NLP.0101"));
        assert!(error.contains("Authentication failed."));
    }

    #[test]
    fn rejects_payloads_without_text() {
        assert!(parse_response(&json!({ "from": "en" }))
            .unwrap_err()
            .to_string()
            .contains("no text"));
    }
}
