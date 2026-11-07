use crate::core::{
    Contact, ContactInput, ContactSuggestion, KnownContact, PebbleError, VcardImportResult,
};
use crate::state::AppState;
use crate::store::Store;
use std::sync::Arc;
use tauri::State;

async fn run_store_blocking<F, T>(
    store: Arc<Store>,
    operation: F,
) -> std::result::Result<T, PebbleError>
where
    F: FnOnce(&Store) -> std::result::Result<T, PebbleError> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || operation(&store))
        .await
        .map_err(|error| PebbleError::Internal(format!("Task join error: {error}")))?
}

fn validated_contact_id(contact_id: &str) -> std::result::Result<&str, PebbleError> {
    let contact_id = contact_id.trim();
    if contact_id.is_empty() {
        return Err(PebbleError::Validation(
            "Contact id must not be empty".to_string(),
        ));
    }
    Ok(contact_id)
}

fn save_contact_with_store(
    store: &Store,
    input: &ContactInput,
) -> std::result::Result<Contact, PebbleError> {
    store.save_contact(input)
}

#[tauri::command]
pub async fn list_contacts(
    state: State<'_, AppState>,
    query: Option<String>,
    favorite_only: Option<bool>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> std::result::Result<Vec<Contact>, PebbleError> {
    let store = state.store.clone();
    run_store_blocking(store, move |store| {
        store.list_contacts(
            query.as_deref(),
            favorite_only.unwrap_or(false),
            limit.unwrap_or(50),
            offset.unwrap_or(0),
        )
    })
    .await
}

#[tauri::command]
pub async fn get_contact_by_email(
    state: State<'_, AppState>,
    address: String,
) -> std::result::Result<Option<Contact>, PebbleError> {
    let store = state.store.clone();
    run_store_blocking(store, move |store| store.get_contact_by_email(&address)).await
}

#[tauri::command]
pub async fn save_contact(
    state: State<'_, AppState>,
    input: ContactInput,
) -> std::result::Result<Contact, PebbleError> {
    let store = state.store.clone();
    run_store_blocking(store, move |store| save_contact_with_store(store, &input)).await
}

#[tauri::command]
pub async fn delete_contact(
    state: State<'_, AppState>,
    contact_id: String,
    suppress_addresses: Option<bool>,
) -> std::result::Result<(), PebbleError> {
    let store = state.store.clone();
    run_store_blocking(store, move |store| {
        store.delete_contact(
            validated_contact_id(&contact_id)?,
            suppress_addresses.unwrap_or(false),
        )
    })
    .await
}

#[tauri::command]
pub async fn set_contact_favorite(
    state: State<'_, AppState>,
    contact_id: String,
    is_favorite: bool,
) -> std::result::Result<(), PebbleError> {
    let store = state.store.clone();
    run_store_blocking(store, move |store| {
        store.set_contact_favorite(validated_contact_id(&contact_id)?, is_favorite)
    })
    .await
}

#[tauri::command]
pub async fn search_contact_suggestions(
    state: State<'_, AppState>,
    account_id: String,
    query: String,
    limit: Option<i64>,
) -> std::result::Result<Vec<ContactSuggestion>, PebbleError> {
    let store = state.store.clone();
    run_store_blocking(store, move |store| {
        store.search_contact_suggestions(&account_id, &query, limit.unwrap_or(20))
    })
    .await
}

#[tauri::command]
pub async fn suppress_contact_suggestion(
    state: State<'_, AppState>,
    address: String,
) -> std::result::Result<(), PebbleError> {
    let store = state.store.clone();
    run_store_blocking(store, move |store| {
        store.suppress_contact_suggestion(&address)
    })
    .await
}

#[tauri::command]
pub async fn import_contacts_vcard(
    state: State<'_, AppState>,
    data: String,
) -> std::result::Result<VcardImportResult, PebbleError> {
    let store = state.store.clone();
    run_store_blocking(store, move |store| store.import_contacts_vcard(&data)).await
}

#[tauri::command]
pub async fn export_contacts_vcard(
    state: State<'_, AppState>,
) -> std::result::Result<String, PebbleError> {
    let store = state.store.clone();
    run_store_blocking(store, Store::export_contacts_vcard).await
}

#[tauri::command]
pub async fn search_contacts(
    state: State<'_, AppState>,
    account_id: String,
    query: String,
    limit: Option<i64>,
) -> std::result::Result<Vec<KnownContact>, PebbleError> {
    let store = state.store.clone();
    run_store_blocking(store, move |store| {
        store.list_known_contacts(&account_id, &query, limit.unwrap_or(20))
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, thread};

    use super::{run_store_blocking, save_contact_with_store};
    use crate::core::{ContactEmailInput, ContactEmailLabel, ContactInput, PebbleError};
    use crate::store::Store;

    fn input(address: &str) -> ContactInput {
        ContactInput {
            id: None,
            display_name: "Alice".to_string(),
            notes: String::new(),
            is_favorite: false,
            emails: vec![ContactEmailInput {
                id: None,
                address: address.to_string(),
                label: ContactEmailLabel::Other,
                is_primary: true,
            }],
        }
    }

    #[test]
    fn contact_command_maps_invalid_email_to_validation() {
        let store = Store::open_in_memory().unwrap();

        assert!(matches!(
            save_contact_with_store(&store, &input("not-an-address")),
            Err(PebbleError::Validation(_))
        ));
    }

    #[test]
    fn contact_command_maps_duplicate_email_to_validation() {
        let store = Store::open_in_memory().unwrap();
        save_contact_with_store(&store, &input("Alice@example.com")).unwrap();

        assert!(matches!(
            save_contact_with_store(&store, &input("alice@EXAMPLE.COM")),
            Err(PebbleError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn contact_store_work_runs_on_a_blocking_thread() {
        let calling_thread = thread::current().id();
        let store = Arc::new(Store::open_in_memory().unwrap());

        let worker_thread = run_store_blocking(store, |_| Ok(thread::current().id()))
            .await
            .unwrap();

        assert_ne!(worker_thread, calling_thread);
    }
}
