import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

function read(...parts: string[]) {
  return readFileSync(resolve(process.cwd(), ...parts), "utf8").replace(/\r\n/g, "\n");
}

const notificationSource = read("src-tauri", "src", "commands", "notification_center.rs");
const indexingSource = read("src-tauri", "src", "commands", "indexing.rs");
const mainSource = read("src", "main.tsx");
const popupSource = read("src", "notification", "NotificationWindow.tsx");

describe("new mail popup", () => {
  it("draws its own window on Windows instead of the system toast", () => {
    // The Windows path used to hand the message to tauri-winrt-notification.
    // It now renders a frameless always-on-top window, so the OS toast (and its
    // AUMID registration) must stay out of the picture.
    expect(notificationSource).not.toContain("tauri_winrt_notification");
    expect(notificationSource).toContain('const WINDOW_LABEL: &str = "notification"');
    expect(notificationSource).toContain("index.html#notification");
    expect(notificationSource).toContain(".decorations(false)");
    expect(notificationSource).toContain(".transparent(true)");
    expect(notificationSource).toContain(".always_on_top(true)");
    expect(notificationSource).toContain(".skip_taskbar(true)");
  });

  it("coalesces a sync burst into a few per-thread cards", () => {
    expect(notificationSource).toContain("pub const DEBOUNCE_MS: u64 = 700");
    expect(notificationSource).toContain("pub const MAX_CARDS: usize = 3");
    expect(notificationSource).toContain("pub fn merge_notifications(");
    expect(notificationSource).toContain("group_key");
  });

  it("stays quiet while the user is looking at the app", () => {
    expect(notificationSource).toContain("fn main_window_is_active(");
    expect(notificationSource).toContain("if main_window_is_active(app) {");
    expect(notificationSource).toContain("dismiss_all");
  });

  it("docks the window flush against the bottom-right corner of the work area", () => {
    // The work area (not the raw monitor) is what keeps the panel above a
    // bottom taskbar; without it "the corner" is off-screen.
    expect(notificationSource).toContain("monitor.work_area()");
    expect(notificationSource).toContain("corner_x - POPUP_WIDTH");
    expect(notificationSource).toContain("corner_y - height");
    // A margin would float the panel away from the corner again.
    expect(notificationSource).not.toMatch(/POPUP_MARGIN_(RIGHT|BOTTOM)\s*:/);
  });

  it("keeps the cards still instead of lifting them on hover", () => {
    expect(popupSource).not.toContain("translateY");
    expect(popupSource).not.toMatch(/pebble-notify-item:hover\s*\{[^}]*transform/);
  });

  it("takes its colours from the app palette so dark theme carries over", () => {
    expect(popupSource).toContain("applyStoredTheme");
    expect(popupSource).toContain('endsWith(":pebble-theme")');
    expect(popupSource).toContain("var(--color-main-bg)");
    expect(popupSource).not.toMatch(/background:\s*#(fff|ffffff)\b/i);
  });

  it("never paints the app splash inside the popup window", () => {
    const html = read("index.html");

    // The static markup is enough to show a spinner, so it has to be switched
    // off from `index.html` itself rather than by the bundle that may not boot.
    expect(html).toContain("location.hash==='#notification'");
    expect(html).toContain("notification-popup");
    expect(html).toContain('html[data-window="notification-popup"] #splash');

    // And the bundle removes the node before awaiting anything.
    const popupBranchAt = mainSource.indexOf("function renderNotificationPopup()");
    expect(popupBranchAt).toBeGreaterThan(-1);
    const removeAt = mainSource.indexOf('document.getElementById("splash")?.remove()', popupBranchAt);
    const awaitAt = mainSource.indexOf("await import(", popupBranchAt);
    expect(removeAt).toBeGreaterThan(popupBranchAt);
    expect(removeAt).toBeLessThan(awaitAt);
    // No IPC round-trip stands between the popup window opening and its cards.
    const popupBody = mainSource.slice(popupBranchAt, mainSource.indexOf("async function bootstrap()"));
    expect(popupBody).not.toContain("initializeProfileStorageNamespace()");
  });

  it("queues notifications only after the rules had their say", () => {
    const evaluateAt = indexingSource.indexOf("engine.evaluate(&stored.message)");
    const enqueueAt = indexingSource.indexOf("notification_center::enqueue");

    expect(evaluateAt).toBeGreaterThan(-1);
    expect(enqueueAt).toBeGreaterThan(-1);
    expect(evaluateAt).toBeLessThan(enqueueAt);
    expect(indexingSource).toContain("silences_notifications");
  });

  it("loads the popup from the shared bundle without booting the app", () => {
    expect(mainSource).toContain('window.location.hash === "#notification"');
    expect(mainSource).toContain("NotificationWindow");
    // The popup must not ask the backend to reveal the main window.
    const popupBranch = mainSource.slice(0, mainSource.indexOf('window.location.hash === "#notification"'));
    expect(popupBranch).not.toContain("showMainWindow()");
    // index.html carries a full-screen splash; a transparent card window must
    // not render it.
    expect(mainSource).toContain('document.getElementById("splash")?.remove()');
  });

  it("grants the popup window its own capability", () => {
    const capabilityPath = resolve(process.cwd(), "src-tauri", "capabilities", "notification.json");
    expect(existsSync(capabilityPath)).toBe(true);

    const capability = JSON.parse(readFileSync(capabilityPath, "utf8"));
    expect(capability.windows).toEqual(["notification"]);

    const mainCapability = JSON.parse(
      readFileSync(resolve(process.cwd(), "src-tauri", "capabilities", "default.json"), "utf8"),
    );
    expect(mainCapability.windows).toEqual(["main"]);
  });
});
