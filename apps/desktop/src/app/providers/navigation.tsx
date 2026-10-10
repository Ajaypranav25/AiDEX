import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react";
import { useHistoryShortcuts } from "../../hooks/useHistoryShortcuts";
import { pageTabs, tabOf, type Route, type TabbedPage } from "../../lib/routes";

/** Entries the history keeps; opening more drops the oldest. */
export const HISTORY_LIMIT = 50;

type Navigation = {
  route: Route;
  /** Opens a route and records the current one for `back`. Reopening the current route adds no entry. */
  navigate: (route: Route) => void;
  /** Replaces the current route without a history entry, e.g. when switching tabs. */
  replace: (route: Route) => void;
  back: () => void;
  forward: () => void;
  canGoBack: boolean;
  canGoForward: boolean;
};

/** Visited routes, oldest first, and the position of the one on screen. */
type History = { entries: readonly Route[]; index: number };

/** The tab a route shows: a tabbed page without a valid `tab` shows its first one. */
const shownTab = (route: Route) => (route.page in pageTabs ? tabOf(route, route.page as TabbedPage) : route.tab);

/** Whether two routes show the same screen. */
const sameRoute = (a: Route, b: Route) => a.page === b.page && a.id === b.id && shownTab(a) === shownTab(b);

function opened(history: History, route: Route): History {
  if (sameRoute(history.entries[history.index], route)) return history;
  const entries = [...history.entries.slice(0, history.index + 1), route].slice(-HISTORY_LIMIT);
  return { entries, index: entries.length - 1 };
}

function replaced(history: History, route: Route): History {
  const { entries, index } = history;
  if (sameRoute(entries[index], route)) return history;
  const without = [...entries.slice(0, index), ...entries.slice(index + 1)];
  // Matching a neighbour would leave two identical entries and a Back or Forward that does nothing.
  if (index > 0 && sameRoute(entries[index - 1], route)) return { entries: without, index: index - 1 };
  if (index < entries.length - 1 && sameRoute(entries[index + 1], route)) return { entries: without, index };
  return { entries: entries.map((entry, position) => (position === index ? route : entry)), index };
}

function movedBy(history: History, step: -1 | 1): History {
  const index = history.index + step;
  return index < 0 || index >= history.entries.length ? history : { ...history, index };
}

const NavigationContext = createContext<Navigation | null>(null);

export function NavigationProvider({ initialRoute, children }: { initialRoute: Route; children: ReactNode }) {
  const [history, setHistory] = useState<History>({ entries: [initialRoute], index: 0 });

  const navigate = useCallback((route: Route) => setHistory((current) => opened(current, route)), []);
  const replace = useCallback((route: Route) => setHistory((current) => replaced(current, route)), []);
  const back = useCallback(() => setHistory((current) => movedBy(current, -1)), []);
  const forward = useCallback(() => setHistory((current) => movedBy(current, 1)), []);

  useHistoryShortcuts(back, forward);

  const value = useMemo(
    () => ({
      route: history.entries[history.index],
      navigate,
      replace,
      back,
      forward,
      canGoBack: history.index > 0,
      canGoForward: history.index < history.entries.length - 1,
    }),
    [history, navigate, replace, back, forward],
  );

  return <NavigationContext.Provider value={value}>{children}</NavigationContext.Provider>;
}

export function useNavigation(): Navigation {
  const navigation = useContext(NavigationContext);
  if (!navigation) throw new Error("useNavigation must be used inside NavigationProvider");
  return navigation;
}
