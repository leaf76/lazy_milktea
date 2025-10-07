import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
// File picker (Tauri v2 plugin)
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import type { ParseSummary } from "./types";
import "./App.css";
import AppShell from "./components/AppShell";
import LogcatView from "./components/LogcatView";

function App() {
  const [, setPath] = useState("");
  const [view, setView] = useState<"dashboard" | "logcat" | "timeline">("dashboard");
  const [summary, setSummary] = useState<ParseSummary | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // parsing entry is parsePath(p); header 'Open' triggers browse → parsePath

  async function browse() {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "Bugreport", extensions: ["zip", "txt"] }],
      } as any);
      if (typeof selected === "string" && selected) {
        setPath(selected);
        await parsePath(selected);
      } else if (Array.isArray(selected) && selected.length > 0) {
        const first = (selected[0] as any).path ?? selected[0];
        if (typeof first === "string") {
          setPath(first);
          await parsePath(first);
        }
      }
    } catch (e) {
      // swallow
    }
  }

  async function parsePath(p: string) {
    setError(null);
    setLoading(true);
    setSummary(null);
    try {
      const res = await invoke<ParseSummary>("parse_bugreport", { path: p });
      setSummary(res);
      setView("dashboard");
    } catch (e: any) {
      setError(e?.toString?.() ?? "Unknown error");
    } finally {
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
    })();
    return () => { unsubs.forEach((u) => { try { u(); } catch {} }); };
  }, []);

  return (
    <AppShell
      current={view}
      onNavigate={setView}
      headerActions={
        <>
          <button className="btn" onClick={() => setView("logcat")}>
            Logcat
          </button>
          <button className="btn btn-primary" onClick={browse} disabled={loading}>
            {loading ? "Opening..." : "Open"}
          </button>
        </>
      }
    >

      {error && (
        <p style={{ color: "#e11" }}>Error: {error}</p>
      )}

      {view === "dashboard" && (
        <section style={{ marginTop: 8 }}>
          <h2>Dashboard</h2>
          {!summary ? (
            <div className="card">No report loaded. Use Open to select a bugreport.</div>
          ) : (
            <>
              <div className="card">
                <div>
                  <strong>Brand</strong>: {summary.device.brand || "-"}
                </div>
                <div>
                  <strong>Model</strong>: {summary.device.model || "-"}
                </div>
                <div>
                  <strong>Android</strong>: {summary.device.androidVersion || "-"} (API {summary.device.apiLevel || 0})
                </div>
                <div>
                  <strong>Build ID</strong>: {summary.device.buildId || "-"}
                </div>
                <div style={{ wordBreak: "break-all" }}>
                  <strong>Fingerprint</strong>: {summary.device.fingerprint || "-"}
                </div>
                <div>
                  <strong>Report Time</strong>: {summary.device.reportTime}
                </div>
              </div>
              <div className="card" style={{ marginTop: 12 }}>
                <div>
                  <strong>Events</strong>: {summary.events}
                </div>
                <div>
                  <strong>ANR</strong>: {summary.anrs}
                </div>
                <div>
                  <strong>Crashes</strong>: {summary.crashes}
                </div>
                <div>
                  <strong>Recent E/F</strong>: {summary.efRecent} <span className="muted">(last window)</span>
                </div>
                <div>
                  <strong>Total E/F</strong>: {summary.efTotal}
                </div>
                <div style={{ marginTop: 12 }}>
                  <button className="btn btn-primary" onClick={() => setView("logcat")}>Go to Logcat</button>
                  <button
                    className="btn"
                    style={{ marginLeft: 8 }}
                    onClick={() => {
                      setView("logcat");
                      window.dispatchEvent(new CustomEvent("lm:logcat:apply", { detail: { levels: ["E","F"] } }));
                    }}
                  >
                    Quick Search E/F
                  </button>
                </div>
              </div>
            </>
          )}
        </section>
      )}

      {view === "logcat" && <LogcatView />}
    </AppShell>
  );
}

export default App;
