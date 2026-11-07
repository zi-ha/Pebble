use crate::core::PebbleError;
use crate::state::AppState;
use std::collections::HashMap;
use tauri::State;

#[tauri::command]
pub async fn get_folder_unread_counts(
    state: State<'_, AppState>,
    account_id: String,
) -> std::result::Result<HashMap<String, u32>, PebbleError> {
    let store = state.store.clone();
    tokio::task::spawn_blocking(move || store.get_folder_unread_counts(&account_id))
        .await
        .map_err(|e| PebbleError::Internal(format!("Task join error: {e}")))?
}
