import { useState } from "react";
import { useTheme } from "../ThemeContext";
import { useAppNavigation } from "../context/NavigationContext";
import { Ionicons } from "../components/Ionicons";
import { fonts } from "../theme";
import { useWallet } from "../context/WalletContext";

export const WithdrawScreen = () => {
  const { c } = useTheme();
  const { goBack } = useAppNavigation();
  const { balance, withdraw } = useWallet();

  const banks = [
    { id: "1", name: "Chase Bank", last4: "4821" },
    { id: "2", name: "Bank of America", last4: "7392" },
  ];

  const [selected, setSelected] = useState(banks[0].id);
  const [amount, setAmount] = useState("");

  const selectedBank = banks.find((b) => b.id === selected) || banks[0];

  const handleWithdraw = () => {
    const val = parseFloat(amount);
    if (val > balance) {
      alert("Error: Insufficient balance!");
      return;
    }
    withdraw(val, selectedBank.name);
    alert(`Success! Withdrew $${val.toFixed(2)} to ${selectedBank.name}.`);
    goBack();
  };

  return (
    <div className="fade-in" style={{ maxWidth: "600px", margin: "0 auto" }}>
      {/* Header */}
      <div style={{ display: "flex", alignItems: "center", gap: "12px", marginBottom: "32px" }}>
        <button onClick={goBack} style={{ padding: "8px", borderRadius: "12px", display: "flex", alignItems: "center", justifyContent: "center" }}>
          <Ionicons name="chevron-back" size={24} color={c.foreground} />
        </button>
        <span style={{ fontSize: "22px", fontWeight: "800", color: c.foreground, fontFamily: fonts.display, letterSpacing: "-0.5px" }}>Withdraw Money</span>
      </div>

      <div className="glass-card responsive-card" style={{ borderRadius: "28px" }}>
        <div style={{ fontSize: "12px", fontWeight: "700", color: c.mutedForeground, letterSpacing: "1.5px", marginBottom: "16px" }}>
          SELECT BANK ACCOUNT
        </div>

        {banks.map((bank) => (
          <div
            key={bank.id}
            onClick={() => setSelected(bank.id)}
            style={{
              display: "flex",
              alignItems: "center",
              gap: "16px",
              padding: "18px 20px",
              borderRadius: "20px",
              border: `1.5px solid ${selected === bank.id ? c.primary : `${c.border}50`}`,
              backgroundColor: selected === bank.id ? `${c.primary}08` : "transparent",
              marginBottom: "12px",
              cursor: "pointer",
            }}
          >
            <div style={{ width: "48px", height: "48px", borderRadius: "16px", backgroundColor: selected === bank.id ? `${c.primary}1A` : c.secondary, display: "flex", alignItems: "center", justifyContent: "center" }}>
              <Ionicons name="business-outline" size={22} color={selected === bank.id ? c.primary : c.foreground} />
            </div>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: "15px", fontWeight: "700", color: c.foreground }}>{bank.name}</div>
              <div style={{ fontSize: "13px", color: c.mutedForeground }}>Checking account ••••{bank.last4}</div>
            </div>
            <div
              style={{
                width: "20px",
                height: "20px",
                borderRadius: "50%",
                border: `2px solid ${selected === bank.id ? c.primary : c.mutedForeground}`,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
              }}
            >
              {selected === bank.id && (
                <div style={{ width: "10px", height: "10px", borderRadius: "50%", backgroundColor: c.primary }} />
              )}
            </div>
          </div>
        ))}

        <div style={{ display: "flex", gap: "6px", marginTop: "16px", fontSize: "13px" }}>
          <span style={{ color: c.mutedForeground }}>Available balance:</span>
          <span style={{ fontWeight: "700", color: c.foreground }}>${balance.toFixed(2)}</span>
        </div>

        <div style={{ fontSize: "12px", fontWeight: "700", color: c.mutedForeground, letterSpacing: "1.5px", marginTop: "32px", marginBottom: "16px" }}>
          AMOUNT TO WITHDRAW
        </div>
        <div style={{ display: "flex", alignItems: "center", padding: "18px 24px", borderRadius: "20px", backgroundColor: c.secondary, marginBottom: "32px" }}>
          <span style={{ fontSize: "22px", fontWeight: "700", color: c.foreground, marginRight: "8px" }}>$</span>
          <input
            placeholder="0.00"
            type="number"
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            style={{ flex: 1, fontSize: "22px", fontWeight: "700", color: c.foreground, width: "100%" }}
          />
        </div>

        <button
          disabled={!amount || parseFloat(amount) === 0 || parseFloat(amount) > balance}
          onClick={handleWithdraw}
          style={{
            width: "100%",
            padding: "18px",
            borderRadius: "20px",
            background: `linear-gradient(135deg, ${c.gradientAccent[0]}, ${c.gradientAccent[1]})`,
            color: "#fff",
            fontWeight: "700",
            fontSize: "16px",
            boxShadow: "0 10px 20px rgba(26, 158, 122, 0.15)",
            opacity: (!amount || parseFloat(amount) === 0 || parseFloat(amount) > balance) ? 0.4 : 1,
          }}
        >
          {parseFloat(amount) > balance ? "Insufficient Balance" : "Confirm Withdrawal"}
        </button>
      </div>
    </div>
  );
};
