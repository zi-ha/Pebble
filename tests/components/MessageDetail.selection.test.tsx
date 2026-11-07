import { createEvent, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import MessageDetail from "../../src/components/MessageDetail";
import type { Message } from "../../src/lib/api";
import { trustSender } from "../../src/lib/api";

const privacyMocks = vi.hoisted(() => ({
  calls: [] as Array<{ messageId: string; privacyMode: unknown }>,
}));
const trustSenderMock = vi.mocked(trustSender);

const mockMessage: Message = {
  id: "message-1",
  account_id: "account-1",
  remote_id: "remote-1",
  message_id_header: null,
  in_reply_to: null,
  references_header: null,
  thread_id: null,
  subject: "Context actions",
  snippet: "Selected text action test",
  from_address: "sender@example.com",
  from_name: "Sender",
  to_list: [{ name: "Destination", address: "destination@example.com" }],
  cc_list: [{ name: "Copied teammate", address: "cc@example.com" }],
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
  body_text: "selected email text inside the message body",
  body_html_raw: "",
};

const bulkMessage: Message = {
  ...mockMessage,
  id: "message-bulk",
  to_list: Array.from({ length: 10 }, (_, index) => ({ name: `Bulk ${index}`, address: `bulk${index}@example.com` })),
  cc_list: [],
};

vi.mock("react-i18next", () => ({
  initReactI18next: {
    type: "3rdParty",
    init: vi.fn(),
  },
  useTranslation: () => ({
    t: (_key: string, options?: unknown) => {
      if (typeof options === "string") return options;
      const opts = (options ?? {}) as Record<string, string | number>;
      const template = typeof opts.defaultValue === "string" ? opts.defaultValue : _key;
      return template.replace(/\{\{(\w+)\}\}/g, (_match, name) => String(opts[name] ?? ""));
    },
  }),
}));

vi.mock("../../src/lib/api", () => ({
  trustSender: vi.fn(),
}));

vi.mock("../../src/hooks/useMessageLoader", () => ({
  useMessageLoader: (messageId: string, privacyMode: unknown) => {
    privacyMocks.calls.push({ messageId, privacyMode });
    return {
    message: {
      ...(messageId === "message-bulk" ? bulkMessage : mockMessage),
      id: messageId,
      has_attachments: messageId === "message-with-attachments",
      from_address: messageId === "message-1" ? "sender@example.com" : "second@example.com",
    },
    setMessage: vi.fn(),
    rendered: { html: "<p>Body</p>", images_blocked: 1, trackers_blocked: [] },
    loading: false,
    };
  },
}));

vi.mock("../../src/hooks/queries", () => ({
  useAccountsQuery: () => ({
    data: [{ id: "account-1", email: "current@example.com" }],
  }),
}));

vi.mock("../../src/hooks/useBilingualTranslation", () => ({
  useBilingualTranslation: () => ({
    bilingualMode: false,
    bilingualResult: null,
    bilingualLoading: false,
    handleBilingualToggle: vi.fn(),
    resetBilingual: vi.fn(),
  }),
}));

vi.mock("../../src/components/MessageActionToolbar", () => ({
  default: () => <div>message actions</div>,
}));

vi.mock("../../src/components/AttachmentList", () => ({
  default: () => <div>attachments</div>,
}));

vi.mock("../../src/components/PrivacyBanner", () => ({
  default: ({ onTrustSender }: { onTrustSender: (trustType: "all") => void }) => (
    <button onClick={() => onTrustSender("all")}>trust sender</button>
  ),
}));

vi.mock("../../src/features/inbox/SnoozePopover", () => ({
  default: () => <div>snooze</div>,
}));

vi.mock("../../src/features/translate/TranslatePopover", () => ({
  default: () => <div>translate popover</div>,
}));

vi.mock("../../src/components/ShadowDomEmail", () => ({
  ShadowDomEmail: ({ html }: { html: string }) => <div>{html}</div>,
}));

vi.mock("../../src/components/ContactAddressAction", () => ({
  default: ({ address }: { address: string }) => (
    <span data-testid="contact-address-action">{address}</span>
  ),
}));

function setSelectedText(text: string) {
  Object.defineProperty(window, "getSelection", {
    configurable: true,
    value: () => ({
      toString: () => text,
      rangeCount: text ? 1 : 0,
      getRangeAt: () => ({
        getBoundingClientRect: () => ({
          left: 80,
          bottom: 120,
          width: 40,
          height: 14,
        }),
      }),
    }),
  });
}

describe("MessageDetail selected-text context actions", () => {
  beforeEach(() => {
    setSelectedText("");
    privacyMocks.calls = [];
    trustSenderMock.mockReset();
    trustSenderMock.mockResolvedValue(undefined);
  });

  it("opens selected-text actions from right click, not from selection alone", () => {
    render(<MessageDetail messageId="message-1" onBack={vi.fn()} />);
    setSelectedText("selected email text");
    const body = screen.getByRole("region", { name: "Message body" });

    fireEvent.mouseUp(body, { clientX: 100, clientY: 120 });

    expect(screen.queryByRole("toolbar", { name: "Selected text actions" })).toBeNull();

    const contextMenu = createEvent.contextMenu(body, { clientX: 100, clientY: 120 });
    const preventDefault = vi.spyOn(contextMenu, "preventDefault");
    fireEvent(body, contextMenu);

    expect(preventDefault).toHaveBeenCalled();
    expect(screen.getByRole("toolbar", { name: "Selected text actions" })).toBeTruthy();
  });

  it("does not suppress the keyboard focus outline on the message body", () => {
    render(<MessageDetail messageId="message-1" onBack={vi.fn()} />);

    const body = screen.getByRole("region", { name: "Message body" });

    expect(body.getAttribute("style")).not.toContain("outline: none");
  });

  it("uses the shared smooth scroll region for the message body", () => {
    render(<MessageDetail messageId="message-1" onBack={vi.fn()} />);

    const body = screen.getByRole("region", { name: "Message body" });

    expect(body.className).toContain("scroll-region");
    expect(body.className).toContain("message-body-scroll");
  });

  it("keeps attachments at the end of the message body instead of docking them below the pane", () => {
    render(<MessageDetail messageId="message-with-attachments" onBack={vi.fn()} />);

    const body = screen.getByRole("region", { name: "Message body" });
    const attachments = screen.getByText("attachments");

    // Attachments scroll with the text, so they must be inside the body region
    // and be its last block — not a sibling taking fixed height from the pane.
    expect(body.contains(attachments)).toBe(true);
    expect(body.lastElementChild).toBe(attachments);
  });

  it("does not render a back button in the reading pane header", () => {
    render(<MessageDetail messageId="message-1" onBack={vi.fn()} />);

    expect(screen.queryByRole("button", { name: "Back" })).toBeNull();
    expect(screen.queryByRole("button", { name: "compose.back" })).toBeNull();
  });

  it("shows message recipients instead of the account email in the header", () => {
    render(<MessageDetail messageId="message-1" onBack={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: "Show message details" }));

    expect(screen.getAllByText(/destination@example\.com/).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/cc@example\.com/).length).toBeGreaterThan(0);
    expect(screen.queryByText(/current@example\.com/)).toBeNull();
  });

  it("keeps the sender line to one row and expands the details on demand", () => {
    render(<MessageDetail messageId="message-1" onBack={vi.fn()} />);

    const heading = screen.getByRole("heading", { name: "Context actions" });
    expect(heading.className).toContain("message-detail-subject");

    expect(screen.getByText("Sender")).toBeTruthy();
    expect(screen.getByText(/2023/)).toBeTruthy();

    // Recipients stay collapsed so the message body gets the vertical space.
    const toggle = screen.getByRole("button", { name: "Show message details" });
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByText(/destination@example\.com/)).toBeNull();

    fireEvent.click(toggle);

    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getAllByText(/destination@example\.com/).length).toBeGreaterThan(0);
  });

  it("puts every action on one row with the view toggles pinned right, above the subject", () => {
    render(<MessageDetail messageId="message-1" onBack={vi.fn()} />);

    const actionRow = document.querySelector(".message-detail-actions") as HTMLElement;
    const rightGroup = document.querySelector(".message-detail-actions-right") as HTMLElement;
    const heading = screen.getByRole("heading", { name: "Context actions" });

    expect(actionRow).toBeTruthy();
    expect(rightGroup).toBeTruthy();

    // Message actions stay on the left of the row.
    const toolbar = screen.getByText("message actions");
    expect(actionRow.contains(toolbar)).toBe(true);
    expect(rightGroup.contains(toolbar)).toBe(false);
    expect(
      toolbar.compareDocumentPosition(rightGroup) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();

    // The former right-aligned toggles keep the right edge.
    expect(rightGroup.contains(screen.getByRole("button", { name: "Snooze message" }))).toBe(true);
    expect(rightGroup.contains(screen.getByRole("button", { name: "Toggle bilingual view" }))).toBe(true);

    // The subject moved below the action row.
    expect(actionRow.contains(heading)).toBe(false);
    expect(
      actionRow.compareDocumentPosition(heading) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("offers contact actions for the sender and each visible recipient", () => {
    render(<MessageDetail messageId="message-1" onBack={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: "Show message details" }));

    expect(screen.getAllByTestId("contact-address-action").map((node) => node.textContent)).toEqual([
      "sender@example.com",
      "destination@example.com",
      "cc@example.com",
    ]);
  });

  it("collapses bulk recipient lists behind a more toggle", () => {
    render(<MessageDetail messageId="message-bulk" onBack={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: "Show message details" }));

    expect(screen.getAllByTestId("contact-address-action")).toHaveLength(4);
    expect(screen.getByRole("button", { name: "+7 more" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "+7 more" }));

    expect(screen.getAllByTestId("contact-address-action")).toHaveLength(11);
    expect(screen.getByRole("button", { name: "Show less" })).toBeTruthy();
  });

  it("does not carry a sender trust override to the next message", async () => {
    const { rerender } = render(<MessageDetail messageId="message-1" onBack={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "trust sender" }));

    await waitFor(() => {
      expect(privacyMocks.calls.at(-1)).toEqual({
        messageId: "message-1",
        privacyMode: { TrustSender: "sender@example.com" },
      });
    });

    rerender(<MessageDetail messageId="message-2" onBack={vi.fn()} />);

    expect(privacyMocks.calls.at(-1)).toEqual({
      messageId: "message-2",
      privacyMode: "LoadOnce",
    });

    rerender(<MessageDetail messageId="message-1" onBack={vi.fn()} />);

    expect(privacyMocks.calls.at(-1)).toEqual({
      messageId: "message-1",
      privacyMode: "LoadOnce",
    });
  });

  it("does not relax privacy when persisting sender trust fails", async () => {
    trustSenderMock.mockRejectedValueOnce(new Error("write failed"));
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);
    render(<MessageDetail messageId="message-1" onBack={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: "trust sender" }));
    await waitFor(() => expect(trustSenderMock).toHaveBeenCalledOnce());

    expect(privacyMocks.calls.at(-1)).toEqual({
      messageId: "message-1",
      privacyMode: "LoadOnce",
    });
    consoleError.mockRestore();
  });
});
