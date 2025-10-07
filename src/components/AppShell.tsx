import type { PropsWithChildren } from "react";

type AppShellProps = PropsWithChildren<{
  current?: "dashboard" | "logcat" | "timeline";
  headerActions?: React.ReactNode;
  onNavigate?: (key: "dashboard" | "logcat" | "timeline") => void;
}>;

export default function AppShell({ children, current = "dashboard", headerActions, onNavigate }: AppShellProps) {
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
    </div>
  );
}
