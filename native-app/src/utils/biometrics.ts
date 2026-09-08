import * as LocalAuthentication from "expo-local-authentication";

export interface BiometricCheckResult {
  hasHardware: boolean;
  isEnrolled: boolean;
  supportedTypes: string[];
}

export interface AuthResult {
  success: boolean;
  error?: string;
  fallbackToPin?: boolean;
}

export const checkBiometricSupport = async (): Promise<BiometricCheckResult> => {
  try {
    const hasHardware = await LocalAuthentication.hasHardwareAsync();
    const isEnrolled = await LocalAuthentication.isEnrolledAsync();
    const types = await LocalAuthentication.supportedAuthenticationTypesAsync();

    const supportedTypes: string[] = types.map((type) => {
      switch (type) {
        case LocalAuthentication.AuthenticationType.FACIAL_RECOGNITION:
          return "Face ID";
        case LocalAuthentication.AuthenticationType.FINGERPRINT:
          return "Touch ID / Fingerprint";
        case LocalAuthentication.AuthenticationType.IRIS:
          return "Iris";
        default:
          return "Biometrics";
      }
    });

    return { hasHardware, isEnrolled, supportedTypes };
  } catch (error) {
    return { hasHardware: false, isEnrolled: false, supportedTypes: [] };
  }
};

export const authenticateBiometric = async (
  promptMessage: string = "Authorize Withdrawal"
): Promise<AuthResult> => {
  try {
    const { hasHardware, isEnrolled } = await checkBiometricSupport();

    if (!hasHardware || !isEnrolled) {
      return { success: false, fallbackToPin: true, error: "Biometrics not available or not enrolled" };
    }

    const result = await LocalAuthentication.authenticateAsync({
      promptMessage,
      fallbackLabel: "Use PIN",
      cancelLabel: "Cancel",
      disableDeviceFallback: false,
    });

    if (result.success) {
      return { success: true };
    }

    const errCode = (result as any).error;

    if (errCode === "user_cancel" || errCode === "system_cancel" || errCode === "app_cancel") {
      return { success: false, error: "Authentication canceled" };
    }

    if (errCode === "user_fallback" || errCode === "not_enrolled" || errCode === "no_hardware") {
      return { success: false, fallbackToPin: true, error: "Fallback requested" };
    }

    return { success: false, fallbackToPin: true, error: errCode || "Biometric check failed" };
  } catch (err: any) {
    return { success: false, fallbackToPin: true, error: err?.message || "Biometric authentication error" };
  }
};
