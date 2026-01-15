import { useCallback, useEffect, useRef, useState, useMemo } from "react";
import { Virtuoso, VirtuosoHandle } from "react-virtuoso";
import { useDebouncedCallback } from "use-debounce";
import type { LogFilters, LogLevel } from "../../types";
import { useLogcatQuery } from "./hooks/useLogcatQuery";
import { useLogHighlight } from "./hooks/useLogHighlight";
import styles from "./LogcatViewV2.module.css";

const ALL_LEVELS: LogLevel[] = ["V", "D", "I", "W", "E", "F"];

const THREADTIME_PATTERN = /^(\d{2})-(\d{2})\s+(\d{2}):(\d{2}):(\d{2})(?:\.\d{1,3})?$/;

const FILTER_PATTERN = /^(\d{4})-(\d{2})-(\d{2})\s+(\d{2}):(\d{2}):(\d{2})$/;

const threadtimeToFilter = (value: string | undefined, year: number): string | undefined => {
  if (!value) return undefined;
  const match = value.match(THREADTIME_PATTERN);
  if (!match) return undefined;
  const [, month, day, hour, minute, second] = match;
  return `${year}-${month}-${day} ${hour}:${minute}:${second}`;
};

const parseThreadtimeInput = (input: string | undefined, year: number): string | undefined => {
  if (!input) return undefined;
  const trimmed = input.trim();
  if (!trimmed) return undefined;
  const match = trimmed.match(THREADTIME_PATTERN);
  if (!match) return undefined;
  const [, month, day, hour, minute, second] = match;
  return `${year}-${month}-${day} ${hour}:${minute}:${second}`;
};

const filterToThreadtime = (value: string | undefined): string => {
  if (!value) return "";
  const match = value.match(FILTER_PATTERN);
  if (!match) return "";
  const [, , month, day, hour, minute, second] = match;
  return `${month}-${day} ${hour}:${minute}:${second}`;
};

const parseFilterDate = (value: string | undefined): Date | null => {
  if (!value) return null;
  const match = value.match(FILTER_PATTERN);
  if (!match) return null;
  const [, year, month, day, hour, minute, second] = match;
  return new Date(Number(year), Number(month) - 1, Number(day), Number(hour), Number(minute), Number(second));
};

export default function LogcatViewV2() {
  const [filters, setFilters] = useState<LogFilters>(() => {
    try {
      const raw = localStorage.getItem("lm.log.filters.v2");
      return raw ? JSON.parse(raw) : {};
    } catch {
      return {};
    }
  });

  const [wrap, setWrap] = useState(false);
  const [showSearch, setShowSearch] = useState(false);
  const [searchText, setSearchText] = useState("");
  const [searchIndex, setSearchIndex] = useState(-1);
  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);

  // Text filter chips state
  const [textChips, setTextChips] = useState<string[]>([]);
  const [textInput, setTextInput] = useState("");

  // Tag filter chips state
  const [tagChips, setTagChips] = useState<string[]>([]);
  const [tagInput, setTagInput] = useState("");

  // Local state for debounced inputs (prevents re-render lag during typing)
  const [localPid, setLocalPid] = useState(filters.pid?.toString() ?? "");
  const [localTid, setLocalTid] = useState(filters.tid?.toString() ?? "");
  const [localNotText, setLocalNotText] = useState(filters.notText ?? "");
  const [localFrom, setLocalFrom] = useState("");
  const [localTo, setLocalTo] = useState("");
  const [timeRangeYear, setTimeRangeYear] = useState(() => new Date().getFullYear());

  const {
    rows,
    loading,
    loadingNext,
    loadingPrev,
    error,
    stats,
    hasMoreNext,
    hasMorePrev,
    firstItemIndex,
    loadInitial,
    loadNext,
    loadPrev,
  } = useLogcatQuery(filters);

  const highlight = useLogHighlight(filters);

  // Persist filters
  useEffect(() => {
    try {
      localStorage.setItem("lm.log.filters.v2", JSON.stringify(filters));
    } catch {/* ignore */}
  }, [filters]);

  // Debounced auto-apply filters
  const debouncedLoadInitial = useDebouncedCallback(() => {
    loadInitial();
  }, 400);

  // Debounced sync from local input state to filters
  const debouncedSyncPid = useDebouncedCallback((val: string) => {
    setFilters((f) => ({ ...f, pid: val ? Number(val) : undefined }));
  }, 300);

  const debouncedSyncTid = useDebouncedCallback((val: string) => {
    setFilters((f) => ({ ...f, tid: val ? Number(val) : undefined }));
  }, 300);

  const debouncedSyncNotText = useDebouncedCallback((val: string) => {
    setFilters((f) => ({ ...f, notText: val || undefined }));
  }, 300);

  const applyTimeRange = useCallback((fromInput: string, toInput: string) => {
    const parsedFrom = parseThreadtimeInput(fromInput, timeRangeYear);
    const parsedTo = parseThreadtimeInput(toInput, timeRangeYear);

    setFilters((f) => {
      const nextFrom = parsedFrom ?? (fromInput.trim() ? f.tsFrom : undefined);
      const nextTo = parsedTo ?? (toInput.trim() ? f.tsTo : undefined);
      const fromDate = parseFilterDate(nextFrom);
      const toDate = parseFilterDate(nextTo);

      if (fromDate && toDate && fromDate > toDate) {
        return { ...f, tsFrom: nextTo, tsTo: nextFrom };
      }

      return {
        ...f,
        tsFrom: nextFrom,
        tsTo: nextTo,
      };
    });
  }, [timeRangeYear]);

  // Auto-apply when filters change
  useEffect(() => {
    debouncedLoadInitial();
  }, [filters, debouncedLoadInitial]);

  // Sync local state when filters change externally (e.g., clear all, click tag/pid)
  useEffect(() => {
    setLocalPid(filters.pid?.toString() ?? "");
  }, [filters.pid]);

  useEffect(() => {
    setLocalTid(filters.tid?.toString() ?? "");
  }, [filters.tid]);

  useEffect(() => {
    setLocalNotText(filters.notText ?? "");
  }, [filters.notText]);

  useEffect(() => {
    if (!stats?.minTsDisplay || !stats?.maxTsDisplay) return;
    const minMatch = stats.minTsDisplay.match(THREADTIME_PATTERN);
    const maxMatch = stats.maxTsDisplay.match(THREADTIME_PATTERN);
    if (!minMatch || !maxMatch) return;
    const fromMonth = Number(minMatch[1]);
    const fromDay = Number(minMatch[2]);
    const toMonth = Number(maxMatch[1]);
    const toDay = Number(maxMatch[2]);
    const year = fromMonth > toMonth || (fromMonth === toMonth && fromDay > toDay)
      ? new Date().getFullYear() - 1
      : new Date().getFullYear();
    setTimeRangeYear(year);
  }, [stats?.minTsDisplay, stats?.maxTsDisplay]);

  useEffect(() => {
    setLocalFrom(filterToThreadtime(filters.tsFrom));
  }, [filters.tsFrom, timeRangeYear]);

  useEffect(() => {
    setLocalTo(filterToThreadtime(filters.tsTo));
  }, [filters.tsTo, timeRangeYear]);

  // Handle custom events
  useEffect(() => {
    const handler = (e: Event) => {
      const ce = e as CustomEvent<Partial<LogFilters>>;
      setFilters((f) => ({ ...f, ...(ce.detail || {}) }));
    };
    window.addEventListener("lm:logcat:apply", handler as EventListener);
    return () => window.removeEventListener("lm:logcat:apply", handler as EventListener);
  }, []);

  const setPid = (pid?: number) => setFilters((f) => ({ ...f, pid }));

  // Escape special regex characters for plain text search
  const escapeRegex = (str: string) => str.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

  // Sync text chips to filters.text (OR logic with |)
  useEffect(() => {
    if (textChips.length === 0) {
      setFilters((f) => ({ ...f, text: undefined }));
    } else if (textChips.length === 1) {
      // Single chip: use as-is (respects textMode)
      setFilters((f) => ({ ...f, text: textChips[0] }));
    } else {
      // Multiple chips: combine with OR logic using |
      const isRegexMode = filters.textMode === "regex";
      const pattern = textChips
        .map((chip) => isRegexMode ? chip : escapeRegex(chip))
        .join("|");
      setFilters((f) => ({ ...f, text: pattern, textMode: "regex" }));
    }
  }, [textChips, filters.textMode]);

  // Add text chip on Enter
  const handleTextInputKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" && textInput.trim()) {
      e.preventDefault();
      const newChip = textInput.trim();
      if (!textChips.includes(newChip)) {
        setTextChips([...textChips, newChip]);
      }
      setTextInput("");
    } else if (e.key === "Backspace" && !textInput && textChips.length > 0) {
      // Remove last chip on Backspace when input is empty
      setTextChips(textChips.slice(0, -1));
    }
  };

  // Remove a specific text chip
  const removeTextChip = (chipToRemove: string) => {
    setTextChips(textChips.filter((chip) => chip !== chipToRemove));
  };

  // Sync tag chips to filters.tag (OR logic with |)
  useEffect(() => {
    if (tagChips.length === 0) {
      setFilters((f) => ({ ...f, tag: undefined }));
    } else {
      // Join with | for OR logic (backend handles the split)
      setFilters((f) => ({ ...f, tag: tagChips.join("|") }));
    }
  }, [tagChips]);

  // Add tag chip on Enter
  const handleTagInputKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" && tagInput.trim()) {
      e.preventDefault();
      const newChip = tagInput.trim();
      if (!tagChips.includes(newChip)) {
        setTagChips([...tagChips, newChip]);
      }
      setTagInput("");
    } else if (e.key === "Backspace" && !tagInput && tagChips.length > 0) {
      setTagChips(tagChips.slice(0, -1));
    }
  };

  // Remove a specific tag chip
  const removeTagChip = (chipToRemove: string) => {
    setTagChips(tagChips.filter((chip) => chip !== chipToRemove));
  };

  // Add tag from clicking on a log row
  const addTagChip = (tag: string) => {
    if (!tagChips.includes(tag)) {
      setTagChips([...tagChips, tag]);
    }
  };

  const toggleLevel = (level: LogLevel) => {
    setFilters((f) => {
      const current = f.levels || [];
      const has = current.includes(level);
      if (has) {
        const next = current.filter((l) => l !== level);
        return { ...f, levels: next.length > 0 ? next : undefined };
      } else {
        return { ...f, levels: [...current, level] };
      }
    });
  };

  const activeFilters = useMemo(() => {
    return Object.entries(filters).filter(
      ([, value]) => value !== undefined && value !== false && !(Array.isArray(value) && value.length === 0)
    );
  }, [filters]);

  const hasFilters = activeFilters.length > 0;

  const scrollToBottom = () => {
    virtuosoRef.current?.scrollToIndex({ index: rows.length - 1, align: "end" });
  };

  // Search within loaded logs
  const searchMatches = useMemo(() => {
    if (!searchText.trim()) return [];
    const query = searchText.toLowerCase();
    return rows
      .map((row, idx) => ({ idx, row }))
      .filter(({ row }) =>
        row.msg.toLowerCase().includes(query) ||
        row.tag.toLowerCase().includes(query)
      )
      .map(({ idx }) => idx);
  }, [rows, searchText]);

  const goToSearchResult = (direction: "next" | "prev") => {
    if (searchMatches.length === 0) return;

    let newIndex: number;
    if (searchIndex === -1) {
      newIndex = 0;
    } else if (direction === "next") {
      newIndex = (searchIndex + 1) % searchMatches.length;
    } else {
      newIndex = (searchIndex - 1 + searchMatches.length) % searchMatches.length;
    }

    setSearchIndex(newIndex);
    const rowIndex = searchMatches[newIndex];
    virtuosoRef.current?.scrollToIndex({ index: rowIndex, align: "center" });
  };

  const handleSearchKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      goToSearchResult(e.shiftKey ? "prev" : "next");
    } else if (e.key === "Escape") {
      closeSearch();
    }
  };

  const closeSearch = () => {
    setShowSearch(false);
    setSearchText("");
    setSearchIndex(-1);
  };

  const openSearch = () => {
    setShowSearch(true);
    // Focus input after render
    setTimeout(() => searchInputRef.current?.focus(), 0);
  };

  // Global Ctrl+F handler
  useEffect(() => {
    const handleGlobalKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "f") {
        e.preventDefault();
        if (showSearch) {
          searchInputRef.current?.focus();
          searchInputRef.current?.select();
        } else {
          openSearch();
        }
      }
    };
    window.addEventListener("keydown", handleGlobalKeyDown);
    return () => window.removeEventListener("keydown", handleGlobalKeyDown);
  }, [showSearch]);

  // Reset search index when search text changes
  useEffect(() => {
    setSearchIndex(-1);
  }, [searchText]);

  const renderRow = useCallback(
    (index: number) => {
      const r = rows[index];
      if (!r) return null;

      const { parts, matches } = highlight(r.msg);
      const isError = r.level === "E" || r.level === "F";
      const isSearchMatch = searchMatches.includes(index);
      const isCurrentMatch = searchIndex >= 0 && searchMatches[searchIndex] === index;

      return (
        <div
          className={`${styles.logRow} ${isError ? (r.level === "F" ? styles.levelF : styles.levelE) : ""} ${isSearchMatch ? styles.searchMatch : ""} ${isCurrentMatch ? styles.currentMatch : ""}`}
        >
          <div className={styles.cellTs}>{r.ts}</div>
          <div
            className={styles.cellPid}
            onClick={() => setPid(r.pid)}
            title="Filter by PID"
          >
            {String(r.pid).padStart(5, " ")}/{String(r.tid).padStart(5, " ")}
          </div>
          <div className={styles.cellLevel}>
            <span className={`${styles.levelBadge} ${styles[r.level]}`}>{r.level}</span>
          </div>
          <div className={styles.cellTag} onClick={() => addTagChip(r.tag)} title="Filter by Tag">
            {r.tag}
          </div>
          <div className={`${styles.cellMsg} ${wrap ? styles.wrap : styles.nowrap}`}>
            {parts.map((part, i) => (
              <span key={i}>
                {part}
                {i < matches.length && <span className={styles.highlight}>{matches[i]}</span>}
              </span>
            ))}
          </div>
        </div>
      );
    },
    [rows, highlight, wrap, searchMatches, searchIndex]
  );

  const formatFilterValue = (key: string, value: unknown): string => {
    if (Array.isArray(value)) return value.join(", ");
    if (key === "tsFrom" || key === "tsTo") {
      return String(value).replace("T", " ");
    }
    return String(value);
  };

  const formatFilterKey = (key: string): string => {
    const keyMap: Record<string, string> = {
      tag: "Tag",
      pid: "PID",
      tid: "TID",
      text: "Text",
      notText: "Exclude",
      levels: "Level",
      tsFrom: "From",
      tsTo: "To",
      textMode: "Mode",
      caseSensitive: "Case",
    };
    return keyMap[key] || key;
  };

  return (
    <div className={styles.container}>
      {/* Header Bar */}
      <div className={styles.header}>
        <div className={styles.titleGroup}>
          <h2 className={styles.title}>LOGCAT</h2>
          {stats && (
            <div className={styles.stats}>
              <div className={styles.statItem}>
                <span>Total</span>
                <strong>{stats.totalRows.toLocaleString()}</strong>
              </div>
              <div className={styles.statItem}>
                <span>Loaded</span>
                <strong>{rows.length.toLocaleString()}</strong>
              </div>
              <div className={`${styles.statItem} ${styles.error}`}>
                <span>E</span>
                <strong>{stats.levelCounts.error}</strong>
              </div>
              <div className={`${styles.statItem} ${styles.fatal}`}>
                <span>F</span>
                <strong>{stats.levelCounts.fatal}</strong>
              </div>
            </div>
          )}
        </div>
        <div className={styles.headerActions}>
          <label className={styles.wrapToggle} title="Wrap long lines">
            <input
              type="checkbox"
              checked={wrap}
              onChange={(e) => setWrap(e.target.checked)}
            />
            <span>Wrap</span>
          </label>
          <button
            className={styles.headerBtn}
            onClick={openSearch}
            title="Search (Ctrl+F)"
            aria-label="Search"
          >
            ⌕
          </button>
          <button
            className={styles.headerBtn}
            onClick={scrollToBottom}
            title="Scroll to bottom"
            aria-label="Scroll to bottom"
          >
            ↓
          </button>
        </div>
      </div>

      {/* Filter Panel */}
      <div className={styles.filterPanel}>
        {/* Row 1: Text Filter with chips */}
        <div className={`${styles.filterRow} ${styles.filterRowPrimary}`}>
          <div className={styles.textFilterGroup}>
            <label className={styles.filterLabel}>Text Filter</label>
            <div className={styles.textFilterInput}>
              <div className={styles.textChipsContainer}>
                {textChips.map((chip, idx) => (
                  <span key={idx} className={styles.textChip}>
                    {chip}
                    <span
                      className={styles.textChipRemove}
                      onClick={() => removeTextChip(chip)}
                    >
                      ×
                    </span>
                  </span>
                ))}
                <input
                  className={styles.textChipInput}
                  placeholder={textChips.length === 0 ? "Type and press Enter..." : "Add more..."}
                  value={textInput}
                  onChange={(e) => setTextInput(e.target.value)}
                  onKeyDown={handleTextInputKeyDown}
                />
              </div>
              <div className={styles.textFilterOptions}>
                <label className={styles.optionSmall} title="Use Regular Expression" aria-label="Use Regular Expression">
                  <input
                    type="checkbox"
                    checked={filters.textMode === "regex"}
                    onChange={(e) =>
                      setFilters((f) => ({ ...f, textMode: e.target.checked ? "regex" : "plain" }))
                    }
                    disabled={textChips.length > 1}
                  />
                  <span>.*</span>
                </label>
                <label className={styles.optionSmall} title="Case Sensitive" aria-label="Case Sensitive">
                  <input
                    type="checkbox"
                    checked={!!filters.caseSensitive}
                    onChange={(e) =>
                      setFilters((f) => ({ ...f, caseSensitive: e.target.checked || undefined }))
                    }
                  />
                  <span>Aa</span>
                </label>
              </div>
            </div>
          </div>
        </div>

        {/* Row 2: Level, Tag, PID, TID, Exclude */}
        <div className={`${styles.filterRow} ${styles.filterRowSecondary}`}>
          <div className={styles.levelFilterCompact}>
            <label className={styles.filterLabel}>Level</label>
            <div className={styles.levelSelector}>
              {ALL_LEVELS.map((level) => (
                <button
                  key={level}
                  data-level={level}
                  className={`${styles.levelBtn} ${
                    (filters.levels || []).includes(level) ? styles.active : ""
                  }`}
                  onClick={() => toggleLevel(level)}
                >
                  {level}
                </button>
              ))}
            </div>
          </div>

          <div className={styles.filterGroup}>
            <label className={styles.filterLabel}>Tag</label>
            <div className={styles.chipsInput}>
              {tagChips.map((chip, idx) => (
                <span key={idx} className={styles.chip}>
                  {chip}
                  <span className={styles.chipRemove} onClick={() => removeTagChip(chip)}>×</span>
                </span>
              ))}
              <input
                className={styles.chipInput}
                placeholder={tagChips.length === 0 ? "ActivityManager..." : ""}
                value={tagInput}
                onChange={(e) => setTagInput(e.target.value)}
                onKeyDown={handleTagInputKeyDown}
              />
            </div>
          </div>

          <div className={styles.filterGroupSmall}>
            <label className={styles.filterLabel}>PID</label>
            <div className={styles.chipsInput}>
              <input
                className={styles.chipInput}
                placeholder="1234"
                value={localPid}
                onChange={(e) => {
                  const val = e.target.value;
                  setLocalPid(val);
                  debouncedSyncPid(val);
                }}
              />
            </div>
          </div>

          <div className={styles.filterGroupSmall}>
            <label className={styles.filterLabel}>TID</label>
            <div className={styles.chipsInput}>
              <input
                className={styles.chipInput}
                placeholder="5678"
                value={localTid}
                onChange={(e) => {
                  const val = e.target.value;
                  setLocalTid(val);
                  debouncedSyncTid(val);
                }}
              />
            </div>
          </div>

          <div className={styles.filterGroup}>
            <label className={styles.filterLabel}>Exclude</label>
            <div className={styles.chipsInput}>
              <input
                className={styles.chipInput}
                placeholder="Noise text..."
                value={localNotText}
                onChange={(e) => {
                  const val = e.target.value;
                  setLocalNotText(val);
                  debouncedSyncNotText(val);
                }}
              />
            </div>
          </div>
        </div>

        {/* Row 3: Time Range */}
        <div className={`${styles.filterRow} ${styles.filterRowTertiary}`}>
          <div className={styles.timeRangeGroup}>
            <label className={styles.filterLabel}>
              Time Range
              <span className={styles.formatHint}>Format: MM-DD HH:mm:ss</span>
              {stats?.minTsDisplay && stats?.maxTsDisplay && (
                <span className={styles.timeRangeHint}>
                  Range: {stats.minTsDisplay} ~ {stats.maxTsDisplay}
                </span>
              )}
            </label>
            <div className={styles.timeRangeInputs}>
              <div className={styles.timeInputWrapper}>
                <input
                  className={styles.timeInput}
                  placeholder="MM-DD HH:mm:ss"
                  value={localFrom}
                  onChange={(e) => {
                    const val = e.target.value;
                    setLocalFrom(val);
                    applyTimeRange(val, localTo);
                  }}
                  onBlur={() => applyTimeRange(localFrom, localTo)}
                />
              </div>
              <span className={styles.timeSeparator}>to</span>
              <div className={styles.timeInputWrapper}>
                <input
                  className={styles.timeInput}
                  placeholder="MM-DD HH:mm:ss"
                  value={localTo}
                  onChange={(e) => {
                    const val = e.target.value;
                    setLocalTo(val);
                    applyTimeRange(localFrom, val);
                  }}
                  onBlur={() => applyTimeRange(localFrom, localTo)}
                />
              </div>
              <div className={styles.timeRangeActions}>
                <button
                  className={styles.timeRangeBtn}
                  onClick={() => {
                    const tsFrom = threadtimeToFilter(stats?.minTsDisplay, timeRangeYear);
                    if (tsFrom) {
                      setFilters((f) => ({ ...f, tsFrom }));
                    }
                  }}
                  disabled={!stats?.minTsDisplay}
                  title="Set to earliest log time"
                >
                  Min
                </button>
                <button
                  className={styles.timeRangeBtn}
                  onClick={() => {
                    const tsTo = threadtimeToFilter(stats?.maxTsDisplay, timeRangeYear);
                    if (tsTo) {
                      setFilters((f) => ({ ...f, tsTo }));
                    }
                  }}
                  disabled={!stats?.maxTsDisplay}
                  title="Set to latest log time"
                >
                  Max
                </button>
                <button
                  className={styles.timeRangeBtn}
                  onClick={() => {
                    setFilters((f) => ({
                      ...f,
                      tsFrom: undefined,
                      tsTo: undefined,
                    }));
                  }}
                  disabled={!filters.tsFrom && !filters.tsTo}
                  title="Clear time range"
                >
                  Clear
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Active Filters Chips */}
      {hasFilters && (
        <div className={styles.chipBar}>
          {activeFilters.map(([key, value]) => (
            <span key={key} className={styles.chip}>
              <span className={styles.chipKey}>{formatFilterKey(key)}:</span>
              <span className={styles.chipValue}>{formatFilterValue(key, value)}</span>
              <span
                className={styles.chipRemove}
                onClick={() => setFilters((f) => ({ ...f, [key]: undefined }))}
              >
                ×
              </span>
            </span>
          ))}
          <button className={styles.clearAllBtn} onClick={() => setFilters({})}>
            Clear All
          </button>
        </div>
      )}

      {/* Log Container */}
      <div className={styles.logContainer}>
        {/* Floating Search Box - only shown when search is open */}
        {showSearch && (
          <div className={styles.floatingSearch}>
            <div className={styles.searchBox}>
              <div className={styles.searchInputWrapper}>
                <span className={styles.searchIcon}>⌕</span>
                <input
                  ref={searchInputRef}
                  type="text"
                  className={styles.searchInput}
                  placeholder="Find in logs..."
                  value={searchText}
                  onChange={(e) => setSearchText(e.target.value)}
                  onKeyDown={handleSearchKeyDown}
                />
              </div>
              <div className={styles.searchDivider} />
              <div className={styles.searchControls}>
                <span className={styles.searchCount}>
                  {searchMatches.length > 0
                    ? `${searchIndex + 1}/${searchMatches.length}`
                    : "0/0"}
                </span>
                <button
                  className={styles.searchNav}
                  onClick={() => goToSearchResult("prev")}
                  disabled={searchMatches.length === 0}
                  title="Previous (Shift+Enter)"
                >
                  ▲
                </button>
                <button
                  className={styles.searchNav}
                  onClick={() => goToSearchResult("next")}
                  disabled={searchMatches.length === 0}
                  title="Next (Enter)"
                >
                  ▼
                </button>
                <button
                  className={styles.searchClose}
                  onClick={closeSearch}
                  title="Close (Esc)"
                >
                  ✕
                </button>
              </div>
            </div>
          </div>
        )}

        {/* Table Header */}
        <div className={styles.logHeader}>
          <div className={styles.logHeaderCell}>Timestamp</div>
          <div className={styles.logHeaderCell}>PID/TID</div>
          <div className={styles.logHeaderCell}>Lvl</div>
          <div className={styles.logHeaderCell}>Tag</div>
          <div className={styles.logHeaderCell}>Message</div>
        </div>

        {/* Error State - only show if no data */}
        {error && rows.length === 0 && (
          <div className={styles.errorState}>
            <div className={styles.emptyIcon}>⚠</div>
            <div className={styles.emptyText}>{error}</div>
          </div>
        )}

        {/* Empty State */}
        {!loading && !error && rows.length === 0 && (
          <div className={styles.emptyState}>
            <div className={styles.emptyIcon}>📋</div>
            <div className={styles.emptyText}>
              {(stats?.totalRows ?? 0) > 0 ? (
                <>
                  No logs match your filters.
                  <br />
                  Try adjusting your time range or clearing filters.
                </>
              ) : (
                <>
                  No logs to display.
                  <br />
                  Open a bugreport file to get started.
                </>
              )}
            </div>
          </div>
        )}

        {/* Log Body with Virtuoso */}
        {rows.length > 0 && (
          <div className={styles.logBody}>
            <Virtuoso
              ref={virtuosoRef}
              totalCount={rows.length}
              firstItemIndex={firstItemIndex}
              itemContent={renderRow}
              startReached={() => {
                if (hasMorePrev && !loadingPrev) loadPrev();
              }}
              endReached={() => {
                if (hasMoreNext && !loadingNext) loadNext();
              }}
              overscan={200}
              style={{ height: "100%" }}
            />
          </div>
        )}
      </div>

      {/* Status Bar */}
      <div className={styles.statusBar}>
        <div className={styles.statusLeft}>
          {(loading || loadingNext || loadingPrev) && (
            <>
              <span className={styles.loadingDot} />
              <span>
                {loading ? "Loading..." : loadingPrev ? "Loading older..." : "Loading newer..."}
              </span>
            </>
          )}
          {!loading && !loadingNext && !loadingPrev && rows.length > 0 && (
            <span>
              Showing {rows.length.toLocaleString()} of {stats?.totalRows.toLocaleString() ?? "?"} rows
            </span>
          )}
        </div>
        <div className={styles.statusRight}>
          {hasMorePrev && <span>↑ More above</span>}
          {hasMoreNext && <span>↓ More below</span>}
        </div>
      </div>
    </div>
  );
}
