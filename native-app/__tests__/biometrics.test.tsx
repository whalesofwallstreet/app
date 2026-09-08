import React from "react";
import { renderHook, act } from "@testing-library/react-native";
import { SecurityProvider, useSecurity } from "../src/SecurityContext";

describe("SecurityContext", () => {
  const wrapper = ({ children }: { children: React.ReactNode }) => (
    <SecurityProvider>{children}</SecurityProvider>
  );

  it("provides default security settings", async () => {
    const { result } = await renderHook(() => useSecurity(), { wrapper });
    expect(result.current.biometricsEnabled).toBe(true);
    expect(result.current.biometricThreshold).toBe(50);
    expect(result.current.userPin).toBe("1234");
  });

  it("allows updating biometric threshold and status", async () => {
    const { result } = await renderHook(() => useSecurity(), { wrapper });

    act(() => {
      result.current.setBiometricThreshold(100);
    });
    expect(result.current.biometricThreshold).toBe(100);

    act(() => {
      result.current.setBiometricsEnabled(false);
    });
    expect(result.current.biometricsEnabled).toBe(false);
  });
});
