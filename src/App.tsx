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
        <section className="dashboard-container">
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
                <div className="dashboard-container">
                  <h2 className="sr-only">Dashboard</h2>

                <div className="dashboard-hero">
                  <h2>System Health Report</h2>
                  <div className="report-meta">
                    <span className="report-meta-item"><span className="report-meta-label">Report Time</span>{summary.device.reportTime}</span>
                    <span className="report-meta-item"><span className="report-meta-label">Device</span>{summary.device.brand} {summary.device.model}</span>
                  </div>
                </div>

                <div className="stats-grid">
                  <div className="stat-card danger">
                    <div className="stat-label">Crashes</div>
                    <div className="stat-value">{summary.crashes}</div>
                    <div className="muted">Critical Failures</div>
                  </div>
                  <div className="stat-card warn">
                    <div className="stat-label">ANRs</div>
                    <div className="stat-value">{summary.anrs}</div>
                    <div className="muted">App Not Responding</div>
                  </div>
                  <div className="stat-card info">
                    <div className="stat-label">Errors & Fatal</div>
                    <div className="stat-value">{summary.efTotal}</div>
                    <div className="muted">Recent: {summary.efRecent}</div>
                  </div>
                  <div className="stat-card success">
                    <div className="stat-label">Battery</div>
                    {summary.device.battery ? (
                      <>
                        <div className="stat-value">{summary.device.battery.level}%</div>
                        <div className="muted">{summary.device.battery.tempC}°C · {summary.device.battery.status}</div>
                      </>
                    ) : (
                      <div className="stat-value is-muted">N/A</div>
                    )}
                  </div>
                </div>

                <div className="device-section">
                  <div className="card">
                    <h3 className="section-title">Device Configuration</h3>
                    <div className="detail-grid">
                      <div className="detail-item">
                        <div className="detail-label">Brand</div>
                        <div className="detail-value">{summary.device.brand}</div>
                      </div>
                      <div className="detail-item">
                        <div className="detail-label">Model</div>
                        <div className="detail-value">{summary.device.model}</div>
                      </div>
                      <div className="detail-item">
                        <div className="detail-label">Android Version</div>
                        <div className="detail-value">{summary.device.androidVersion}</div>
                      </div>
                      <div className="detail-item">
                        <div className="detail-label">API Level</div>
                        <div className="detail-value">{summary.device.apiLevel}</div>
                      </div>
                      <div className="detail-item span-2">
                        <div className="detail-label">Build ID</div>
                        <div className="detail-value">{summary.device.buildId}</div>
                      </div>
                      <div className="detail-item span-2">
                        <div className="detail-label">Fingerprint</div>
                        <div className="detail-value detail-mono">{summary.device.fingerprint}</div>
                      </div>
                      <div className="detail-item">
                        <div className="detail-label">Uptime</div>
                        <div className="detail-value">{Math.floor(summary.device.uptimeMs / 3600000)}h {Math.round((summary.device.uptimeMs % 3600000) / 60000)}m</div>
                      </div>
                      <div className="detail-item">
                        <div className="detail-label">Total Events</div>
                        <div className="detail-value">{summary.events.toLocaleString()}</div>
                      </div>
                    </div>
                  </div>

                  <div className="actions-panel">
                    <h3>Quick Actions</h3>
                    <p className="muted actions-hint">
                      Jump directly to logs to investigate issues identified in this report.
                    </p>
                    <button className="btn btn-primary actions-button" onClick={() => setView("logcat")}>
                      Explore Logcat
                    </button>
                    <button
                      className="btn actions-button actions-danger"
                      onClick={() => {
                        setView("logcat");
                        window.dispatchEvent(new CustomEvent("lm:logcat:apply", { detail: { levels: ["E", "F"] } }));
                      }}
                    >
                      Filter Critical Errors
                    </button>
                  </div>
                </div>
              </div>
            </>
          )}
        </section>
      )}

      {view === "timeline" && (
        <section className="timeline-container">
          {!summary ? (
            <div className="empty-state" style={{ gridColumn: "1 / -1" }}>
              <div className="empty-icon">
                <svg fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
              </div>
              <div style={{ fontSize: "1.25rem", fontWeight: 600, marginBottom: 4 }}>No Timeline Data</div>
              <div className="muted">Load a bugreport to visualize the timeline of events.</div>
              <button className="btn btn-primary" onClick={browse} style={{ marginTop: 24, padding: "10px 24px" }}>
                Open Bugreport
              </button>
            </div>
          ) : (
            <>
              <div className="timeline-sidebar">
                <div className="timeline-summary-card">
                  <h3 style={{ marginTop: 0, fontSize: "0.9rem", textTransform: "uppercase", letterSpacing: "0.05em", color: "var(--muted)" }}>Timeline Summary</h3>
                  <div style={{ fontSize: "1.5rem", fontWeight: 700, margin: "8px 0" }}>
                    {((summary.crashes + summary.anrs + summary.efTotal) > 0) ? "Issues Found" : "System Normal"}
                  </div>
                  <div className="muted" style={{ fontSize: "0.9rem" }}>
                    Analysis covering {Math.floor(summary.device.uptimeMs / 3600000)}h {Math.round((summary.device.uptimeMs % 3600000) / 60000)}m of uptime.
                  </div>
                </div>

                <div className="timeline-legend">
                  <div className="legend-item">
                    <span className="legend-dot" style={{ background: "var(--accent)" }}></span>
                    <span>System / Report</span>
                  </div>
                  <div className="legend-item">
                    <span className="legend-dot" style={{ background: "var(--error)" }}></span>
                    <span>Crash / Fatal</span>
                  </div>
                  <div className="legend-item">
                    <span className="legend-dot" style={{ background: "var(--warn)" }}></span>
                    <span>ANR</span>
                  </div>
                  <div className="legend-item">
                    <span className="legend-dot" style={{ background: "var(--primary)" }}></span>
                    <span>Error</span>
                  </div>
                </div>
              </div>

              <div className="timeline-feed">
                <div className="timeline-item">
                  <div className="timeline-marker success">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><path d="M18.36 6.64a9 9 0 1 1-12.73 0"></path><line x1="12" y1="2" x2="12" y2="12"></line></svg>
                  </div>
                  <div className="timeline-content">
                    <span className="timeline-time">
                      ~{Math.floor(summary.device.uptimeMs / 3600000)} hours before report
                    </span>
                    <h3 className="timeline-title">System Boot</h3>
                    <div className="muted">
                      Device started. Android {summary.device.androidVersion} (API {summary.device.apiLevel}).
                      <br />
                      Build: {summary.device.buildId}
                    </div>
                  </div>
                </div>

                {(summary.crashes > 0 || summary.anrs > 0 || summary.efTotal > 0) && (
                  <div className="timeline-item">
                    <div className={`timeline-marker ${summary.crashes > 0 ? "danger" : "warn"}`}>
                      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"></path><line x1="12" y1="9" x2="12" y2="13"></line><line x1="12" y1="17" x2="12.01" y2="17"></line></svg>
                    </div>
                    <div className="timeline-content" style={{ borderColor: summary.crashes > 0 ? "rgba(239,68,68,0.3)" : undefined }}>
                      <span className="timeline-time">During Uptime</span>
                      <h3 className="timeline-title" style={{ color: summary.crashes > 0 ? "var(--error)" : "var(--warn)" }}>
                        Critical Issues Detected
                      </h3>
                      <div className="muted">
                        Analysis identified potential stability issues during this session.
                      </div>
                      <div className="timeline-stats">
                        {summary.crashes > 0 && (
                          <div className="t-stat">
                            <span className="t-stat-val" style={{ color: "var(--error)" }}>{summary.crashes}</span>
                            <span className="t-stat-label">Crashes</span>
                          </div>
                        )}
                        {summary.anrs > 0 && (
                          <div className="t-stat">
                            <span className="t-stat-val" style={{ color: "var(--warn)" }}>{summary.anrs}</span>
                            <span className="t-stat-label">ANRs</span>
                          </div>
                        )}
                        {summary.efTotal > 0 && (
                          <div className="t-stat">
                            <span className="t-stat-val" style={{ color: "var(--primary)" }}>{summary.efTotal}</span>
                            <span className="t-stat-label">Errors</span>
                          </div>
                        )}
                      </div>
                    </div>
                  </div>
                )}

                <div className="timeline-item">
                  <div className="timeline-marker">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><polyline points="22 12 18 12 15 21 9 3 6 12 2 12"></polyline></svg>
                  </div>
                  <div className="timeline-content">
                    <span className="timeline-time">Log Activity</span>
                    <h3 className="timeline-title">System Logging</h3>
                    <div className="muted">
                      Processed {summary.events.toLocaleString()} log events. 
                      {summary.device.battery && ` Battery level at ${summary.device.battery.level}%.`}
                    </div>
                  </div>
                </div>

                <div className="timeline-item">
                  <div className="timeline-marker success">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path><polyline points="14 2 14 8 20 8"></polyline><line x1="16" y1="13" x2="8" y2="13"></line><line x1="16" y1="17" x2="8" y2="17"></line><polyline points="10 9 9 9 8 9"></polyline></svg>
                  </div>
                  <div className="timeline-content">
                    <span className="timeline-time">{summary.device.reportTime}</span>
                    <h3 className="timeline-title">Report Generated</h3>
                    <div className="muted">
                      Bugreport snapshot captured.
                    </div>
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
