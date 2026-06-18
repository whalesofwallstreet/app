import React, { createContext, useContext, useState, useEffect } from "react";
import { useColorScheme } from "react-native";
import { colors } from "./theme";

import AsyncStorage from "@react-native-async-storage/async-storage";

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
  const systemScheme = useColorScheme();
  const [theme, setThemeState] = useState<Theme>(systemScheme === "dark" ? "dark" : "light");

  useEffect(() => {
    AsyncStorage.getItem("user_theme").then((savedTheme) => {
      if (savedTheme === "dark" || savedTheme === "light") {
        setThemeState(savedTheme);
      }
    });
  }, []);

  const setTheme = (t: Theme) => {
    setThemeState(t);
    AsyncStorage.setItem("user_theme", t);
  };

  const c = theme === "dark" ? colors.dark : colors.light;

  return (
    <ThemeContext.Provider value={{ theme, setTheme, c }}>
      {children}
    </ThemeContext.Provider>
  );
};

export const useTheme = () => useContext(ThemeContext);
