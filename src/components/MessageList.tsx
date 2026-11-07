import { useRef, useMemo, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useQuery } from "@tanstack/react-query";
import { Inbox } from "lucide-react";
import type { MessageSummary } from "@/lib/api";
import { getMessageLabelsBatch } from "@/lib/api";
import { useAccountsQuery, useFoldersForAccountsQuery } from "@/hooks/queries";
import { useMailStore } from "@/stores/mail.store";
import { roleForSelection } from "@/lib/folderAggregation";
import { assignAccountColors, getAccountColor, getAccountLabel } from "@/lib/accountColors";
import MessageItem from "./MessageItem";
import { MessageListSkeleton } from "./Skeleton";

interface Props {
  messages: MessageSummary[];
  selectedMessageId: string | null;
  onSelectMessage: (id: string) => void;
  loading: boolean;
  onToggleStar?: (messageId: string, newStarred: boolean) => void;
  onLoadMore?: () => void;
  hasNextPage?: boolean;
  isFetchingNextPage?: boolean;
}

/**
 * Day buckets used to section the list, the way Apple Mail and most modern
 * clients do. Mail from today is what users scan first, so the newest bucket
 * gets a label instead of repeating a date on every row.
 */
type DayBucket = "today" | "yesterday" | "thisWeek" | "earlier";

const DAY_BUCKET_KEYS: Record<DayBucket, string> = {
  today: "inbox.groupToday",
  yesterday: "inbox.groupYesterday",
  thisWeek: "inbox.groupThisWeek",
  earlier: "inbox.groupEarlier",
};

const DAY_BUCKET_FALLBACKS: Record<DayBucket, string> = {
  today: "Today",
  yesterday: "Yesterday",
  thisWeek: "This week",
  earlier: "Earlier",
};

const DAY_MS = 24 * 60 * 60 * 1000;

function dayBucket(timestamp: number, now: Date): DayBucket {
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const value = timestamp * 1000;
  if (value >= startOfToday) return "today";
  if (value >= startOfToday - DAY_MS) return "yesterday";
  // Monday-based week start, matching the section label "This week".
  const weekdayIndex = (now.getDay() + 6) % 7;
  const startOfWeek = startOfToday - weekdayIndex * DAY_MS;
  if (value >= startOfWeek) return "thisWeek";
  return "earlier";
}

type ListEntry =
  | { kind: "header"; bucket: DayBucket }
  | { kind: "message"; message: MessageSummary };

function buildListEntries(messages: MessageSummary[], now: Date): ListEntry[] {
  const entries: ListEntry[] = [];
  let currentBucket: DayBucket | null = null;

  for (const message of messages) {
    const bucket = dayBucket(message.date, now);
    if (bucket !== currentBucket) {
      entries.push({ kind: "header", bucket });
      currentBucket = bucket;
    }
    entries.push({ kind: "message", message });
  }

  return entries;
}

export default function MessageList({
  messages,
  selectedMessageId,
  onSelectMessage,
  loading,
  onToggleStar,
  onLoadMore,
  hasNextPage = false,
  isFetchingNextPage = false,
}: Props) {
  const { t } = useTranslation();
  const parentRef = useRef<HTMLDivElement>(null);
  const activeAccountId = useMailStore((s) => s.activeAccountId);
  const activeFolderId = useMailStore((s) => s.activeFolderId);
  const { data: accounts = [] } = useAccountsQuery();
  const accountsById = useMemo(
    () => new Map(accounts.map((account) => [account.id, account])),
    [accounts],
  );
  const accountColorsById = useMemo(() => assignAccountColors(accounts), [accounts]);
  const showAccountColorMarkers = !activeAccountId && accounts.length > 1;
  const folderAccountIds = useMemo(
    () => activeAccountId ? [activeAccountId] : accounts.map((account) => account.id),
    [accounts, activeAccountId],
  );
  const { data: folders = [] } = useFoldersForAccountsQuery(folderAccountIds);
  // Offer spam action only when NOT already viewing the spam folder
  const activeFolderRole = roleForSelection(activeFolderId, folders);
  const spamFolder = activeAccountId ? folders.find((f) => f.account_id === activeAccountId && f.role === "spam") : undefined;
  const spamFolderId = activeFolderRole !== "spam" ? spamFolder?.id : undefined;
  const messageIds = useMemo(() => messages.map((m) => m.id), [messages]);
  const messageIdsKey = useMemo(() => messageIds.join(","), [messageIds]);
  const { data: labelsByMessage = {} } = useQuery({
    queryKey: ["message-labels", messageIdsKey],
    queryFn: () => getMessageLabelsBatch(messageIds),
    staleTime: 60_000,
    enabled: messageIds.length > 0,
  });

  const entries = useMemo(() => buildListEntries(messages, new Date()), [messages]);

  const virtualizer = useVirtualizer({
    count: entries.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (index) => (entries[index]?.kind === "header" ? 30 : 68),
    measureElement: (el) => el.getBoundingClientRect().height,
    overscan: 5,
  });

  // Scroll selected message into view on keyboard navigation
  useEffect(() => {
    if (!selectedMessageId) return;
    const idx = entries.findIndex(
      (entry) => entry.kind === "message" && entry.message.id === selectedMessageId,
    );
    if (idx >= 0) {
      virtualizer.scrollToIndex(idx, { align: "auto" });
    }
  }, [selectedMessageId, entries, virtualizer]);

  if (loading) {
    return <MessageListSkeleton />;
  }

  if (messages.length === 0) {
    return (
      <div
        className="fade-in"
        style={{
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          height: "100%",
          color: "var(--color-text-secondary)",
          fontSize: "14px",
          gap: "8px",
        }}
      >
        <Inbox size={32} strokeWidth={1.2} />
        {t("common.noMessages")}
      </div>
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <div
        ref={parentRef}
        className="scroll-region message-list-scroll"
        role="listbox"
        aria-label={t("inbox.messageList", "Messages")}
        style={{ flex: 1, overflow: "auto" }}
      >
        <div
          style={{
            height: `${virtualizer.getTotalSize()}px`,
            position: "relative",
          }}
        >
          {virtualizer.getVirtualItems().map((virtualItem) => {
            const entry = entries[virtualItem.index];
            if (!entry) return null;

            if (entry.kind === "header") {
              return (
                <div
                  key={virtualItem.key}
                  ref={virtualizer.measureElement}
                  data-index={virtualItem.index}
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    transform: `translateY(${virtualItem.start}px)`,
                  }}
                >
                  <div className="message-list-section-label">
                    {t(DAY_BUCKET_KEYS[entry.bucket], DAY_BUCKET_FALLBACKS[entry.bucket])}
                  </div>
                </div>
              );
            }

            const message = entry.message;
            const account = accountsById.get(message.account_id);
            return (
              <div
                key={virtualItem.key}
                ref={virtualizer.measureElement}
                data-index={virtualItem.index}
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  transform: `translateY(${virtualItem.start}px)`,
                }}
              >
                <MessageItem
                  message={message}
                  labels={labelsByMessage[message.id] ?? []}
                  isSelected={message.id === selectedMessageId}
                  onClick={() => onSelectMessage(message.id)}
                  onToggleStar={onToggleStar}
                  spamFolderId={spamFolderId}
                  folderRole={activeFolderRole}
                  accountColor={
                    showAccountColorMarkers
                      ? accountColorsById.get(message.account_id) ?? getAccountColor(account, message.account_id)
                      : undefined
                  }
                  accountLabel={showAccountColorMarkers ? getAccountLabel(account, message.account_id) : undefined}
                />
              </div>
            );
          })}
        </div>
        {onLoadMore && hasNextPage && (
          <div style={{ padding: "12px", textAlign: "center" }}>
            <button
              onClick={() => {
                if (!isFetchingNextPage) onLoadMore();
              }}
              disabled={isFetchingNextPage}
              style={{
                padding: "6px 20px",
                fontSize: "13px",
                border: "1px solid var(--color-border)",
                borderRadius: "6px",
                background: "transparent",
                color: "var(--color-text-secondary)",
                cursor: isFetchingNextPage ? "default" : "pointer",
                opacity: isFetchingNextPage ? 0.7 : 1,
              }}
            >
              {isFetchingNextPage ? t("common.loading", "Loading...") : t("common.loadMore", "Load more")}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
