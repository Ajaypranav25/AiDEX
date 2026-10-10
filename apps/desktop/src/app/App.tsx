"use client";

import { useState } from "react";
import { MainLayout } from "../layouts/MainLayout";
import { HomePage } from "../features/home/HomePage";
import { navigationItems, type PageId } from "../lib/navigation";

export function App() {
  const [activePage, setActivePage] = useState<PageId>("home");
  const activeLabel = navigationItems.find((item) => item.id === activePage)?.label ?? "Home";

  return (
    <MainLayout activePage={activePage} onNavigate={setActivePage}>
      <HomePage activeLabel={activeLabel} onNavigate={setActivePage} />
    </MainLayout>
  );
}
