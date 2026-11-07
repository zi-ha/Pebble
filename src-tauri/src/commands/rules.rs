use crate::core::{new_id, now_timestamp, PebbleError, Rule};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn create_rule(
    state: State<'_, AppState>,
    name: String,
    priority: i32,
    conditions: String,
    actions: String,
) -> std::result::Result<Rule, PebbleError> {
    let now = now_timestamp();
    let rule = Rule {
        id: new_id(),
        name,
        priority,
        conditions,
        actions,
        is_enabled: true,
        created_at: now,
        updated_at: now,
    };
    state.store.insert_rule(&rule)?;
    Ok(rule)
}

#[tauri::command]
pub async fn list_rules(state: State<'_, AppState>) -> std::result::Result<Vec<Rule>, PebbleError> {
    state.store.list_rules()
}

#[tauri::command]
pub async fn update_rule(
    state: State<'_, AppState>,
    rule: Rule,
) -> std::result::Result<(), PebbleError> {
    state.store.update_rule(&rule)
}

#[tauri::command]
pub async fn delete_rule(
    state: State<'_, AppState>,
    rule_id: String,
) -> std::result::Result<(), PebbleError> {
    state.store.delete_rule(&rule_id)
}
