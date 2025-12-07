import { useMemo, useCallback } from "react";
import type { LogFilters } from "../../../types";

const SAFE_HIGHLIGHT_LIMIT = 500;

// Simple dangerous pattern detection
const DANGEROUS_PATTERNS = [
  /\(\.\+\)\+/,
  /\(\.\*\)\*/,
  /\(a\+\)\+/,
  /\(a\*\)\*/,
];

function isDangerousRegex(pattern: string): boolean {
  return DANGEROUS_PATTERNS.some((dp) => dp.test(pattern));
}

export function useLogHighlight(filters: LogFilters) {
  const highlightFn = useMemo((): ((text: string) => { parts: string[]; matches: string[] }) => {
    const query = filters.text?.trim();
    if (!query) {
      return (text: string) => ({ parts: [text], matches: [] });
    }

    const mode = filters.textMode || "plain";
    const caseSensitive = filters.caseSensitive || false;

    if (mode === "regex") {
      // Check for dangerous patterns
      if (isDangerousRegex(query)) {
        console.warn("Potentially dangerous regex, falling back to plain text");
        return createPlainHighlight(query, caseSensitive);
      }

      try {
        const re = new RegExp(query, caseSensitive ? "g" : "gi");
        return createRegexHighlight(re);
      } catch {
        return createPlainHighlight(query, caseSensitive);
      }
    }

    return createPlainHighlight(query, caseSensitive);
  }, [filters.text, filters.textMode, filters.caseSensitive]);

  return useCallback(
    (text: string): { parts: string[]; matches: string[] } => {
      // Skip highlighting for very long strings
      if (text.length > SAFE_HIGHLIGHT_LIMIT) {
        return { parts: [text], matches: [] };
      }
      return highlightFn(text);
    },
    [highlightFn]
  );
}

function createPlainHighlight(
  query: string,
  caseSensitive: boolean
): (text: string) => { parts: string[]; matches: string[] } {
  return (text: string) => {
    const searchText = caseSensitive ? text : text.toLowerCase();
    const searchQuery = caseSensitive ? query : query.toLowerCase();

    const parts: string[] = [];
    const matches: string[] = [];
    let lastIndex = 0;

    let index = searchText.indexOf(searchQuery, lastIndex);
    while (index !== -1) {
      parts.push(text.slice(lastIndex, index));
      matches.push(text.slice(index, index + query.length));
      lastIndex = index + query.length;
      index = searchText.indexOf(searchQuery, lastIndex);
    }

    parts.push(text.slice(lastIndex));
    return { parts, matches };
  };
}

function createRegexHighlight(
  re: RegExp
): (text: string) => { parts: string[]; matches: string[] } {
  return (text: string) => {
    const parts = text.split(re);
    const matches = text.match(re) || [];
    return { parts, matches };
  };
}
