import { useState, useMemo } from "react";
import { useTheme } from "../ThemeContext";
import { useAppNavigation } from "../context/NavigationContext";
import { fonts } from "../theme";
import { contacts } from "../data/mockData";
import { Ionicons } from "../components/Ionicons";
import { useWallet } from "../context/WalletContext";

export const SendScreen = () => {
  const { c } = useTheme();
  const { goBack, params } = useAppNavigation();
  const { balance, addTransaction } = useWallet();
  const preselected = params?.to;

  const [step, setStep] = useState<"recipient" | "amount" | "confirm">(preselected ? "amount" : "recipient");
  const [query, setQuery] = useState("");
  const [selectedContact, setSelectedContact] = useState<any>(
    preselected ? contacts.find((co) => co.username === preselected) || null : null
  );
  const [amount, setAmount] = useState("");
  const [note, setNote] = useState("");
  const [sending, setSending] = useState(false);

  const filtered = useMemo(
    () => contacts.filter((co) => co.name.toLowerCase().includes(query.toLowerCase()) || co.username.toLowerCase().includes(query.toLowerCase())),
    [query]
  );

  const handleKeyPress = (val: string) => {
    if (val === "del") setAmount((p) => p.slice(0, -1));
    else if (val === "." && !amount.includes(".")) setAmount((p) => p + ".");
    else if (val !== ".") {
      if (amount.includes(".") && amount.split(".")[1].length >= 2) return;
      setAmount((p) => p + val);
    }
  };

  const handleSend = () => {
    setSending(true);
    setTimeout(() => {
      addTransaction({
        type: "sent",
        name: selectedContact.name,
        username: selectedContact.username,
        avatar: selectedContact.avatar || "https://images.unsplash.com/photo-1535713875002-d1d0cf377fde?auto=format&fit=crop&q=80&w=200&h=200",
        amount: parseFloat(amount),
        note: note,
      });
      goBack();
    }, 1500);
  };

  const handleBack = () => {
    if (step === "confirm") setStep("amount");
    else if (step === "amount") {
      if (preselected) goBack();
      else setStep("recipient");
    } else goBack();
  };

  return (
    <div className="fade-in" style={{ maxWidth: "600px", margin: "0 auto" }}>
      {/* Header */}
      <div style={{ display: "flex", alignItems: "center", gap: "12px", marginBottom: "32px" }}>
        <button onClick={handleBack} style={{ padding: "8px", borderRadius: "12px", display: "flex", alignItems: "center", justifyContent: "center" }}>
          <Ionicons name="chevron-back" size={24} color={c.foreground} />
        </button>
        <span style={{ fontSize: "22px", fontWeight: "800", color: c.foreground, fontFamily: fonts.display, letterSpacing: "-0.5px" }}>Transfer Money</span>
      </div>

      {step === "recipient" && (
        <div className="glass-card responsive-card" style={{ borderRadius: "28px" }}>
          {/* Search Box */}
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: "12px",
              padding: "14px 18px",
              borderRadius: "18px",
              backgroundColor: c.secondary,
              marginBottom: "24px",
            }}
          >
            <Ionicons name="search" size={18} color={c.mutedForeground} />
            <input
              placeholder="Email, username, or name"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              style={{ flex: 1, fontSize: "15px", color: c.foreground, width: "100%" }}
            />
          </div>

          {/* Send as link */}
          <div
            onClick={() => {
              setSelectedContact({ id: "link", name: "Claimable Link", username: "Anyone with the link", avatar: "" });
              setStep("amount");
            }}
            style={{
              display: "flex",
              alignItems: "center",
              gap: "14px",
              padding: "16px 20px",
              borderRadius: "20px",
              backgroundColor: `${c.primary}0D`,
              border: `1px solid ${c.primary}20`,
              marginBottom: "24px",
              cursor: "pointer",
            }}
          >
            <div style={{ width: "48px", height: "48px", borderRadius: "16px", backgroundColor: `${c.primary}1A`, display: "flex", alignItems: "center", justifyContent: "center" }}>
              <Ionicons name="link" size={20} color={c.primary} />
            </div>
            <div>
              <div style={{ fontSize: "15px", fontWeight: "700", color: c.foreground }}>Send as Link</div>
              <div style={{ fontSize: "13px", color: c.mutedForeground }}>Anyone can claim it</div>
            </div>
          </div>

          <div style={{ fontSize: "12px", fontWeight: "700", color: c.mutedForeground, letterSpacing: "1.5px", marginBottom: "16px" }}>
            CONTACTS
          </div>

          <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
            {filtered.map((co) => (
              <div
                key={co.id}
                onClick={() => { setSelectedContact(co); setStep("amount"); }}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "14px",
                  padding: "12px 14px",
                  borderRadius: "18px",
                  cursor: "pointer",
                }}
                className="tx-item-hover"
              >
                <img src={co.avatar} alt={co.name} style={{ width: "48px", height: "48px", borderRadius: "16px", objectFit: "cover" }} />
                <div>
                  <div style={{ fontSize: "15px", fontWeight: "700", color: c.foreground }}>{co.name}</div>
                  <div style={{ fontSize: "13px", color: c.mutedForeground }}>{co.username}</div>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {step === "amount" && selectedContact && (
        <div className="glass-card responsive-card" style={{ borderRadius: "28px", display: "flex", flexDirection: "column", alignItems: "center", textAlign: "center" }}>
          {selectedContact.id === "link" ? (
            <div style={{ width: "72px", height: "72px", borderRadius: "24px", backgroundColor: `${c.primary}1A`, display: "flex", alignItems: "center", justifyContent: "center", marginBottom: "12px" }}>
              <Ionicons name="link" size={30} color={c.primary} />
            </div>
          ) : (
            <img src={selectedContact.avatar} alt={selectedContact.name} style={{ width: "72px", height: "72px", borderRadius: "24px", objectFit: "cover", marginBottom: "12px" }} />
          )}
          <div style={{ fontSize: "18px", fontWeight: "700", color: c.foreground }}>{selectedContact.name}</div>
          <div style={{ fontSize: "13px", color: c.mutedForeground, marginBottom: "28px" }}>{selectedContact.username}</div>

          <div style={{ fontSize: "52px", fontWeight: "800", letterSpacing: "-2px", color: c.foreground, marginBottom: "8px", fontFamily: fonts.display }}>
            ${amount || "0"}
          </div>
          <div style={{ fontSize: "13px", color: c.mutedForeground, marginBottom: "24px" }}>
            {selectedContact.id === "link" ? "Via claimable link" : "Sent via digital USD"}
          </div>

          <input
            placeholder="Add a note..."
            value={note}
            onChange={(e) => setNote(e.target.value)}
            style={{ width: "70%", textAlign: "center", padding: "10px 0", borderBottom: `2px solid ${c.border}`, fontSize: "15px", color: c.foreground, marginBottom: "32px" }}
          />

          <div style={{ display: "flex", gap: "6px", marginBottom: "16px", fontSize: "13px" }}>
            <span style={{ color: c.mutedForeground }}>Available balance:</span>
            <span style={{ fontWeight: "700", color: c.foreground }}>${balance.toFixed(2)}</span>
          </div>

          {/* Keypad */}
          <div style={{ display: "flex", flexWrap: "wrap", maxWidth: "280px", width: "100%", marginBottom: "32px" }}>
            {["1","2","3","4","5","6","7","8","9",".","0","del"].map((key) => (
              <button
                key={key}
                onClick={() => handleKeyPress(key)}
                style={{
                  width: "33.33%",
                  padding: "16px 0",
                  fontSize: "20px",
                  fontWeight: "700",
                  color: c.foreground,
                  borderRadius: "16px",
                }}
                className="keypad-key"
              >
                {key === "del" ? "⌫" : key}
              </button>
            ))}
          </div>

          <button
            disabled={!amount || parseFloat(amount) === 0 || parseFloat(amount) > balance}
            onClick={() => setStep("confirm")}
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
            {parseFloat(amount) > balance ? "Insufficient Balance" : "Continue"}
          </button>
        </div>
      )}

      {step === "confirm" && selectedContact && (
        <div className="glass-card responsive-card" style={{ borderRadius: "28px", display: "flex", flexDirection: "column", alignItems: "center" }}>
          {sending ? (
            <div style={{ textAlign: "center", padding: "40px 0" }} className="fade-in">
              <div style={{ width: "88px", height: "88px", borderRadius: "50%", backgroundColor: c.success, display: "flex", alignItems: "center", justifyContent: "center", margin: "0 auto 24px auto", boxShadow: "0 10px 24px rgba(46, 158, 106, 0.25)" }}>
                <Ionicons name="checkmark" size={36} color="#fff" />
              </div>
              <div style={{ fontSize: "22px", fontWeight: "800", color: c.foreground, marginBottom: "8px" }}>Money Sent Successfully!</div>
              <div style={{ fontSize: "16px", color: c.mutedForeground }}>
                ${parseFloat(amount).toFixed(2)} to {selectedContact.name}
              </div>
            </div>
          ) : (
            <div style={{ width: "100%", display: "flex", flexDirection: "column", alignItems: "center" }} className="fade-in">
              {selectedContact.id === "link" ? (
                <div style={{ width: "72px", height: "72px", borderRadius: "24px", backgroundColor: `${c.primary}1A`, display: "flex", alignItems: "center", justifyContent: "center", marginBottom: "12px" }}>
                  <Ionicons name="link" size={30} color={c.primary} />
                </div>
              ) : (
                <img src={selectedContact.avatar} alt={selectedContact.name} style={{ width: "72px", height: "72px", borderRadius: "24px", objectFit: "cover", marginBottom: "12px" }} />
              )}
              <div style={{ fontSize: "18px", fontWeight: "700", color: c.foreground }}>{selectedContact.name}</div>
              <div style={{ fontSize: "13px", color: c.mutedForeground, marginBottom: "32px" }}>{selectedContact.username}</div>

              <div style={{ width: "100%", borderRadius: "24px", backgroundColor: c.secondary, padding: "24px", display: "flex", flexDirection: "column", gap: "18px", marginBottom: "32px" }}>
                <div style={{ display: "flex", justifyContent: "space-between" }}>
                  <span style={{ fontSize: "15px", color: c.mutedForeground }}>Amount</span>
                  <span style={{ fontSize: "15px", fontWeight: "700", color: c.foreground }}>${parseFloat(amount).toFixed(2)}</span>
                </div>
                <div style={{ display: "flex", justifyContent: "space-between" }}>
                  <span style={{ fontSize: "15px", color: c.mutedForeground }}>Network Fee</span>
                  <span style={{ fontSize: "15px", fontWeight: "700", color: c.success }}>$0.00 (Sponsored)</span>
                </div>
                <div style={{ display: "flex", justifyContent: "space-between" }}>
                  <span style={{ fontSize: "15px", color: c.mutedForeground }}>Arrival</span>
                  <span style={{ fontSize: "15px", fontWeight: "700", color: c.foreground }}>Instant</span>
                </div>
                {note && (
                  <div style={{ display: "flex", justifyContent: "space-between" }}>
                    <span style={{ fontSize: "15px", color: c.mutedForeground }}>Note</span>
                    <span style={{ fontSize: "15px", fontWeight: "700", color: c.foreground }}>{note}</span>
                  </div>
                )}
              </div>

              <button
                onClick={handleSend}
                style={{
                  width: "100%",
                  padding: "18px",
                  borderRadius: "20px",
                  background: `linear-gradient(135deg, ${c.gradientAccent[0]}, ${c.gradientAccent[1]})`,
                  color: "#fff",
                  fontWeight: "700",
                  fontSize: "16px",
                  boxShadow: "0 10px 20px rgba(26, 158, 122, 0.15)",
                }}
              >
                Send Money
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
