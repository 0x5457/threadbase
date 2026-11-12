import { jsonRequest, request } from "./client";
import type { SettingsDto } from "./types/SettingsDto";
import type { SettingsPatch } from "./types/SettingsPatch";
import type { StateResponse } from "./types/StateResponse";

export type Settings = SettingsDto;

export function getState() {
  return request<StateResponse>("/api/state");
}

export function updateSettings(patch: SettingsPatch) {
  return jsonRequest<StateResponse>("/api/settings", "PATCH", patch);
}
