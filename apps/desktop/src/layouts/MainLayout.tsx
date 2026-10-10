import type { ReactNode } from "react";
import { navigationItems, type PageId } from "../lib/navigation";

type Props = { activePage: PageId; onNavigate: (page: PageId) => void; children: ReactNode };

export function MainLayout({ activePage, onNavigate, children }: Props) {
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <a className="brand" href="#home" onClick={(event) => { event.preventDefault(); onNavigate("home"); }}>
          <span className="brand-mark">A</span>
          <span>AiDEX<small>PORTABLE AI WORKSPACE</small></span>
        </a>
        <div className="nav-label">WORKSPACE</div>
        <nav aria-label="Main navigation">
          {navigationItems.map(({ id, label, icon }) => (
            <button className={`nav-item${activePage === id ? " active" : ""}`} key={id} onClick={() => onNavigate(id)} aria-current={activePage === id ? "page" : undefined}>
              <span className="nav-icon" aria-hidden="true">{icon}</span>{label}
            </button>
          ))}
        </nav>
        <div className="sidebar-footer"><span className="status-dot" />Local shell <span className="version">v0.1</span></div>
      </aside>
      <main className="main-area">
        <header className="topbar"><span className="breadcrumb">AiDEX <span>/</span> {navigationItems.find((item) => item.id === activePage)?.label}</span><span className="runtime-badge"><span className="status-dot" />Core not connected</span></header>
        {children}
      </main>
    </div>
  );
}
