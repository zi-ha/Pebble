import { useEffect, useRef } from "react";
import { Search, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useUIStore } from "@/stores/ui.store";

/**
 * The single search entry point of the app.
 *
 * The typed text lives in `ui.store#searchQuery`, which `SearchView` reads as its
 * query source, so results render in the main area while the box stays in the sidebar.
 */
export default function SidebarSearch({ collapsed }: { collapsed: boolean }) {
  const { t } = useTranslation();
  const query = useUIStore((s) => s.searchQuery);
  const setSearchQuery = useUIStore((s) => s.setSearchQuery);
  const activeView = useUIStore((s) => s.activeView);
  const setActiveView = useUIStore((s) => s.setActiveView);
  const requestSearchFocus = useUIStore((s) => s.requestSearchFocus);
  const focusSignal = useUIStore((s) => s.searchFocusSignal);
  const inputRef = useRef<HTMLInputElement>(null);
  const handledSignal = useRef(useUIStore.getState().searchFocusSignal);

  // Focus the box when a shortcut (or the collapsed icon) asks for it. Runs after
  // the expand re-render, so the input exists by the time the effect fires.
  useEffect(() => {
    if (collapsed || focusSignal === handledSignal.current) return;
    handledSignal.current = focusSignal;
    inputRef.current?.focus();
    inputRef.current?.select();
  }, [focusSignal, collapsed]);

  function openSearchView() {
    if (useUIStore.getState().activeView === "search") return;
    setActiveView("search");
  }

  function handleChange(value: string) {
    setSearchQuery(value);
    if (value.trim()) openSearchView();
  }

  function handleSubmit(e: React.SyntheticEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!query.trim()) return;
    openSearchView();
    inputRef.current?.blur();
  }

  function handleClear() {
    setSearchQuery("");
    inputRef.current?.focus();
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Escape") {
      e.preventDefault();
      if (query) {
        setSearchQuery("");
        return;
      }
      e.currentTarget.blur();
    }
  }

  if (collapsed) {
    return (
      <div className="sidebar-search sidebar-search--collapsed">
        <button
          type="button"
          className={`sidebar-search-icon${activeView === "search" ? " is-active" : ""}`}
          aria-label={t("search.title", "Search")}
          title={t("search.title", "Search")}
          onClick={() => requestSearchFocus()}
        >
          <Search size={16} />
        </button>
      </div>
    );
  }

  return (
    <div className="sidebar-search">
      <form onSubmit={handleSubmit} role="search">
        <div className={`sidebar-search-shell${activeView === "search" ? " is-active" : ""}`}>
          <Search size={14} aria-hidden="true" className="sidebar-search-glyph" />
          <input
            ref={inputRef}
            type="search"
            value={query}
            onChange={(e) => handleChange(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={t("sidebar.searchPlaceholder", "Search mail")}
            aria-label={t("search.title", "Search")}
            className="sidebar-search-input"
            spellCheck={false}
          />
          {query && (
            <button
              type="button"
              onClick={handleClear}
              aria-label={t("search.clear", "Clear search")}
              title={t("search.clear", "Clear search")}
              className="sidebar-search-clear"
            >
              <X size={12} />
            </button>
          )}
        </div>
      </form>
    </div>
  );
}
