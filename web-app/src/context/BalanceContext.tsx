import React, { createContext, useContext, useState } from "react";
import { currentUser as initialUser } from "../data/mockData";

type BalanceContextType = {
  balance: number;
  setBalance: React.Dispatch<React.SetStateAction<number>>;
  updateBalance: (delta: number) => void;
};

const BalanceContext = createContext<BalanceContextType | undefined>(undefined);

export const BalanceProvider = ({ children }: { children: React.ReactNode }) => {
  const [balance, setBalance] = useState<number>(initialUser.balance);

  const updateBalance = (delta: number) => {
    setBalance((prev) => prev + delta);
  };

  return (
    <BalanceContext.Provider value={{ balance, setBalance, updateBalance }}>
      {children}
    </BalanceContext.Provider>
  );
};

export const useWalletBalance = () => {
  const context = useContext(BalanceContext);
  if (!context) throw new Error("useWalletBalance must be used within a BalanceProvider");
  return context;
};
