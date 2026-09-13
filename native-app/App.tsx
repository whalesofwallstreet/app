import React from "react";
import AsyncStorage from "@react-native-async-storage/async-storage";
import { QueryClient } from "@tanstack/react-query";
import { createAsyncStoragePersister } from "@tanstack/query-async-storage-persister";
import { PersistQueryClientProvider } from "@tanstack/react-query-persist-client";

import { NavigationContainer } from "@react-navigation/native";
import { createNativeStackNavigator } from "@react-navigation/native-stack";
import { createBottomTabNavigator } from "@react-navigation/bottom-tabs";
import { SafeAreaProvider } from "react-native-safe-area-context";
import { GestureHandlerRootView } from "react-native-gesture-handler";
import { StatusBar } from "expo-status-bar";
import { Ionicons } from "@expo/vector-icons";
import { ThemeProvider, useTheme } from "./src/ThemeContext";
import { WalletProvider } from "./src/WalletContext";
import { SecurityProvider } from "./src/SecurityContext";
import { WalletConnectProvider } from "./src/WalletConnectContext";
import { linkingConfig } from "./src/utils/linking";

import HomeScreen from "./src/screens/HomeScreen";
import SendScreen from "./src/screens/SendScreen";
import ActivityScreen from "./src/screens/ActivityScreen";
import ProfileScreen from "./src/screens/ProfileScreen";
import DepositScreen from "./src/screens/DepositScreen";
import WithdrawScreen from "./src/screens/WithdrawScreen";
import SaveScreen from "./src/screens/SaveScreen";
import InvestScreen from "./src/screens/InvestScreen";
import TransactionDetailScreen from "./src/screens/TransactionDetailScreen";
import NotificationsScreen from "./src/screens/NotificationsScreen";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      gcTime: 1000 * 60 * 60 * 24,
    },
  },
});

const asyncStoragePersister = createAsyncStoragePersister({
  storage: AsyncStorage,
  key: "wow-query-cache",
});

const Stack = createNativeStackNavigator();
const Tab = createBottomTabNavigator();

const tabIcons: Record<
  string,
  { active: string; inactive: string; rotation?: number }
> = {
  Home: { active: "home", inactive: "home-outline" },
  Send: {
    active: "arrow-forward",
    inactive: "arrow-forward-outline",
    rotation: -45,
  },
  Activity: { active: "time", inactive: "time-outline" },
  Profile: { active: "person", inactive: "person-outline" },
};

function HomeTabs() {
  const { c } = useTheme();

  return (
    <Tab.Navigator
      screenOptions={({ route }) => ({
        headerShown: false,
        tabBarStyle: {
          backgroundColor: c.background,
          borderTopColor: c.border + "80",
          borderTopWidth: 1,
          paddingTop: 6,
        },
        tabBarActiveTintColor: c.primary,
        tabBarInactiveTintColor: c.mutedForeground,
        tabBarLabelStyle: { fontSize: 10, fontWeight: "600" },
        tabBarIcon: ({ focused, color, size }) => {
          const icons = tabIcons[route.name];
          const iconName = focused ? icons.active : icons.inactive;
          return (
            <Ionicons
              name={iconName as any}
              size={20}
              color={color}
              style={
                icons.rotation
                  ? { transform: [{ rotate: `${icons.rotation}deg` }] }
                  : undefined
              }
            />
          );
        },
      })}
    >
      <Tab.Screen name="Home" component={HomeScreen} />
      <Tab.Screen
        name="Send"
        component={SendScreen}
        options={{ tabBarLabel: "Send" }}
      />
      <Tab.Screen name="Activity" component={ActivityScreen} />
      <Tab.Screen name="Profile" component={ProfileScreen} />
    </Tab.Navigator>
  );
}

function AppNavigator() {
  const { theme } = useTheme();

  return (
    <>
      <StatusBar style={theme === "dark" ? "light" : "dark"} />
      <NavigationContainer linking={linkingConfig}>
        <Stack.Navigator screenOptions={{ headerShown: false }}>
          <Stack.Screen name="Tabs" component={HomeTabs} />
          <Stack.Screen name="Deposit" component={DepositScreen} />
          <Stack.Screen name="Withdraw" component={WithdrawScreen} />
          <Stack.Screen name="Save" component={SaveScreen} />
          <Stack.Screen name="Invest" component={InvestScreen} />
          <Stack.Screen
            name="TransactionDetail"
            component={TransactionDetailScreen}
          />
          <Stack.Screen name="Notifications" component={NotificationsScreen} />
        </Stack.Navigator>
      </NavigationContainer>
    </>
  );
}

export default function App() {
  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <PersistQueryClientProvider
        client={queryClient}
        persistOptions={{
          persister: asyncStoragePersister,
          maxAge: 1000 * 60 * 60 * 24,
          dehydrateOptions: {
            shouldDehydrateQuery: (query) =>
              query.queryKey[0] === "wallet-portfolio",
          },
        }}
      >
        <SafeAreaProvider>
          <ThemeProvider>
            <SecurityProvider>
              <WalletConnectProvider>
                <WalletProvider>
                  <AppNavigator />
                </WalletProvider>
              </WalletConnectProvider>
            </SecurityProvider>
          </ThemeProvider>
        </SafeAreaProvider>
      </PersistQueryClientProvider>
    </GestureHandlerRootView>
  );
}
