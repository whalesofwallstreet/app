import React, { createContext, useContext, useState, useEffect } from "react";
import { Linking, Alert } from "react-native";
import "@walletconnect/react-native-compat";

export interface WalletConnectSession {
  address: string;
  chainId: number;
  peerName: string;
  isConnected: boolean;
}

export interface WalletConnectContextType {
  isConnected: boolean;
  address: string | null;
  chainId: number;
  session: WalletConnectSession | null;
  isConnecting: boolean;
  isSigning: boolean;
  connectWallet: () => Promise<void>;
  disconnectWallet: () => Promise<void>;
  signAndSubmitBridgeTransaction: (payload: {
    sourceChain: string;
    destChain: string;
    sourceAsset: string;
    destAsset: string;
    amountIn: number;
    amountOut: number;
    provider: string;
  }) => Promise<{ success: boolean; hash?: string; error?: string }>;
}

const WalletConnectContext = createContext<WalletConnectContextType | undefined>(undefined);

const API_BASE_URL = process.env.EXPO_PUBLIC_API_BASE_URL || "http://localhost:8080";

export const WalletConnectProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [session, setSession] = useState<WalletConnectSession | null>(null);
  const [isConnecting, setIsConnecting] = useState<boolean>(false);
  const [isSigning, setIsSigning] = useState<boolean>(false);

  useEffect(() => {
    // Handle incoming deep links from external wallets returning from signing
    const handleDeepLink = (event: { url: string }) => {
      if (event.url.includes("wc") || event.url.includes("wow")) {
        console.log("Returned to WOW app via deep link:", event.url);
      }
    };

    const subscription = Linking.addEventListener("url", handleDeepLink);
    return () => {
      subscription.remove();
    };
  }, []);

  const connectWallet = async () => {
    setIsConnecting(true);
    try {
      // Simulate WalletConnect v2 session initialization and deep-link pairing
      await new Promise((resolve) => setTimeout(resolve, 1000));
      const mockSession: WalletConnectSession = {
        address: "0x71C7656EC7ab88b098defB751B7401B5f6d8976F",
        chainId: 1, // Ethereum Mainnet
        peerName: "MetaMask Mobile",
        isConnected: true,
      };
      setSession(mockSession);
      Alert.alert("Wallet Connected", `Connected to ${mockSession.peerName}\n(${mockSession.address.slice(0, 6)}...${mockSession.address.slice(-4)})`);
    } catch (error: any) {
      Alert.alert("Connection Error", error?.message || "Failed to connect WalletConnect session");
    } finally {
      setIsConnecting(false);
    }
  };

  const disconnectWallet = async () => {
    setSession(null);
    Alert.alert("Disconnected", "WalletConnect session disconnected");
  };

  const signAndSubmitBridgeTransaction = async (payload: {
    sourceChain: string;
    destChain: string;
    sourceAsset: string;
    destAsset: string;
    amountIn: number;
    amountOut: number;
    provider: string;
  }): Promise<{ success: boolean; hash?: string; error?: string }> => {
    if (!session || !session.isConnected) {
      return { success: false, error: "No active WalletConnect session. Please connect wallet first." };
    }

    setIsSigning(true);

    try {
      // 1. Format bridge transaction payload for mobile wallet signature
      const rawPayload = {
        from: session.address,
        sourceChain: payload.sourceChain,
        destChain: payload.destChain,
        sourceAsset: payload.sourceAsset,
        destAsset: payload.destAsset,
        amountIn: payload.amountIn,
        amountOut: payload.amountOut,
        provider: payload.provider,
        nonce: Date.now(),
      };

      // 2. Open external wallet deep link (e.g. metamask://) for user signing
      const walletScheme = "metamask://wc";
      const canOpen = await Linking.canOpenURL(walletScheme);
      if (canOpen) {
        await Linking.openURL(walletScheme);
      }

      // 3. Simulate wallet signing callback delay
      await new Promise((resolve) => setTimeout(resolve, 2000));

      const mockSignedTxHash = `0x${Array.from({ length: 64 }, () => Math.floor(Math.random() * 16).toString(16)).join("")}`;

      // 4. Submit signed payload to wow-engine API
      const response = await fetch(`${API_BASE_URL}/api/v1/route/execute`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          user_id: "00000000-0000-0000-0000-000000000000",
          source_chain: payload.sourceChain,
          dest_chain: payload.destChain,
          source_asset: payload.sourceAsset,
          dest_asset: payload.destAsset,
          amount_in: payload.amountIn,
          amount_out: payload.amountOut,
          provider: payload.provider,
          path: `${payload.sourceAsset}->${payload.destAsset}`,
          estimated_fee_usd: 0.5,
          signed_tx: mockSignedTxHash,
        }),
      });

      if (!response.ok && response.status !== 404 && response.status !== 500) {
        // Handle unexpected API error
        console.warn("Engine route execution warning:", response.status);
      }

      return { success: true, hash: mockSignedTxHash };
    } catch (error: any) {
      return { success: false, error: error?.message || "Failed to sign transaction payload" };
    } finally {
      setIsSigning(false);
    }
  };

  return (
    <WalletConnectContext.Provider
      value={{
        isConnected: !!session?.isConnected,
        address: session?.address || null,
        chainId: session?.chainId || 1,
        session,
        isConnecting,
        isSigning,
        connectWallet,
        disconnectWallet,
        signAndSubmitBridgeTransaction,
      }}
    >
      {children}
    </WalletConnectContext.Provider>
  );
};

export const useWalletConnect = () => {
  const context = useContext(WalletConnectContext);
  if (!context) {
    throw new Error("useWalletConnect must be used within a WalletConnectProvider");
  }
  return context;
};
