use crate::core::PebbleError;
use crate::state::AppState;
use crate::store::labels::Label;
use tauri::State;

#[tauri::command]
pub async fn get_message_labels(
    state: State<'_, AppState>,
    message_id: String,
) -> std::result::Result<Vec<Label>, PebbleError> {
    state.store.get_message_labels(&message_id)
}

#[tauri::command]
pub async fn get_message_labels_batch(
    state: State<'_, AppState>,
    message_ids: Vec<String>,
) -> std::result::Result<std::collections::HashMap<String, Vec<Label>>, PebbleError> {
    state.store.get_message_labels_batch(&message_ids)
}

#[tauri::command]
pub async fn add_message_label(
    state: State<'_, AppState>,
    message_id: String,
    label_name: String,
) -> std::result::Result<(), PebbleError> {
    state.store.add_label(&message_id, &label_name)
}

#[tauri::command]
pub async fn remove_message_label(
    state: State<'_, AppState>,
    message_id: String,
    label_name: String,
) -> std::result::Result<(), PebbleError> {
    state.store.remove_label(&message_id, &label_name)
}

#[tauri::command]
pub async fn list_labels(
    state: State<'_, AppState>,
) -> std::result::Result<Vec<Label>, PebbleError> {
    state.store.list_labels()
}
