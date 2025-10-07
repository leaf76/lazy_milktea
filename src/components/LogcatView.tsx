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
  const [fileSize, setFileSize] = useState<number>(0);
  const [totalRows, setTotalRows] = useState<number | undefined>(undefined);
  const reqRef = useRef(0);
  const [minInput, setMinInput] = useState<string>("");
  const [maxInput, setMaxInput] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [auto, setAuto] = useState(true);
  const [wrap, setWrap] = useState(false);

  useEffect(() => {
    try { localStorage.setItem("lm.log.filters", JSON.stringify(filters)); } catch {}
  }, [filters]);

  

  const debouncedReset = useDebouncedCallback(() => {
    const myId = ++(reqRef.current);
    setRows([]);
    setCursor(undefined);
    // start fresh
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

  const levelQuick = (profile: "E" | "WE" | "IWE" | "ALL") => {
    if (profile === "E") setFilters((f) => ({ ...f, levels: ["E", "F"] }));
    if (profile === "WE") setFilters((f) => ({ ...f, levels: ["W", "E", "F"] }));
    if (profile === "IWE") setFilters((f) => ({ ...f, levels: ["I", "W", "E", "F"] }));
    if (profile === "ALL") setFilters((f) => ({ ...f, levels: undefined }));
  };

  const highlight = useCallback((text: string) => {
    const q = filters.text?.trim();
    if (!q) return <span className="log-msg">{text}</span>;
    if (filters.textMode === "regex") {
      try {
        const re = new RegExp(q, filters.caseSensitive ? "g" : "gi");
        const parts = text.split(re);
        const matches = text.match(re) || [];
        const out: JSX.Element[] = [];
        for (let i = 0; i < parts.length; i++) {
          out.push(<span key={`p${i}`}>{parts[i]}</span>);
          if (i < matches.length) out.push(<span key={`m${i}`} className="hi">{matches[i]}</span>);
        }
        return <span className="log-msg">{out}</span>;
      } catch {
        // fallback to plain
      }
    }
    const esc = q.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const re = new RegExp(`(${esc})`, filters.caseSensitive ? "g" : "gi");
    const parts = text.split(re);
    return (
      <span className="log-msg">
        {parts.map((p, i) => (re.test(p) ? <span key={i} className="hi">{p}</span> : <span key={i}>{p}</span>))}
      </span>
    );
  }, [filters.text, filters.textMode, filters.caseSensitive]);

  const setTag = (tag?: string) => setFilters((f) => ({ ...f, tag }));
  const setPid = (pid?: number) => setFilters((f) => ({ ...f, pid }));

  function toLocalInput(ms: number): string {
    const d = new Date(ms);
    const pad = (n: number, w = 2) => n.toString().padStart(w, "0");
    const yyyy = d.getFullYear();
    const MM = pad(d.getMonth() + 1);
    const dd = pad(d.getDate());
    const hh = pad(d.getHours());
    const mm = pad(d.getMinutes());
    const ss = pad(d.getSeconds());
    return `${yyyy}-${MM}-${dd}T${hh}:${mm}:${ss}`;
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

  // listen to cross-view quick apply events
  useEffect(() => {
    const handler = (e: Event) => {
      const ce = e as CustomEvent<Partial<LogFilters>>;
      setFilters((f) => ({ ...f, ...(ce.detail || {}) }));
      setTimeout(() => { debouncedReset(); }, 0);
    };
    window.addEventListener("lm:logcat:apply", handler as EventListener);
    return () => window.removeEventListener("lm:logcat:apply", handler as EventListener);
  }, [debouncedReset]);

  return (
    <section style={{ marginTop: 8 }}>
      <h2>Logcat</h2>
      <div className="card toolbar-grid">
        <div className="toolbar-row">
          <div style={{ gridColumn: "span 4" }}>
            <input className="dt-input" type="datetime-local" step="1" min={minInput || undefined} max={maxInput || undefined} placeholder="From" value={filters.tsFrom ?? ""} onChange={(e) => setFilters((f) => ({ ...f, tsFrom: clampInput(e.currentTarget.value, minInput, maxInput) }))} />
          </div>
          <div style={{ gridColumn: "span 4" }}>
            <input className="dt-input" type="datetime-local" step="1" min={minInput || undefined} max={maxInput || undefined} placeholder="To" value={filters.tsTo ?? ""} onChange={(e) => setFilters((f) => ({ ...f, tsTo: clampInput(e.currentTarget.value, minInput, maxInput) }))} />
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
          <input className="input" placeholder="TID" value={filters.tid ?? ""} onChange={(e) => setFilters((f) => ({ ...f, tid: e.currentTarget.value ? Number(e.currentTarget.value) : undefined }))} />
          <input className="input" placeholder="Text contains" value={filters.text ?? ""} onChange={(e) => setFilters((f) => ({ ...f, text: e.currentTarget.value || undefined }))} />
          <input className="input" placeholder="Text exclude" value={filters.notText ?? ""} onChange={(e) => setFilters((f) => ({ ...f, notText: e.currentTarget.value || undefined }))} />
          <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
            <button className="btn btn-ghost" type="button" onClick={() => levelQuick("E")}>E</button>
            <button className="btn btn-ghost" type="button" onClick={() => levelQuick("WE")}>W+E</button>
            <button className="btn btn-ghost" type="button" onClick={() => levelQuick("IWE")}>I+W+E</button>
            <button className="btn btn-ghost" type="button" onClick={() => levelQuick("ALL")}>All</button>
            <button className="btn btn-ghost" type="button" onClick={() => setTag("ActivityManager")}>AM</button>
            <button className="btn btn-ghost" type="button" onClick={() => { setTag(undefined); setPid(undefined); setFilters((f) => ({ ...f, levels: ["E","F"], text: undefined })); }}>E/F only</button>
          </div>
          <div style={{ display: "flex", gap: 12, alignItems: "center" }}>
            <label style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
              <input type="checkbox" checked={filters.textMode === "regex"} onChange={(e) => setFilters((f) => ({ ...f, textMode: e.currentTarget.checked ? "regex" : "plain" }))} /> Regex
            </label>
            <label style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
              <input type="checkbox" checked={!!filters.caseSensitive} onChange={(e) => setFilters((f) => ({ ...f, caseSensitive: e.currentTarget.checked || undefined }))} /> Case
            </label>
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
        {typeof filters.tid === "number" && (
          <span className="chip">tid: {filters.tid} <span className="x" onClick={() => setFilters((f) => ({ ...f, tid: undefined }))}>✕</span></span>
        )}
        {filters.levels && (
          <span className="chip">levels: {filters.levels.join(",")} <span className="x" onClick={() => setFilters((f) => ({ ...f, levels: undefined }))}>✕</span></span>
        )}
        {filters.text && (
          <span className="chip">text: {filters.text} <span className="x" onClick={() => setFilters((f) => ({ ...f, text: undefined }))}>✕</span></span>
        )}
        {filters.notText && (
          <span className="chip">not: {filters.notText} <span className="x" onClick={() => setFilters((f) => ({ ...f, notText: undefined }))}>✕</span></span>
        )}
        {(filters.textMode === "regex") && (
          <span className="chip">regex <span className="x" onClick={() => setFilters((f) => ({ ...f, textMode: "plain" }))}>✕</span></span>
        )}
        {filters.caseSensitive && (
          <span className="chip">case <span className="x" onClick={() => setFilters((f) => ({ ...f, caseSensitive: undefined }))}>✕</span></span>
        )}
        {filters.tsFrom && (
          <span className="chip">from: {filters.tsFrom} <span className="x" onClick={() => setFilters((f) => ({ ...f, tsFrom: undefined }))}>✕</span></span>
        )}
        {filters.tsTo && (
          <span className="chip">to: {filters.tsTo} <span className="x" onClick={() => setFilters((f) => ({ ...f, tsTo: undefined }))}>✕</span></span>
        )}
        {(filters.tag || filters.pid || filters.tid || filters.levels || filters.text || filters.notText || filters.tsFrom || filters.tsTo || filters.textMode === "regex" || filters.caseSensitive) && (
          <button className="btn" onClick={() => setFilters({})}>Clear</button>
        )}
      </div>

      <div className="card log-view" style={{ marginTop: 8, height: 420 }}>
        {err && <div style={{ color: '#f87171', marginBottom: 8 }}>Error: {err}</div>}
        {(fileSize > 0) && (
          <div style={{ display: "flex", alignItems: "center", gap: 12, marginBottom: 8 }}>
            <div className="progress" style={{ flex: 1 }}>
              <div className="progress-bar" style={{ width: `${Math.min(100, (cursor ?? 0) / fileSize * 100).toFixed(1)}%` }} />
            </div>
            <div className="muted" style={{ minWidth: 180, textAlign: "right" }}>
              {typeof totalRows === 'number' ? `${rows.length}/${totalRows} rows` : `${rows.length} rows`} · {((cursor ?? 0) / Math.max(1, fileSize) * 100).toFixed(1)}%
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
