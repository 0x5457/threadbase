import { jsonRequest, request } from "./client";
import type { CreatedMcpToken } from "./types/CreatedMcpToken";
import type { CreateMcpTokenRequest } from "./types/CreateMcpTokenRequest";
import type { McpScope } from "./types/McpScope";
import type { McpTokensResponse } from "./types/McpTokensResponse";

export function listMcpTokens() {
  return request<McpTokensResponse>("/api/mcp/tokens");
}

export function createMcpToken(label: string, scopes: McpScope[]) {
  const body: CreateMcpTokenRequest = { label, scopes };
  return jsonRequest<CreatedMcpToken>("/api/mcp/tokens", "POST", body);
}

export function revokeMcpToken(tokenId: string) {
  return jsonRequest<McpTokensResponse>(
    `/api/mcp/tokens/${tokenId}`,
    "DELETE",
  );
}
