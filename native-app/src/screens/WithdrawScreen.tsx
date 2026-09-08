import React, { useState } from "react";
import { View, Text, TouchableOpacity, TextInput, ScrollView, StyleSheet, ActivityIndicator, Alert } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { useTheme } from "../ThemeContext";
import { useWallet } from "../WalletContext";
import { useSecurity } from "../SecurityContext";
import { authenticateBiometric } from "../utils/biometrics";
import { PinModal } from "../components/PinModal";
import { LinearGradient } from "expo-linear-gradient";
import { shadows } from "../theme";

const banks = [
  { id: "1", name: "Chase Bank", last4: "4821" },
  { id: "2", name: "Bank of America", last4: "7392" },
];

const WithdrawScreen = ({ navigation }: any) => {
  const { c } = useTheme();
  const { balance, withdraw } = useWallet();
  const { biometricsEnabled, biometricThreshold, userPin } = useSecurity();
  const [selected, setSelected] = useState(banks[0].id);
  const [amount, setAmount] = useState("");
  const [loading, setLoading] = useState(false);
  const [success, setSuccess] = useState(false);
  const [showPinModal, setShowPinModal] = useState(false);

  const selectedBank = banks.find((b) => b.id === selected) || banks[0];

  const executeWithdrawal = (val: number) => {
    setLoading(true);
    setTimeout(() => {
      withdraw(val, selectedBank.name);
      setLoading(false);
      setSuccess(true);
    }, 1500);
  };

  const handleWithdrawSubmit = async () => {
    const val = parseFloat(amount);
    if (isNaN(val) || val <= 0) return;
    if (val > balance) {
      Alert.alert("Insufficient Balance", `Your current balance is $${balance.toFixed(2)}.`);
      return;
    }

    // Step-up Biometric authentication if amount >= threshold
    if (biometricsEnabled && val >= biometricThreshold) {
      const auth = await authenticateBiometric(`Authorize withdrawal of $${val.toFixed(2)}`);

      if (auth.success) {
        executeWithdrawal(val);
      } else if (auth.fallbackToPin) {
        setShowPinModal(true);
      } else {
        Alert.alert("Authentication Canceled", "Withdrawal was canceled because biometric check failed or was canceled.");
      }
    } else {
      executeWithdrawal(val);
    }
  };

  const handlePinSuccess = () => {
    setShowPinModal(false);
    const val = parseFloat(amount);
    executeWithdrawal(val);
  };

  const handleBack = () => navigation.goBack();

  return (
    <SafeAreaView style={[styles.safe, { backgroundColor: c.background }]} edges={["top"]}>
      <ScrollView contentContainerStyle={styles.content} showsVerticalScrollIndicator={false}>
        <View style={styles.header}>
          <TouchableOpacity onPress={handleBack} style={styles.backBtn}>
            <Ionicons name="chevron-back" size={20} color={c.foreground} />
          </TouchableOpacity>
          <Text style={[styles.title, { color: c.foreground }]}>Withdraw</Text>
        </View>

        {success ? (
          <View style={styles.successWrap}>
            <View style={[styles.successIconWrap, { backgroundColor: c.success + "1A" }]}>
              <Ionicons name="checkmark-circle" size={64} color={c.success} />
            </View>
            <Text style={[styles.successTitle, { color: c.foreground }]}>Withdrawal Complete!</Text>
            <Text style={[styles.successDesc, { color: c.mutedForeground }]}>
              ${parseFloat(amount).toFixed(2)} has been sent to {selectedBank.name} (••••{selectedBank.last4}).
            </Text>
            <TouchableOpacity onPress={() => navigation.goBack()} activeOpacity={0.8}>
              <LinearGradient
                colors={c.gradientAccent as [string, string]}
                start={{ x: 0, y: 0 }}
                end={{ x: 1, y: 0 }}
                style={styles.ctaBtn}
              >
                <Text style={styles.ctaText}>Go to Dashboard</Text>
              </LinearGradient>
            </TouchableOpacity>
          </View>
        ) : loading ? (
          <View style={styles.loadingWrap}>
            <ActivityIndicator size="large" color={c.primary} />
            <Text style={[styles.loadingText, { color: c.mutedForeground }]}>Transferring funds...</Text>
          </View>
        ) : (
          <>
            <Text style={[styles.sectionLabel, { color: c.mutedForeground }]}>SELECT BANK ACCOUNT</Text>

            {banks.map((bank) => (
              <TouchableOpacity
                key={bank.id}
                onPress={() => setSelected(bank.id)}
                activeOpacity={0.7}
                style={[
                  styles.bankCard,
                  {
                    backgroundColor: selected === bank.id ? c.primary + "1A" : c.card,
                    borderColor: selected === bank.id ? c.primary + "33" : c.border + "50",
                  },
                  selected === bank.id ? {} : shadows.card,
                ]}
              >
                <View style={[styles.bankIcon, { backgroundColor: selected === bank.id ? c.primary + "26" : c.secondary }]}>
                  <Ionicons name="business-outline" size={20} color={selected === bank.id ? c.primary : c.foreground} />
                </View>
                <View style={styles.methodInfo}>
                  <Text style={[styles.bankName, { color: c.foreground }]}>{bank.name}</Text>
                  <Text style={[styles.bankLast4, { color: c.mutedForeground }]}>••••{bank.last4}</Text>
                </View>
                {selected === bank.id && (
                  <Ionicons name="checkmark-circle" size={20} color={c.primary} />
                )}
              </TouchableOpacity>
            ))}

            <View style={styles.balanceInfo}>
              <Text style={{ color: c.mutedForeground, fontSize: 13 }}>Available Balance: </Text>
              <Text style={{ color: c.foreground, fontSize: 13, fontWeight: "700" }}>
                ${balance.toLocaleString("en-US", { minimumFractionDigits: 2 })}
              </Text>
            </View>

            <Text style={[styles.sectionLabel, { color: c.mutedForeground, marginTop: 28 }]}>AMOUNT</Text>
            <View style={[styles.amountWrap, { backgroundColor: c.card, borderColor: c.border + "50" }, shadows.card]}>
              <Text style={[styles.dollarSign, { color: c.foreground }]}>$</Text>
              <TextInput
                placeholder="0.00"
                placeholderTextColor={c.mutedForeground + "66"}
                value={amount}
                onChangeText={setAmount}
                keyboardType="numeric"
                style={[styles.amountInput, { color: c.foreground }]}
              />
            </View>

            {biometricsEnabled && (
              <View style={styles.securityBadge}>
                <Ionicons name="shield-checkmark" size={14} color={c.primary} />
                <Text style={[styles.securityBadgeText, { color: c.mutedForeground }]}>
                  FaceID required for withdrawals over ${biometricThreshold}
                </Text>
              </View>
            )}

            <TouchableOpacity
              disabled={!amount || parseFloat(amount) === 0}
              onPress={handleWithdrawSubmit}
              activeOpacity={0.8}
              style={{ marginTop: 20 }}
            >
              <LinearGradient
                colors={c.gradientAccent as [string, string]}
                start={{ x: 0, y: 0 }}
                end={{ x: 1, y: 0 }}
                style={[styles.ctaBtn, (!amount || parseFloat(amount) === 0) && { opacity: 0.4 }]}
              >
                <Text style={styles.ctaText}>Withdraw</Text>
              </LinearGradient>
            </TouchableOpacity>
          </>
        )}
      </ScrollView>

      <PinModal
        visible={showPinModal}
        expectedPin={userPin}
        onSuccess={handlePinSuccess}
        onCancel={() => setShowPinModal(false)}
      />
    </SafeAreaView>
  );
};

const styles = StyleSheet.create({
  safe: { flex: 1 },
  content: { paddingHorizontal: 20, paddingBottom: 100 },
  header: { flexDirection: "row", alignItems: "center", gap: 12, paddingVertical: 20 },
  backBtn: { padding: 8, borderRadius: 16 },
  title: { fontSize: 18, fontWeight: "700", letterSpacing: -0.3 },
  sectionLabel: { fontSize: 11, fontWeight: "600", letterSpacing: 1.5, marginBottom: 12 },
  bankCard: {
    flexDirection: "row", alignItems: "center", gap: 14, padding: 16, borderRadius: 16,
    borderWidth: 1, marginBottom: 10,
  },
  bankIcon: { width: 44, height: 44, borderRadius: 16, alignItems: "center", justifyContent: "center" },
  methodInfo: { flex: 1 },
  bankName: { fontSize: 14, fontWeight: "600" },
  bankLast4: { fontSize: 12 },
  balanceInfo: { flexDirection: "row", marginTop: 8 },
  amountWrap: {
    flexDirection: "row", alignItems: "center", paddingHorizontal: 16, paddingVertical: 16,
    borderRadius: 16, borderWidth: 1,
  },
  dollarSign: { fontSize: 18, fontWeight: "700", marginRight: 4 },
  amountInput: { flex: 1, fontSize: 18, fontWeight: "700" },
  securityBadge: {
    flexDirection: "row",
    alignItems: "center",
    gap: 6,
    marginTop: 12,
    paddingHorizontal: 4,
  },
  securityBadgeText: {
    fontSize: 12,
    fontWeight: "500",
  },
  ctaBtn: { paddingVertical: 16, borderRadius: 16, alignItems: "center" },
  ctaText: { color: "#fff", fontSize: 16, fontWeight: "700" },
  loadingWrap: { paddingVertical: 40, alignItems: "center" },
  loadingText: { marginTop: 12, fontSize: 14, fontWeight: "500" },
  successWrap: { alignItems: "center", paddingVertical: 40 },
  successIconWrap: { width: 96, height: 96, borderRadius: 48, alignItems: "center", justifyContent: "center", marginBottom: 20 },
  successTitle: { fontSize: 22, fontWeight: "800", marginBottom: 8 },
  successDesc: { fontSize: 14, color: "#666", textAlign: "center", paddingHorizontal: 40, marginBottom: 32 },
});

export default WithdrawScreen;
