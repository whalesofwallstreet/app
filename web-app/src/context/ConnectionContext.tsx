import React, { createContext, useContext } from "react";
import { useWalletConnection } from "../hooks/useWalletConnection";

type ConnectionContextType = ReturnType<typeof useWalletConnection>;

const ConnectionContext = createContext<ConnectionContextType | undefined>(undefined);

export const ConnectionProvider = ({ children }: { children: React.ReactNode }) => {
  const walletConnection = useWalletConnection();

  return (
    <ConnectionContext.Provider value={walletConnection}>
      {children}
    </ConnectionContext.Provider>
  );
};

export const useConnectionContext = () => {
  const context = useContext(ConnectionContext);
  if (!context) throw new Error("useConnectionContext must be used within a ConnectionProvider");
  return context;
};
