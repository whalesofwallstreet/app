import React from "react";
import { renderHook, act } from "@testing-library/react-native";
import { WalletConnectProvider, useWalletConnect } from "../src/WalletConnectContext";

describe("WalletConnectContext", () => {
  const wrapper = ({ children }: { children: React.ReactNode }) => (
    <WalletConnectProvider>{children}</WalletConnectProvider>
  );

  it("provides default disconnected state", async () => {
    const { result } = await renderHook(() => useWalletConnect(), { wrapper });
    expect(result.current.isConnected).toBe(false);
    expect(result.current.address).toBeNull();
    expect(result.current.session).toBeNull();
  });

  it("connects wallet session when connectWallet is invoked", async () => {
    const { result } = await renderHook(() => useWalletConnect(), { wrapper });

    await act(async () => {
      await result.current.connectWallet();
    });

    expect(result.current.isConnected).toBe(true);
    expect(result.current.address).toBeTruthy();
    expect(result.current.session?.peerName).toBe("MetaMask Mobile");
  });

  it("disconnects session when disconnectWallet is invoked", async () => {
    const { result } = await renderHook(() => useWalletConnect(), { wrapper });

    await act(async () => {
      await result.current.connectWallet();
    });
    expect(result.current.isConnected).toBe(true);

    await act(async () => {
      await result.current.disconnectWallet();
    });
    expect(result.current.isConnected).toBe(false);
    expect(result.current.address).toBeNull();
  });
});
