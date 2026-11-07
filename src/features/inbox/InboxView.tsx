import { useMailStore } from "@/stores/mail.store";
import { useAccountsQuery, useMessagesQuery, useFoldersForAccountsQuery, patchMessagesCache } from "@/hooks/queries";
import { useUIStore } from "@/stores/ui.store";
import { useToastStore } from "@/stores/toast.store";
import MessageList from "@/components/MessageList";
import MessageDetail from "@/components/MessageDetail";
import MessageDetailEmpty from "@/components/MessageDetailEmpty";
import { useState, useCallback, useMemo } from "react";
import ConfirmDialog from "@/components/ConfirmDialog";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { Mail, Trash2 } from "lucide-react";
import { emptyTrash } from "@/lib/api";
import { folderIdsForSelection, roleForSelection } from "@/lib/folderAggregation";

export default function InboxView() {
  const { t } = useTranslation();
  const setActiveView = useUIStore((s) => s.setActiveView);
  const activeFolderId = useMailStore((s) => s.activeFolderId);
  const activeAccountId = useMailStore((s) => s.activeAccountId);
  const selectedMessageId = useMailStore((s) => s.selectedMessageId);
  const setSelectedMessage = useMailStore((s) => s.setSelectedMessage);
  const { data: accounts = [] } = useAccountsQuery();
  const folderAccountIds = useMemo(
    () => activeAccountId ? [activeAccountId] : accounts.map((account) => account.id),
    [accounts, activeAccountId],
  );
  const { data: folders = [] } = useFoldersForAccountsQuery(folderAccountIds);
  const queryClient = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);
  const [showTrashConfirm, setShowTrashConfirm] = useState(false);

  const activeFolderRole = roleForSelection(activeFolderId, folders);
  const isTrashFolder = activeFolderRole === "trash";
  const selectedFolderIds = folderIdsForSelection(activeFolderId, folders);
  const queryFolderId = selectedFolderIds[0] ?? null;
  const queryFolderIds = selectedFolderIds.length > 1 ? selectedFolderIds : undefined;

  const {
    data: messages,
    isLoading: loadingMessages,
    hasNextPage,
    isFetchingNextPage,
    fetchNextPage,
  } = useMessagesQuery(queryFolderId, queryFolderIds);

  const handleLoadMore = useCallback(() => {
    if (hasNextPage && !isFetchingNextPage) fetchNextPage();
  }, [hasNextPage, isFetchingNextPage, fetchNextPage]);
  const handleToggleStar = useCallback((messageId: string, newStarred: boolean) => {
    patchMessagesCache(queryClient, (page) =>
      page.map((m) => (m.id === messageId ? { ...m, is_starred: newStarred } : m)),
    );
  }, [queryClient]);

  // No accounts or no folder selected — show welcome / setup prompt
  if (accounts.length === 0 || !activeFolderId) {
    return (
      <div className="fade-in" style={{
        display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
        height: "100%", gap: "16px", color: "var(--color-text-secondary)",
      }}>
        <Mail size={48} strokeWidth={1.2} />
        <p style={{ fontSize: "16px", fontWeight: 500, color: "var(--color-text-primary)", margin: 0 }}>
          {t("inbox.welcome", "Welcome to Pebble")}
        </p>
        <p style={{ fontSize: "13px", margin: 0 }}>
          {t("inbox.addAccountHint", "Add an email account to get started")}
        </p>
        <button
          onClick={() => setActiveView("settings")}
          style={{
            marginTop: "8px", padding: "8px 20px", borderRadius: "6px",
            border: "none", backgroundColor: "var(--color-accent)", color: "#fff",
            fontSize: "13px", fontWeight: 600, cursor: "pointer",
          }}
        >
          {t("settings.addAccount", "Add Account")}
        </button>
      </div>
    );
  }

  return (
    <div className="mail-panes">
      {/* Column 2 — always present, fixed width */}
      <div className="mail-list-pane">
        {isTrashFolder && messages.length > 0 && (
          <button
            type="button"
            onClick={() => setShowTrashConfirm(true)}
            className="mail-list-floating-action"
            title={t("messageActions.emptyTrash", "Empty Trash")}
            aria-label={t("messageActions.emptyTrash", "Empty Trash")}
          >
            <Trash2 size={15} />
          </button>
        )}

        <MessageList
          messages={messages}
          selectedMessageId={selectedMessageId}
          onSelectMessage={setSelectedMessage}
          loading={loadingMessages}
          onLoadMore={handleLoadMore}
          hasNextPage={hasNextPage}
          isFetchingNextPage={isFetchingNextPage}
          onToggleStar={handleToggleStar}
        />
      </div>

      {/* Column 3 — always present */}
      <div className="mail-detail-pane">
        {selectedMessageId ? (
          <MessageDetail
            messageId={selectedMessageId}
            onBack={() => setSelectedMessage(null)}
            folderRole={activeFolderRole}
          />
        ) : (
          <MessageDetailEmpty />
        )}
      </div>

      {showTrashConfirm && (
        <ConfirmDialog
          title={t("messageActions.emptyTrash", "Empty Trash")}
          message={t("messageActions.emptyTrashConfirm", "Permanently delete all messages in Trash?")}
          destructive
          onCancel={() => setShowTrashConfirm(false)}
          onConfirm={async () => {
            setShowTrashConfirm(false);
            try {
              const targetAccountIds = activeAccountId ? [activeAccountId] : accounts.map((account) => account.id);
              const counts = await Promise.all(targetAccountIds.map((accountId) => emptyTrash(accountId)));
              const count = counts.reduce((sum, current) => sum + current, 0);
              queryClient.invalidateQueries({ queryKey: ["messages"] });
              addToast({ message: t("messageActions.emptyTrashSuccess", { count }), type: "success" });
            } catch {
              addToast({ message: t("messageActions.emptyTrashFailed"), type: "error" });
            }
          }}
        />
      )}
    </div>
  );
}
