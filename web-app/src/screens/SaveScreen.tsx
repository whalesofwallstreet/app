import { useTheme } from "../ThemeContext";
import { useAppNavigation } from "../context/NavigationContext";
import { Ionicons } from "../components/Ionicons";
import { fonts, shadows } from "../theme";

export const SaveScreen = () => {
  const { c } = useTheme();
  const { goBack } = useAppNavigation();

  const saveOptions = [
    { icon: "wallet-outline", label: "Savings Goal", desc: "Set a target and auto-save", rate: "4.5% APY" },
    { icon: "shield-checkmark-outline", label: "Fixed Deposit", desc: "Lock funds for higher returns", rate: "6.2% APY" },
    { icon: "trending-up-outline", label: "Flex Save", desc: "Save and withdraw anytime", rate: "3.1% APY" },
  ];

  return (
    <div className="fade-in" style={{ maxWidth: "700px", margin: "0 auto" }}>
      {/* Header */}
      <div style={{ display: "flex", alignItems: "center", gap: "12px", marginBottom: "32px" }}>
        <button onClick={goBack} style={{ padding: "8px", borderRadius: "12px", display: "flex", alignItems: "center", justifyContent: "center" }}>
          <Ionicons name="chevron-back" size={24} color={c.foreground} />
        </button>
        <span style={{ fontSize: "22px", fontWeight: "800", color: c.foreground, fontFamily: fonts.display, letterSpacing: "-0.5px" }}>Savings Growth Hub</span>
      </div>

      <div style={{ fontSize: "12px", fontWeight: "700", color: c.mutedForeground, letterSpacing: "1.5px", marginBottom: "16px" }}>
        CHOOSE A HIGH-YIELD PLANS
      </div>

      {saveOptions.map((opt) => (
        <div
          key={opt.label}
          onClick={() => alert(`Subscribed to ${opt.label} at ${opt.rate} APY!`)}
          style={{ display: "flex", alignItems: "center", gap: "20px", padding: "20px 24px", borderRadius: "24px", backgroundColor: c.card, border: `1px solid ${c.border}50`, marginBottom: "16px", cursor: "pointer", boxShadow: shadows.card }}
          className="tx-item-hover"
        >
          <div style={{ width: "52px", height: "52px", borderRadius: "18px", backgroundColor: `${c.primary}12`, display: "flex", alignItems: "center", justifyContent: "center" }}>
            <Ionicons name={opt.icon} size={24} color={c.primary} />
          </div>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: "16px", fontWeight: "700", color: c.foreground }}>{opt.label}</div>
            <div style={{ fontSize: "13px", color: c.mutedForeground }}>{opt.desc}</div>
          </div>
          <div style={{ padding: "8px 16px", borderRadius: "14px", backgroundColor: `${c.primary}12`, fontSize: "13px", fontWeight: "700", color: c.primary }}>
            {opt.rate}
          </div>
        </div>
      ))}
    </div>
  );
};
