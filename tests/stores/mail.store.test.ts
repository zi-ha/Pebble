import { describe, it, expect, beforeEach } from "vitest";
import { useMailStore } from "../../src/stores/mail.store";

describe("MailStore", () => {
  beforeEach(() => {
    useMailStore.setState({
      activeAccountId: null,
      activeFolderId: null,
      selectedMessageId: null,
    });
  });

  it("should have correct initial state", () => {
    const state = useMailStore.getState();
    expect(state.activeAccountId).toBeNull();
    expect(state.activeFolderId).toBeNull();
    expect(state.selectedMessageId).toBeNull();
  });

  it("should set selected message", () => {
    useMailStore.getState().setSelectedMessage("msg-1");
    expect(useMailStore.getState().selectedMessageId).toBe("msg-1");

    useMailStore.getState().setSelectedMessage(null);
    expect(useMailStore.getState().selectedMessageId).toBeNull();
  });

  it("should set active account and reset dependent state", () => {
    useMailStore.setState({
      activeFolderId: "f1",
      selectedMessageId: "m1",
    });

    useMailStore.getState().setActiveAccountId("a1");

    const state = useMailStore.getState();
    expect(state.activeAccountId).toBe("a1");
    expect(state.activeFolderId).toBeNull();
    expect(state.selectedMessageId).toBeNull();
  });

  it("should set active folder and reset message selection", () => {
    useMailStore.setState({
      activeAccountId: "a1",
      selectedMessageId: "m1",
    });

    useMailStore.getState().setActiveFolderId("f2");

    const state = useMailStore.getState();
    expect(state.activeFolderId).toBe("f2");
    expect(state.selectedMessageId).toBeNull();
    expect(state.activeAccountId).toBe("a1");
  });
});
