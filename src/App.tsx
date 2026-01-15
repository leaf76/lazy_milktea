import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
// File picker (Tauri v2 plugin)
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import type { ParseSummary } from "./types";
import "./App.css";
import AppShell, { type ParseProgress } from "./components/AppShell";
import { LogcatViewV2 } from "./components/logcat";

function App() {
  const [, setPath] = useState("");
  const [view, setView] = useState<"dashboard" | "logcat" | "timeline">("dashboard");
  const [summary, setSummary] = useState<ParseSummary | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [parseProgress, setParseProgress] = useState<ParseProgress | null>(null);

  // parsing entry is parsePath(p); header 'Open' triggers browse → parsePath

  async function browse() {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "Bugreport", extensions: ["zip", "txt"] }],
      });
      if (typeof selected === "string" && selected) {
        setPath(selected);
        await parsePath(selected);
        return;
      }
      if (Array.isArray(selected) && selected.length > 0) {
        const first = selected[0];
        if (typeof first === "string") {
          setPath(first);
          await parsePath(first);
          return;
        }
        if (first && typeof first.path === "string") {
          setPath(first.path);
          await parsePath(first.path);
        }
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to open file");
    }
  }

  async function parsePath(p: string) {
    setError(null);
    setLoading(true);
    setSummary(null);
    setParseProgress({ percent: 0, phase: "starting", bytesRead: 0, totalBytes: 0, rowsProcessed: 0, details: p });
    try {
      const res = await invoke<ParseSummary>("parse_bugreport_streaming", { path: p });
      setSummary(res);
      setView("logcat");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unknown error");
    } finally {
      setParseProgress(null);
      setLoading(false);
    }
  }

  // logcat view manages its own filters and results

  useEffect(() => {
    let unsubs: Array<() => void> = [];
    (async () => {
      unsubs.push(await listen("menu://open", () => browse()));
      unsubs.push(await listen("nav://dashboard", () => setView("dashboard")));
      unsubs.push(await listen("nav://logcat", () => setView("logcat")));
      unsubs.push(await listen("nav://timeline", () => setView("timeline")));

      unsubs.push(await listen<ParseProgress>("parse://progress", (event) => {
        const payload = event.payload;
        setParseProgress({
          ...payload,
          percent: Math.min(100, Math.max(0, payload.percent)),
        });
      }));
    })();
    return () => {
      unsubs.forEach((u) => {
        try {
          u();
        } catch (err) {
          console.warn("Failed to unsubscribe", err);
        }
      });
    };
  }, []);

  return (
    <AppShell
      current={view}
      onNavigate={setView}
      progress={parseProgress}
      headerActions={
        <>
          <button className="btn btn-primary" onClick={browse} disabled={loading}>
            {loading ? "Opening..." : "Open Bugreport"}
          </button>
        </>
      }
    >

      {error && (
        <div style={{ color: "var(--error)", background: "rgba(239,68,68,0.1)", padding: 12, borderRadius: 8, marginBottom: 16, border: "1px solid rgba(239,68,68,0.2)" }}>
          <strong>Error:</strong> {error} {parseProgress?.details && <span className="muted">({parseProgress.details})</span>}
        </div>
      )}

      {view === "dashboard" && (
        <section style={{ marginTop: 8 }}>
          {!summary ? (
            <div className="empty-state">
              <div className="empty-icon">
                <svg fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M9 13h6m-3-3v6m5 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                </svg>
              </div>
              <div style={{ fontSize: "1.25rem", fontWeight: 600, marginBottom: 4 }}>No Bugreport Loaded</div>
              <div style={{ color: "var(--muted)" }}>Open a bugreport zip or txt file to start analyzing.</div>
              <button className="btn btn-primary" onClick={browse} style={{ marginTop: 24, padding: "10px 24px" }}>
                Select File
              </button>
            </div>
          ) : (
            <>
              <h2 style={{ marginBottom: 16 }}>Overview</h2>
              <div className="dashboard-grid">
                <div className="card">
                  <h3 style={{ margin: "0 0 12px 0", fontSize: "1rem", color: "var(--primary)" }}>Device Info</h3>
                  <div style={{ display: "grid", gap: 8 }}>
                    <div><strong>Brand:</strong> {summary.device.brand || "-"}</div>
                    <div><strong>Model:</strong> {summary.device.model || "-"}</div>
                    <div><strong>Android:</strong> {summary.device.androidVersion || "-"} (API {summary.device.apiLevel || 0})</div>
                    <div><strong>Build:</strong> {summary.device.buildId || "-"}</div>
                    <div style={{ wordBreak: "break-all", fontSize: "0.9em", color: "var(--muted)" }}>{summary.device.fingerprint || "-"}</div>
                    <div><strong>Time:</strong> {summary.device.reportTime}</div>
                    {summary.device.uptimeMs > 0 && (
                      <div><strong>Uptime:</strong> {Math.round(summary.device.uptimeMs / 60000)} min</div>
                    )}
                    {summary.device.battery && (
                      <div><strong>Battery:</strong> {summary.device.battery.level}% · {summary.device.battery.tempC}°C · {summary.device.battery.status}</div>
                    )}
                  </div>
                </div>

                <div className="card">
                  <h3 style={{ margin: "0 0 12px 0", fontSize: "1rem", color: "var(--primary)" }}>Statistics</h3>
                  <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
                    <div className="stat-box">
                      <div className="label muted">Events</div>
                      <div className="value" style={{ fontSize: "1.5rem", fontWeight: 600 }}>{summary.events}</div>
                    </div>
                    <div className="stat-box">
                      <div className="label muted">Crashes</div>
                      <div className="value" style={{ fontSize: "1.5rem", fontWeight: 600, color: "var(--error)" }}>{summary.crashes}</div>
                    </div>
                    <div className="stat-box">
                      <div className="label muted">ANRs</div>
                      <div className="value" style={{ fontSize: "1.5rem", fontWeight: 600, color: "var(--warn)" }}>{summary.anrs}</div>
                    </div>
                    <div className="stat-box">
                      <div className="label muted">Total E/F</div>
                      <div className="value" style={{ fontSize: "1.5rem", fontWeight: 600 }}>{summary.efTotal}</div>
                    </div>
                  </div>
                  
                  <div style={{ marginTop: 20, display: "flex", gap: 8 }}>
                    <button className="btn btn-primary" style={{ flex: 1 }} onClick={() => setView("logcat")}>
                      View Logcat
                    </button>
                    <button
                      className="btn"
                      style={{ flex: 1 }}
                      onClick={() => {
                        setView("logcat");
                        window.dispatchEvent(new CustomEvent("lm:logcat:apply", { detail: { levels: ["E", "F"] } }));
                      }}
                    >
                      Filter Errors
                    </button>
                  </div>
                </div>
              </div>
            </>
          )}
        </section>
      )}

      {view === "logcat" && <LogcatViewV2 />}
    </AppShell>
  );
}

export default App;
