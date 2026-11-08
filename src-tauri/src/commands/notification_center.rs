//! New-mail notification dispatch.
//!
//! Three concerns live here:
//! 1. deciding whether a stored message is worth telling the user about,
//! 2. coalescing a burst of arrivals into a few per-thread cards instead of one
//!    buzz per message,
//! 3. drawing the card, either in the custom popup window (Windows) or through
//!    the platform notification centre.
//!
//! The pipeline calls [`enqueue`] only *after* user rules have run, so a rule
//! that archives or marks a message read keeps it quiet.

use crate::core::FolderRole;
use crate::events;
use crate::mail::StoredMessage;
use crate::state::AppState;
use crate::store::Store;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;
use tracing::warn;

/// How long to keep collecting messages before showing anything. A sync burst
/// usually lands within a few hundred milliseconds; waiting this long turns
/// "50 cards" into "3 cards".
pub const DEBOUNCE_MS: u64 = 700;

/// Upper bound on simultaneously visible cards. Anything beyond this is
/// dropped: the tray indicator and the list already cover the rest.
pub const MAX_CARDS: usize = 3;

/// Popup geometry, in logical pixels.
const POPUP_WIDTH: f64 = 384.0;
const POPUP_MARGIN_RIGHT: f64 = 24.0;
const POPUP_MARGIN_BOTTOM: f64 = 48.0;
const POPUP_CARD_HEIGHT: f64 = 116.0;
const POPUP_PADDING: f64 = 16.0;

pub const WINDOW_LABEL: &str = "notification";

#[cfg(all(not(windows), not(target_os = "linux"), not(target_os = "macos")))]
use tauri_plugin_notification::NotificationExt;

// The action id only matters where a platform notification centre exists.
// Windows draws its own popup now, so these are compiled but unused there.
#[cfg(any(target_os = "linux", windows))]
#[cfg_attr(windows, allow(dead_code))]
const NOTIFICATION_OPEN_ACTION: &str = "open";

/// One message that wants to be shown. Kept private: outside callers only ever
/// see merged [`NotificationGroup`]s.
#[derive(Clone, Debug)]
pub struct PendingNotification {
    pub account_id: String,
    pub thread_id: String,
    pub message_id: String,
    pub sender: String,
    pub subject: String,
    pub snippet: String,
    pub received_at: i64,
}

/// A card: one per conversation, with `count` messages behind it.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct NotificationGroup {
    pub id: String,
    pub account_id: String,
    pub thread_id: String,
    pub message_id: String,
    pub sender: String,
    pub subject: String,
    pub snippet: String,
    pub received_at: i64,
    pub count: usize,
}

fn group_key(item: &PendingNotification) -> String {
    if item.thread_id.trim().is_empty() {
        format!("{}\u{1}{}", item.account_id, item.message_id)
    } else {
        format!("{}\u{1}{}", item.account_id, item.thread_id)
    }
}

/// Merge a burst of arrivals into at most [`MAX_CARDS`] cards.
///
/// The newest message of a conversation becomes the card's click target, since
/// that is what the user means by "the new mail".
pub fn merge_notifications(items: Vec<PendingNotification>) -> Vec<NotificationGroup> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, NotificationGroup> = HashMap::new();

    for item in items {
        let key = group_key(&item);
        match groups.get_mut(&key) {
            Some(existing) => {
                existing.count += 1;
                if item.received_at >= existing.received_at {
                    existing.received_at = item.received_at;
                    existing.message_id = item.message_id;
                    existing.sender = item.sender;
                    existing.subject = item.subject;
                    existing.snippet = item.snippet;
                }
            }
            None => {
                order.push(key.clone());
                groups.insert(
                    key.clone(),
                    NotificationGroup {
                        id: key,
                        account_id: item.account_id,
                        thread_id: item.thread_id,
                        message_id: item.message_id,
                        sender: item.sender,
                        subject: item.subject,
                        snippet: item.snippet,
                        received_at: item.received_at,
                        count: 1,
                    },
                );
            }
        }
    }

    let mut merged: Vec<NotificationGroup> = order
        .into_iter()
        .filter_map(|key| groups.remove(&key))
        .collect();
    merged.sort_by_key(|group| std::cmp::Reverse(group.received_at));
    merged.truncate(MAX_CARDS);
    merged
}

fn sender_of(stored: &StoredMessage) -> String {
    let name = stored.message.from_name.trim();
    if name.is_empty() {
        stored.message.from_address.trim().to_string()
    } else {
        name.to_string()
    }
}

fn snippet_of(stored: &StoredMessage) -> String {
    let snippet = stored.message.snippet.trim();
    if !snippet.is_empty() {
        return snippet.chars().take(140).collect();
    }
    let body = stored.message.body_text.trim();
    body.chars().take(140).collect()
}

fn pending_from(stored: &StoredMessage) -> PendingNotification {
    PendingNotification {
        account_id: stored.message.account_id.clone(),
        thread_id: stored.message.thread_id.clone().unwrap_or_default(),
        message_id: stored.message.id.clone(),
        sender: sender_of(stored),
        subject: stored.message.subject.trim().to_string(),
        snippet: snippet_of(stored),
        received_at: stored.message.date,
    }
}

fn notifications_enabled(app: &AppHandle) -> bool {
    app.try_state::<AppState>()
        .is_some_and(|state| state.notifications_enabled.load(Ordering::SeqCst))
}

/// Only tell the user about mail that landed in an inbox folder and that the
/// sync worker itself flagged as genuinely new.
pub fn should_notify(store: &Store, stored: &StoredMessage) -> crate::core::Result<bool> {
    if !stored.notify || stored.message.is_deleted || stored.message.is_draft {
        return Ok(false);
    }

    let folder_ids: HashSet<&str> = stored.folder_ids.iter().map(String::as_str).collect();
    if folder_ids.is_empty() {
        return Ok(false);
    }

    let folders = store.list_folders(&stored.message.account_id)?;
    Ok(folders.iter().any(|folder| {
        folder.role == Some(FolderRole::Inbox) && folder_ids.contains(folder.id.as_str())
    }))
}

/// Queue a message for the notification centre. Never blocks and never shows
/// anything synchronously — the debounced collector decides what to draw.
pub fn enqueue(app: &AppHandle, store: &Store, stored: &StoredMessage) {
    if !notifications_enabled(app) {
        return;
    }

    match should_notify(store, stored) {
        Ok(true) => {}
        Ok(false) => return,
        Err(e) => {
            warn!(
                "Failed to evaluate new mail notification eligibility for message {}: {e}",
                stored.message.id
            );
            return;
        }
    }

    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let sender = state.notification_tx.lock().ok().and_then(|tx| tx.clone());
    if let Some(tx) = sender {
        let _ = tx.send(pending_from(stored));
    }
}

/// Create the channel and start the collector. Called once during setup.
pub fn install(app: &AppHandle) {
    let (tx, rx) = mpsc::unbounded_channel::<PendingNotification>();
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut slot) = state.notification_tx.lock() {
            *slot = Some(tx);
        }
    }

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        collect_notifications(app_handle, rx).await;
    });
}

async fn collect_notifications(
    app: AppHandle,
    mut rx: mpsc::UnboundedReceiver<PendingNotification>,
) {
    loop {
        let Some(first) = rx.recv().await else {
            break;
        };

        let mut batch = vec![first];
        let deadline =
            tokio::time::Instant::now() + tokio::time::Duration::from_millis(DEBOUNCE_MS);
        loop {
            tokio::select! {
                more = rx.recv() => match more {
                    Some(item) => batch.push(item),
                    None => break,
                },
                _ = tokio::time::sleep_until(deadline) => break,
            }
            if batch.len() >= MAX_CARDS * 8 {
                break;
            }
        }

        let groups = merge_notifications(batch);
        if groups.is_empty() {
            continue;
        }
        publish(&app, groups);
    }
}

fn main_window_is_active(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .map(|window| window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false))
        .unwrap_or(false)
}

fn publish(app: &AppHandle, groups: Vec<NotificationGroup>) {
    if !notifications_enabled(app) {
        return;
    }

    // The user is already reading mail; the list refreshes itself and a popup
    // would only steal focus.
    if main_window_is_active(app) {
        return;
    }

    crate::commands::notifications::mark_attention_indicator(app);

    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut stored) = state.notification_groups.lock() {
            *stored = groups.clone();
        }
    }

    #[cfg(windows)]
    {
        if let Err(e) = show_popup(app, &groups) {
            warn!("Failed to show the custom notification popup, falling back: {e}");
            show_native_notifications(app, &groups);
        }
    }

    #[cfg(not(windows))]
    {
        show_native_notifications(app, &groups);
    }
}

fn popup_height(cards: usize) -> f64 {
    POPUP_PADDING * 2.0 + (cards.max(1) as f64) * POPUP_CARD_HEIGHT
}

fn ensure_popup_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        return Ok(window);
    }

    tauri::WebviewWindowBuilder::new(
        app,
        WINDOW_LABEL,
        tauri::WebviewUrl::App("index.html#notification".into()),
    )
    .title("Pebble")
    .inner_size(POPUP_WIDTH, popup_height(1))
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .resizable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .visible(false)
    .build()
    .map_err(|e| e.to_string())
}

fn place_bottom_right(window: &tauri::WebviewWindow, height: f64) {
    // A window that has never been shown has no current monitor yet, so fall
    // back to the primary one — otherwise the first card of a session lands
    // wherever the OS decided to put it.
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let scale = monitor.scale_factor();
    let monitor_size = monitor.size().to_logical::<f64>(scale);
    let monitor_origin = monitor.position().to_logical::<f64>(scale);
    let x = monitor_origin.x + monitor_size.width - POPUP_WIDTH - POPUP_MARGIN_RIGHT;
    let y = monitor_origin.y + monitor_size.height - height - POPUP_MARGIN_BOTTOM;
    let _ = window.set_position(tauri::LogicalPosition::new(x, y));
}

fn render_popup(window: &tauri::WebviewWindow, groups: &[NotificationGroup]) -> Result<(), String> {
    let height = popup_height(groups.len());
    let _ = window.set_size(tauri::LogicalSize::new(POPUP_WIDTH, height));
    place_bottom_right(window, height);
    window
        .emit(events::MAIL_NOTIFICATION_GROUPS, groups)
        .map_err(|e| e.to_string())
}

#[cfg(windows)]
fn show_popup(app: &AppHandle, groups: &[NotificationGroup]) -> Result<(), String> {
    let window = ensure_popup_window(app)?;
    render_popup(&window, groups)?;
    window.show().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn hide_popup(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        let _ = window.hide();
    }
}

fn body_for(group: &NotificationGroup) -> String {
    let subject = group.subject.trim();
    let body = match (group.sender.is_empty(), subject.is_empty()) {
        (true, true) => "New message".to_string(),
        (true, false) => subject.to_string(),
        (false, true) => group.sender.clone(),
        (false, false) => format!("{}: {subject}", group.sender),
    };

    if group.count > 1 {
        format!("{body} (+{})", group.count - 1)
    } else {
        body
    }
}

// ---------------------------------------------------------------------------
// Platform notification centres (used on macOS/Linux, and as the Windows
// fallback when the popup window cannot be created).
// ---------------------------------------------------------------------------

/// Payload contract shared by the popup and the platform notification centre.
pub fn open_payload(account_id: &str, message_id: &str) -> serde_json::Value {
    serde_json::json!({
        "account_id": account_id,
        "message_id": message_id,
    })
}

fn open_message(app: &AppHandle, account_id: &str, message_id: &str) {
    crate::commands::notifications::clear_attention_indicator(app);
    dismiss_all(app);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    let _ = app.emit(
        events::MAIL_NOTIFICATION_OPEN,
        open_payload(account_id, message_id),
    );
}

#[cfg(any(target_os = "linux", windows))]
#[cfg_attr(windows, allow(dead_code))]
pub fn is_notification_open_action(action: &str) -> bool {
    matches!(action, "default" | NOTIFICATION_OPEN_ACTION)
}

#[cfg(all(not(windows), not(target_os = "linux"), not(target_os = "macos")))]
fn show_default_notification(app: &AppHandle, body: &str) -> Result<(), String> {
    app.notification()
        .builder()
        .title("Pebble - New Mail")
        .body(body)
        .show()
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "linux")]
fn show_linux_notification(
    app: &AppHandle,
    body: &str,
    account_id: &str,
    message_id: &str,
) -> Result<(), String> {
    crate::commands::notifications::ensure_notification_environment(app)?;

    let mut notification = notify_rust::Notification::new();
    notification
        .summary("Pebble - New Mail")
        .body(body)
        .appname("Pebble")
        .auto_icon()
        .action("default", "Open");

    let handle = notification.show().map_err(|e| e.to_string())?;
    let app_handle = app.clone();
    let account_id = account_id.to_string();
    let message_id = message_id.to_string();

    std::thread::Builder::new()
        .name("pebble-notification-open".to_string())
        .spawn(move || {
            handle.wait_for_action(|action| {
                if is_notification_open_action(action) {
                    open_message(&app_handle, &account_id, &message_id);
                }
            });
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn show_macos_notification(
    app: &AppHandle,
    body: &str,
    account_id: &str,
    message_id: &str,
) -> Result<(), String> {
    crate::commands::notifications::ensure_notification_environment(app)?;

    let app_handle = app.clone();
    let account_id = account_id.to_string();
    let message_id = message_id.to_string();
    let body = body.to_string();
    let app_id = if tauri::is_dev() {
        "com.apple.Terminal".to_string()
    } else {
        app.config().identifier.clone()
    };

    std::thread::Builder::new()
        .name("pebble-notification-open".to_string())
        .spawn(move || {
            let _ = mac_notification_sys::set_application(&app_id);
            let mut notification = mac_notification_sys::Notification::new();
            notification
                .title("Pebble - New Mail")
                .message(&body)
                .main_button(mac_notification_sys::MainButton::SingleAction("Open"))
                .wait_for_click(true);

            match notification.send() {
                Ok(response)
                    if matches!(
                        response,
                        mac_notification_sys::NotificationResponse::Click
                            | mac_notification_sys::NotificationResponse::ActionButton(_)
                    ) =>
                {
                    open_message(&app_handle, &account_id, &message_id);
                }
                Ok(_) => {}
                Err(e) => warn!("Failed to show clickable macOS notification: {e}"),
            }
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn show_native_notifications(app: &AppHandle, groups: &[NotificationGroup]) {
    for group in groups {
        let body = body_for(group);
        let result = {
            #[cfg(target_os = "linux")]
            {
                show_linux_notification(app, &body, &group.account_id, &group.message_id)
            }
            #[cfg(target_os = "macos")]
            {
                show_macos_notification(app, &body, &group.account_id, &group.message_id)
            }
            #[cfg(all(not(target_os = "linux"), not(target_os = "macos")))]
            {
                show_default_notification(app, &body)
            }
        };
        if let Err(e) = result {
            warn!("Failed to show new mail notification: {e}");
        }
    }
}

/// Fallback when the popup window cannot be created or drawn: hand the card to
/// the Windows notification centre. This path still needs the AppUserModelID
/// registration that lives in `notifications`.
#[cfg(windows)]
fn show_native_notifications(app: &AppHandle, groups: &[NotificationGroup]) {
    for group in groups {
        let body = body_for(group);
        if let Err(e) = crate::commands::notifications::show_desktop_notification(
            app,
            "Pebble - New Mail",
            &body,
        ) {
            warn!("Failed to show fallback Windows notification: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Commands used by the popup window.
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_notification_groups(state: tauri::State<AppState>) -> Vec<NotificationGroup> {
    state
        .notification_groups
        .lock()
        .map(|groups| groups.clone())
        .unwrap_or_default()
}

fn current_groups(app: &AppHandle) -> Vec<NotificationGroup> {
    app.try_state::<AppState>()
        .and_then(|state| state.notification_groups.lock().ok().map(|g| g.clone()))
        .unwrap_or_default()
}

fn apply_groups(app: &AppHandle, groups: Vec<NotificationGroup>) {
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut stored) = state.notification_groups.lock() {
            *stored = groups.clone();
        }
    }

    if groups.is_empty() {
        hide_popup(app);
        return;
    }

    #[cfg(windows)]
    {
        if let Ok(window) = ensure_popup_window(app) {
            let _ = render_popup(&window, &groups);
            let _ = window.show();
        }
    }
}

/// Render the popup once, ignoring focus, so the settings page can show the
/// user what a real notification looks like without waiting for mail.
pub fn show_test_card(app: &AppHandle, subject: Option<String>, body: Option<String>) {
    let group = NotificationGroup {
        id: "test".to_string(),
        account_id: String::new(),
        thread_id: String::new(),
        message_id: String::new(),
        sender: "Pebble".to_string(),
        subject: subject.unwrap_or_else(|| "New mail looks like this".to_string()),
        snippet: body.unwrap_or_else(|| {
            "Click the card to open the message; dismiss it with the close button.".to_string()
        }),
        received_at: crate::core::now_timestamp(),
        count: 1,
    };
    apply_groups(app, vec![group]);
}

#[tauri::command]
pub fn dismiss_notification(app: AppHandle, id: String) {
    let remaining: Vec<NotificationGroup> = current_groups(&app)
        .into_iter()
        .filter(|group| group.id != id)
        .collect();
    apply_groups(&app, remaining);
}

pub fn dismiss_all(app: &AppHandle) {
    apply_groups(app, Vec::new());
}

#[tauri::command]
pub fn dismiss_all_notifications(app: AppHandle) {
    dismiss_all(&app);
}

#[tauri::command]
pub fn open_notification_message(app: AppHandle, account_id: String, message_id: String) {
    open_message(&app, &account_id, &message_id);
}

/// The popup window measures its own content and asks for the matching size,
/// so cards never get clipped when a subject wraps.
#[tauri::command]
pub fn set_notification_popup_height(app: AppHandle, height: f64) {
    let Some(window) = app.get_webview_window(WINDOW_LABEL) else {
        return;
    };
    let height = height.clamp(80.0, 720.0);
    let _ = window.set_size(tauri::LogicalSize::new(POPUP_WIDTH, height));
    place_bottom_right(&window, height);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(thread: &str, message: &str, received_at: i64) -> PendingNotification {
        PendingNotification {
            account_id: "acc".to_string(),
            thread_id: thread.to_string(),
            message_id: message.to_string(),
            sender: "Ada".to_string(),
            subject: "Subject".to_string(),
            snippet: "Snippet".to_string(),
            received_at,
        }
    }

    #[test]
    fn merges_messages_of_one_conversation_into_a_single_card() {
        let merged = merge_notifications(vec![
            pending("t1", "m1", 100),
            pending("t1", "m2", 200),
            pending("t1", "m3", 150),
        ]);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].count, 3);
        assert_eq!(merged[0].message_id, "m2");
        assert_eq!(merged[0].received_at, 200);
    }

    #[test]
    fn keeps_separate_conversations_and_orders_by_newest() {
        let merged = merge_notifications(vec![
            pending("t1", "m1", 100),
            pending("t2", "m2", 300),
            pending("t3", "m3", 200),
        ]);

        let ids: Vec<&str> = merged.iter().map(|g| g.message_id.as_str()).collect();
        assert_eq!(ids, vec!["m2", "m3", "m1"]);
    }

    #[test]
    fn caps_the_number_of_visible_cards() {
        let items: Vec<PendingNotification> = (0..6)
            .map(|i| pending(&format!("t{i}"), &format!("m{i}"), 100 + i as i64))
            .collect();

        let merged = merge_notifications(items);
        assert_eq!(merged.len(), MAX_CARDS);
        assert_eq!(merged[0].message_id, "m5");
    }

    #[test]
    fn falls_back_to_message_identity_when_a_thread_is_missing() {
        let merged = merge_notifications(vec![
            pending("", "m1", 100),
            pending("", "m2", 200),
            pending("", "m1", 300),
        ]);

        assert_eq!(merged.len(), 2);
        let m1 = merged.iter().find(|g| g.message_id == "m1").unwrap();
        assert_eq!(m1.count, 2);
    }

    #[test]
    fn a_single_message_reports_no_extra_count_in_the_body() {
        let group = NotificationGroup {
            id: "k".to_string(),
            account_id: "acc".to_string(),
            thread_id: "t".to_string(),
            message_id: "m".to_string(),
            sender: "Ada".to_string(),
            subject: "Hello".to_string(),
            snippet: String::new(),
            received_at: 1,
            count: 1,
        };
        assert_eq!(body_for(&group), "Ada: Hello");

        let group = NotificationGroup { count: 3, ..group };
        assert_eq!(body_for(&group), "Ada: Hello (+2)");
    }
}
