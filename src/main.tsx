import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";
import { queryClient } from "@/lib/query-client";
import { initializeProfileStorageNamespace } from "@/lib/profileStorage";
import { logStartupTiming } from "@/lib/startupTiming";
import "./styles/index.css";

async function bootstrap() {
  logStartupTiming("frontend entry loaded");
  await initializeProfileStorageNamespace();
  await import("@/lib/i18n");

  // The custom new-mail popup is a second, frameless window that loads the same
  // bundle. It must not boot the full app (or ask to show the main window), and
  // the full-screen splash baked into index.html has to go: this window is
  // transparent and only as tall as its cards.
  if (window.location.hash === "#notification") {
    document.getElementById("splash")?.remove();
    document.getElementById("splash-style")?.remove();
    const { default: NotificationWindow } = await import("./notification/NotificationWindow");
    createRoot(document.getElementById("root")!).render(
      <StrictMode>
        <NotificationWindow />
      </StrictMode>,
    );
    return;
  }

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
