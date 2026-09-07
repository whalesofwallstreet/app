import React from "react";
import { renderHook } from "@testing-library/react";
import { ConnectionProvider, useConnectionContext } from "../ConnectionContext";
import { useAccount, useConnect, useDisconnect, useBalance } from "wagmi";

jest.mock("wagmi", () => ({
  useAccount: jest.fn(),
  useConnect: jest.fn(),
  useDisconnect: jest.fn(),
  useBalance: jest.fn(),
}));

describe("ConnectionContext", () => {
  beforeEach(() => {
    jest.clearAllMocks();

    (useAccount as jest.Mock).mockReturnValue({
      address: "0x1234567890123456789012345678901234567890",
      isConnected: true,
      isConnecting: false,
      chain: { id: 11155111, name: "Sepolia" },
    });

    (useConnect as jest.Mock).mockReturnValue({
      connect: jest.fn(),
      connectors: [{ id: "injected", name: "Injected" }],
      error: null,
    });

    (useDisconnect as jest.Mock).mockReturnValue({
      disconnect: jest.fn(),
      error: null,
    });

    (useBalance as jest.Mock).mockReturnValue({
      data: { value: 1000000000000000000n, decimals: 18, symbol: "ETH" },
      isLoading: false,
      refetch: jest.fn(),
      error: null,
    });
  });

  const wrapper = ({ children }: { children: React.ReactNode }) => (
    <ConnectionProvider>{children}</ConnectionProvider>
  );

  it("provides wallet connection state through context", () => {
    const { result } = renderHook(() => useConnectionContext(), { wrapper });
    expect(result.current.address).toBe("0x1234567890123456789012345678901234567890");
    expect(result.current.isConnected).toBe(true);
    expect(result.current.chain?.name).toBe("Sepolia");
  });

  it("throws error when used outside ConnectionProvider", () => {
    expect(() => renderHook(() => useConnectionContext())).toThrow(
      "useConnectionContext must be used within a ConnectionProvider"
    );
  });
});
