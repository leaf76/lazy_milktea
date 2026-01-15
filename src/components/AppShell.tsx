import type { PropsWithChildren } from "react";

export type ParseProgress = {
  percent: number;
  phase: string;
  bytesRead: number;
  totalBytes: number;
  rowsProcessed: number;
  details?: string;
};

type AppShellProps = PropsWithChildren<{
  current?: "dashboard" | "logcat" | "timeline";
  headerActions?: React.ReactNode;
  onNavigate?: (key: "dashboard" | "logcat" | "timeline") => void;
  progress?: ParseProgress | null;
}>;

const formatBytes = (value: number) => {
  if (!value) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1);
  const sized = value / Math.pow(1024, index);
  const digits = sized < 10 ? 1 : 0;
  return `${sized.toFixed(digits)} ${units[index]}`;
};

export default function AppShell({ children, current = "dashboard", headerActions, onNavigate, progress }: AppShellProps) {
  return (
    <div className="app-shell">
      <div className="app-bg" />
      <aside className="sidebar">
        <div className="brand">Lazy Milktea</div>
        <nav className="nav" aria-label="Main navigation">
          <button className={current === "dashboard" ? "active" : ""} onClick={() => onNavigate?.("dashboard")}>Dashboard</button>
          <button className={current === "logcat" ? "active" : ""} onClick={() => onNavigate?.("logcat")}>Logcat</button>
          <button className={current === "timeline" ? "active" : ""} onClick={() => onNavigate?.("timeline")}>Timeline</button>
        </nav>
      </aside>
      <header className="header">
        <div className="title">Bugreport Analyzer</div>
        <div style={{ marginLeft: "auto", display: "flex", gap: 8 }}>{headerActions}</div>
      </header>
      <main className="main">{children}</main>

      {progress && (
        <div className="progress-overlay" role="status" aria-live="polite">
          <div className="progress-card">
            <div style={{ fontWeight: 600, marginBottom: 8 }}>Importing Bugreport...</div>
            <div className="progress-track" aria-hidden>
              <div className="progress-fill" style={{ width: `${progress.percent}%` }} />
            </div>
            <div className="progress-stats">
              <span>{progress.phase}</span>
              <span>{Math.round(progress.percent)}%</span>
            </div>
            <div className="progress-stats">
              <span>{progress.rowsProcessed.toLocaleString()} rows</span>
              <span>{formatBytes(progress.bytesRead)} / {formatBytes(progress.totalBytes)}</span>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
