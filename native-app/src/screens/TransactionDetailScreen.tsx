import React, { useState } from "react";
import { View, Text, TouchableOpacity, Image, ScrollView, StyleSheet } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { useTheme } from "../ThemeContext";
import { useWallet } from "../WalletContext";
import { shareDeepLink } from "../utils/linking";
import { shadows } from "../theme";

const TransactionDetailScreen = ({ navigation, route }: any) => {
  const { c } = useTheme();
  const { transactions } = useWallet();
  const id = route?.params?.id;
  const tx = transactions.find((t) => t.id === id) || transactions[0];
  const [showTech, setShowTech] = useState(false);

  if (!tx) {
    return (
      <SafeAreaView style={[styles.safe, { backgroundColor: c.background }]}>
        <View style={styles.notFound}>
          <Text style={{ color: c.mutedForeground }}>Transaction not found</Text>
        </View>
      </SafeAreaView>
    );
  }

  const isReceived = tx.type === "received";

  return (
    <SafeAreaView style={[styles.safe, { backgroundColor: c.background }]} edges={["top"]}>
      <ScrollView contentContainerStyle={styles.content} showsVerticalScrollIndicator={false}>
        <View style={styles.header}>
          <View style={styles.headerLeft}>
            <TouchableOpacity onPress={() => navigation.goBack()} style={styles.backBtn}>
              <Ionicons name="chevron-back" size={20} color={c.foreground} />
            </TouchableOpacity>
            <Text style={[styles.title, { color: c.foreground }]}>Transaction Details</Text>
          </View>
          <TouchableOpacity
            onPress={() => shareDeepLink("transaction", tx.id)}
            style={[styles.shareBtn, { borderColor: c.border + "50" }]}
          >
            <Ionicons name="share-outline" size={20} color={c.foreground} />
          </TouchableOpacity>
        </View>

        <View style={styles.center}>
          <View style={[styles.statusIcon, { backgroundColor: (isReceived ? c.success : c.primary) + "1A" }]}>
            <Ionicons
              name="arrow-forward"
              size={32}
              color={isReceived ? c.success : c.primary}
              style={{ transform: [{ rotate: isReceived ? "135deg" : "-45deg" }] }}
            />
          </View>

          <Text style={[styles.amount, isReceived && { color: c.success }, !isReceived && { color: c.foreground }]}>
            {isReceived ? "+" : "−"}${tx.amount.toFixed(2)}
          </Text>
          <Text style={[styles.sub, { color: c.mutedForeground }]}>
            {isReceived ? "Received from" : "Sent to"}{" "}
            <Text style={{ fontWeight: "600", color: c.foreground }}>{tx.name}</Text>
          </Text>

          <View style={[styles.card, { backgroundColor: c.card, borderColor: c.border + "50" }, shadows.card]}>
            <View style={[styles.cardHeader, { borderBottomColor: c.border + "80" }]}>
              <Image source={{ uri: tx.avatar }} style={[styles.avatar, { backgroundColor: c.secondary }]} />
              <View>
                <Text style={[styles.cardName, { color: c.foreground }]}>{tx.name}</Text>
                <Text style={[styles.cardUsername, { color: c.mutedForeground }]}>{tx.username}</Text>
              </View>
            </View>

            <View style={styles.row}>
              <Text style={[styles.rowLabel, { color: c.mutedForeground }]}>Status</Text>
              <View style={styles.statusRow}>
                <Ionicons
                  name={tx.status === "completed" ? "checkmark-circle" : "time-outline"}
                  size={14}
                  color={tx.status === "completed" ? c.success : c.warning}
                />
                <Text style={[styles.rowValue, { color: tx.status === "completed" ? c.success : c.warning }]}>
                  {tx.status}
                </Text>
              </View>
            </View>
            <View style={styles.row}>
              <Text style={[styles.rowLabel, { color: c.mutedForeground }]}>Date</Text>
              <Text style={[styles.rowValue, { color: c.foreground }]}>{tx.date}</Text>
            </View>
            <View style={styles.row}>
              <Text style={[styles.rowLabel, { color: c.mutedForeground }]}>Fee</Text>
              <Text style={[styles.rowValue, { color: c.success }]}>$0.00</Text>
            </View>
            {tx.note && (
              <View style={styles.row}>
                <Text style={[styles.rowLabel, { color: c.mutedForeground }]}>Note</Text>
                <Text style={[styles.rowValue, { color: c.foreground }]}>{tx.note}</Text>
              </View>
            )}
          </View>

          <TouchableOpacity onPress={() => setShowTech(!showTech)} style={styles.techBtn}>
            <Text style={[styles.techBtnText, { color: c.mutedForeground }]}>Technical Info</Text>
            <Ionicons name={showTech ? "chevron-up" : "chevron-down"} size={12} color={c.mutedForeground} />
          </TouchableOpacity>

          {showTech && (
            <View style={[styles.techCard, { backgroundColor: c.secondary + "99", borderColor: c.border + "50" }]}>
              <Text style={[styles.techText, { color: c.mutedForeground }]}>Network: Stellar Mainnet</Text>
              <Text style={[styles.techText, { color: c.mutedForeground }]}>TX Hash: 32f7e5...c487</Text>
              <Text style={[styles.techText, { color: c.mutedForeground }]}>Ledger: #2020777</Text>
              <Text style={[styles.techText, { color: c.mutedForeground }]}>Fee: Sponsored (free)</Text>
            </View>
          )}

        </View>
      </ScrollView>
    </SafeAreaView>
  );
};

const styles = StyleSheet.create({
  safe: { flex: 1 },
  notFound: { flex: 1, alignItems: "center", justifyContent: "center" },
  content: { paddingHorizontal: 20, paddingBottom: 100 },
  header: { flexDirection: "row", justifyContent: "space-between", alignItems: "center", paddingVertical: 20 },
  headerLeft: { flexDirection: "row", alignItems: "center", gap: 12 },
  backBtn: { padding: 8, borderRadius: 16 },
  title: { fontSize: 18, fontWeight: "700", letterSpacing: -0.3 },
  shareBtn: { padding: 8, borderRadius: 16, borderWidth: 1 },
  center: { alignItems: "center", paddingTop: 16 },
  statusIcon: { width: 80, height: 80, borderRadius: 24, alignItems: "center", justifyContent: "center", marginBottom: 20 },
  amount: { fontSize: 36, fontWeight: "800", letterSpacing: -2, marginBottom: 6 },
  sub: { fontSize: 14, marginBottom: 24 },
  card: { width: "100%", borderRadius: 24, padding: 20, gap: 16, marginBottom: 16, borderWidth: 1 },
  cardHeader: { flexDirection: "row", alignItems: "center", gap: 12, paddingBottom: 16, borderBottomWidth: 1 },
  avatar: { width: 44, height: 44, borderRadius: 16 },
  cardName: { fontSize: 14, fontWeight: "600" },
  cardUsername: { fontSize: 12 },
  row: { flexDirection: "row", justifyContent: "space-between", alignItems: "center" },
  rowLabel: { fontSize: 14 },
  rowValue: { fontSize: 14, fontWeight: "600", textTransform: "capitalize" },
  statusRow: { flexDirection: "row", alignItems: "center", gap: 6 },
  techBtn: { flexDirection: "row", alignItems: "center", gap: 6, paddingVertical: 8 },
  techBtnText: { fontSize: 12 },
  techCard: { width: "100%", borderRadius: 16, padding: 16, gap: 8, borderWidth: 1 },
  techText: { fontSize: 12, fontFamily: "monospace" },
});

export default TransactionDetailScreen;
