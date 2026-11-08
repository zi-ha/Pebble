import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import {
  dismissAllNotifications,
  dismissNotification,
  getNotificationCards,
  openNotificationMessage,
  setNotificationPopupHeight,
  type NotificationCard,
} from "@/lib/api";

/** How long a card stays before it slides away on its own. */
const AUTO_DISMISS_MS = 9000;

const AVATAR_COLORS = ["#2563eb", "#0f766e", "#b45309", "#7c3aed", "#be123c", "#0369a1"];

function avatarColor(accountId: string): string {
  let hash = 0;
  for (let i = 0; i < accountId.length; i += 1) {
    hash = (hash * 31 + accountId.charCodeAt(i)) % 997;
  }
  return AVATAR_COLORS[hash % AVATAR_COLORS.length];
}

function initials(card: NotificationCard): string {
  const source = card.sender.trim() || card.subject.trim();
  return source ? source.slice(0, 1).toUpperCase() : "P";
}

function relativeTime(timestamp: number, now: number): string {
  const seconds = Math.max(0, Math.round(now / 1000 - timestamp));
  if (seconds < 60) return "";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.round(minutes / 60);
  return `${hours}h`;
}

const STYLES = `
  html, body, #root { background: transparent !important; margin: 0; overflow: hidden; }
  .pebble-notify-root {
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 8px;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif;
  }
  .pebble-notify-card {
    position: relative;
    box-sizing: border-box;
    display: flex;
    gap: 12px;
    padding: 12px 14px 12px 12px;
    background: #ffffff;
    border: 1px solid rgba(15, 23, 42, 0.08);
    border-radius: 14px;
    box-shadow: 0 12px 32px rgba(15, 23, 42, 0.18), 0 1px 2px rgba(15, 23, 42, 0.06);
    cursor: pointer;
    text-align: left;
    width: 100%;
    overflow: hidden;
    transition: transform 120ms ease, box-shadow 120ms ease;
  }
  .pebble-notify-card:hover {
    transform: translateY(-1px);
    box-shadow: 0 16px 38px rgba(15, 23, 42, 0.22), 0 1px 2px rgba(15, 23, 42, 0.06);
  }
  .pebble-notify-card:focus-visible {
    outline: 2px solid #2563eb;
    outline-offset: 2px;
  }
  .pebble-notify-avatar {
    flex: 0 0 auto;
    width: 34px;
    height: 34px;
    border-radius: 10px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #ffffff;
    font-size: 15px;
    font-weight: 600;
    letter-spacing: 0.02em;
  }
  .pebble-notify-body { flex: 1 1 auto; min-width: 0; }
  .pebble-notify-head {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin-bottom: 3px;
  }
  .pebble-notify-sender {
    flex: 1 1 auto;
    min-width: 0;
    font-size: 13.5px;
    font-weight: 600;
    color: #0f172a;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pebble-notify-time {
    flex: 0 0 auto;
    font-size: 11.5px;
    color: #94a3b8;
    font-variant-numeric: tabular-nums;
  }
  .pebble-notify-subject {
    font-size: 13px;
    font-weight: 500;
    color: #1e293b;
    line-height: 1.35;
    display: -webkit-box;
    -webkit-line-clamp: 1;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .pebble-notify-snippet {
    margin-top: 3px;
    font-size: 12px;
    color: #64748b;
    line-height: 1.4;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .pebble-notify-count {
    flex: 0 0 auto;
    align-self: flex-start;
    margin-left: 6px;
    padding: 1px 7px;
    border-radius: 999px;
    background: #eff6ff;
    color: #1d4ed8;
    font-size: 11px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .pebble-notify-close {
    position: absolute;
    top: 6px;
    right: 6px;
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: #94a3b8;
    font-size: 15px;
    line-height: 1;
    cursor: pointer;
    opacity: 0;
    transition: opacity 120ms ease, background 120ms ease;
  }
  .pebble-notify-card:hover .pebble-notify-close,
  .pebble-notify-close:focus-visible { opacity: 1; }
  .pebble-notify-close:hover { background: rgba(15, 23, 42, 0.06); color: #334155; }
  .pebble-notify-progress {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 2px;
    width: 100%;
    background: currentColor;
    transform-origin: left center;
    color: #2563eb;
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
  const rootRef = useRef<HTMLDivElement | null>(null);
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

  // Resize the window to whatever the cards actually need.
  useEffect(() => {
    const element = rootRef.current;
    if (!element) return;
    const height = Math.ceil(element.getBoundingClientRect().height) + 16;
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
            className="pebble-notify-card"
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
            <div
              className="pebble-notify-avatar"
              style={{ background: avatarColor(card.account_id) }}
              aria-hidden="true"
            >
              {initials(card)}
            </div>
            <div className="pebble-notify-body">
              <div className="pebble-notify-head">
                <span className="pebble-notify-sender">{card.sender || t("notification.unknownSender")}</span>
                <span className="pebble-notify-time">{time}</span>
                {card.count > 1 && (
                  <span className="pebble-notify-count" aria-label={t("notification.threadCount", { count: card.count })}>
                    +{card.count - 1}
                  </span>
                )}
              </div>
              <div className="pebble-notify-subject">{card.subject || t("notification.noSubject")}</div>
              {card.snippet && <div className="pebble-notify-snippet">{card.snippet}</div>}
            </div>
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
            <span className="pebble-notify-progress" aria-hidden="true" />
          </div>
        );
      }),
    [cards, now, open, dismiss, t],
  );

  return (
    <>
      <style>{STYLES}</style>
      <div
        ref={rootRef}
        className={`pebble-notify-root${paused ? " pebble-notify-paused" : ""}`}
        style={{ "--pebble-dismiss": `${AUTO_DISMISS_MS}ms` } as React.CSSProperties}
      >
        {rendered}
      </div>
    </>
  );
}
