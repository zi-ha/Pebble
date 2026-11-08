import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { NotificationCard } from "@/lib/api";

const mocks = vi.hoisted(() => ({
  listener: null as null | ((event: { payload: unknown }) => void),
  getNotificationCards: vi.fn(),
  dismissNotification: vi.fn(() => Promise.resolve()),
  dismissAllNotifications: vi.fn(() => Promise.resolve()),
  openNotificationMessage: vi.fn(() => Promise.resolve()),
  setNotificationPopupHeight: vi.fn(() => Promise.resolve()),
}));

vi.mock("@/lib/api", () => ({
  getNotificationCards: mocks.getNotificationCards,
  dismissNotification: mocks.dismissNotification,
  dismissAllNotifications: mocks.dismissAllNotifications,
  openNotificationMessage: mocks.openNotificationMessage,
  setNotificationPopupHeight: mocks.setNotificationPopupHeight,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((_name: string, handler: (event: { payload: unknown }) => void) => {
    mocks.listener = handler;
    return Promise.resolve(() => {});
  }),
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: { count?: number }) =>
      options?.count === undefined ? key : `${key}:${options.count}`,
  }),
}));

function card(overrides: Partial<NotificationCard> = {}): NotificationCard {
  return {
    id: "account-1-thread-1",
    account_id: "account-1",
    thread_id: "thread-1",
    message_id: "message-1",
    sender: "Ada Lovelace",
    subject: "Quarterly numbers",
    snippet: "Here is the summary you asked for",
    received_at: Math.floor(Date.now() / 1000),
    count: 1,
    ...overrides,
  };
}

async function renderWindow(cards: NotificationCard[] = [card()]) {
  mocks.getNotificationCards.mockResolvedValue(cards);
  const { default: NotificationWindow } = await import("@/notification/NotificationWindow");
  render(<NotificationWindow />);
  await waitFor(() => expect(mocks.getNotificationCards).toHaveBeenCalled());
}

describe("new mail popup", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.listener = null;
    mocks.getNotificationCards.mockResolvedValue([]);
  });

  it("renders the cards the backend already has", async () => {
    await renderWindow([card()]);

    expect(await screen.findByText("Ada Lovelace")).toBeTruthy();
    expect(screen.getByText("Quarterly numbers")).toBeTruthy();
    expect(screen.getByText("Here is the summary you asked for")).toBeTruthy();
  });

  it("replaces its list when a new batch arrives", async () => {
    await renderWindow([card()]);
    expect(await screen.findByText("Ada Lovelace")).toBeTruthy();

    await act(async () => {
      mocks.listener?.({
        payload: [
          card({ id: "b", sender: "Grace Hopper", subject: "Notes", message_id: "message-2" }),
          card({ id: "c", sender: "Alan Turing", subject: "Draft", message_id: "message-3" }),
        ],
      });
    });

    expect(await screen.findByText("Grace Hopper")).toBeTruthy();
    expect(screen.getByText("Alan Turing")).toBeTruthy();
    expect(screen.queryByText("Ada Lovelace")).toBeNull();
  });

  it("opens the message the card points at", async () => {
    await renderWindow([card()]);

    const target = await screen.findByRole("button", { name: "Ada Lovelace: Quarterly numbers" });
    fireEvent.click(target);

    expect(mocks.openNotificationMessage).toHaveBeenCalledWith("account-1", "message-1");
  });

  it("dismisses a single card without touching the others", async () => {
    await renderWindow([
      card(),
      card({ id: "b", sender: "Grace Hopper", subject: "Notes", message_id: "message-2" }),
    ]);

    const closeButtons = await screen.findAllByRole("button", { name: "notification.dismiss" });
    fireEvent.click(closeButtons[0]);

    expect(mocks.dismissNotification).toHaveBeenCalledWith("account-1-thread-1");
    await waitFor(() => expect(screen.queryByText("Ada Lovelace")).toBeNull());
    expect(screen.getByText("Grace Hopper")).toBeTruthy();
  });

  it("shows how many messages sit behind a merged card", async () => {
    await renderWindow([card({ count: 4 })]);

    expect(await screen.findByText("+3")).toBeTruthy();
    expect(screen.getByLabelText("notification.threadCount:4")).toBeTruthy();
  });

  it("clears every card on Escape", async () => {
    await renderWindow([card()]);
    expect(await screen.findByText("Ada Lovelace")).toBeTruthy();

    fireEvent.keyDown(window, { key: "Escape" });

    await waitFor(() => expect(mocks.dismissAllNotifications).toHaveBeenCalled());
    expect(screen.queryByText("Ada Lovelace")).toBeNull();
  });

  it("asks the window to shrink-wrap the cards it draws", async () => {
    await renderWindow([card()]);

    await waitFor(() => expect(mocks.setNotificationPopupHeight).toHaveBeenCalled());
    const height = mocks.setNotificationPopupHeight.mock.calls[0][0] as number;
    expect(height).toBeGreaterThan(0);
    expect(height).toBeLessThanOrEqual(720);
  });
});
