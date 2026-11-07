//! Signing and encoding helpers shared by the Chinese cloud translation vendors.
//!
//! They all authenticate with a hand-rolled signature rather than an OAuth token,
//! but no two agree on how:
//!
//! | Vendor     | Digest                | Transport                       |
//! |------------|-----------------------|---------------------------------|
//! | Baidu      | `MD5(appid+q+salt+key)` | form body                     |
//! | Tencent    | TC3-HMAC-SHA256       | `Authorization` header          |
//! | Alibaba    | HMAC-SHA1 + base64    | `Signature` query parameter     |
//! | Volcengine | V4-HMAC-SHA256        | `Authorization` header          |
//!
//! Everything here is pure so it can be unit tested against fixed vectors.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256};

pub fn hex(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

pub fn md5_hex(data: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(data.as_bytes());
    hex(&hasher.finalize())
}

pub fn sha256_hex(data: &str) -> String {
    hex(&Sha256::digest(data.as_bytes()))
}

pub fn hmac_sha256(key: &[u8], data: &str) -> Vec<u8> {
    let mut mac =
        <Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC accepts keys of any length");
    mac.update(data.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

pub fn hmac_sha256_hex(key: &[u8], data: &str) -> String {
    hex(&hmac_sha256(key, data))
}

pub fn hmac_sha1_base64(key: &[u8], data: &str) -> String {
    let mut mac =
        <Hmac<Sha1> as Mac>::new_from_slice(key).expect("HMAC accepts keys of any length");
    mac.update(data.as_bytes());
    BASE64.encode(mac.finalize().into_bytes())
}

/// RFC 3986 percent-encoding: unreserved characters (`A-Z a-z 0-9 - _ . ~`) stay
/// verbatim, everything else — including `/` and space — is `%XX` encoded.
///
/// Both Alibaba Cloud and Volcengine build their canonical strings this way.
pub fn percent_encode(value: &str) -> String {
    const SET: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~');
    percent_encoding::utf8_percent_encode(value, SET).to_string()
}

/// The three clock readings the signed APIs ask for, all derived from one instant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UtcStamp {
    /// `YYYY-MM-DD` — signing-scope date.
    pub date: String,
    /// `YYYY-MM-DDTHH:MM:SSZ` — Alibaba Cloud `Timestamp`.
    pub iso8601: String,
    /// `YYYYMMDDTHHMMSSZ` — Volcengine `X-Date`.
    pub amz_date: String,
    /// Seconds since the Unix epoch.
    pub seconds: i64,
}

pub fn utc_from_unix(seconds: i64) -> UtcStamp {
    let (year, month, day) = crate::core::utc_date_from_unix_seconds(seconds);
    let second_of_day = seconds.rem_euclid(86_400);
    let hour = second_of_day / 3_600;
    let minute = (second_of_day % 3_600) / 60;
    let second = second_of_day % 60;
    let date = format!("{year:04}-{month:02}-{day:02}");
    UtcStamp {
        iso8601: format!("{date}T{hour:02}:{minute:02}:{second:02}Z"),
        amz_date: format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z"),
        date,
        seconds,
    }
}

pub fn utc_now() -> UtcStamp {
    utc_from_unix(crate::core::now_timestamp())
}

/// A nonce that only has to be unique per request (Alibaba Cloud and Baidu salts).
pub fn nonce() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}{sequence:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_matches_known_vector() {
        // RFC 1321 test suite.
        assert_eq!(
            md5_hex("abc"),
            "900150983cd24fb0d6963f7d28e17f72".to_string()
        );
    }

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256_hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn hmac_sha256_matches_rfc4231_vector() {
        // RFC 4231, test case 2.
        assert_eq!(
            hmac_sha256_hex(b"Jefe", "what do ya want for nothing?"),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn hmac_sha1_matches_rfc2202_vector() {
        // RFC 2202, test case 2: digest effcdf6ae5eb2fa2d27416d5f184df9c259a7c79.
        assert_eq!(
            hmac_sha1_base64(b"Jefe", "what do ya want for nothing?"),
            "7/zfauXrL6LSdBbV8YTfnCWafHk="
        );
    }

    #[test]
    fn percent_encoding_keeps_unreserved_and_escapes_the_rest() {
        assert_eq!(percent_encode("aZ0-_.~"), "aZ0-_.~");
        assert_eq!(percent_encode("/"), "%2F");
        assert_eq!(percent_encode("a b"), "a%20b");
        assert_eq!(percent_encode("你好"), "%E4%BD%A0%E5%A5%BD");
        assert_eq!(percent_encode("a+b=c&d"), "a%2Bb%3Dc%26d");
    }

    #[test]
    fn formats_utc_clock_readings() {
        // 2024-02-29T13:45:06Z
        let stamp = utc_from_unix(1_709_214_306);
        assert_eq!(stamp.date, "2024-02-29");
        assert_eq!(stamp.iso8601, "2024-02-29T13:45:06Z");
        assert_eq!(stamp.amz_date, "20240229T134506Z");
        assert_eq!(stamp.seconds, 1_709_214_306);
    }

    #[test]
    fn nonce_is_unique_enough() {
        assert_ne!(nonce(), nonce());
    }
}
