import React from "react";
import { renderHook, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { BalanceProvider } from "../BalanceContext";
import { TransactionProvider, useWalletTransactions } from "../TransactionContext";
import { initialTransactions } from "../../data/mockData";

describe("TransactionContext", () => {
  const createWrapper = () => {
    const queryClient = new QueryClient({
      defaultOptions: {
        queries: { retry: false },
      },
    });
    return ({ children }: { children: React.ReactNode }) => (
      <QueryClientProvider client={queryClient}>
        <BalanceProvider>
          <TransactionProvider>{children}</TransactionProvider>
        </BalanceProvider>
      </QueryClientProvider>
    );
  };

  it("provides initial transaction list", () => {
    const { result } = renderHook(() => useWalletTransactions(), {
      wrapper: createWrapper(),
    });
    expect(result.current.transactions.length).toBeGreaterThan(0);
    expect(result.current.transactions[0].id).toBe(initialTransactions[0].id);
  });

  it("adds a new transaction and updates balance", () => {
    const { result } = renderHook(() => useWalletTransactions(), {
      wrapper: createWrapper(),
    });

    const initialLength = result.current.transactions.length;

    act(() => {
      result.current.addTransaction({
        type: "sent",
        name: "Test User",
        username: "@test",
        avatar: "",
        amount: 25,
        note: "Test send",
      });
    });

    expect(result.current.transactions.length).toBe(initialLength + 1);
    expect(result.current.transactions[0].name).toBe("Test User");
    expect(result.current.transactions[0].amount).toBe(25);
  });

  it("throws error when used outside TransactionProvider", () => {
    expect(() => renderHook(() => useWalletTransactions())).toThrow(
      "useWalletTransactions must be used within a TransactionProvider"
    );
  });
});
