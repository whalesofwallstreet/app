import React, { useState, useEffect } from "react";
import { Modal, View, Text, TouchableOpacity, StyleSheet, Animated } from "react-native";
import { Ionicons } from "@expo/vector-icons";
import { useTheme } from "../ThemeContext";

interface PinModalProps {
  visible: boolean;
  onSuccess: () => void;
  onCancel: () => void;
  expectedPin?: string;
}

export const PinModal: React.FC<PinModalProps> = ({
  visible,
  onSuccess,
  onCancel,
  expectedPin = "1234",
}) => {
  const { c } = useTheme();
  const [pin, setPin] = useState<string>("");
  const [errorMsg, setErrorMsg] = useState<string>("");

  useEffect(() => {
    if (visible) {
      setPin("");
      setErrorMsg("");
    }
  }, [visible]);

  const handleKeyPress = (digit: string) => {
    setErrorMsg("");
    if (pin.length < 4) {
      const nextPin = pin + digit;
      setPin(nextPin);
      if (nextPin.length === 4) {
        if (nextPin === expectedPin) {
          setTimeout(() => {
            onSuccess();
          }, 150);
        } else {
          setTimeout(() => {
            setErrorMsg("Incorrect PIN. Please try again.");
            setPin("");
          }, 150);
        }
      }
    }
  };

  const handleBackspace = () => {
    setErrorMsg("");
    setPin((prev) => prev.slice(0, -1));
  };

  if (!visible) return null;

  return (
    <Modal visible={visible} transparent animationType="slide">
      <View style={[styles.overlay, { backgroundColor: "rgba(0,0,0,0.6)" }]}>
        <View style={[styles.modalCard, { backgroundColor: c.card, borderColor: c.border }]}>
          <TouchableOpacity style={styles.closeBtn} onPress={onCancel}>
            <Ionicons name="close" size={24} color={c.foreground} />
          </TouchableOpacity>

          <View style={styles.headerWrap}>
            <View style={[styles.lockIcon, { backgroundColor: c.primary + "1A" }]}>
              <Ionicons name="lock-closed" size={28} color={c.primary} />
            </View>
            <Text style={[styles.title, { color: c.foreground }]}>Enter Device PIN</Text>
            <Text style={[styles.subtitle, { color: c.mutedForeground }]}>
              Step-up authentication required to authorize withdrawal
            </Text>
          </View>

          {/* Dots */}
          <View style={styles.dotsWrap}>
            {[0, 1, 2, 3].map((idx) => {
              const isFilled = pin.length > idx;
              return (
                <View
                  key={idx}
                  style={[
                    styles.dot,
                    {
                      backgroundColor: isFilled ? c.primary : "transparent",
                      borderColor: isFilled ? c.primary : c.border,
                    },
                  ]}
                />
              );
            })}
          </View>

          {errorMsg ? <Text style={styles.errorText}>{errorMsg}</Text> : <View style={{ height: 18 }} />}

          {/* Keypad */}
          <View style={styles.keypadGrid}>
            {["1", "2", "3", "4", "5", "6", "7", "8", "9", "", "0", "del"].map((key, i) => {
              if (key === "") {
                return <View key={i} style={styles.keyItem} />;
              }
              return (
                <TouchableOpacity
                  key={i}
                  activeOpacity={0.7}
                  style={[styles.keyItem, styles.keyBtn, { backgroundColor: c.secondary }]}
                  onPress={() => {
                    if (key === "del") handleBackspace();
                    else handleKeyPress(key);
                  }}
                >
                  {key === "del" ? (
                    <Ionicons name="backspace-outline" size={22} color={c.foreground} />
                  ) : (
                    <Text style={[styles.keyText, { color: c.foreground }]}>{key}</Text>
                  )}
                </TouchableOpacity>
              );
            })}
          </View>
        </View>
      </View>
    </Modal>
  );
};

const styles = StyleSheet.create({
  overlay: {
    flex: 1,
    justifyContent: "flex-end",
  },
  modalCard: {
    borderTopLeftRadius: 32,
    borderTopRightRadius: 32,
    padding: 24,
    paddingBottom: 40,
    borderWidth: 1,
  },
  closeBtn: {
    alignSelf: "flex-end",
    padding: 6,
  },
  headerWrap: {
    alignItems: "center",
    marginBottom: 20,
  },
  lockIcon: {
    width: 60,
    height: 60,
    borderRadius: 30,
    alignItems: "center",
    justifyContent: "center",
    marginBottom: 12,
  },
  title: {
    fontSize: 20,
    fontWeight: "800",
  },
  subtitle: {
    fontSize: 13,
    textAlign: "center",
    marginTop: 4,
  },
  dotsWrap: {
    flexDirection: "row",
    justifyContent: "center",
    gap: 16,
    marginVertical: 16,
  },
  dot: {
    width: 16,
    height: 16,
    borderRadius: 8,
    borderWidth: 2,
  },
  errorText: {
    color: "#ff4d4f",
    fontSize: 13,
    fontWeight: "600",
    textAlign: "center",
    marginBottom: 8,
  },
  keypadGrid: {
    flexDirection: "row",
    flexWrap: "wrap",
    justifyContent: "center",
    gap: 12,
  },
  keyItem: {
    width: "28%",
    height: 56,
    borderRadius: 16,
    alignItems: "center",
    justifyContent: "center",
  },
  keyBtn: {},
  keyText: {
    fontSize: 22,
    fontWeight: "700",
  },
});
