//! Tencent Cloud Machine Translation (TMT, `TextTranslate`).
//!
//! Every call is signed with TC3-HMAC-SHA256: the request is canonicalised into a
//! hash, that hash is signed with a key chain derived from the secret key for the
//! request's date, and the result travels in the `Authorization` header.
//!
//! Docs: <https://cloud.tencent.com/document/api/551/15619>

use crate::core::{PebbleError, Result};
use serde_json::{json, Value};

use crate::translate::chunk::split_chunks;
use crate::translate::deeplx::build_segments;
use crate::translate::sign;
use crate::translate::types::TranslateResult;

pub const TENCENT_HOST: &str = "tmt.tencentcloudapi.com";
const SERVICE: &str = "tmt";
const ACTION: &str = "TextTranslate";
const VERSION: &str = "2018-03-21";
const CONTENT_TYPE: &str = "application/json; charset=utf-8";
pub const DEFAULT_REGION: &str = "ap-guangzhou";
/// TMT rejects a single `SourceText` longer than 2,000 characters.
const MAX_CHUNK_CHARS: usize = 1_900;

/// Map an app language code onto TMT's vocabulary (`jp` / `kr` / `zh-TW`).
pub fn to_tencent_lang(code: &str) -> Option<String> {
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
        "zh-tw" | "zh-hk" | "zh-mo" | "zh-hant" | "zh-cht" => "zh-TW".to_string(),
        "ja" | "jp" => "jp".to_string(),
        "ko" | "kr" => "kr".to_string(),
        other => other.to_string(),
    })
}

fn payload_for(text: &str, source: &str, target: &str, project_id: i64) -> String {
    json!({
        "SourceText": text,
        "Source": source,
        "Target": target,
        "ProjectId": project_id,
    })
    .to_string()
}

/// TC3 signer for one credential pair.
///
/// The service name and content type are part of the signed material, so they are
/// fields rather than constants: `tmt` in production, anything else when testing
/// the algorithm against the documented sample of another product.
pub struct Tc3Signer<'a> {
    pub secret_id: &'a str,
    pub secret_key: &'a str,
    pub service: &'a str,
    pub content_type: &'a str,
}

impl Tc3Signer<'_> {
    /// `POST\n/\n\n<canonical headers>\n<signed headers>\n<sha256(payload)>`
    pub fn canonical_request(&self, payload: &str, host: &str) -> String {
        format!(
            "POST\n/\n\ncontent-type:{}\nhost:{host}\n\ncontent-type;host\n{}",
            self.content_type,
            sign::sha256_hex(payload)
        )
    }

    pub fn string_to_sign(&self, timestamp: i64, date: &str, canonical_request: &str) -> String {
        format!(
            "TC3-HMAC-SHA256\n{timestamp}\n{date}/{}/tc3_request\n{}",
            self.service,
            sign::sha256_hex(canonical_request)
        )
    }

    pub fn signature(&self, date: &str, string_to_sign: &str) -> String {
        let secret_date = sign::hmac_sha256(format!("TC3{}", self.secret_key).as_bytes(), date);
        let secret_service = sign::hmac_sha256(&secret_date, self.service);
        let secret_signing = sign::hmac_sha256(&secret_service, "tc3_request");
        sign::hmac_sha256_hex(&secret_signing, string_to_sign)
    }

    pub fn authorization(&self, timestamp: i64, date: &str, payload: &str, host: &str) -> String {
        let canonical = self.canonical_request(payload, host);
        let to_sign = self.string_to_sign(timestamp, date, &canonical);
        let signature = self.signature(date, &to_sign);
        format!(
            "TC3-HMAC-SHA256 Credential={}/{date}/{}/tc3_request, \
             SignedHeaders=content-type;host, Signature={signature}",
            self.secret_id, self.service
        )
    }
}

pub fn parse_response(value: &Value) -> Result<String> {
    let response = value.get("Response").unwrap_or(value);

    if let Some(error) = response.get("Error") {
        let code = error.get("Code").and_then(Value::as_str).unwrap_or("");
        let message = error.get("Message").and_then(Value::as_str).unwrap_or("");
        return Err(PebbleError::Translate(format!(
            "Tencent translate error {code}: {message}"
        )));
    }

    response
        .get("TargetText")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            PebbleError::Translate(format!("Tencent translate returned no text: {value}"))
        })
}

/// Everything the TMT endpoint needs besides the text itself.
pub struct TencentCredentials<'a> {
    pub secret_id: &'a str,
    pub secret_key: &'a str,
    pub region: Option<&'a str>,
    pub project_id: Option<i64>,
    pub endpoint: Option<&'a str>,
}

pub async fn translate(
    client: &reqwest::Client,
    credentials: &TencentCredentials<'_>,
    text: &str,
    from: &str,
    to: &str,
) -> Result<TranslateResult> {
    let host = credentials
        .endpoint
        .and_then(|url| reqwest::Url::parse(url).ok())
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| TENCENT_HOST.to_string());
    let url = format!("https://{host}/");
    let region = credentials
        .region
        .map(str::trim)
        .filter(|region| !region.is_empty())
        .unwrap_or(DEFAULT_REGION);
    let project_id = credentials.project_id.unwrap_or(0);
    let source = to_tencent_lang(from).unwrap_or_else(|| "auto".to_string());
    let target = to_tencent_lang(to).unwrap_or_else(|| "zh".to_string());
    let signer = Tc3Signer {
        secret_id: credentials.secret_id.trim(),
        secret_key: credentials.secret_key.trim(),
        service: SERVICE,
        content_type: CONTENT_TYPE,
    };

    let mut translated = String::new();
    for chunk in split_chunks(text, MAX_CHUNK_CHARS) {
        if chunk.trim().is_empty() {
            translated.push_str(chunk);
            continue;
        }

        let payload = payload_for(chunk, &source, &target, project_id);
        let stamp = sign::utc_now();
        let authorization = signer.authorization(stamp.seconds, &stamp.date, &payload, &host);

        let response = client
            .post(&url)
            .header(reqwest::header::AUTHORIZATION, authorization)
            .header(reqwest::header::CONTENT_TYPE, CONTENT_TYPE)
            .header("X-TC-Action", ACTION)
            .header("X-TC-Version", VERSION)
            .header("X-TC-Timestamp", stamp.seconds.to_string())
            .header("X-TC-Region", region)
            .body(payload)
            .send()
            .await
            .map_err(|e| {
                PebbleError::Translate(format!("Tencent translate request failed: {e}"))
            })?;

        let status = response.status();
        let value: Value = response.json().await.map_err(|e| {
            PebbleError::Translate(format!("Tencent translate response parse failed: {e}"))
        })?;

        if !status.is_success() {
            return Err(PebbleError::Translate(format!(
                "Tencent translate error {status}: {value}"
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

    // The SecretId never takes part in the signature, so the tests use an
    // obviously fake one instead of the values printed in Tencent's docs:
    // GitHub's secret scanning matches `AKID` + 32 characters and blocks the
    // push otherwise.
    const TEST_SECRET_ID: &str = "AKID-not-a-real-secret-id-0000";

    fn tmt_signer() -> Tc3Signer<'static> {
        Tc3Signer {
            secret_id: TEST_SECRET_ID,
            secret_key: "Gu5t9xGARNpq86cd98joQYCN3Cozk1qA",
            service: SERVICE,
            content_type: CONTENT_TYPE,
        }
    }

    #[test]
    fn maps_tencent_language_codes() {
        assert_eq!(to_tencent_lang("zh").as_deref(), Some("zh"));
        assert_eq!(to_tencent_lang("zh-TW").as_deref(), Some("zh-TW"));
        assert_eq!(to_tencent_lang("ja").as_deref(), Some("jp"));
        assert_eq!(to_tencent_lang("ko").as_deref(), Some("kr"));
        assert_eq!(to_tencent_lang("auto").as_deref(), Some("auto"));
        assert_eq!(to_tencent_lang(""), None);
    }

    #[test]
    fn canonical_request_signs_content_type_and_host_only() {
        let payload = payload_for("hello", "en", "zh", 0);
        let canonical = tmt_signer().canonical_request(&payload, TENCENT_HOST);

        assert!(canonical.starts_with("POST\n/\n\ncontent-type:application/json; charset=utf-8\nhost:tmt.tencentcloudapi.com\n\ncontent-type;host\n"));
        assert!(canonical.ends_with(&sign::sha256_hex(&payload)));
    }

    #[test]
    fn string_to_sign_uses_the_request_date_and_service() {
        let payload = payload_for("hello", "en", "zh", 0);
        let signer = tmt_signer();
        let canonical = signer.canonical_request(&payload, TENCENT_HOST);
        let to_sign = signer.string_to_sign(1_709_214_306, "2024-02-29", &canonical);

        assert!(to_sign.starts_with("TC3-HMAC-SHA256\n1709214306\n2024-02-29/tmt/tc3_request\n"));
        assert!(to_sign.ends_with(&sign::sha256_hex(&canonical)));
    }

    #[test]
    fn authorization_header_has_the_tc3_shape() {
        let payload = payload_for("hello", "en", "zh", 0);
        let header =
            tmt_signer().authorization(1_709_214_306, "2024-02-29", &payload, TENCENT_HOST);

        assert!(header.starts_with(
            "TC3-HMAC-SHA256 Credential=AKID-not-a-real-secret-id-0000/2024-02-29/tmt/tc3_request, SignedHeaders=content-type;host, Signature="
        ));
        // The signature is a 64-character hex digest.
        let signature = header.rsplit("Signature=").next().unwrap();
        assert_eq!(signature.len(), 64);
        assert!(signature.chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// The TC3 algorithm is shared by every Tencent Cloud product, so the sample
    /// from the official signature documentation (`cvm`, 2019-02-25) is a
    /// ground-truth vector for this code. The expected value was produced with an
    /// independent implementation of the same documented steps.
    ///
    /// Only the documented SecretKey is kept: it is what the signature is derived
    /// from. The documented SecretId is replaced with a placeholder, because it
    /// matches GitHub's `AKID…` secret pattern and blocks every push.
    #[test]
    fn matches_the_documented_tc3_sample() {
        let signer = Tc3Signer {
            secret_id: "AKID-not-a-real-secret-id-1111",
            secret_key: "Gu5t9xGARNpq86cd98joQYCN3EXAMPLE",
            service: "cvm",
            content_type: "application/json; charset=utf-8",
        };
        let payload =
            r#"{"Limit": 1, "Filters": [{"Values": ["未命名"], "Name": "instance-name"}]}"#;

        assert_eq!(
            signer.signature(
                "2019-02-25",
                &signer.string_to_sign(
                    1_551_113_065,
                    "2019-02-25",
                    &signer.canonical_request(payload, "cvm.tencentcloudapi.com")
                )
            ),
            "57ed31a395c63c472410096cc67e56aa39aa2b06b960d4f31beea21236106ca9"
        );
    }

    #[test]
    fn parses_translated_text() {
        let payload = json!({
            "Response": {
                "TargetText": "你好",
                "Source": "en",
                "Target": "zh",
                "RequestId": "0000"
            }
        });
        assert_eq!(parse_response(&payload).unwrap(), "你好");
    }

    #[test]
    fn surfaces_api_errors() {
        let payload = json!({
            "Response": {
                "Error": { "Code": "AuthFailure.SignatureFailure", "Message": "bad signature" },
                "RequestId": "0000"
            }
        });
        let error = parse_response(&payload).unwrap_err().to_string();
        assert!(error.contains("AuthFailure.SignatureFailure"));
        assert!(error.contains("bad signature"));
    }

    #[test]
    fn rejects_payload_without_text() {
        assert!(parse_response(&json!({ "Response": {} }))
            .unwrap_err()
            .to_string()
            .contains("no text"));
    }
}
