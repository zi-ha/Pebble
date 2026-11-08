import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { deriveAccountColor } from "@/lib/accountColors";
import {
  dismissAllNotifications,
  dismissNotification,
  getNotificationCards,
  openNotificationMessage,
  setNotificationPopupHeight,
  type NotificationCard,
} from "@/lib/api";

/** How long a card stays before it fades out on its own. */
const AUTO_DISMISS_MS = 9000;

/**
 * The popup is its own window, so it has to resolve the theme itself: the app
 * store that normally sets `data-theme` lives in the main window. Preferences
 * are stored under a profile-scoped key, and this window deliberately skips the
 * namespace round-trip, so the scoped key is found by suffix.
 */
function storedTheme(): string | null {
  try {
    const direct = localStorage.getItem("pebble-theme");
    if (direct) return direct;
    for (let index = 0; index < localStorage.length; index += 1) {
      const key = localStorage.key(index);
      if (key && key.endsWith(":pebble-theme")) {
        return localStorage.getItem(key);
      }
    }
  } catch {
    return null;
  }
  return null;
}

function applyStoredTheme() {
  const stored = storedTheme();
  const prefersDark = window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
  const resolved = stored === "dark" || stored === "light" ? stored : prefersDark ? "dark" : "light";
  document.documentElement.setAttribute("data-theme", resolved);
}

applyStoredTheme();

function relativeTime(timestamp: number, now: number): string {
  const seconds = Math.max(0, Math.round(now / 1000 - timestamp));
  if (seconds < 60) return "";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.round(minutes / 60);
  return `${hours}h`;
}

// Docked panel: the window sits in the corner of the work area, so only the
// top-left corner is rounded — the other three edges are the screen edge.
const STYLES = `
  html, body, #root { background: transparent !important; margin: 0; overflow: hidden; }
  .pebble-notify-root {
    box-sizing: border-box;
    height: 100%;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif;
  }
  .pebble-notify-panel {
    position: relative;
    box-sizing: border-box;
    width: 100%;
    background: var(--color-main-bg);
    border-top: 1px solid var(--color-border);
    border-left: 1px solid var(--color-border);
    border-radius: 12px 0 0 0;
    overflow: hidden;
  }
  .pebble-notify-item {
    display: flex;
    align-items: flex-start;
    padding: 10px 12px 11px 13px;
    background: var(--color-main-bg);
    cursor: pointer;
    outline: none;
    transition: background-color 110ms ease;
  }
  .pebble-notify-item + .pebble-notify-item { border-top: 1px solid var(--color-border); }
  .pebble-notify-item:hover { background: var(--color-bg-hover); }
  .pebble-notify-item:active { background: var(--color-bg-secondary); }
  .pebble-notify-content { flex: 1 1 auto; min-width: 0; }
  .pebble-notify-head { display: flex; align-items: center; gap: 6px; }
  .pebble-notify-dot {
    flex: 0 0 auto;
    width: 6px;
    height: 6px;
    border-radius: 50%;
  }
  .pebble-notify-sender {
    flex: 1 1 auto;
    min-width: 0;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pebble-notify-count {
    flex: 0 0 auto;
    padding: 0 5px;
    border-radius: 999px;
    background: var(--color-bg-secondary);
    color: var(--color-text-secondary);
    font-size: 10.5px;
    font-weight: 600;
    line-height: 15px;
    font-variant-numeric: tabular-nums;
  }
  .pebble-notify-time {
    flex: 0 0 auto;
    color: var(--color-text-secondary);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .pebble-notify-close {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    margin-right: -3px;
    padding: 0;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--color-text-secondary);
    font-size: 14px;
    line-height: 1;
    cursor: pointer;
    opacity: 0;
    transition: opacity 110ms ease, background-color 110ms ease;
  }
  .pebble-notify-item:hover .pebble-notify-close,
  .pebble-notify-close:focus-visible { opacity: 1; }
  .pebble-notify-close:hover { background: var(--color-bg-secondary); color: var(--color-text-primary); }
  .pebble-notify-subject {
    margin-top: 1px;
    font-size: 12.5px;
    font-weight: 500;
    line-height: 1.35;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pebble-notify-snippet {
    margin-top: 2px;
    font-size: 11.5px;
    line-height: 1.4;
    color: var(--color-text-secondary);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .pebble-notify-progress {
    position: absolute;
    left: 0;
    bottom: 0;
    width: 100%;
    height: 2px;
    background: var(--color-accent);
    opacity: 0.5;
    transform-origin: left center;
  }
  @media (prefers-reduced-motion: no-preference) {
    .pebble-notify-progress { animation: pebble-notify-drain var(--pebble-dismiss, 9s) linear forwards; }
    .pebble-notify-paused .pebble-notify-progress { animation-play-state: paused; }
  }
  @keyframes pebble-notify-drain {
    from { transform: scaleX(1); }
    to { transform: scaleX(0); }
  }
`;

export default function NotificationWindow() {
  const { t } = useTranslation();
  const [cards, setCards] = useState<NotificationCard[]>([]);
  const [paused, setPaused] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const panelRef = useRef<HTMLDivElement | null>(null);
  const lastHeight = useRef(0);
  const timers = useRef<number[]>([]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void getNotificationCards().then((initial) => {
      if (!disposed) setCards(initial ?? []);
    });

    void listen<NotificationCard[]>("mail:notification-groups", (event) => {
      setCards(event.payload ?? []);
    }).then((stop) => {
      if (disposed) {
        stop();
        return;
      }
      unlisten = stop;
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    const clock = window.setInterval(() => setNow(Date.now()), 30_000);
    return () => window.clearInterval(clock);
  }, []);

  const dismiss = useCallback((id: string) => {
    setCards((current) => current.filter((card) => card.id !== id));
    void dismissNotification(id);
  }, []);

  const dismissAll = useCallback(() => {
    setCards([]);
    void dismissAllNotifications();
  }, []);

  const open = useCallback((card: NotificationCard) => {
    setCards([]);
    void openNotificationMessage(card.account_id, card.message_id);
  }, []);

  // Resize the window to whatever the panel actually needs. The panel is
  // bottom-aligned, so being a pixel off never leaves a gap at the corner.
  // Layout effect so the first painted frame is already the right size.
  useLayoutEffect(() => {
    const element = panelRef.current;
    if (!element) return;
    const height = Math.ceil(element.getBoundingClientRect().height) + 1;
    if (Math.abs(height - lastHeight.current) < 1) return;
    lastHeight.current = height;
    void setNotificationPopupHeight(height);
  }, [cards]);

  // Auto-dismiss, paused while the pointer rests on a card.
  useEffect(() => {
    timers.current.forEach((timer) => window.clearTimeout(timer));
    timers.current = [];
    if (paused || cards.length === 0) return;

    cards.forEach((card) => {
      timers.current.push(window.setTimeout(() => dismiss(card.id), AUTO_DISMISS_MS));
    });

    return () => {
      timers.current.forEach((timer) => window.clearTimeout(timer));
      timers.current = [];
    };
  }, [cards, paused, dismiss]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") dismissAll();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [dismissAll]);

  const rendered = useMemo(
    () =>
      cards.map((card) => {
        const time = relativeTime(card.received_at, now);
        return (
          <div
            key={card.id}
            className="pebble-notify-item"
            role="button"
            tabIndex={0}
            aria-label={`${card.sender}: ${card.subject}`}
            onClick={() => open(card)}
            onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                open(card);
              }
            }}
            onMouseEnter={() => setPaused(true)}
            onMouseLeave={() => setPaused(false)}
          >
            <div className="pebble-notify-content">
              <div className="pebble-notify-head">
                <span
                  className="pebble-notify-dot"
                  style={{ background: deriveAccountColor(card.account_id) }}
                  aria-hidden="true"
                />
                <span className="pebble-notify-sender">{card.sender || t("notification.unknownSender")}</span>
                {card.count > 1 && (
                  <span className="pebble-notify-count" aria-label={t("notification.threadCount", { count: card.count })}>
                    +{card.count - 1}
                  </span>
                )}
                <span className="pebble-notify-time">{time}</span>
                <button
                  type="button"
                  className="pebble-notify-close"
                  aria-label={t("notification.dismiss")}
                  onClick={(event) => {
                    event.stopPropagation();
                    dismiss(card.id);
                  }}
                >
                  ×
                </button>
              </div>
              <div className="pebble-notify-subject">{card.subject || t("notification.noSubject")}</div>
              {card.snippet && <div className="pebble-notify-snippet">{card.snippet}</div>}
            </div>
          </div>
        );
      }),
    [cards, now, open, dismiss, t],
  );

  return (
    <>
      <style>{STYLES}</style>
      <div
        className={`pebble-notify-root${paused ? " pebble-notify-paused" : ""}`}
        style={{ "--pebble-dismiss": `${AUTO_DISMISS_MS}ms` } as React.CSSProperties}
      >
        <div className="pebble-notify-panel" ref={panelRef}>
          {rendered}
          <span className="pebble-notify-progress" aria-hidden="true" />
        </div>
      </div>
    </>
  );
}
