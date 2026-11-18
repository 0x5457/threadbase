import { jsonRequest, request } from "./client";
import type { AgentDto } from "./types/AgentDto";
import type { AgentUpsertRequest } from "./types/AgentUpsertRequest";

export type Agent = AgentDto;

export function listAgents() {
  return request<AgentDto[]>("/api/agents");
}

export function addAgent(input: AgentUpsertRequest) {
  return jsonRequest<AgentDto>("/api/agents", "POST", input);
}

export function updateAgent(id: string, input: AgentUpsertRequest) {
  return jsonRequest<AgentDto>(
    `/api/agents/${encodeURIComponent(id)}`,
    "PATCH",
    input,
  );
}

export function deleteAgent(id: string) {
  return jsonRequest<{ ok: boolean }>(
    `/api/agents/${encodeURIComponent(id)}`,
    "DELETE",
  );
}

/** Select items for the catalogue, falling back to the built-ins until it
 * loads so a dropdown never renders an empty list. */
export function agentItems(agents: AgentDto[] | undefined) {
  return agents
    ? agents.map((agent) => ({ value: agent.id, label: agent.label }))
    : FALLBACK_AGENT_ITEMS;
}

const FALLBACK_AGENT_ITEMS = [
  { value: "codex", label: "Codex" },
  { value: "claude", label: "Claude Code" },
  { value: "opencode", label: "opencode" },
  { value: "pi", label: "pi" },
];
