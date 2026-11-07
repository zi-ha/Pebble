use super::network::get_global_proxy_raw;
use crate::commands::encrypted_store::{ACTIVE_TRANSLATE_CONFIG_ID, TRANSLATE_CONFIG_PURPOSE};
use crate::state::AppState;
use pebble_core::{now_timestamp, PebbleError, TranslateConfig};
use pebble_crypto::CryptoService;
use pebble_store::Store;
use pebble_translate::types::{TranslateProviderConfig, TranslateResult};
use pebble_translate::TranslateService;
use tauri::State;

/// Decode a hex string to bytes.
fn hex_decode(s: &str) -> std::result::Result<Vec<u8>, PebbleError> {
    let bytes = s.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return Err(PebbleError::Internal(
            "Invalid hex string length".to_string(),
        ));
    }
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let high = hex_nibble(pair[0])
                .ok_or_else(|| PebbleError::Internal("Invalid hex digit".to_string()))?;
            let low = hex_nibble(pair[1])
                .ok_or_else(|| PebbleError::Internal("Invalid hex digit".to_string()))?;
            Ok((high << 4) | low)
        })
        .collect()
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Encode bytes to a hex string.
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Decrypt the config field of a TranslateConfig using the app's crypto service.
/// If the stored value is legacy plaintext JSON, migrates it to encrypted form in-place.
pub(crate) fn decrypt_config(
    state: &AppState,
    stored: &str,
) -> std::result::Result<String, PebbleError> {
    decrypt_config_with_store(&state.crypto, &state.store, stored)
}

fn decrypt_config_with_store(
    crypto: &CryptoService,
    store: &Store,
    stored: &str,
) -> std::result::Result<String, PebbleError> {
    if serde_json::from_str::<serde_json::Value>(stored).is_ok() {
        // Legacy plaintext config — migrate to encrypted form in-place.
        let encrypted = encrypt_config_with_crypto(crypto, stored)?;
        store.compare_exchange_translate_config_blob(stored, &encrypted)?;
        return Ok(stored.to_string());
    }
    let bytes = hex_decode(stored)?;
    let needs_migration = pebble_crypto::CryptoService::ciphertext_needs_migration(&bytes);
    let decrypted =
        crypto.decrypt_for(TRANSLATE_CONFIG_PURPOSE, ACTIVE_TRANSLATE_CONFIG_ID, &bytes)?;
    let plaintext = String::from_utf8(decrypted)
        .map_err(|e| PebbleError::Internal(format!("Invalid UTF-8 in decrypted config: {e}")))?;
    if needs_migration {
        let encrypted = encrypt_config_with_crypto(crypto, &plaintext)?;
        store.compare_exchange_translate_config_blob(stored, &encrypted)?;
    }
    Ok(plaintext)
}

/// Encrypt a plaintext config string for storage.
pub(crate) fn encrypt_config(
    state: &AppState,
    plaintext: &str,
) -> std::result::Result<String, PebbleError> {
    encrypt_config_with_crypto(&state.crypto, plaintext)
}

fn encrypt_config_with_crypto(
    crypto: &CryptoService,
    plaintext: &str,
) -> std::result::Result<String, PebbleError> {
    let encrypted = crypto.encrypt_for(
        TRANSLATE_CONFIG_PURPOSE,
        ACTIVE_TRANSLATE_CONFIG_ID,
        plaintext.as_bytes(),
    )?;
    Ok(hex_encode(&encrypted))
}

#[tauri::command]
pub async fn translate_text(
    state: State<'_, AppState>,
    text: String,
    from_lang: String,
    to_lang: String,
) -> std::result::Result<TranslateResult, PebbleError> {
    // No saved engine yet — fall back to the built-in Microsoft endpoint so
    // translation works without any setup.
    let provider_config = match state.store.get_translate_config()? {
        Some(config) => {
            if !config.is_enabled {
                return Err(PebbleError::Translate(
                    "Translation is disabled".to_string(),
                ));
            }
            let decrypted = decrypt_config(&state, &config.config)?;
            serde_json::from_str(&decrypted)
                .map_err(|e| PebbleError::Translate(format!("Invalid config: {e}")))?
        }
        None => TranslateService::builtin_config(),
    };

    validate_provider_config(&provider_config)?;

    let proxy = get_global_proxy_raw(&state.crypto, &state.store)?;

    TranslateService::translate_with_proxy(
        &provider_config,
        proxy.as_ref(),
        &text,
        &from_lang,
        &to_lang,
    )
    .await
}

#[tauri::command]
pub async fn get_translate_config(
    state: State<'_, AppState>,
) -> std::result::Result<Option<TranslateConfig>, PebbleError> {
    let config = state.store.get_translate_config()?;
    // Return config with decrypted config field so frontend can display/edit it
    match config {
        Some(mut tc) => {
            tc.config = decrypt_config(&state, &tc.config)?;
            Ok(Some(tc))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn save_translate_config(
    state: State<'_, AppState>,
    provider_type: String,
    config: String,
    is_enabled: bool,
) -> std::result::Result<(), PebbleError> {
    // Validate URL(s) in config before persisting
    let provider_config: TranslateProviderConfig = serde_json::from_str(&config)
        .map_err(|e| PebbleError::Translate(format!("Invalid config: {e}")))?;
    validate_provider_config(&provider_config)?;

    let now = now_timestamp();
    // Encrypt config before storing
    let encrypted = encrypt_config(&state, &config)?;
    let tc = TranslateConfig {
        id: "active".to_string(),
        provider_type,
        config: encrypted,
        is_enabled,
        created_at: now,
        updated_at: now,
    };
    state.store.save_translate_config(&tc)
}

/// Validate URL(s) in a TranslateProviderConfig.
fn validate_provider_config(
    provider_config: &TranslateProviderConfig,
) -> std::result::Result<(), PebbleError> {
    match provider_config {
        TranslateProviderConfig::DeepLX { endpoint } => validate_translate_url(endpoint),
        TranslateProviderConfig::GenericApi { endpoint, .. } => validate_translate_url(endpoint),
        TranslateProviderConfig::LLM { endpoint, .. } => validate_translate_url(endpoint),
        TranslateProviderConfig::DeepL { .. } => Ok(()), // uses official API, no custom URL
        // Built-in endpoints are fixed; only a user-supplied override needs
        // validating.
        TranslateProviderConfig::Microsoft { endpoint, .. } => validate_optional_endpoint(endpoint),
        TranslateProviderConfig::Baidu {
            app_id,
            secret_key,
            endpoint,
        } => {
            require_credential("Baidu AppID", app_id)?;
            require_credential("Baidu secret key", secret_key)?;
            validate_optional_endpoint(endpoint)
        }
        TranslateProviderConfig::Tencent {
            secret_id,
            secret_key,
            endpoint,
            ..
        } => {
            require_credential("Tencent SecretId", secret_id)?;
            require_credential("Tencent SecretKey", secret_key)?;
            validate_optional_endpoint(endpoint)
        }
        TranslateProviderConfig::Aliyun {
            access_key_id,
            access_key_secret,
            endpoint,
            ..
        } => {
            require_credential("Alibaba Cloud AccessKey ID", access_key_id)?;
            require_credential("Alibaba Cloud AccessKey Secret", access_key_secret)?;
            validate_optional_endpoint(endpoint)
        }
        TranslateProviderConfig::Volcengine {
            access_key,
            secret_key,
            endpoint,
            ..
        } => {
            require_credential("Volcengine Access Key", access_key)?;
            require_credential("Volcengine Secret Key", secret_key)?;
            validate_optional_endpoint(endpoint)
        }
        TranslateProviderConfig::Huawei {
            access_key,
            secret_key,
            project_id,
            endpoint,
            ..
        } => {
            require_credential("Huawei Access Key", access_key)?;
            require_credential("Huawei Secret Key", secret_key)?;
            require_credential("Huawei Project ID", project_id)?;
            validate_optional_endpoint(endpoint)
        }
        TranslateProviderConfig::Caiyun { token, endpoint } => {
            require_credential("Caiyun token", token)?;
            validate_optional_endpoint(endpoint)
        }
    }
}

/// Cloud vendors need real credentials — catch the empty case before spending a
/// round trip on a request that can only fail.
fn require_credential(label: &str, value: &str) -> std::result::Result<(), PebbleError> {
    if value.trim().is_empty() {
        return Err(PebbleError::Validation(format!("{label} is required")));
    }
    Ok(())
}

fn validate_optional_endpoint(endpoint: &Option<String>) -> std::result::Result<(), PebbleError> {
    match endpoint
        .as_deref()
        .map(str::trim)
        .filter(|endpoint| !endpoint.is_empty())
    {
        Some(endpoint) => validate_translate_url(endpoint),
        None => Ok(()),
    }
}

/// Validate that a translate endpoint URL is safe (HTTPS required, HTTP only for localhost).
fn validate_translate_url(url: &str) -> std::result::Result<(), PebbleError> {
    if url.starts_with("https://") {
        return Ok(());
    }
    if let Some(after_scheme) = url.strip_prefix("http://") {
        // Extract host from http://host[:port]/...
        let host = after_scheme
            .split('/')
            .next()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("");
        if host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "[::1]" {
            return Ok(());
        }
        return Err(PebbleError::Validation(
            "Only HTTPS URLs are allowed for remote services".into(),
        ));
    }
    Err(PebbleError::Validation("Unsupported URL scheme".into()))
}

#[tauri::command]
pub async fn test_translate_connection(
    state: State<'_, AppState>,
    config: String,
) -> std::result::Result<String, PebbleError> {
    let provider_config: TranslateProviderConfig = serde_json::from_str(&config)
        .map_err(|e| PebbleError::Translate(format!("Invalid config: {e}")))?;

    // Validate endpoint URLs before making any requests
    validate_provider_config(&provider_config)?;

    let proxy = get_global_proxy_raw(&state.crypto, &state.store)?;
    let result = TranslateService::translate_with_proxy(
        &provider_config,
        proxy.as_ref(),
        "Hello",
        "en",
        "zh",
    )
    .await?;
    Ok(result.translated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pebble_store::Store;

    fn save_config(store: &Store, config: &str) {
        let now = now_timestamp();
        store
            .save_translate_config(&TranslateConfig {
                id: ACTIVE_TRANSLATE_CONFIG_ID.to_string(),
                provider_type: "deepl".to_string(),
                config: config.to_string(),
                is_enabled: true,
                created_at: now,
                updated_at: now,
            })
            .unwrap();
    }

    #[test]
    fn cloud_engines_require_a_key_pair() {
        let missing_app_id = serde_json::from_str::<TranslateProviderConfig>(
            r#"{"type":"baidu","app_id":" ","secret_key":"secret"}"#,
        )
        .unwrap();
        assert!(validate_provider_config(&missing_app_id)
            .unwrap_err()
            .to_string()
            .contains("Baidu AppID"));

        let missing_secret = serde_json::from_str::<TranslateProviderConfig>(
            r#"{"type":"tencent","secret_id":"AKID","secret_key":""}"#,
        )
        .unwrap();
        assert!(validate_provider_config(&missing_secret)
            .unwrap_err()
            .to_string()
            .contains("Tencent SecretKey"));

        let complete = serde_json::from_str::<TranslateProviderConfig>(
            r#"{"type":"volcengine","access_key":"AK","secret_key":"SK"}"#,
        )
        .unwrap();
        assert!(validate_provider_config(&complete).is_ok());
    }

    #[test]
    fn huawei_needs_a_project_id_on_top_of_the_key_pair() {
        let missing_project = serde_json::from_str::<TranslateProviderConfig>(
            r#"{"type":"huawei","access_key":"AK","secret_key":"SK","project_id":" "}"#,
        )
        .unwrap();
        assert!(validate_provider_config(&missing_project)
            .unwrap_err()
            .to_string()
            .contains("Huawei Project ID"));

        let complete = serde_json::from_str::<TranslateProviderConfig>(
            r#"{"type":"huawei","access_key":"AK","secret_key":"SK","project_id":"abc123"}"#,
        )
        .unwrap();
        assert!(validate_provider_config(&complete).is_ok());
    }

    #[test]
    fn caiyun_needs_a_token() {
        let missing =
            serde_json::from_str::<TranslateProviderConfig>(r#"{"type":"caiyun","token":""}"#)
                .unwrap();
        assert!(validate_provider_config(&missing)
            .unwrap_err()
            .to_string()
            .contains("Caiyun token"));

        let complete =
            serde_json::from_str::<TranslateProviderConfig>(r#"{"type":"caiyun","token":"tok"}"#)
                .unwrap();
        assert!(validate_provider_config(&complete).is_ok());
    }

    #[test]
    fn cloud_engines_still_validate_a_custom_endpoint() {
        let insecure = serde_json::from_str::<TranslateProviderConfig>(
            r#"{"type":"aliyun","access_key_id":"id","access_key_secret":"secret",
                "endpoint":"http://mt.example.com/"}"#,
        )
        .unwrap();
        assert!(validate_provider_config(&insecure).is_err());

        let local = serde_json::from_str::<TranslateProviderConfig>(
            r#"{"type":"aliyun","access_key_id":"id","access_key_secret":"secret",
                "endpoint":"http://localhost:8080/translate"}"#,
        )
        .unwrap();
        assert!(validate_provider_config(&local).is_ok());
    }

    #[test]
    fn legacy_encrypted_translate_config_is_lazily_migrated_to_v1() {
        let crypto = pebble_crypto::CryptoService::from_key([11_u8; 32]);
        let store = Store::open_in_memory().unwrap();
        let plaintext = r#"{"api_key":"secret"}"#;
        let legacy = hex_encode(&crypto.encrypt(plaintext.as_bytes()).unwrap());
        save_config(&store, &legacy);

        assert_eq!(
            decrypt_config_with_store(&crypto, &store, &legacy).unwrap(),
            plaintext
        );

        let migrated_hex = store.get_translate_config().unwrap().unwrap().config;
        let migrated = hex_decode(&migrated_hex).unwrap();
        assert!(!pebble_crypto::CryptoService::ciphertext_needs_migration(
            &migrated
        ));
        assert_eq!(
            crypto
                .decrypt_for(
                    TRANSLATE_CONFIG_PURPOSE,
                    ACTIVE_TRANSLATE_CONFIG_ID,
                    &migrated,
                )
                .unwrap(),
            plaintext.as_bytes()
        );
    }

    #[test]
    fn plaintext_translate_config_is_lazily_migrated_to_v1() {
        let crypto = pebble_crypto::CryptoService::from_key([11_u8; 32]);
        let store = Store::open_in_memory().unwrap();
        let plaintext = r#"{"api_key":"secret"}"#;
        save_config(&store, plaintext);

        assert_eq!(
            decrypt_config_with_store(&crypto, &store, plaintext).unwrap(),
            plaintext
        );

        let migrated_hex = store.get_translate_config().unwrap().unwrap().config;
        let migrated = hex_decode(&migrated_hex).unwrap();
        assert!(!pebble_crypto::CryptoService::ciphertext_needs_migration(
            &migrated
        ));
    }

    #[test]
    fn translate_config_rejects_non_ascii_hex_without_panicking() {
        let result = std::panic::catch_unwind(|| hex_decode("aéx"));

        assert!(
            result.is_ok(),
            "invalid translate config hex must not panic"
        );
        let error = result.unwrap().unwrap_err().to_string();
        assert!(error.contains("Invalid hex"));
    }
}
