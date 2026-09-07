import React from "react";
import { renderHook, act } from "@testing-library/react";
import { BalanceProvider, useWalletBalance } from "../BalanceContext";
import { currentUser } from "../../data/mockData";

describe("BalanceContext", () => {
  const wrapper = ({ children }: { children: React.ReactNode }) => (
    <BalanceProvider>{children}</BalanceProvider>
  );

  it("provides default user balance", () => {
    const { result } = renderHook(() => useWalletBalance(), { wrapper });
    expect(result.current.balance).toBe(currentUser.balance);
  });

  it("allows updating balance via updateBalance", () => {
    const { result } = renderHook(() => useWalletBalance(), { wrapper });
    act(() => {
      result.current.updateBalance(50);
    });
    expect(result.current.balance).toBe(currentUser.balance + 50);

    act(() => {
      result.current.updateBalance(-20);
    });
    expect(result.current.balance).toBe(currentUser.balance + 30);
  });

  it("throws error when used outside BalanceProvider", () => {
    expect(() => renderHook(() => useWalletBalance())).toThrow(
      "useWalletBalance must be used within a BalanceProvider"
    );
  });
});
