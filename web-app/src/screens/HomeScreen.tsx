import { useTheme } from "../ThemeContext";
import { useAppNavigation } from "../context/NavigationContext";
import { fonts, shadows } from "../theme";
import { currentUser as initialUser } from "../data/mockData";
import { Ionicons } from "../components/Ionicons";
import { BalanceCard } from "../components/BalanceCard";
import { ActionButtons } from "../components/ActionButtons";
import { QuickContacts } from "../components/QuickContacts";
import { TransactionItem } from "../components/TransactionItem";
import { type Transaction } from "../data/mockData";
import { useWallet } from "../context/WalletContext";

export const HomeScreen = ({ txs }: { txs: Transaction[] }) => {
  const { c } = useTheme();
  const { navigate } = useAppNavigation();
  const { balance } = useWallet();

  const hour = new Date().getHours();
  const greeting = hour < 12 ? "Good morning" : hour < 18 ? "Good afternoon" : "Good evening";
  const greetingIcon = hour < 12 ? "sunny-outline" : hour < 18 ? "partly-sunny-outline" : "moon-outline";

  return (
    <div className="fade-in">
      {/* Top Header Greetings */}
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "32px" }}>
        <div>
          <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
            <span style={{ fontSize: "14px", fontWeight: "600", color: c.mutedForeground }}>{greeting}</span>
            <Ionicons name={greetingIcon} size={14} color={c.mutedForeground} />
          </div>
          <h1 style={{ fontSize: "28px", fontWeight: "800", color: c.foreground, fontFamily: fonts.display, letterSpacing: "-1px" }}>
            Welcome back, {initialUser.name}!
          </h1>
        </div>

        <button
          onClick={() => navigate("Notifications")}
          style={{
            position: "relative",
            width: "44px",
            height: "44px",
            borderRadius: "16px",
            border: `1px solid ${c.border}80`,
            backgroundColor: c.card,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            boxShadow: shadows.card,
          }}
        >
          <Ionicons name="notifications-outline" size={20} color={c.foreground} />
          <div style={{ position: "absolute", top: "10px", right: "10px", width: "8px", height: "8px", borderRadius: "50%", backgroundColor: c.primary, border: `1.5px solid ${c.card}` }} />
        </button>
      </div>

      {/* Main Responsive Grid */}
      <div className="dashboard-grid">
        {/* Left main column */}
        <div>
          <BalanceCard balance={balance} />
          <ActionButtons />
          <QuickContacts />
        </div>

        {/* Right column */}
        <div>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "16px" }}>
            <span style={{ fontSize: "12px", fontWeight: "700", color: c.mutedForeground, letterSpacing: "1.5px" }}>
              RECENT TRANSACTIONS
            </span>
            <span
              onClick={() => navigate("Tabs")} // Triggers bottom activity naturally on mobile
              style={{ fontSize: "13px", fontWeight: "600", color: c.primary, cursor: "pointer" }}
            >
              See all
            </span>
          </div>
          <div className="glass-card" style={{ borderRadius: "28px", padding: "10px" }}>
            {txs.slice(0, 5).map((tx) => (
              <TransactionItem
                key={tx.id}
                transaction={tx}
                onClick={() => navigate("TransactionDetail", { id: tx.id })}
              />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
};
