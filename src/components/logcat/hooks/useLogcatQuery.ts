import { useState, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { LogRow, LogFilters, QueryResponse, QueryCursor, CursorDirection, LogcatStats } from "../../../types";

const BATCH_SIZE = 500;

// Dynamic buffer size based on total rows:
// - < 20K rows: load all (no trimming)
// - 20K-100K rows: keep 50% in memory
// - > 100K rows: keep max 50K rows
function getMaxBufferSize(totalRows: number): number {
  if (totalRows < 20000) return Infinity;
  if (totalRows < 100000) return Math.ceil(totalRows * 0.5);
  return 50000;
}

interface QueryState {
  rows: LogRow[];
  nextCursor: QueryCursor | null;
  prevCursor: QueryCursor | null;
  hasMoreNext: boolean;
  hasMorePrev: boolean;
  loading: boolean;
  loadingNext: boolean;
  loadingPrev: boolean;
  error: string | null;
  stats: LogcatStats | null;
  firstItemIndex: number;
}

export function useLogcatQuery(filters: LogFilters) {
  const [state, setState] = useState<QueryState>({
    rows: [],
    nextCursor: null,
    prevCursor: null,
    hasMoreNext: false,
    hasMorePrev: false,
    loading: false,
    loadingNext: false,
    loadingPrev: false,
    error: null,
    stats: null,
    firstItemIndex: 0,
  });

  const reqRef = useRef(0);

  const loadInitial = useCallback(async () => {
    const myId = ++reqRef.current;
    setState((s) => ({ ...s, loading: true, error: null, rows: [], firstItemIndex: 0 }));

    try {
      // Get stats first
      const stats = await invoke<LogcatStats>("get_logcat_stats", { filters });

      // Initial query
      const response = await invoke<QueryResponse>("query_logcat_v2", {
        filters,
        cursor: null,
        limit: BATCH_SIZE,
        direction: "forward" as CursorDirection,
      });

      if (reqRef.current !== myId) return;

      setState((s) => ({
        ...s,
        rows: response.rows,
        nextCursor: response.nextCursor,
        prevCursor: response.prevCursor,
        hasMoreNext: response.hasMoreNext,
        hasMorePrev: response.hasMorePrev,
        loading: false,
        stats,
        firstItemIndex: 0,
      }));
    } catch (e: any) {
      if (reqRef.current !== myId) return;
      setState((s) => ({
        ...s,
        loading: false,
        error: e?.message || String(e),
      }));
    }
  }, [filters]);

  const loadNext = useCallback(async () => {
    if (state.loadingNext || !state.hasMoreNext || !state.nextCursor) return;

    setState((s) => ({ ...s, loadingNext: true }));

    try {
      const response = await invoke<QueryResponse>("query_logcat_v2", {
        filters,
        cursor: state.nextCursor,
        limit: BATCH_SIZE,
        direction: "forward" as CursorDirection,
      });

      setState((s) => {
        let newRows = [...s.rows, ...response.rows];
        let newFirstItemIndex = s.firstItemIndex;
        let didTrim = false;

        // Dynamic buffer management based on total rows
        const maxBuffer = getMaxBufferSize(s.stats?.totalRows ?? newRows.length);
        if (newRows.length > maxBuffer) {
          const excess = newRows.length - maxBuffer;
          newRows = newRows.slice(excess);
          newFirstItemIndex += excess;
          didTrim = true;
        }

        return {
          ...s,
          rows: newRows,
          nextCursor: response.nextCursor,
          hasMoreNext: response.hasMoreNext,
          hasMorePrev: didTrim ? true : s.hasMorePrev,
          loadingNext: false,
          firstItemIndex: newFirstItemIndex,
        };
      });
    } catch (e: any) {
      // Don't show error if we already have data - just stop loading
      const errMsg = e?.message || String(e);
      const isStaleError = errMsg.includes("cursor invalid") || errMsg.includes("Filter changed");
      setState((s) => ({
        ...s,
        loadingNext: false,
        hasMoreNext: isStaleError ? false : s.hasMoreNext,
        error: s.rows.length > 0 ? null : errMsg,
      }));
    }
  }, [filters, state.loadingNext, state.hasMoreNext, state.nextCursor, state.stats]);

  const loadPrev = useCallback(async () => {
    if (state.loadingPrev || !state.hasMorePrev || !state.prevCursor) return;

    setState((s) => ({ ...s, loadingPrev: true }));

    try {
      const response = await invoke<QueryResponse>("query_logcat_v2", {
        filters,
        cursor: state.prevCursor,
        limit: BATCH_SIZE,
        direction: "backward" as CursorDirection,
      });

      setState((s) => {
        // Reverse because backward query returns in reverse order
        const newRowsToAdd = [...response.rows].reverse();
        let newRows = [...newRowsToAdd, ...s.rows];
        let newFirstItemIndex = s.firstItemIndex - newRowsToAdd.length;
        let didTrim = false;

        // Dynamic buffer management based on total rows
        const maxBuffer = getMaxBufferSize(s.stats?.totalRows ?? newRows.length);
        if (newRows.length > maxBuffer) {
          newRows = newRows.slice(0, maxBuffer);
          didTrim = true;
        }

        return {
          ...s,
          rows: newRows,
          prevCursor: response.prevCursor,
          hasMorePrev: response.hasMorePrev,
          hasMoreNext: didTrim ? true : s.hasMoreNext,
          loadingPrev: false,
          firstItemIndex: Math.max(0, newFirstItemIndex),
        };
      });
    } catch (e: any) {
      // Don't show error if we already have data - just stop loading
      const errMsg = e?.message || String(e);
      const isStaleError = errMsg.includes("cursor invalid") || errMsg.includes("Filter changed");
      setState((s) => ({
        ...s,
        loadingPrev: false,
        hasMorePrev: isStaleError ? false : s.hasMorePrev, // Stop trying if cursor is stale
        error: s.rows.length > 0 ? null : errMsg, // Only show error if no data
      }));
    }
  }, [filters, state.loadingPrev, state.hasMorePrev, state.prevCursor]);

  const jumpToTime = useCallback(async (targetTime: string) => {
    const myId = ++reqRef.current;
    setState((s) => ({ ...s, loading: true, error: null }));

    try {
      const response = await invoke<QueryResponse>("jump_to_time", {
        filters,
        targetTime,
        limit: BATCH_SIZE,
      });

      if (reqRef.current !== myId) return;

      setState((s) => ({
        ...s,
        rows: response.rows,
        nextCursor: response.nextCursor,
        prevCursor: response.prevCursor,
        hasMoreNext: response.hasMoreNext,
        hasMorePrev: response.hasMorePrev,
        loading: false,
        firstItemIndex: 0,
      }));
    } catch (e: any) {
      if (reqRef.current !== myId) return;
      setState((s) => ({
        ...s,
        loading: false,
        error: e?.message || String(e),
      }));
    }
  }, [filters]);

  return {
    ...state,
    loadInitial,
    loadNext,
    loadPrev,
    jumpToTime,
  };
}
