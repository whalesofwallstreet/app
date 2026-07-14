
import { motion, AnimatePresence } from "framer-motion";
import { ThemeProvider, useTheme } from "./ThemeContext";
import { NavigationProvider, useAppNavigation } from "./context/NavigationContext";
import { WalletProvider, useWallet } from "./context/WalletContext";
import { Ionicons } from "./components/Ionicons";

// Import Extracted Screens
import { HomeScreen } from "./screens/HomeScreen";
import { SendScreen } from "./screens/SendScreen";
import { ActivityScreen } from "./screens/ActivityScreen";
import { ProfileScreen } from "./screens/ProfileScreen";
import { DepositScreen } from "./screens/DepositScreen";
import { WithdrawScreen } from "./screens/WithdrawScreen";
import { SaveScreen } from "./screens/SaveScreen";
import { InvestScreen } from "./screens/InvestScreen";
import { TransactionDetailScreen } from "./screens/TransactionDetailScreen";
import { NotificationsScreen } from "./screens/NotificationsScreen";

// ==========================================
// Main Responsive App Layout
// ==========================================

const AppSimulator = () => {
  const { c, theme, setTheme } = useTheme();
  const { currentScreen, activeTab, setActiveTab, navigate } = useAppNavigation();
  const { transactions, deposit } = useWallet();

  // Render correct page
  const renderScreen = () => {
    let content;
    switch (currentScreen) {
      case "Tabs":
        if (activeTab === "Home") content = <HomeScreen txs={transactions} />;
        else if (activeTab === "Send") content = <SendScreen />;
        else if (activeTab === "Activity") content = <ActivityScreen txs={transactions} />;
        else if (activeTab === "Profile") content = <ProfileScreen />;
        else content = <HomeScreen txs={transactions} />;
        break;
      case "Deposit":
        content = <DepositScreen onAddMoney={(amt) => deposit(amt, "Stellar Deposit")} />;
        break;
      case "Withdraw":
        content = <WithdrawScreen />;
        break;
      case "Save":
        content = <SaveScreen />;
        break;
      case "Invest":
        content = <InvestScreen />;
        break;
      case "TransactionDetail":
        content = <TransactionDetailScreen txs={transactions} />;
        break;
      case "Notifications":
        content = <NotificationsScreen />;
        break;
      default:
        content = <HomeScreen txs={transactions} />;
    }
    return (
      <AnimatePresence mode="wait">
        <motion.div
          key={`${currentScreen}-${currentScreen === "Tabs" ? activeTab : ""}`}
          initial={{ opacity: 0, y: 15 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -15 }}
          transition={{ duration: 0.22 }}
          style={{ width: "100%" }}
        >
          {content}
        </motion.div>
      </AnimatePresence>
    );
  };

  // Sidebar item configuration for desktop
  const sidebarItems = [
    { id: "Home", icon: "home-outline", label: "Dashboard", action: () => { navigate("Tabs"); setActiveTab("Home"); } },
    { id: "Send", icon: "arrow-forward-outline", label: "Transfer Money", action: () => navigate("Send") },
    { id: "Activity", icon: "time-outline", label: "Activity Ledger", action: () => { navigate("Tabs"); setActiveTab("Activity"); } },
    { id: "Profile", icon: "person-outline", label: "Settings Profile", action: () => { navigate("Tabs"); setActiveTab("Profile"); } },
    { id: "Notifications", icon: "notifications-outline", label: "Notifications", action: () => navigate("Notifications") },
  ];

  return (
    <div className="app-container">
      {/* Responsive Sidebar for Desktop */}
      <aside className="app-sidebar">
        {/* Brand logo beside text */}
        <div className="sidebar-logo">
          <img src="/logo.png" alt="WOW Logo" style={{ width: "32px", height: "32px", objectFit: "contain" }} />
          <span>WOW.</span>
        </div>

        {/* Sidebar menu */}
        <nav className="sidebar-menu">
          {sidebarItems.map((item) => {
            const isActive = currentScreen === "Tabs" 
              ? (activeTab === "Home" && item.id === "Home") || (activeTab === "Activity" && item.id === "Activity") || (activeTab === "Profile" && item.id === "Profile")
              : (currentScreen === item.id);
            return (
              <div
                key={item.id}
                onClick={item.action}
                className="sidebar-item"
                style={{
                  color: isActive ? c.primary : c.foreground,
                  backgroundColor: isActive ? `${c.primary}12` : "transparent",
                }}
              >
                <Ionicons name={item.icon} size={20} color={isActive ? c.primary : c.foreground} />
                <span>{item.label}</span>
              </div>
            );
          })}
        </nav>

        {/* Sidebar theme toggler */}
        <div
          onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
          style={{
            display: "flex",
            alignItems: "center",
            gap: "12px",
            padding: "12px 18px",
            borderRadius: "16px",
            cursor: "pointer",
            backgroundColor: c.secondary,
            marginTop: "auto",
          }}
        >
          <Ionicons name={theme === "dark" ? "moon-outline" : "sunny-outline"} size={18} color={c.primary} />
          <span style={{ fontSize: "13px", fontWeight: "700", color: c.foreground }}>
            {theme === "dark" ? "Dark Mode" : "Light Mode"}
          </span>
        </div>
      </aside>

      {/* Main Content Pane */}
      <main className="app-content" style={{ backgroundColor: c.background }}>
        <div className="content-inner">
          {renderScreen()}
        </div>
      </main>

      {/* Floating bottom tabs bar simulator for Mobile viewports */}
      <div className="mobile-tab-bar">
        <div className="mobile-tab-item" onClick={() => { navigate("Tabs"); setActiveTab("Home"); }}>
          <Ionicons name={activeTab === "Home" && currentScreen === "Tabs" ? "home" : "home-outline"} size={22} color={activeTab === "Home" && currentScreen === "Tabs" ? c.primary : c.mutedForeground} />
          <span style={{ fontSize: "11px", fontWeight: "600", color: activeTab === "Home" && currentScreen === "Tabs" ? c.primary : c.mutedForeground }}>Home</span>
        </div>
        <div className="mobile-tab-item" onClick={() => { navigate("Tabs"); setActiveTab("Send"); }}>
          <Ionicons name={activeTab === "Send" && currentScreen === "Tabs" ? "arrow-forward" : "arrow-forward-outline"} size={22} color={activeTab === "Send" && currentScreen === "Tabs" ? c.primary : c.mutedForeground} style={{ transform: "rotate(-45deg)" }} />
          <span style={{ fontSize: "11px", fontWeight: "600", color: activeTab === "Send" && currentScreen === "Tabs" ? c.primary : c.mutedForeground }}>Send</span>
        </div>
        <div className="mobile-tab-item" onClick={() => { navigate("Tabs"); setActiveTab("Activity"); }}>
          <Ionicons name={activeTab === "Activity" && currentScreen === "Tabs" ? "time" : "time-outline"} size={22} color={activeTab === "Activity" && currentScreen === "Tabs" ? c.primary : c.mutedForeground} />
          <span style={{ fontSize: "11px", fontWeight: "600", color: activeTab === "Activity" && currentScreen === "Tabs" ? c.primary : c.mutedForeground }}>Activity</span>
        </div>
        <div className="mobile-tab-item" onClick={() => { navigate("Tabs"); setActiveTab("Profile"); }}>
          <Ionicons name={activeTab === "Profile" && currentScreen === "Tabs" ? "person" : "person-outline"} size={22} color={activeTab === "Profile" && currentScreen === "Tabs" ? c.primary : c.mutedForeground} />
          <span style={{ fontSize: "11px", fontWeight: "600", color: activeTab === "Profile" && currentScreen === "Tabs" ? c.primary : c.mutedForeground }}>Profile</span>
        </div>
      </div>
    </div>
  );
};

export default function App() {
  return (
    <ThemeProvider>
      <NavigationProvider>
        <WalletProvider>
          {/* Glow elements in the background constrained to viewport bounds */}
          <div className="glow-wrapper">
            <div className="web-bg-glow" style={{ top: "10%", left: "20%" }} />
            <div className="web-bg-glow-2" style={{ bottom: "10%", right: "20%" }} />
          </div>
          
          <AppSimulator />
        </WalletProvider>
      </NavigationProvider>
    </ThemeProvider>
  );
}
