import { useCallback, useEffect, useRef, useState } from "react";
import { Virtuoso } from "react-virtuoso";
import { invoke } from "@tauri-apps/api/core";
import type { LogRow, LogFilters, LogStreamResp } from "../types";

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
  const [hasMore, setHasMore] = useState(true);
  const [fileSize, setFileSize] = useState<number>(0);
  const [totalRows, setTotalRows] = useState<number | undefined>(undefined);
  const [loading, setLoading] = useState(false);
  const [auto, setAuto] = useState(true);
  const [wrap, setWrap] = useState(false);

  useEffect(() => {
    try { localStorage.setItem("lm.log.filters", JSON.stringify(filters)); } catch {}
  }, [filters]);

  const loadMore = useCallback(async (): Promise<boolean> => {
    if (loading || !hasMore) return false;
    setLoading(true);
    try {
      const res = await invoke<LogStreamResp>("query_logcat_stream", { filters, cursor, limit: batch });
      setRows((prev) => prev.concat(res.rows));
      setCursor(res.nextCursor);
      const more = !res.exhausted && res.rows.length > 0;
      setHasMore(more);
      if (res.fileSize) setFileSize(res.fileSize);
      if (typeof res.totalRows === "number") setTotalRows(res.totalRows);
      return more;
    } finally {
      setLoading(false);
    }
  }, [filters, cursor, batch, loading, hasMore]);

  const debouncedReset = useDebouncedCallback(() => {
    setRows([]);
    setCursor(undefined);
    setHasMore(true);
    (async () => {
      // always load from start to end once
      while (await loadMore()) {
        await new Promise((r) => setTimeout(r, 0));
      }
    })();
  }, 400);
  useEffect(() => { if (auto) debouncedReset(); }, [filters, batch, auto]);

  const levelQuick = (profile: "E" | "WE" | "IWE" | "ALL") => {
    if (profile === "E") setFilters((f) => ({ ...f, levels: ["E", "F"] }));
    if (profile === "WE") setFilters((f) => ({ ...f, levels: ["W", "E", "F"] }));
    if (profile === "IWE") setFilters((f) => ({ ...f, levels: ["I", "W", "E", "F"] }));
    if (profile === "ALL") setFilters((f) => ({ ...f, levels: undefined }));
  };

  const highlight = useCallback((text: string) => {
    const q = filters.text?.trim();
    if (!q) return <span className="log-msg">{text}</span>;
    const re = new RegExp(`(${q.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")})`, "ig");
    const parts = text.split(re);
    return (
      <span className="log-msg">
        {parts.map((p, i) => (re.test(p) ? <span key={i} className="hi">{p}</span> : <span key={i}>{p}</span>))}
      </span>
    );
  }, [filters.text]);

  const setTag = (tag?: string) => setFilters((f) => ({ ...f, tag }));
  const setPid = (pid?: number) => setFilters((f) => ({ ...f, pid }));

  // listen to cross-view quick apply events
  useEffect(() => {
    const handler = (e: Event) => {
      const ce = e as CustomEvent<Partial<LogFilters>>;
      setFilters((f) => ({ ...f, ...(ce.detail || {}) }));
      setRows([]);
      setCursor(undefined);
      setHasMore(true);
      // next tick to ensure filters updated
      setTimeout(() => { loadMore(); }, 0);
    };
    window.addEventListener("lm:logcat:apply", handler as EventListener);
    return () => window.removeEventListener("lm:logcat:apply", handler as EventListener);
  }, [loadMore]);

  return (
    <section style={{ marginTop: 8 }}>
      <h2>Logcat</h2>
      <div className="card toolbar-grid">
        <div className="toolbar-row">
          <div style={{ gridColumn: "span 4" }}>
            <input className="dt-input" type="datetime-local" placeholder="From" value={filters.tsFrom ?? ""} onChange={(e) => setFilters((f) => ({ ...f, tsFrom: e.currentTarget.value || undefined }))} />
          </div>
          <div style={{ gridColumn: "span 4" }}>
            <input className="dt-input" type="datetime-local" placeholder="To" value={filters.tsTo ?? ""} onChange={(e) => setFilters((f) => ({ ...f, tsTo: e.currentTarget.value || undefined }))} />
          </div>
          <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
            <button className="btn btn-primary" type="button" onClick={() => debouncedReset()} disabled={loading}>Search</button>
          </div>
          <div style={{ display: "flex", gap: 12, alignItems: "center", justifyContent: "flex-end" }}>
            <label><input type="checkbox" checked={auto} onChange={(e) => setAuto(e.currentTarget.checked)} /> Auto</label>
            <label><input type="checkbox" checked={wrap} onChange={(e) => setWrap(e.currentTarget.checked)} /> Wrap</label>
          </div>
        </div>
        <div className="toolbar-row">
          <input className="input" placeholder="Tag contains" value={filters.tag ?? ""} onChange={(e) => setFilters((f) => ({ ...f, tag: e.currentTarget.value || undefined }))} />
          <input className="input" placeholder="PID" value={filters.pid ?? ""} onChange={(e) => setFilters((f) => ({ ...f, pid: e.currentTarget.value ? Number(e.currentTarget.value) : undefined }))} />
          <input className="input" placeholder="Text contains" value={filters.text ?? ""} onChange={(e) => setFilters((f) => ({ ...f, text: e.currentTarget.value || undefined }))} />
          <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
            <button className="btn btn-ghost" type="button" onClick={() => levelQuick("E")}>E</button>
            <button className="btn btn-ghost" type="button" onClick={() => levelQuick("WE")}>W+E</button>
            <button className="btn btn-ghost" type="button" onClick={() => levelQuick("IWE")}>I+W+E</button>
            <button className="btn btn-ghost" type="button" onClick={() => levelQuick("ALL")}>All</button>
            <button className="btn btn-ghost" type="button" onClick={() => setTag("ActivityManager")}>AM</button>
            <button className="btn btn-ghost" type="button" onClick={() => { setTag(undefined); setPid(undefined); setFilters((f) => ({ ...f, levels: ["E","F"], text: undefined })); }}>E/F only</button>
          </div>
        </div>
      </div>

      <div className="chipbar" style={{ marginTop: 8 }}>
        {filters.tag && (
          <span className="chip">tag: {filters.tag} <span className="x" onClick={() => setTag(undefined)}>✕</span></span>
        )}
        {typeof filters.pid === "number" && (
          <span className="chip">pid: {filters.pid} <span className="x" onClick={() => setPid(undefined)}>✕</span></span>
        )}
        {filters.levels && (
          <span className="chip">levels: {filters.levels.join(",")} <span className="x" onClick={() => setFilters((f) => ({ ...f, levels: undefined }))}>✕</span></span>
        )}
        {filters.text && (
          <span className="chip">text: {filters.text} <span className="x" onClick={() => setFilters((f) => ({ ...f, text: undefined }))}>✕</span></span>
        )}
        {filters.tsFrom && (
          <span className="chip">from: {filters.tsFrom} <span className="x" onClick={() => setFilters((f) => ({ ...f, tsFrom: undefined }))}>✕</span></span>
        )}
        {filters.tsTo && (
          <span className="chip">to: {filters.tsTo} <span className="x" onClick={() => setFilters((f) => ({ ...f, tsTo: undefined }))}>✕</span></span>
        )}
        {(filters.tag || filters.pid || filters.levels || filters.text || filters.tsFrom || filters.tsTo) && (
          <button className="btn" onClick={() => setFilters({})}>Clear</button>
        )}
      </div>

      <div className="card log-view" style={{ marginTop: 8, height: 420 }}>
        {(fileSize > 0) && (
          <div style={{ display: "flex", alignItems: "center", gap: 12, marginBottom: 8 }}>
            <div className="progress" style={{ flex: 1 }}>
              <div className="progress-bar" style={{ width: `${Math.min(100, (cursor ?? 0) / fileSize * 100).toFixed(1)}%` }} />
            </div>
            <div className="muted" style={{ minWidth: 140, textAlign: "right" }}>
              {totalRows ? `${rows.length}/${totalRows} rows` : `${rows.length} rows`} · {((cursor ?? 0) / Math.max(1, fileSize) * 100).toFixed(1)}%
            </div>
          </div>
        )}
        <Virtuoso
          totalCount={rows.length}
          itemContent={(index) => {
            const r = rows[index];
            if (!r) return null;
            return (
              <div className="log-row log-grid" style={{ whiteSpace: wrap ? "pre-wrap" : "pre" }}>
                <span className="log-col-ts">{r.ts}</span>
                <span className="log-col-pid" onClick={() => setPid(r.pid)} title="Filter by PID">{String(r.pid).padStart(5, " ")}/{String(r.tid).padStart(5, " ")}</span>
                <span className={`log-col-level level-${r.level}`}>{r.level}</span>
                <span className="log-col-tag" onClick={() => setTag(r.tag)} title="Filter by Tag">{r.tag}</span>
                <span className="log-col-msg">{highlight(r.msg)}</span>
              </div>
            );
          }}
        />
        {!loading && rows.length === 0 && <div className="muted">No results</div>}
      </div>
    </section>
  );
}
