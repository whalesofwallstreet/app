import { linkingConfig, shareDeepLink } from "../src/utils/linking";
import { Share } from "react-native";

jest.mock("react-native", () => {
  const rn = jest.requireActual("react-native");
  rn.Share = {
    share: jest.fn().mockResolvedValue({ action: "sharedAction" }),
    sharedAction: "sharedAction",
    dismissedAction: "dismissedAction",
  };
  return rn;
});

describe("Deep Linking & Portfolio Sharing Utility", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it("configures deep link prefixes and route mapping correctly", () => {
    expect(linkingConfig.prefixes).toContain("wow://");
    expect(linkingConfig.prefixes).toContain("https://wow.app");
    expect(linkingConfig.config?.screens).toBeDefined();
  });

  it("triggers native OS Share sheet for portfolio deep link", async () => {
    const success = await shareDeepLink("portfolio", "1234");

    expect(Share.share).toHaveBeenCalledWith({
      title: "Check out my WOW Portfolio",
      message: "Check out my WOW Portfolio: wow://portfolio/1234",
      url: "wow://portfolio/1234",
    });
    expect(success).toBe(true);
  });

  it("triggers native OS Share sheet for transaction deep link", async () => {
    const success = await shareDeepLink("transaction", "t_9876");

    expect(Share.share).toHaveBeenCalledWith({
      title: "WOW Transaction Receipt",
      message: "WOW Transaction Receipt: wow://transaction/t_9876",
      url: "wow://transaction/t_9876",
    });
    expect(success).toBe(true);
  });
});
