import React from "react";
import { render } from "@testing-library/react-native";
import BalanceCard from "../src/components/BalanceCard";
import { ThemeProvider } from "../src/ThemeContext";

// Simple mock for expo-linear-gradient and expo-status-bar
jest.mock("expo-linear-gradient", () => {
  const { View } = require("react-native");
  return {
    LinearGradient: View,
  };
});

describe("BalanceCard Component", () => {
  it("renders correctly with balance visible", async () => {
    const { getByText } = await render(
      <ThemeProvider>
        <BalanceCard balance={1240.5} />
      </ThemeProvider>
    );

    expect(getByText("$1,240.50")).toBeTruthy();
    expect(getByText("AVAILABLE BALANCE")).toBeTruthy();
  });
});
