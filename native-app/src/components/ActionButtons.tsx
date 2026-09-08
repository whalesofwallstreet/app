import React from "react";
import { View, Text, TouchableOpacity, StyleSheet } from "react-native";
import { Ionicons, MaterialCommunityIcons } from "@expo/vector-icons";
import { useTheme } from "../ThemeContext";
import { LinearGradient } from "expo-linear-gradient";

const actions = [
  { icon: "arrow-forward" as const, label: "Transfer", route: "Send", rotation: -45, iconSet: "ionicons" as const },
  { icon: "add-circle-outline" as const, label: "Deposit", route: "Deposit", iconSet: "ionicons" as const },
  { icon: "piggy-bank-outline" as const, label: "Save", route: "Save", iconSet: "material" as const },
  { icon: "bar-chart-outline" as const, label: "Invest", route: "Invest", iconSet: "ionicons" as const },
];

const ActionButtons = ({ navigation }: { navigation: any }) => {
  const { c } = useTheme();

  return (
    <View style={styles.container}>
      {actions.map((action, i) => (
        <TouchableOpacity
          key={action.label}
          onPress={() => navigation.navigate(action.route)}
          activeOpacity={0.7}
          style={styles.button}
        >
          {i === 0 ? (
            <LinearGradient
              colors={c.gradientAccent as [string, string]}
              start={{ x: 0, y: 0 }}
              end={{ x: 1, y: 1 }}
              style={styles.iconWrap}
            >
              <Ionicons 
                name={action.icon as any} 
                size={18} 
                color="#fff" 
                style={{ transform: [{ rotate: `${action.rotation || 0}deg` }] }}
              />
            </LinearGradient>
          ) : (
            <View style={[styles.iconWrap, { backgroundColor: c.secondary }]}>
              {action.iconSet === "material" ? (
                <MaterialCommunityIcons name={action.icon as any} size={18} color={c.foreground} />
              ) : (
                <Ionicons name={action.icon as any} size={18} color={c.foreground} />
              )}
            </View>
          )}
          <Text style={[styles.label, { color: c.mutedForeground }]}>{action.label}</Text>
        </TouchableOpacity>
      ))}
    </View>
  );
};

const styles = StyleSheet.create({
  container: { flexDirection: "row", justifyContent: "space-between", gap: 4 },
  button: { flex: 1, alignItems: "center", gap: 6, paddingVertical: 8 },
  iconWrap: { width: 44, height: 44, borderRadius: 16, alignItems: "center", justifyContent: "center" },
  label: { fontSize: 10, fontWeight: "600", letterSpacing: 0.5 },
});

export default ActionButtons;
