use crate::commands::encrypted_store::{
    load_secure_user_data, lock_secure_user_data_key, store_secure_user_data,
};
use crate::core::{new_id, now_timestamp, PebbleError};
use crate::state::AppState;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
use tauri::State;

const EMAIL_TEMPLATES_KEY: &str = "email_templates";
const EMAIL_SIGNATURES_KEY: &str = "email_signatures";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmailTemplate {
    pub id: String,
    pub name: String,
    pub subject: String,
    pub body: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveEmailTemplateRequest {
    pub name: String,
    pub subject: String,
    pub body: String,
    #[serde(default)]
    pub deduplicate_by_content: bool,
}

fn decrypt_json<T: DeserializeOwned>(
    state: &AppState,
    key: &str,
) -> Result<Option<T>, PebbleError> {
    let Some(decrypted) = load_secure_user_data(&state.crypto, &state.store, key)? else {
        return Ok(None);
    };
    serde_json::from_slice(&decrypted)
        .map(Some)
        .map_err(|e| PebbleError::Internal(format!("Invalid secure user data for {key}: {e}")))
}

fn encrypt_json<T: Serialize>(state: &AppState, key: &str, value: &T) -> Result<(), PebbleError> {
    let plaintext = serde_json::to_vec(value)
        .map_err(|e| PebbleError::Internal(format!("Failed to serialize secure user data: {e}")))?;
    store_secure_user_data(&state.crypto, &state.store, key, &plaintext)
}

fn normalize_template_input(input: SaveEmailTemplateRequest) -> SaveEmailTemplateRequest {
    SaveEmailTemplateRequest {
        name: input.name.trim().to_string(),
        subject: input.subject,
        body: input.body,
        deduplicate_by_content: input.deduplicate_by_content,
    }
}

#[tauri::command]
pub async fn list_email_templates(
    state: State<'_, AppState>,
) -> Result<Vec<EmailTemplate>, PebbleError> {
    Ok(decrypt_json(&state, EMAIL_TEMPLATES_KEY)?.unwrap_or_default())
}

#[tauri::command]
pub async fn save_email_template(
    state: State<'_, AppState>,
    template: SaveEmailTemplateRequest,
) -> Result<EmailTemplate, PebbleError> {
    save_email_template_for_state(&state, template).await
}

async fn save_email_template_for_state(
    state: &AppState,
    template: SaveEmailTemplateRequest,
) -> Result<EmailTemplate, PebbleError> {
    let template = normalize_template_input(template);
    if template.name.is_empty() {
        return Err(PebbleError::Validation(
            "Template name cannot be empty".to_string(),
        ));
    }

    let _guard = lock_secure_user_data_key(state, EMAIL_TEMPLATES_KEY).await;
    let mut templates: Vec<EmailTemplate> =
        decrypt_json(state, EMAIL_TEMPLATES_KEY)?.unwrap_or_default();
    if template.deduplicate_by_content {
        if let Some(existing) = templates.iter().find(|existing| {
            existing.name == template.name
                && existing.subject == template.subject
                && existing.body == template.body
        }) {
            return Ok(existing.clone());
        }
    }
    let saved = EmailTemplate {
        id: new_id(),
        name: template.name,
        subject: template.subject,
        body: template.body,
        created_at: now_timestamp(),
    };
    templates.push(saved.clone());
    encrypt_json(state, EMAIL_TEMPLATES_KEY, &templates)?;
    Ok(saved)
}

#[tauri::command]
pub async fn delete_email_template(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), PebbleError> {
    delete_email_template_for_state(&state, id).await
}

async fn delete_email_template_for_state(state: &AppState, id: String) -> Result<(), PebbleError> {
    let _guard = lock_secure_user_data_key(state, EMAIL_TEMPLATES_KEY).await;
    let mut templates: Vec<EmailTemplate> =
        decrypt_json(state, EMAIL_TEMPLATES_KEY)?.unwrap_or_default();
    templates.retain(|template| template.id != id);
    if templates.is_empty() {
        state.store.delete_secure_user_data(EMAIL_TEMPLATES_KEY)
    } else {
        encrypt_json(state, EMAIL_TEMPLATES_KEY, &templates)
    }
}

#[tauri::command]
pub async fn get_email_signature(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<String, PebbleError> {
    let signatures: HashMap<String, String> =
        decrypt_json(&state, EMAIL_SIGNATURES_KEY)?.unwrap_or_default();
    Ok(signatures.get(&account_id).cloned().unwrap_or_default())
}

#[tauri::command]
pub async fn set_email_signature(
    state: State<'_, AppState>,
    account_id: String,
    signature: String,
) -> Result<(), PebbleError> {
    set_email_signature_for_state(&state, account_id, signature).await
}

#[tauri::command]
pub async fn migrate_email_signature_if_absent(
    state: State<'_, AppState>,
    account_id: String,
    signature: String,
) -> Result<String, PebbleError> {
    migrate_email_signature_if_absent_for_state(&state, account_id, signature).await
}

async fn migrate_email_signature_if_absent_for_state(
    state: &AppState,
    account_id: String,
    signature: String,
) -> Result<String, PebbleError> {
    let _guard = lock_secure_user_data_key(state, EMAIL_SIGNATURES_KEY).await;
    let mut signatures: HashMap<String, String> =
        decrypt_json(state, EMAIL_SIGNATURES_KEY)?.unwrap_or_default();
    if let Some(current) = signatures.get(&account_id) {
        return Ok(current.clone());
    }
    if signature.trim().is_empty() {
        return Ok(String::new());
    }
    signatures.insert(account_id, signature.clone());
    encrypt_json(state, EMAIL_SIGNATURES_KEY, &signatures)?;
    Ok(signature)
}

async fn set_email_signature_for_state(
    state: &AppState,
    account_id: String,
    signature: String,
) -> Result<(), PebbleError> {
    let _guard = lock_secure_user_data_key(state, EMAIL_SIGNATURES_KEY).await;
    let mut signatures: HashMap<String, String> =
        decrypt_json(state, EMAIL_SIGNATURES_KEY)?.unwrap_or_default();
    let stored_signature = if signature.trim().is_empty() {
        // Keep an explicit empty value as a tombstone. A delayed legacy
        // migration must distinguish "the user cleared this signature" from
        // "this account has never had secure signature state".
        String::new()
    } else {
        signature
    };
    signatures.insert(account_id, stored_signature);
    encrypt_json(state, EMAIL_SIGNATURES_KEY, &signatures)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::CryptoService;
    use crate::search::TantivySearch;
    use crate::store::Store;
    use std::sync::Arc;

    fn request(name: &str, deduplicate_by_content: bool) -> SaveEmailTemplateRequest {
        SaveEmailTemplateRequest {
            name: name.to_string(),
            subject: format!("{name} subject"),
            body: format!("{name} body"),
            deduplicate_by_content,
        }
    }

    fn test_state() -> AppState {
        let (snooze_stop_tx, _snooze_stop_rx) = std::sync::mpsc::channel();
        AppState::new(
            Store::open_in_memory().unwrap(),
            TantivySearch::open_in_memory().unwrap(),
            CryptoService::from_key([42; 32]),
            snooze_stop_tx,
            std::env::temp_dir().join(format!("pebble-user-data-test-{}", new_id())),
        )
    }

    #[test]
    fn template_names_are_trimmed_before_storage() {
        let normalized = normalize_template_input(SaveEmailTemplateRequest {
            name: "  Intro  ".to_string(),
            subject: "Subject".to_string(),
            body: "Body".to_string(),
            deduplicate_by_content: false,
        });

        assert_eq!(normalized.name, "Intro");
        assert_eq!(normalized.subject, "Subject");
        assert_eq!(normalized.body, "Body");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_signatures_for_two_accounts_do_not_lose_each_other() {
        let state = Arc::new(test_state());
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let first = {
            let state = Arc::clone(&state);
            let barrier = Arc::clone(&barrier);
            tokio::spawn(async move {
                barrier.wait().await;
                set_email_signature_for_state(&state, "account-1".to_string(), "one".to_string())
                    .await
            })
        };
        let second = {
            let state = Arc::clone(&state);
            let barrier = Arc::clone(&barrier);
            tokio::spawn(async move {
                barrier.wait().await;
                set_email_signature_for_state(&state, "account-2".to_string(), "two".to_string())
                    .await
            })
        };

        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();
        let signatures: HashMap<String, String> =
            decrypt_json(&state, EMAIL_SIGNATURES_KEY).unwrap().unwrap();
        assert_eq!(signatures.get("account-1").map(String::as_str), Some("one"));
        assert_eq!(signatures.get("account-2").map(String::as_str), Some("two"));
    }

    #[tokio::test]
    async fn legacy_signature_migration_does_not_overwrite_a_newer_save() {
        let state = test_state();
        set_email_signature_for_state(&state, "account-1".to_string(), "new signature".to_string())
            .await
            .unwrap();

        let result = migrate_email_signature_if_absent_for_state(
            &state,
            "account-1".to_string(),
            "legacy signature".to_string(),
        )
        .await
        .unwrap();

        assert_eq!(result, "new signature");
        let signatures: HashMap<String, String> =
            decrypt_json(&state, EMAIL_SIGNATURES_KEY).unwrap().unwrap();
        assert_eq!(
            signatures.get("account-1").map(String::as_str),
            Some("new signature")
        );
    }

    #[tokio::test]
    async fn legacy_signature_migration_does_not_restore_an_explicitly_cleared_signature() {
        let state = test_state();
        set_email_signature_for_state(&state, "account-1".to_string(), String::new())
            .await
            .unwrap();

        let result = migrate_email_signature_if_absent_for_state(
            &state,
            "account-1".to_string(),
            "legacy signature".to_string(),
        )
        .await
        .unwrap();

        assert_eq!(result, "");
        let signatures: HashMap<String, String> =
            decrypt_json(&state, EMAIL_SIGNATURES_KEY).unwrap().unwrap();
        assert_eq!(signatures.get("account-1").map(String::as_str), Some(""));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_template_save_and_delete_do_not_lose_either_update() {
        let state = Arc::new(test_state());
        let existing = save_email_template_for_state(&state, request("existing", false))
            .await
            .unwrap();
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let save = {
            let state = Arc::clone(&state);
            let barrier = Arc::clone(&barrier);
            tokio::spawn(async move {
                barrier.wait().await;
                save_email_template_for_state(&state, request("new", false)).await
            })
        };
        let delete = {
            let state = Arc::clone(&state);
            let barrier = Arc::clone(&barrier);
            tokio::spawn(async move {
                barrier.wait().await;
                delete_email_template_for_state(&state, existing.id).await
            })
        };

        let saved = save.await.unwrap().unwrap();
        delete.await.unwrap().unwrap();
        let templates: Vec<EmailTemplate> = decrypt_json(&state, EMAIL_TEMPLATES_KEY)
            .unwrap()
            .unwrap_or_default();
        assert_eq!(templates, vec![saved]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_legacy_template_migration_is_idempotent_by_content() {
        let state = Arc::new(test_state());
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let migrate = |state: Arc<AppState>, barrier: Arc<tokio::sync::Barrier>| {
            tokio::spawn(async move {
                barrier.wait().await;
                save_email_template_for_state(&state, request("legacy", true)).await
            })
        };
        let first = migrate(Arc::clone(&state), Arc::clone(&barrier));
        let second = migrate(Arc::clone(&state), Arc::clone(&barrier));

        let first = first.await.unwrap().unwrap();
        let second = second.await.unwrap().unwrap();
        let templates: Vec<EmailTemplate> =
            decrypt_json(&state, EMAIL_TEMPLATES_KEY).unwrap().unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(templates.len(), 1);
    }
}
