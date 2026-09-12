import React, { useState, useMemo, useCallback } from "react";
import {
  View, Text, TouchableOpacity, Image, TextInput, ScrollView, StyleSheet, KeyboardAvoidingView, Platform, Alert
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { useTheme } from "../ThemeContext";
import { useWallet } from "../WalletContext";
import { useWalletConnect } from "../WalletConnectContext";
import { contacts } from "../data/mockData";
import { LinearGradient } from "expo-linear-gradient";
import { useFocusEffect } from "@react-navigation/native";

const SendScreen = ({ navigation, route }: any) => {
  const { c } = useTheme();
  const { balance, addTransaction } = useWallet();
  const { isConnected, address, connectWallet, signAndSubmitBridgeTransaction, isSigning } = useWalletConnect();
  const preselected = route?.params?.to;

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

  // Reset all state when screen is focused
  useFocusEffect(
    useCallback(() => {
      const currentPreselected = route?.params?.to;
      if (currentPreselected) {
        const foundContact = contacts.find((co) => co.username === currentPreselected);
        if (foundContact) {
          setStep("amount");
          setSelectedContact(foundContact);
          setQuery("");
          setAmount("");
          setNote("");
          setSending(false);
        }
      } else {
        setStep("recipient");
        setQuery("");
        setSelectedContact(null);
        setAmount("");
        setNote("");
        setSending(false);
      }
    }, [route?.params?.to])
  );

  const handleKeyPress = (val: string) => {
    if (val === "del") setAmount((p) => p.slice(0, -1));
    else if (val === "." && !amount.includes(".")) setAmount((p) => p + ".");
    else if (val !== ".") {
      if (amount.includes(".") && amount.split(".")[1].length >= 2) return;
      setAmount((p) => p + val);
    }
  };

  const handleSend = async () => {
    const val = parseFloat(amount);
    if (isNaN(val) || val <= 0) return;
    if (val > balance) {
      Alert.alert("Insufficient Funds", "Your transaction cannot be completed due to insufficient balance.");
      return;
    }

    setSending(true);

    if (isConnected) {
      const res = await signAndSubmitBridgeTransaction({
        sourceChain: "Ethereum",
        destChain: "Stellar",
        sourceAsset: "USDC",
        destAsset: "USDC",
        amountIn: val,
        amountOut: val,
        provider: "cctp",
      });

      if (!res.success) {
        setSending(false);
        Alert.alert("Signing Failed", res.error || "Failed to sign bridge transaction in mobile wallet.");
        return;
      }
    }

    setTimeout(() => {
      addTransaction({
        type: "sent",
        name: selectedContact.name,
        username: selectedContact.username,
        avatar: selectedContact.avatar || "https://images.unsplash.com/photo-1535713875002-d1d0cf377fde?auto=format&fit=crop&q=80&w=200&h=200",
        amount: val,
        note: note,
      });
      setSending(false);
      navigation.goBack();
    }, 1200);
  };

  const handleBack = () => {
    if (step === "confirm") setStep("amount");
    else if (step === "amount") {
      if (preselected) navigation.goBack();
      else setStep("recipient");
    } else navigation.goBack();
  };

  return (
    <SafeAreaView style={[styles.safe, { backgroundColor: c.background }]} edges={["top"]}>
      <KeyboardAvoidingView
        behavior={Platform.OS === "ios" ? "padding" : "height"}
        style={styles.keyboardAvoid}
      >
        <ScrollView 
          contentContainerStyle={styles.content} 
          showsVerticalScrollIndicator={false}
          keyboardShouldPersistTaps="handled"
        >
          {/* Header */}
          <View style={styles.header}>
            <TouchableOpacity onPress={handleBack} style={[styles.backBtn, { backgroundColor: "transparent" }]}>
              <Ionicons name="chevron-back" size={20} color={c.foreground} />
            </TouchableOpacity>
            <Text style={[styles.title, { color: c.foreground }]}>Transfer</Text>

            <TouchableOpacity
              onPress={connectWallet}
              style={[
                styles.wcHeaderBadge,
                { backgroundColor: isConnected ? c.success + "1A" : c.primary + "1A" },
              ]}
            >
              <Ionicons name="wallet-outline" size={14} color={isConnected ? c.success : c.primary} />
              <Text style={{ fontSize: 11, fontWeight: "700", color: isConnected ? c.success : c.primary }}>
                {isConnected ? `${address?.slice(0, 4)}...${address?.slice(-3)}` : "Connect Wallet"}
              </Text>
            </TouchableOpacity>
          </View>

          {/* Step 1: Recipient */}
          {step === "recipient" && (
            <View>
              <View style={[styles.searchWrap, { backgroundColor: c.card, borderColor: c.border + "50" }]}>
                <Ionicons name="search" size={16} color={c.mutedForeground} />
                <TextInput
                  placeholder="Email, username, or name"
                  placeholderTextColor={c.mutedForeground}
                  value={query}
                  onChangeText={setQuery}
                  style={[styles.searchInput, { color: c.foreground }]}
                />
              </View>

              {/* Send as link */}
              <TouchableOpacity
                onPress={() => {
                  setSelectedContact({ id: "link", name: "Claimable Link", username: "Anyone with the link", avatar: "" });
                  setStep("amount");
                }}
                activeOpacity={0.7}
                style={[styles.linkBtn, { backgroundColor: c.card, borderColor: c.border + "50" }]}
              >
                <View style={[styles.linkIcon, { backgroundColor: c.primary + "1A" }]}>
                  <Ionicons name="link" size={20} color={c.primary} />
                </View>
                <View>
                  <Text style={[styles.linkTitle, { color: c.foreground }]}>Send as Link</Text>
                  <Text style={[styles.linkDesc, { color: c.mutedForeground }]}>Anyone can claim it</Text>
                </View>
              </TouchableOpacity>

              <Text style={[styles.sectionLabel, { color: c.mutedForeground }]}>CONTACTS</Text>

              {filtered.map((co) => (
                <TouchableOpacity
                  key={co.id}
                  onPress={() => { setSelectedContact(co); setStep("amount"); }}
                  activeOpacity={0.7}
                  style={styles.contactRow}
                >
                  <Image source={{ uri: co.avatar }} style={[styles.contactAvatar, { backgroundColor: c.secondary }]} />
                  <View>
                    <Text style={[styles.contactName, { color: c.foreground }]}>{co.name}</Text>
                    <Text style={[styles.contactUsername, { color: c.mutedForeground }]}>{co.username}</Text>
                  </View>
                </TouchableOpacity>
              ))}
            </View>
          )}

          {/* Step 2: Amount */}
          {step === "amount" && selectedContact && (
            <View style={styles.amountWrap}>
              {selectedContact.id === "link" ? (
                <View style={[styles.linkIconLg, { backgroundColor: c.primary + "1A" }]}>
                  <Ionicons name="link" size={28} color={c.primary} />
                </View>
              ) : (
                <Image source={{ uri: selectedContact.avatar }} style={[styles.amountAvatar, { backgroundColor: c.secondary }]} />
              )}
              <Text style={[styles.amountName, { color: c.foreground }]}>{selectedContact.name}</Text>
              <Text style={[styles.amountUsername, { color: c.mutedForeground }]}>{selectedContact.username}</Text>

              <Text style={[styles.amountDisplay, { color: c.foreground }]}>${amount || "0"}</Text>
              <Text style={[styles.amountSub, { color: c.mutedForeground }]}>
                {selectedContact.id === "link" ? "Via claimable link" : "Sent via digital USD"}
              </Text>

              <TextInput
                placeholder="Add a note..."
                placeholderTextColor={c.mutedForeground}
                value={note}
                onChangeText={setNote}
                style={[styles.noteInput, { borderBottomColor: c.border, color: c.foreground }]}
              />

              <View style={styles.balanceInfo}>
                <Text style={{ color: c.mutedForeground, fontSize: 13 }}>Available Balance: </Text>
                <Text style={{ color: c.foreground, fontSize: 13, fontWeight: "700" }}>
                  ${balance.toLocaleString("en-US", { minimumFractionDigits: 2 })}
                </Text>
              </View>

              {/* Keypad */}
              <View style={styles.keypad}>
                {["1","2","3","4","5","6","7","8","9",".","0","del"].map((key) => (
                  <TouchableOpacity
                    key={key}
                    onPress={() => handleKeyPress(key)}
                    activeOpacity={0.6}
                    style={[styles.key, { backgroundColor: "transparent" }]}
                  >
                    <Text style={[styles.keyText, { color: c.foreground }]}>
                      {key === "del" ? "⌫" : key}
                    </Text>
                  </TouchableOpacity>
                ))}
              </View>

              <TouchableOpacity
                disabled={!amount || parseFloat(amount) === 0}
                onPress={() => {
                  const val = parseFloat(amount);
                  if (val > balance) {
                    Alert.alert("Insufficient Funds", "Amount exceeds your available wallet balance.");
                    return;
                  }
                  setStep("confirm");
                }}
                activeOpacity={0.8}
                style={{ width: "100%" }}
              >
                <LinearGradient
                  colors={c.gradientAccent as [string, string]}
                  start={{ x: 0, y: 0 }}
                  end={{ x: 1, y: 0 }}
                  style={[styles.ctaBtn, (!amount || parseFloat(amount) === 0) && { opacity: 0.4 }]}
                >
                  <Text style={styles.ctaText}>Continue</Text>
                </LinearGradient>
              </TouchableOpacity>
            </View>
          )}

          {/* Step 3: Confirm */}
          {step === "confirm" && selectedContact && (
            <View style={styles.amountWrap}>
              {sending ? (
                <View style={styles.successWrap}>
                  <View style={[styles.successIcon, { backgroundColor: c.success }]}>
                    <Ionicons name="checkmark" size={32} color="#fff" />
                  </View>
                  <Text style={[styles.successTitle, { color: c.foreground }]}>Money Sent!</Text>
                  <Text style={[styles.successSub, { color: c.mutedForeground }]}>
                    ${parseFloat(amount).toFixed(2)} to {selectedContact.name}
                  </Text>
                </View>
              ) : (
                <>
                  {selectedContact.id === "link" ? (
                    <View style={[styles.linkIconLg, { backgroundColor: c.primary + "1A" }]}>
                      <Ionicons name="link" size={28} color={c.primary} />
                    </View>
                  ) : (
                    <Image source={{ uri: selectedContact.avatar }} style={[styles.amountAvatar, { backgroundColor: c.secondary }]} />
                  )}
                  <Text style={[styles.amountName, { color: c.foreground }]}>{selectedContact.name}</Text>
                  <Text style={[styles.amountUsername, { color: c.mutedForeground, marginBottom: 24 }]}>{selectedContact.username}</Text>

                  <View style={[styles.confirmCard, { backgroundColor: c.card, borderColor: c.border + "50" }]}>
                    <View style={styles.confirmRow}>
                      <Text style={[styles.confirmLabel, { color: c.mutedForeground }]}>Amount</Text>
                      <Text style={[styles.confirmValue, { color: c.foreground }]}>${parseFloat(amount).toFixed(2)}</Text>
                    </View>
                    <View style={styles.confirmRow}>
                      <Text style={[styles.confirmLabel, { color: c.mutedForeground }]}>Fee</Text>
                      <Text style={[styles.confirmValue, { color: c.success }]}>$0.00</Text>
                    </View>
                    <View style={styles.confirmRow}>
                      <Text style={[styles.confirmLabel, { color: c.mutedForeground }]}>Arrival</Text>
                      <Text style={[styles.confirmValue, { color: c.foreground }]}>Instant</Text>
                    </View>
                    {note ? (
                      <View style={styles.confirmRow}>
                        <Text style={[styles.confirmLabel, { color: c.mutedForeground }]}>Note</Text>
                        <Text style={[styles.confirmValue, { color: c.foreground }]}>{note}</Text>
                      </View>
                    ) : null}
                  </View>

                  <TouchableOpacity onPress={handleSend} activeOpacity={0.8} style={{ width: "100%" }}>
                    <LinearGradient
                      colors={c.gradientAccent as [string, string]}
                      start={{ x: 0, y: 0 }}
                      end={{ x: 1, y: 0 }}
                      style={styles.ctaBtn}
                    >
                      <Text style={styles.ctaText}>Send</Text>
                    </LinearGradient>
                  </TouchableOpacity>
                </>
              )}
            </View>
          )}
        </ScrollView>
      </KeyboardAvoidingView>
    </SafeAreaView>
  );
};

const styles = StyleSheet.create({
  safe: { flex: 1 },
  keyboardAvoid: { flex: 1 },
  content: { paddingHorizontal: 20, paddingBottom: 100 },
  header: { flexDirection: "row", alignItems: "center", justifyContent: "space-between", paddingVertical: 20 },
  backBtn: { padding: 8, borderRadius: 16 },
  title: { fontSize: 18, fontWeight: "700", letterSpacing: -0.3, flex: 1, marginLeft: 8 },
  wcHeaderBadge: {
    flexDirection: "row",
    alignItems: "center",
    gap: 6,
    paddingHorizontal: 10,
    paddingVertical: 6,
    borderRadius: 12,
  },
  searchWrap: {
    flexDirection: "row", alignItems: "center", gap: 10, paddingHorizontal: 16, paddingVertical: 14,
    borderRadius: 16, borderWidth: 1, marginBottom: 16,
  },
  searchInput: { flex: 1, fontSize: 14 },
  linkBtn: {
    flexDirection: "row", alignItems: "center", gap: 12, padding: 14, borderRadius: 16, borderWidth: 1, marginBottom: 8,
  },
  linkIcon: { width: 44, height: 44, borderRadius: 16, alignItems: "center", justifyContent: "center" },
  linkTitle: { fontSize: 14, fontWeight: "600" },
  linkDesc: { fontSize: 12 },
  sectionLabel: { fontSize: 11, fontWeight: "600", letterSpacing: 1.5, marginTop: 16, marginBottom: 8 },
  contactRow: { flexDirection: "row", alignItems: "center", gap: 12, padding: 14, borderRadius: 16 },
  contactAvatar: { width: 44, height: 44, borderRadius: 16 },
  contactName: { fontSize: 14, fontWeight: "600" },
  contactUsername: { fontSize: 12 },
  amountWrap: { alignItems: "center" },
  linkIconLg: { width: 64, height: 64, borderRadius: 16, alignItems: "center", justifyContent: "center", marginBottom: 8 },
  amountAvatar: { width: 64, height: 64, borderRadius: 16, marginBottom: 8 },
  amountName: { fontSize: 16, fontWeight: "700" },
  amountUsername: { fontSize: 12, marginBottom: 24 },
  amountDisplay: { fontSize: 48, fontWeight: "800", letterSpacing: -2, minHeight: 60, marginBottom: 4 },
  amountSub: { fontSize: 12, marginBottom: 24 },
  noteInput: { width: "80%", textAlign: "center", paddingVertical: 10, borderBottomWidth: 1, fontSize: 14, marginBottom: 24 },
  balanceInfo: { flexDirection: "row", marginBottom: 12 },
  keypad: { flexDirection: "row", flexWrap: "wrap", width: "80%", marginBottom: 24 },
  key: { width: "33.33%", paddingVertical: 14, alignItems: "center", borderRadius: 16 },
  keyText: { fontSize: 18, fontWeight: "600" },
  ctaBtn: { width: "100%", paddingVertical: 16, paddingHorizontal: 20, borderRadius: 16, alignItems: "center" },
  ctaText: { color: "#fff", fontSize: 16, fontWeight: "700" },
  confirmCard: { width: "100%", borderRadius: 24, padding: 20, gap: 16, marginBottom: 32, borderWidth: 1 },
  confirmRow: { flexDirection: "row", justifyContent: "space-between" },
  confirmLabel: { fontSize: 14 },
  confirmValue: { fontSize: 14, fontWeight: "600" },
  successWrap: { alignItems: "center", gap: 16, paddingTop: 24 },
  successIcon: { width: 80, height: 80, borderRadius: 24, alignItems: "center", justifyContent: "center" },
  successTitle: { fontSize: 18, fontWeight: "700" },
  successSub: { fontSize: 14 },
});

export default SendScreen;
