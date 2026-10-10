import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react";
import type { Route } from "../../lib/routes";

type Navigation = {
  route: Route;
  /** Opens a route and records the current one for `back`. */
  navigate: (route: Route) => void;
  /** Replaces the current route without a history entry, e.g. when switching tabs. */
  replace: (route: Route) => void;
  back: () => void;
  canGoBack: boolean;
};

const NavigationContext = createContext<Navigation | null>(null);

export function NavigationProvider({ initialRoute, children }: { initialRoute: Route; children: ReactNode }) {
  const [stack, setStack] = useState<readonly Route[]>([initialRoute]);

  const navigate = useCallback((route: Route) => setStack((current) => [...current, route]), []);
  const replace = useCallback((route: Route) => setStack((current) => [...current.slice(0, -1), route]), []);
  const back = useCallback(() => setStack((current) => (current.length > 1 ? current.slice(0, -1) : current)), []);

  const value = useMemo(
    () => ({ route: stack[stack.length - 1], navigate, replace, back, canGoBack: stack.length > 1 }),
    [stack, navigate, replace, back],
  );

  return <NavigationContext.Provider value={value}>{children}</NavigationContext.Provider>;
}

export function useNavigation(): Navigation {
  const navigation = useContext(NavigationContext);
  if (!navigation) throw new Error("useNavigation must be used inside NavigationProvider");
  return navigation;
}
