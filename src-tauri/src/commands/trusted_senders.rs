use crate::core::{now_timestamp, PebbleError, TrustType, TrustedSender};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn trust_sender(
    state: State<'_, AppState>,
    account_id: String,
    email: String,
    trust_type: TrustType,
) -> std::result::Result<(), PebbleError> {
    let sender = TrustedSender {
        account_id,
        email,
        trust_type,
        created_at: now_timestamp(),
    };
    state.store.trust_sender(&sender)
}

#[tauri::command]
pub async fn list_trusted_senders(
    state: State<'_, AppState>,
    account_id: String,
) -> std::result::Result<Vec<TrustedSender>, PebbleError> {
    state.store.list_trusted_senders(&account_id)
}

#[tauri::command]
pub async fn remove_trusted_sender(
    state: State<'_, AppState>,
    account_id: String,
    email: String,
) -> std::result::Result<(), PebbleError> {
    state.store.remove_trusted_sender(&account_id, &email)
}
