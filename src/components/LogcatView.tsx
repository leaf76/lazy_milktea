import { useCallback, useEffect, useRef, useState } from "react";
import { Virtuoso, VirtuosoHandle } from "react-virtuoso";
import { invoke } from "@tauri-apps/api/core";
import type { LogRow, LogFilters, LogStreamResp } from "../types";
import styles from "./LogcatView.module.css";

function useDebouncedCallback(fn: () => void, delay = 400) {
  const timer = useRef<number | null>(null);
  return useCallback(() => {
    if (timer.current) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => fn(), delay);
  }, [fn, delay]);
}

export default function LogcatView() {
  const [filters, setFilters] = useState<LogFilters>(() => {
    try {
      const raw = localStorage.getItem("lm.log.filters");
      return raw ? JSON.parse(raw) : {};
    } catch {
      return {};
    }
  });
  const [rows, setRows] = useState<LogRow[]>([]);
  const [cursor, setCursor] = useState<number | undefined>(undefined);
  const [batch] = useState(1000); // internal chunk size for smooth loading
  const [fileSize, setFileSize] = useState<number>(0);
  const [totalRows, setTotalRows] = useState<number | undefined>(undefined);
  const reqRef = useRef(0);
  const [minInput, setMinInput] = useState<string>("");
  const [maxInput, setMaxInput] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [auto, setAuto] = useState(true);
  const [wrap, setWrap] = useState(false);
  const virtuosoRef = useRef<VirtuosoHandle>(null);

  useEffect(() => {
    try { localStorage.setItem("lm.log.filters", JSON.stringify(filters)); } catch {}
  }, [filters]);

  const debouncedReset = useDebouncedCallback(() => {
    const myId = ++(reqRef.current);
    setRows([]);
    setCursor(undefined);
    setTotalRows(undefined);
    setLoading(true);
    (async () => {
      let cur: number | undefined = undefined;
      let loaded = 0;
      try {
        setErr(null);
        while (true) {
          const res: LogStreamResp = await invoke<LogStreamResp>("query_logcat_stream", { filters, cursor: cur, limit: batch });
          if (reqRef.current !== myId) return;
          if (res.fileSize) setFileSize(res.fileSize);
          if (res.minIsoMs && res.maxIsoMs) {
            setMinInput(toLocalInput(res.minIsoMs));
            setMaxInput(toLocalInput(res.maxIsoMs));
          }
          setRows((prev) => {
            const merged = prev.concat(res.rows);
            loaded = merged.length;
            return merged;
          });
          cur = res.nextCursor;
          setCursor(res.nextCursor);
          const more = !res.exhausted && res.rows.length > 0;
          if (!more) { setTotalRows(loaded); break; }
          await new Promise((r) => setTimeout(r, 0));
        }
      } catch (e: any) {
        setErr(e?.message || String(e));
      } finally {
        setLoading(false);
      }
    })();
  }, 400);
  useEffect(() => { if (auto) debouncedReset(); }, [filters, batch, auto]);

  const highlight = useCallback((text: string) => {
    const q = filters.text?.trim();
    if (!q) return <span className={styles.logColMsg}>{text}</span>;
    try {
      const re = new RegExp(q, filters.caseSensitive ? "g" : "gi");
      const parts = text.split(re);
      const matches = text.match(re) || [];
      return (
        <span className={styles.logColMsg}>
          {parts.map((part, i) => (
            <>
              <span key={`p${i}`}>{part}</span>
              {i < matches.length && <span key={`m${i}`} className={styles.hi}>{matches[i]}</span>}
            </>
          ))}
        </span>
      );
    } catch {
      return <span className={styles.logColMsg}>{text}</span>; // fallback for invalid regex
    }
  }, [filters.text, filters.textMode, filters.caseSensitive]);

  const setTag = (tag?: string) => setFilters((f) => ({ ...f, tag }));
  const setPid = (pid?: number) => setFilters((f) => ({ ...f, pid }));

  function toLocalInput(ms: number): string {
    const d = new Date(ms);
    const pad = (n: number, w = 2) => n.toString().padStart(w, "0");
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  }

  function clampInput(v: string, minV: string, maxV: string): string | undefined {
    if (!v) return undefined;
    const toMs = (s: string) => s ? new Date(s).getTime() : undefined;
    const x = toMs(v);
    const lo = toMs(minV);
    const hi = toMs(maxV);
    if (typeof x !== 'number' || isNaN(x)) return undefined;
    if (typeof lo === 'number' && x < lo) return minV || undefined;
    if (typeof hi === 'number' && x > hi) return maxV || undefined;
    return v;
  }

  useEffect(() => {
    const handler = (e: Event) => {
      const ce = e as CustomEvent<Partial<LogFilters>>;
      setFilters((f) => ({ ...f, ...(ce.detail || {}) }));
      setTimeout(() => { debouncedReset(); }, 0);
    };
    window.addEventListener("lm:logcat:apply", handler as EventListener);
    return () => window.removeEventListener("lm:logcat:apply", handler as EventListener);
  }, [debouncedReset]);

  const hasFilters = Object.values(filters).some(v => v !== undefined);

  return (
    <section>
      <h2>Logcat</h2>
      <div className={`card ${styles.toolbar}`}>
        <div className={styles.mainControls}>
          <div style={{ gridColumn: "span 2" }}><input className={styles.input} placeholder="Tag" value={filters.tag ?? ""} onChange={(e) => setFilters((f) => ({ ...f, tag: e.currentTarget.value || undefined }))} /></div>
          <div style={{ gridColumn: "span 2" }}><input className={styles.input} placeholder="PID" value={filters.pid ?? ""} onChange={(e) => setFilters((f) => ({ ...f, pid: e.currentTarget.value ? Number(e.currentTarget.value) : undefined }))} /></div>
          <div style={{ gridColumn: "span 4" }}><input className={styles.input} placeholder="Text contains" value={filters.text ?? ""} onChange={(e) => setFilters((f) => ({ ...f, text: e.currentTarget.value || undefined }))} /></div>
        </div>
        <div className={styles.searchControls}>
          <button className="btn btn-primary" type="button" onClick={() => debouncedReset()} disabled={loading}>Search</button>
          <button className="btn" type="button" onClick={() => virtuosoRef.current?.scrollToIndex({ index: rows.length - 1, align: "end" })}>Bottom</button>
        </div>
        <div className={styles.viewOptions}>
          <label className={styles.label}><input type="checkbox" checked={auto} onChange={(e) => setAuto(e.currentTarget.checked)} /> Auto-reload</label>
          <label className={styles.label}><input type="checkbox" checked={wrap} onChange={(e) => setWrap(e.currentTarget.checked)} /> Wrap lines</label>
        </div>
        <div className={styles.toolbarRow} style={{ marginTop: "var(--space-2)" }}>
          <div style={{ gridColumn: "span 4" }}><input className={styles.dtInput} type="datetime-local" step="1" min={minInput || undefined} max={maxInput || undefined} placeholder="From" value={filters.tsFrom ?? ""} onChange={(e) => setFilters((f) => ({ ...f, tsFrom: clampInput(e.currentTarget.value, minInput, maxInput) }))} /></div>
          <div style={{ gridColumn: "span 4" }}><input className={styles.dtInput} type="datetime-local" step="1" min={minInput || undefined} max={maxInput || undefined} placeholder="To" value={filters.tsTo ?? ""} onChange={(e) => setFilters((f) => ({ ...f, tsTo: clampInput(e.currentTarget.value, minInput, maxInput) }))} /></div>
          <div style={{ gridColumn: "span 4", display: "flex", gap: "var(--space-2)", alignItems: "center" }}>
            <label className={styles.label}><input type="checkbox" checked={filters.textMode === "regex"} onChange={(e) => setFilters((f) => ({ ...f, textMode: e.currentTarget.checked ? "regex" : "plain" }))} /> Regex</label>
            <label className={styles.label}><input type="checkbox" checked={!!filters.caseSensitive} onChange={(e) => setFilters((f) => ({ ...f, caseSensitive: e.currentTarget.checked || undefined }))} /> Case</label>
          </div>
        </div>
      </div>

      {hasFilters && (
        <div className={styles.chipBar}>
          {Object.entries(filters).map(([key, value]) => {
            if (value === undefined || value === false || (Array.isArray(value) && value.length === 0)) return null;
            const displayValue = Array.isArray(value) ? value.join(',') : String(value);
            return (
              <span key={key} className={styles.chip}>
                {key}: {displayValue}
                <span className={styles.x} onClick={() => setFilters(f => ({ ...f, [key]: undefined }))}>✕</span>
              </span>
            );
          })}
          <button className={styles.clearBtn} onClick={() => setFilters({})}>Clear All</button>
        </div>
      )}

      <div className={styles.logView} style={{ marginTop: "var(--space-4)", height: 500 }}>
        <div className={styles.logHeader}>
          <span>Timestamp</span>
          <span>PID/TID</span>
          <span>Level</span>
          <span>Tag</span>
          <span>Message</span>
        </div>
        {err && <div style={{ color: 'var(--error)', padding: "var(--space-4)" }}>Error: {err}</div>}
        {(fileSize > 0) && (
          <div className={styles.progressContainer}>
            <div className={styles.progress}><div className={styles.progressBar} style={{ width: `${Math.min(100, (cursor ?? 0) / fileSize * 100).toFixed(1)}%` }} /></div>
            <div className={styles.statusText}>{typeof totalRows === 'number' ? `${rows.length}/${totalRows} rows` : `${rows.length} rows`} · {((cursor ?? 0) / Math.max(1, fileSize) * 100).toFixed(1)}%</div>
          </div>
        )}
        <Virtuoso
          ref={virtuosoRef}
          totalCount={rows.length}
          itemContent={(index) => {
            const r = rows[index];
            if (!r) return null;
            return (
              <div className={styles.logRow} style={{ whiteSpace: wrap ? "pre-wrap" : "pre" }}>
                <span className={styles.logColTs}>{r.ts}</span>
                <span className={styles.logColPid} onClick={() => setPid(r.pid)} title="Filter by PID">{String(r.pid).padStart(5, " ")}/{String(r.tid).padStart(5, " ")}</span>
                <span className={`${styles.logColLevel} level-${r.level}`}>{r.level}</span>
                <span className={styles.logColTag} onClick={() => setTag(r.tag)} title="Filter by Tag">{r.tag}</span>
                {highlight(r.msg)}
              </div>
            );
          }}
        />
        {!loading && rows.length === 0 && <div className={styles.noResults}>No results</div>}
      </div>
    </section>
  );
}
