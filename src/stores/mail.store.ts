import { create } from "zustand";
import { createSingleton } from "./singleton";

interface MailState {
  // ─── UI State ──────────────────────────────────────────────────────────────
  activeAccountId: string | null;
  activeFolderId: string | null;
  selectedMessageId: string | null;

  setActiveAccountId: (accountId: string | null) => void;
  setActiveFolderId: (folderId: string | null) => void;
  setSelectedMessage: (messageId: string | null) => void;
}

export const useMailStore = createSingleton("mail", () =>
  create<MailState>((set) => ({
  activeAccountId: null,
  activeFolderId: null,
  selectedMessageId: null,

  setActiveAccountId: (accountId) => {
    set({
      activeAccountId: accountId,
      activeFolderId: null,
      selectedMessageId: null,
    });
  },

  setActiveFolderId: (folderId) => {
    set({
      activeFolderId: folderId,
      selectedMessageId: null,
    });
  },

  setSelectedMessage: (messageId) => {
    set({ selectedMessageId: messageId });
  },
  })),
);
