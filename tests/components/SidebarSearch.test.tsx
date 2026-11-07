import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import SidebarSearch from "../../src/components/SidebarSearch";
import { useUIStore } from "../../src/stores/ui.store";

vi.mock("react-i18next", () => ({
  initReactI18next: {
    type: "3rdParty",
    init: vi.fn(),
  },
  useTranslation: () => ({
    t: (key: string, fallback?: string) => {
      const labels: Record<string, string> = {
        "search.title": "Search",
        "sidebar.searchPlaceholder": "Search mail",
        "search.clear": "Clear search",
      };
      return labels[key] ?? fallback ?? key;
    },
  }),
}));

describe("SidebarSearch", () => {
  beforeEach(() => {
    useUIStore.setState({
      activeView: "inbox",
      searchQuery: "",
      sidebarCollapsed: false,
      searchFocusSignal: 0,
    });
  });

  it("writes the query into the store and opens the search view", () => {
    render(<SidebarSearch collapsed={false} />);

    const input = screen.getByRole("searchbox", { name: "Search" });
    fireEvent.change(input, { target: { value: "invoice" } });

    expect(useUIStore.getState().searchQuery).toBe("invoice");
    expect(useUIStore.getState().activeView).toBe("search");
  });

  it("keeps the typed text without leaving the current view when cleared", () => {
    useUIStore.setState({ activeView: "search", searchQuery: "invoice" });
    render(<SidebarSearch collapsed={false} />);

    fireEvent.click(screen.getByRole("button", { name: "Clear search" }));

    expect(useUIStore.getState().searchQuery).toBe("");
    expect(useUIStore.getState().activeView).toBe("search");
  });

  it("collapses to an icon button that expands the sidebar", () => {
    useUIStore.setState({ sidebarCollapsed: true });
    render(<SidebarSearch collapsed />);

    fireEvent.click(screen.getByRole("button", { name: "Search" }));

    expect(useUIStore.getState().sidebarCollapsed).toBe(false);
    expect(useUIStore.getState().searchFocusSignal).toBe(1);
  });
});
