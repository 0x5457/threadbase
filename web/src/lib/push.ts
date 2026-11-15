import {
  getPushConfig,
  subscribePush,
  unsubscribePush,
} from "../api";

export type PushState = "unsupported" | "denied" | "off" | "on";

export function pushSupported() {
  return (
    typeof window !== "undefined" &&
    "serviceWorker" in navigator &&
    "PushManager" in window &&
    "Notification" in window
  );
}

export async function registerServiceWorker() {
  if (!("serviceWorker" in navigator)) return null;
  return navigator.serviceWorker.register("/sw.js");
}

function applicationServerKey(value: string) {
  const padding = "=".repeat((4 - (value.length % 4)) % 4);
  const base64 = (value + padding).replace(/-/g, "+").replace(/_/g, "/");
  const raw = window.atob(base64);
  const bytes = new Uint8Array(raw.length);
  for (let index = 0; index < raw.length; index += 1) {
    bytes[index] = raw.charCodeAt(index);
  }
  return bytes;
}

export async function currentPushState(): Promise<PushState> {
  if (!pushSupported()) return "unsupported";
  if (Notification.permission === "denied") return "denied";
  const registration = await navigator.serviceWorker.getRegistration();
  const subscription = await registration?.pushManager.getSubscription();
  return subscription ? "on" : "off";
}

export async function enablePush() {
  if (!pushSupported()) {
    throw new Error("This browser does not support push notifications");
  }
  const permission = await Notification.requestPermission();
  if (permission !== "granted") {
    throw new Error("Notification permission was not granted");
  }
  const config = await getPushConfig();
  if (!config.enabled || !config.publicKey) {
    throw new Error("Push is not configured on this server");
  }
  const registration =
    (await navigator.serviceWorker.getRegistration()) ??
    (await registerServiceWorker());
  if (!registration) throw new Error("Could not register the service worker");
  await navigator.serviceWorker.ready;
  const existing = await registration.pushManager.getSubscription();
  const subscription =
    existing ??
    (await registration.pushManager.subscribe({
      userVisibleOnly: true,
      applicationServerKey: applicationServerKey(config.publicKey),
    }));
  const json = subscription.toJSON();
  if (!json.endpoint || !json.keys?.p256dh || !json.keys.auth) {
    throw new Error("The browser returned an incomplete subscription");
  }
  await subscribePush({
    endpoint: json.endpoint,
    keys: { p256dh: json.keys.p256dh, auth: json.keys.auth },
  });
}

export async function disablePush() {
  if (!pushSupported()) return;
  const registration = await navigator.serviceWorker.getRegistration();
  const subscription = await registration?.pushManager.getSubscription();
  if (!subscription) return;
  await unsubscribePush(subscription.endpoint);
  await subscription.unsubscribe();
}
