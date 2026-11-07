import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useTranslation } from "react-i18next";
import { Search, SlidersHorizontal, Loader, X } from "lucide-react";
import type { AdvancedSearchQuery, SearchHit } from "@/lib/api";
import { advancedSearch, searchMessages } from "@/lib/api";
import { useUIStore } from "@/stores/ui.store";
import SearchFilters from "./SearchFilters";
import SearchResultItem from "./SearchResultItem";
import MessageDetail from "@/components/MessageDetail";
import MessageDetailEmpty from "@/components/MessageDetailEmpty";

const emptyFilters: AdvancedSearchQuery = {};
const SEARCH_DEBOUNCE_MS = 250;

function hasActiveFilters(filters: AdvancedSearchQuery): boolean {
  return !!(
    filters.from ||
    filters.to ||
    filters.subject ||
    filters.dateFrom ||
    filters.dateTo ||
    filters.hasAttachment ||
    filters.folderId
  );
}

export default function SearchView() {
  const { t } = useTranslation();
  // The sidebar search box owns the query; this view only renders its results.
  const query = useUIStore((s) => s.searchQuery);
  const [debouncedQuery, setDebouncedQuery] = useState(() => query.trim());
  const [filters, setFilters] = useState<AdvancedSearchQuery>(emptyFilters);
  const [showFilters, setShowFilters] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const trimmed = query.trim();
  const filtersActive = hasActiveFilters(filters);

  useEffect(() => {
    const timer = setTimeout(() => setDebouncedQuery(trimmed), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [trimmed]);

  const searchEnabled = debouncedQuery.length > 0 || filtersActive;

  const { data: results = [], isLoading: loading, error: queryError, refetch } = useQuery({
    queryKey: ["search", debouncedQuery, filters],
    queryFn: () =>
      filtersActive
        ? advancedSearch({ ...filters, text: debouncedQuery || undefined })
        : searchMessages(debouncedQuery),
    enabled: searchEnabled,
    staleTime: 60_000,
    placeholderData: (prev: SearchHit[] | undefined) => prev,
  });

  // A new term invalidates the open result.
  useEffect(() => {
    setSelectedId(null);
  }, [debouncedQuery]);

  const resultsParentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: results.length,
    getScrollElement: () => resultsParentRef.current,
    estimateSize: () => 76,
    measureElement: (el) => el.getBoundingClientRect().height,
    overscan: 5,
  });

  const clearSearchInput = useCallback(() => {
    useUIStore.getState().setSearchQuery("");
  }, []);

  const resultCountLabel = useMemo(() => {
    if (!searchEnabled) return null;
    return t("search.resultCount", { total: results.length, defaultValue: "{{total}} results" });
  }, [searchEnabled, results.length, t]);

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", overflow: "hidden" }}>
      <div className="search-results-header">
        <Search size={15} aria-hidden="true" className="search-results-glyph" />
        <span className="search-results-title">
          {debouncedQuery
            ? t("search.resultsFor", { query: debouncedQuery, defaultValue: "Results for \"{{query}}\"" })
            : t("search.title", "Search")}
        </span>
        {!loading && resultCountLabel && (
          <span className="search-results-count">{resultCountLabel}</span>
        )}
        {debouncedQuery && (
          <button
            type="button"
            onClick={clearSearchInput}
            className="search-toolbar-button search-toolbar-icon-button"
            aria-label={t("search.clear", "Clear search")}
            title={t("search.clear", "Clear search")}
          >
            <X size={15} />
          </button>
        )}
        <button
          type="button"
          onClick={() => setShowFilters(!showFilters)}
          title={t("search.filters", "Filters")}
          aria-label={t("search.filters", "Filters")}
          className={`search-toolbar-button search-toolbar-icon-button ${
            showFilters || filtersActive ? "is-active" : ""
          }`}
        >
          <SlidersHorizontal size={16} />
        </button>
      </div>

      {showFilters && (
        <SearchFilters
          filters={filters}
          onChange={setFilters}
          onClear={() => setFilters(emptyFilters)}
        />
      )}

      <div className="mail-panes">
        <div ref={resultsParentRef} className="scroll-region search-results-scroll mail-list-pane mail-list-pane--scroll">
          {loading && (
            <div className="fade-in search-empty-state">
              <Loader size={20} className="spinner" />
              <span>{t("common.loading", "Loading...")}</span>
            </div>
          )}

          {!loading && queryError && (
            <div className="fade-in search-empty-state">
              <p style={{ color: "var(--color-error, #e53e3e)", margin: 0 }}>
                {t("search.error", "Search failed")}
              </p>
              <p style={{ fontSize: "13px", margin: 0 }}>{queryError?.message}</p>
              <button type="button" onClick={() => refetch()} className="search-empty-action">
                {t("common.retry", "Retry")}
              </button>
            </div>
          )}

          {!loading && !queryError && searchEnabled && results.length === 0 && (
            <div className="fade-in search-empty-state">
              <Search size={28} strokeWidth={1.2} />
              {t("search.noResults", "No results found")}
            </div>
          )}

          {!loading && !queryError && !searchEnabled && (
            <div className="fade-in search-empty-state">
              <Search size={32} strokeWidth={1.2} />
              <span>{t("search.hintFromSidebar", "Type in the sidebar search box to search your mail")}</span>
            </div>
          )}

          {!loading && results.length > 0 && (
            <div
              role="listbox"
              aria-label={t("search.results", "Search results")}
              style={{ height: `${virtualizer.getTotalSize()}px`, width: "100%", position: "relative" }}
            >
              {virtualizer.getVirtualItems().map((virtualItem) => {
                const hit = results[virtualItem.index];
                return (
                  <div
                    key={hit.message_id}
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
                    <SearchResultItem
                      hit={hit}
                      isSelected={hit.message_id === selectedId}
                      onClick={() => setSelectedId(hit.message_id)}
                      query={debouncedQuery}
                    />
                  </div>
                );
              })}
            </div>
          )}
        </div>

        <div className="mail-detail-pane">
          {selectedId ? (
            <MessageDetail messageId={selectedId} onBack={() => setSelectedId(null)} />
          ) : (
            <MessageDetailEmpty />
          )}
        </div>
      </div>
    </div>
  );
}
