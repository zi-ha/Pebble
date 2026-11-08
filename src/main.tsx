import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";
import { queryClient } from "@/lib/query-client";
import { initializeProfileStorageNamespace } from "@/lib/profileStorage";
import { logStartupTiming } from "@/lib/startupTiming";
import "./styles/index.css";

/**
 * The new-mail popup is a second, frameless window that loads this same bundle.
 * It is identified by its Tauri window label — the URL hash it was opened with
 * is only a fallback, because a window that fails to match would boot the whole
 * app inside a 384px card.
 */
function isNotificationPopup(): boolean {
  const label = (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ as
    | { metadata?: { currentWindow?: { label?: string } } }
    | undefined;
  const windowLabel = label?.metadata?.currentWindow?.label;
  if (windowLabel) return windowLabel === "notification";
  return window.location.hash === "#notification";
}

const POPUP = isNotificationPopup();

function renderNotificationPopup() {
  // Drop the splash before anything else is awaited: this window is a card, and
  // a card that sits on a spinner looks like a hung app. `index.html` already
  // hides it, this removes the node for good.
  document.getElementById("splash")?.remove();
  document.getElementById("splash-style")?.remove();

  void (async () => {
    await import("@/lib/i18n");
    const { default: NotificationWindow } = await import("./notification/NotificationWindow");
    createRoot(document.getElementById("root")!).render(
      <StrictMode>
        <NotificationWindow />
      </StrictMode>,
    );
  })();
}

async function bootstrap() {
  logStartupTiming("frontend entry loaded");

  if (POPUP) {
    // No profile namespace and no `showMainWindow()`: the popup draws from the
    // backend state it is handed and must appear even if that call never lands.
    renderNotificationPopup();
    return;
  }

  await initializeProfileStorageNamespace();
  await import("@/lib/i18n");

  const [{ default: App }, { showMainWindow }] = await Promise.all([
    import("./App"),
    import("@/lib/showMainWindow"),
  ]);

  void showMainWindow();

  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <App />
      </QueryClientProvider>
    </StrictMode>,
  );
}

void bootstrap();
