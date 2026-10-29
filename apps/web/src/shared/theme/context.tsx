import {
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useState,
  useSyncExternalStore,
} from "react";
import { followSystem, storedTheme, storeTheme, systemIsDark, type Theme } from "./model";

interface Appearance {
  readonly theme: Theme;
  readonly setTheme: (theme: Theme) => void;
  readonly error: unknown;
}

const ThemeContext = createContext<Appearance | null>(null);

export function ThemeProvider({
  children,
  setNativeTheme,
}: {
  readonly children: ReactNode;
  readonly setNativeTheme: ((theme: Theme) => Promise<void>) | undefined;
}) {
  const [theme, setPreference] = useState(storedTheme);
  const [error, setError] = useState<unknown>(null);
  const systemDark = useSyncExternalStore(followSystem, systemIsDark, () => false);
  const resolved = theme === "system" ? (systemDark ? "dark" : "light") : theme;
  useLayoutEffect(() => {
    document.documentElement.setAttribute("data-theme", resolved);
  }, [resolved]);
  useEffect(() => {
    let current = true;
    setError(null);
    if (setNativeTheme) {
      void setNativeTheme(theme).catch((reason: unknown) => {
        if (current) setError(reason);
      });
    }
    return () => {
      current = false;
    };
  }, [theme, setNativeTheme]);
  const value = useMemo<Appearance>(
    () => ({
      theme,
      error,
      setTheme: (next) => {
        storeTheme(next);
        setPreference(next);
      },
    }),
    [theme, error],
  );
  return <ThemeContext value={value}>{children}</ThemeContext>;
}

export function useTheme(): Appearance {
  const theme = useContext(ThemeContext);
  if (theme === null) throw new Error("useTheme outside ThemeProvider");
  return theme;
}
