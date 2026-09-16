import React, { useState, useEffect, useRef, useCallback } from "react";
import { Modal, View, Text, TouchableOpacity, StyleSheet, Animated } from "react-native";
import * as Haptics from "expo-haptics";
import { Ionicons } from "@expo/vector-icons";
import { useTheme } from "../ThemeContext";

const MAX_ATTEMPTS = 5;
const LOCKOUT_DURATION_SECS = 30;

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
  const [failedAttempts, setFailedAttempts] = useState<number>(0);
  const [lockedUntil, setLockedUntil] = useState<number | null>(null);
  const [lockCountdown, setLockCountdown] = useState<number>(0);

  const shakeAnim = useRef(new Animated.Value(0)).current;
  const dotScale = useRef([0, 1, 2, 3].map(() => new Animated.Value(1))).current;
  const countdownRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Reset state when modal opens
  useEffect(() => {
    if (visible) {
      setPin("");
      setErrorMsg("");
      // Preserve lockout state across open/close if still locked
      if (lockedUntil && Date.now() < lockedUntil) {
        startCountdownTimer(lockedUntil);
      } else {
        setFailedAttempts(0);
        setLockedUntil(null);
        setLockCountdown(0);
      }
    }
    return () => {
      if (countdownRef.current) clearInterval(countdownRef.current);
    };
  }, [visible]);

  const startCountdownTimer = useCallback((until: number) => {
    if (countdownRef.current) clearInterval(countdownRef.current);
    const remaining = Math.ceil((until - Date.now()) / 1000);
    setLockCountdown(remaining);

    countdownRef.current = setInterval(() => {
      const secsLeft = Math.ceil((until - Date.now()) / 1000);
      if (secsLeft <= 0) {
        if (countdownRef.current) clearInterval(countdownRef.current);
        setLockedUntil(null);
        setLockCountdown(0);
        setFailedAttempts(0);
        setErrorMsg("");
      } else {
        setLockCountdown(secsLeft);
      }
    }, 1000);
  }, []);

  const triggerShake = useCallback(() => {
    shakeAnim.setValue(0);
    Animated.sequence([
      Animated.timing(shakeAnim, { toValue: 10, duration: 50, useNativeDriver: true }),
      Animated.timing(shakeAnim, { toValue: -10, duration: 50, useNativeDriver: true }),
      Animated.timing(shakeAnim, { toValue: 8, duration: 50, useNativeDriver: true }),
      Animated.timing(shakeAnim, { toValue: -8, duration: 50, useNativeDriver: true }),
      Animated.timing(shakeAnim, { toValue: 4, duration: 50, useNativeDriver: true }),
      Animated.timing(shakeAnim, { toValue: 0, duration: 50, useNativeDriver: true }),
    ]).start();
  }, [shakeAnim]);

  const animateDotPop = useCallback((index: number) => {
    Animated.sequence([
      Animated.timing(dotScale[index], { toValue: 1.35, duration: 80, useNativeDriver: true }),
      Animated.timing(dotScale[index], { toValue: 1, duration: 80, useNativeDriver: true }),
    ]).start();
  }, [dotScale]);

  const isLocked = lockedUntil !== null && Date.now() < lockedUntil;

  const handleKeyPress = useCallback((digit: string) => {
    if (isLocked) return;
    setErrorMsg("");

    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light);

    if (pin.length < 4) {
      const nextPin = pin + digit;
      setPin(nextPin);
      animateDotPop(nextPin.length - 1);

      if (nextPin.length === 4) {
        if (nextPin === expectedPin) {
          Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success);
          // Reset lockout state on success
          setFailedAttempts(0);
          setLockedUntil(null);
          setLockCountdown(0);
          setTimeout(() => {
            onSuccess();
          }, 150);
        } else {
          Haptics.notificationAsync(Haptics.NotificationFeedbackType.Error);
          triggerShake();

          const newAttempts = failedAttempts + 1;
          setFailedAttempts(newAttempts);

          setTimeout(() => {
            if (newAttempts >= MAX_ATTEMPTS) {
              const until = Date.now() + LOCKOUT_DURATION_SECS * 1000;
              setLockedUntil(until);
              setErrorMsg(`Too many attempts. Try again in ${LOCKOUT_DURATION_SECS}s.`);
              startCountdownTimer(until);
            } else {
              const remaining = MAX_ATTEMPTS - newAttempts;
              setErrorMsg(`Incorrect PIN. ${remaining} attempt${remaining === 1 ? "" : "s"} remaining.`);
            }
            setPin("");
          }, 150);
        }
      }
    }
  }, [pin, isLocked, expectedPin, failedAttempts, onSuccess, triggerShake, animateDotPop, startCountdownTimer]);

  const handleBackspace = useCallback(() => {
    if (isLocked) return;
    setErrorMsg("");
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light);
    setPin((prev) => prev.slice(0, -1));
  }, [isLocked]);

  if (!visible) return null;

  return (
    <Modal visible={visible} transparent animationType="slide">
      <View style={[styles.overlay, { backgroundColor: "rgba(0,0,0,0.6)" }]}>
        <View style={[styles.modalCard, { backgroundColor: c.card, borderColor: c.border }]}>
          <TouchableOpacity style={styles.closeBtn} onPress={onCancel}>
            <Ionicons name="close" size={24} color={c.foreground} />
          </TouchableOpacity>

          <View style={styles.headerWrap}>
            <View style={[styles.lockIcon, { backgroundColor: isLocked ? "#ff4d4f1A" : c.primary + "1A" }]}>
              <Ionicons
                name={isLocked ? "lock-open" : "lock-closed"}
                size={28}
                color={isLocked ? "#ff4d4f" : c.primary}
              />
            </View>
            <Text style={[styles.title, { color: c.foreground }]}>
              {isLocked ? "PIN Locked" : "Enter Device PIN"}
            </Text>
            <Text style={[styles.subtitle, { color: c.mutedForeground }]}>
              {isLocked
                ? `Too many failed attempts. Please wait.`
                : "Step-up authentication required to authorize withdrawal"}
            </Text>
          </View>

          {/* Dots with shake animation */}
          <Animated.View style={[styles.dotsWrap, { transform: [{ translateX: shakeAnim }] }]}>
            {[0, 1, 2, 3].map((idx) => {
              const isFilled = pin.length > idx;
              return (
                <Animated.View
                  key={idx}
                  style={[
                    styles.dot,
                    {
                      backgroundColor: isFilled ? c.primary : "transparent",
                      borderColor: isFilled ? c.primary : c.border,
                      transform: [{ scale: dotScale[idx] }],
                    },
                  ]}
                />
              );
            })}
          </Animated.View>

          {/* Error / lockout countdown */}
          {isLocked && lockCountdown > 0 ? (
            <View style={styles.lockoutWrap}>
              <Ionicons name="time-outline" size={14} color="#ff4d4f" />
              <Text style={styles.lockoutText}>
                Try again in {lockCountdown}s
              </Text>
            </View>
          ) : errorMsg ? (
            <Text style={styles.errorText}>{errorMsg}</Text>
          ) : (
            <View style={{ height: 18 }} />
          )}

          {/* Keypad */}
          <View style={styles.keypadGrid}>
            {["1", "2", "3", "4", "5", "6", "7", "8", "9", "", "0", "del"].map((key, i) => {
              if (key === "") {
                return <View key={i} style={styles.keyItem} />;
              }
              return (
                <TouchableOpacity
                  key={i}
                  activeOpacity={isLocked ? 1 : 0.7}
                  style={[
                    styles.keyItem,
                    styles.keyBtn,
                    {
                      backgroundColor: c.secondary,
                      opacity: isLocked ? 0.35 : 1,
                    },
                  ]}
                  onPress={() => {
                    if (key === "del") handleBackspace();
                    else handleKeyPress(key);
                  }}
                  disabled={isLocked}
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
  lockoutWrap: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: 6,
    marginBottom: 8,
    paddingVertical: 6,
    paddingHorizontal: 16,
    borderRadius: 12,
    backgroundColor: "#ff4d4f10",
    alignSelf: "center",
  },
  lockoutText: {
    color: "#ff4d4f",
    fontSize: 13,
    fontWeight: "700",
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

