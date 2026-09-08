import React, { createContext, useContext, useState } from "react";

export interface SecurityContextType {
  biometricsEnabled: boolean;
  biometricThreshold: number;
  userPin: string;
  setBiometricsEnabled: (enabled: boolean) => void;
  setBiometricThreshold: (threshold: number) => void;
  setUserPin: (pin: string) => void;
}

const SecurityContext = createContext<SecurityContextType | undefined>(undefined);

export const SecurityProvider = ({ children }: { children: React.ReactNode }) => {
  const [biometricsEnabled, setBiometricsEnabled] = useState<boolean>(true);
  const [biometricThreshold, setBiometricThreshold] = useState<number>(50);
  const [userPin, setUserPin] = useState<string>("1234");

  return (
    <SecurityContext.Provider
      value={{
        biometricsEnabled,
        biometricThreshold,
        userPin,
        setBiometricsEnabled,
        setBiometricThreshold,
        setUserPin,
      }}
    >
      {children}
    </SecurityContext.Provider>
  );
};

export const useSecurity = () => {
  const context = useContext(SecurityContext);
  if (!context) throw new Error("useSecurity must be used within a SecurityProvider");
  return context;
};
