import { LinkingOptions } from "@react-navigation/native";
import { Share, Alert } from "react-native";

export const linkingConfig: LinkingOptions<any> = {
  prefixes: ["wow://", "https://wow.app"],
  config: {
    initialRouteName: "Tabs",
    screens: {
      Tabs: {
        path: "",
        screens: {
          Home: "home",
          Send: "send",
          Activity: "activity",
          Profile: "profile",
        },
      },
      TransactionDetail: "transaction/:id",
      Deposit: "deposit",
      Withdraw: "withdraw",
      Save: "save",
      Invest: "invest",
      Notifications: "notifications",
    },
  },
};

export const shareDeepLink = async (
  type: "portfolio" | "transaction",
  id: string
): Promise<boolean> => {
  try {
    const url = `wow://${type}/${id}`;
    const title =
      type === "portfolio" ? "Check out my WOW Portfolio" : "WOW Transaction Receipt";
    const message = `${title}: ${url}`;

    const result = await Share.share({
      title,
      message,
      url,
    });

    return result.action === Share.sharedAction;
  } catch (error: any) {
    Alert.alert("Share Error", error?.message || "Failed to share link");
    return false;
  }
};
