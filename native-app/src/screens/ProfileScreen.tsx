import React from "react";
import { View, Text, TouchableOpacity, Image, ScrollView, Switch, StyleSheet } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { useTheme } from "../ThemeContext";
import { useSecurity } from "../SecurityContext";
import { useWalletConnect } from "../WalletConnectContext";
import { currentUser } from "../data/mockData";
import { shadows } from "../theme";

const thresholdOptions = [0, 50, 100, 250];

const ProfileScreen = ({ navigation }: any) => {
  const { c, theme, setTheme } = useTheme();
  const { biometricsEnabled, setBiometricsEnabled, biometricThreshold, setBiometricThreshold } = useSecurity();
  const { isConnected, address, session, connectWallet, disconnectWallet, isConnecting } = useWalletConnect();

  return (
    <SafeAreaView style={[styles.safe, { backgroundColor: c.background }]} edges={["top"]}>
      <ScrollView contentContainerStyle={styles.content} showsVerticalScrollIndicator={false}>
        <View style={styles.header}>
          <TouchableOpacity onPress={() => navigation.goBack()} style={styles.backBtn}>
            <Ionicons name="chevron-back" size={20} color={c.foreground} />
          </TouchableOpacity>
          <Text style={[styles.title, { color: c.foreground }]}>Profile & Settings</Text>
        </View>

        {/* Avatar */}
        <View style={styles.profileWrap}>
          <View style={styles.avatarWrap}>
            <Image source={{ uri: currentUser.avatar }} style={[styles.avatar, { backgroundColor: c.secondary }]} />
            <View style={[styles.onlineDot, { backgroundColor: c.success, borderColor: c.background }]} />
          </View>
          <Text style={[styles.profileName, { color: c.foreground }]}>{currentUser.name}</Text>
          <Text style={[styles.profileUsername, { color: c.primary }]}>{currentUser.username}</Text>
          <Text style={[styles.profileEmail, { color: c.mutedForeground }]}>{currentUser.email}</Text>
        </View>

        {/* Security & Biometrics Section */}
        <Text style={[styles.sectionHeader, { color: c.mutedForeground }]}>SECURITY & BIOMETRICS</Text>
        <View style={[styles.darkCard, { backgroundColor: c.card, borderColor: c.border + "50" }, shadows.card]}>
          <View style={[styles.menuIcon, { backgroundColor: c.primary + "1A" }]}>
            <Ionicons name="scan-outline" size={20} color={c.primary} />
          </View>
          <View style={styles.menuInfo}>
            <Text style={[styles.menuLabel, { color: c.foreground }]}>FaceID / Biometric Auth</Text>
            <Text style={[styles.menuDesc, { color: c.mutedForeground }]}>
              {biometricsEnabled ? "Require FaceID before withdrawals" : "Disabled"}
            </Text>
          </View>
          <Switch
            value={biometricsEnabled}
            onValueChange={setBiometricsEnabled}
            trackColor={{ false: c.secondary, true: c.primary + "60" }}
            thumbColor={biometricsEnabled ? c.primary : "#f4f4f4"}
          />
        </View>

        {biometricsEnabled && (
          <View style={[styles.thresholdCard, { backgroundColor: c.card, borderColor: c.border + "50" }, shadows.card]}>
            <Text style={[styles.menuLabel, { color: c.foreground, marginBottom: 4 }]}>FaceID Threshold</Text>
            <Text style={[styles.menuDesc, { color: c.mutedForeground, marginBottom: 12 }]}>
              Require FaceID for withdrawals equal to or above:
            </Text>
            <View style={styles.thresholdPills}>
              {thresholdOptions.map((opt) => {
                const isSelected = biometricThreshold === opt;
                return (
                  <TouchableOpacity
                    key={opt}
                    onPress={() => setBiometricThreshold(opt)}
                    style={[
                      styles.pill,
                      {
                        backgroundColor: isSelected ? c.primary : c.secondary,
                      },
                    ]}
                  >
                    <Text
                      style={[
                        styles.pillText,
                        { color: isSelected ? "#fff" : c.foreground },
                      ]}
                    >
                      {opt === 0 ? "All" : `$${opt}`}
                    </Text>
                  </TouchableOpacity>
                );
              })}
            </View>
          </View>
        )}

        {/* WalletConnect Session Card */}
        <Text style={[styles.sectionHeader, { color: c.mutedForeground, marginTop: 24 }]}>WALLETCONNECT (MOBILE SIGNING)</Text>
        <View style={[styles.darkCard, { backgroundColor: c.card, borderColor: c.border + "50" }, shadows.card]}>
          <View style={[styles.menuIcon, { backgroundColor: isConnected ? c.success + "1A" : c.primary + "1A" }]}>
            <Ionicons name="wallet-outline" size={20} color={isConnected ? c.success : c.primary} />
          </View>
          <View style={styles.menuInfo}>
            <Text style={[styles.menuLabel, { color: c.foreground }]}>
              {isConnected ? session?.peerName || "Wallet Connected" : "Connect Mobile Wallet"}
            </Text>
            <Text style={[styles.menuDesc, { color: c.mutedForeground }]}>
              {isConnected ? `${address?.slice(0, 6)}...${address?.slice(-4)}` : "Sign bridge transactions via deep link"}
            </Text>
          </View>
          <TouchableOpacity
            onPress={isConnected ? disconnectWallet : connectWallet}
            disabled={isConnecting}
            style={[
              styles.wcBtn,
              { backgroundColor: isConnected ? c.secondary : c.primary },
            ]}
          >
            <Text style={{ fontSize: 12, fontWeight: "700", color: isConnected ? c.foreground : "#fff" }}>
              {isConnecting ? "..." : isConnected ? "Disconnect" : "Connect"}
            </Text>
          </TouchableOpacity>
        </View>

        {/* Dark Mode */}
        <Text style={[styles.sectionHeader, { color: c.mutedForeground, marginTop: 24 }]}>PREFERENCES</Text>
        <View style={[styles.darkCard, { backgroundColor: c.card, borderColor: c.border + "50" }, shadows.card]}>
          <View style={[styles.menuIcon, { backgroundColor: c.secondary }]}>
            <Ionicons name={theme === "dark" ? "moon-outline" : "sunny-outline"} size={18} color={c.primary} />
          </View>
          <View style={styles.menuInfo}>
            <Text style={[styles.menuLabel, { color: c.foreground }]}>Dark Mode</Text>
            <Text style={[styles.menuDesc, { color: c.mutedForeground }]}>{theme === "dark" ? "On" : "Off"}</Text>
          </View>
          <Switch
            value={theme === "dark"}
            onValueChange={(v) => setTheme(v ? "dark" : "light")}
            trackColor={{ false: c.secondary, true: c.primary + "60" }}
            thumbColor={theme === "dark" ? c.primary : "#f4f4f4"}
          />
        </View>
      </ScrollView>
    </SafeAreaView>
  );
};

const styles = StyleSheet.create({
  safe: { flex: 1 },
  content: { paddingHorizontal: 20, paddingBottom: 100 },
  header: { flexDirection: "row", alignItems: "center", gap: 12, paddingVertical: 20 },
  backBtn: { padding: 8, borderRadius: 16 },
  title: { fontSize: 18, fontWeight: "700", letterSpacing: -0.3 },
  profileWrap: { alignItems: "center", marginBottom: 28 },
  avatarWrap: { position: "relative", marginBottom: 12 },
  avatar: { width: 80, height: 80, borderRadius: 24 },
  onlineDot: { position: "absolute", bottom: -4, right: -4, width: 24, height: 24, borderRadius: 12, borderWidth: 3 },
  profileName: { fontSize: 20, fontWeight: "700", letterSpacing: -0.3 },
  profileUsername: { fontSize: 14, fontWeight: "600" },
  profileEmail: { fontSize: 12, marginTop: 2 },
  sectionHeader: { fontSize: 11, fontWeight: "600", letterSpacing: 1.5, marginBottom: 10 },
  darkCard: {
    flexDirection: "row", alignItems: "center", gap: 14, borderRadius: 24, padding: 16, marginBottom: 12, borderWidth: 1,
  },
  thresholdCard: {
    borderRadius: 24, padding: 16, marginBottom: 12, borderWidth: 1,
  },
  thresholdPills: {
    flexDirection: "row",
    gap: 8,
  },
  pill: {
    flex: 1,
    paddingVertical: 10,
    borderRadius: 14,
    alignItems: "center",
  },
  pillText: {
    fontSize: 13,
    fontWeight: "700",
  },
  menuIcon: { width: 40, height: 40, borderRadius: 16, alignItems: "center", justifyContent: "center" },
  menuInfo: { flex: 1 },
  menuLabel: { fontSize: 14, fontWeight: "600" },
  menuDesc: { fontSize: 12 },
  wcBtn: {
    paddingHorizontal: 12,
    paddingVertical: 8,
    borderRadius: 12,
  },
});

export default ProfileScreen;
