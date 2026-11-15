import { jsonRequest, request } from "./client";
import type { PushConfigResponse } from "./types/PushConfigResponse";
import type { PushSubscriptionRequest } from "./types/PushSubscriptionRequest";
import type { PushUnsubscribeRequest } from "./types/PushUnsubscribeRequest";

export type PushConfig = PushConfigResponse;

export function getPushConfig() {
  return request<PushConfigResponse>("/api/push/config");
}

export function subscribePush(body: PushSubscriptionRequest) {
  return jsonRequest<{ ok: boolean }>("/api/push/subscribe", "POST", body);
}

export function unsubscribePush(endpoint: string) {
  const body: PushUnsubscribeRequest = { endpoint };
  return jsonRequest<{ ok: boolean }>("/api/push/unsubscribe", "POST", body);
}
