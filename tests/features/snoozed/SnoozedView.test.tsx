import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Message, SnoozedMessage } from "../../../src/lib/api";
import { useMailStore } from "../../../src/stores/mail.store";
import { useUIStore } from "../../../src/stores/ui.store";

const mocks = vi.hoisted(() => ({
  listSnoozed: vi.fn<() => Promise<SnoozedMessage[]>>(),
  getMessagesBatch: vi.fn<(ids: string[]) => Promise<Message[]>>(),
  unsnoozeMessage: vi.fn<(messageId: string) => Promise<void>>(),
}));

vi.mock("react-i18next", () => ({
  initReactI18next: {
    type: "3rdParty",
    init: vi.fn(),
  },
  useTranslation: () => ({
    t: (_key: string, fallback?: string) => fallback ?? _key,
  }),
}));

vi.mock("../../../src/lib/api", () => ({
  listSnoozed: mocks.listSnoozed,
  getMessagesBatch: mocks.getMessagesBatch,
  unsnoozeMessage: mocks.unsnoozeMessage,
}));

vi.mock("../../../src/stores/toast.store", () => ({
  useToastStore: {
    getState: () => ({ addToast: vi.fn() }),
  },
}));

import SnoozedView from "../../../src/features/snoozed/SnoozedView";

function makeMessage(id: string): Message {
  return {
    id,
    account_id: "account-1",
    remote_id: `remote-${id}`,
    message_id_header: null,
    in_reply_to: null,
    references_header: null,
    thread_id: null,
    subject: "Snoozed subject",
    snippet: "Snoozed snippet",
    from_address: "sender@example.com",
    from_name: "Sender",
    to_list: [],
    cc_list: [],
    bcc_list: [],
    has_attachments: false,
    is_read: true,
    is_starred: false,
    is_draft: false,
    date: 1_700_000_000,
    remote_version: null,
    is_deleted: false,
    deleted_at: null,
    created_at: 1_700_000_000,
    updated_at: 1_700_000_000,
    body_text: "Body",
    body_html_raw: "",
  };
}

describe("SnoozedView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.listSnoozed.mockResolvedValue([{
      message_id: "message-2",
      snoozed_at: 1_700_000_000,
      unsnoozed_at: 1_700_003_600,
      return_to: "inbox",
    }]);
    mocks.getMessagesBatch.mockResolvedValue([makeMessage("message-2")]);
    mocks.unsnoozeMessage.mockResolvedValue();
    useUIStore.setState({ activeView: "snoozed" });
    useMailStore.setState({ selectedMessageId: null });
  });

  it("opens a snoozed message in the inbox reading pane", async () => {
    render(<SnoozedView />);

    fireEvent.click(await screen.findByText("Snoozed subject"));

    expect(useUIStore.getState().activeView).toBe("inbox");
    expect(useMailStore.getState().selectedMessageId).toBe("message-2");
  });
});
