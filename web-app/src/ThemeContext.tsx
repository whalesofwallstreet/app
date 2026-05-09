import React, { createContext, useContext, useState, useEffect } from "react";
import { colors } from "./theme";

type Theme = "light" | "dark";

const ThemeContext = createContext<{
  theme: Theme;
  setTheme: (t: Theme) => void;
  c: typeof colors.light;
}>({
  theme: "light",
  setTheme: () => {},
  c: colors.light,
});

export const ThemeProvider = ({ children }: { children: React.ReactNode }) => {
  const [theme, setThemeState] = useState<Theme>(() => {
    if (typeof window !== "undefined" && window.matchMedia) {
      return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
    }
    return "light";
  });

  const setTheme = (t: Theme) => {
    setThemeState(t);
  };

  useEffect(() => {
    const root = window.document.documentElement;
    if (theme === "dark") {
      root.classList.add("dark");
    } else {
      root.classList.remove("dark");
    }
  }, [theme]);

  const c = theme === "dark" ? colors.dark : colors.light;

  return (
    <ThemeContext.Provider value={{ theme, setTheme, c }}>
      {children}
    </ThemeContext.Provider>
  );
};

export const useTheme = () => useContext(ThemeContext);
