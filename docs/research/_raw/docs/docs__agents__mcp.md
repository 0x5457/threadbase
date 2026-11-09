---
title: "MCP Servers"
url: "https://delta.dev/docs/agents/mcp"
description: "Connect remote MCP servers to give Delta's agents access to external tools."
---

# MCP Servers

Connect remote MCP servers to give Delta's agents access to external tools.

Connect an MCP (Model Context Protocol) server to let Delta’s agents use tools from external services.

## [Connect an MCP server](#connect-an-mcp-server)

Delta connects to remote HTTPS MCP servers with [OAuth, bearer tokens, custom headers, or no authentication](#authentication).

1.  Sign in to Delta and open **Settings > MCP** in the desktop app.
2.  Make sure the switch at the top of the MCP settings page is on.
3.  Open **Available** and choose **Install** beside a provider. For another server, choose **Add Custom Server**, enter its MCP endpoint URL, and choose **Connect**. Leave **Authentication** set to **Automatic Detection**, or [supply your own OAuth app credentials](#use-your-own-oauth-app) or [a bearer token and headers](#use-a-bearer-token-or-custom-headers).
4.  If the server requires authorization, choose **Sign in** beside it and complete authorization in your browser.
5.  Wait for the server’s status to show **Connected**. Its tools are available to the agent on your next turn.

### [Authentication](#authentication)

Delta supports MCP servers with these authentication options:

*   No authentication.
*   OAuth 2.1 for all servers that support dynamic client registration.
*   OAuth through apps manually registered by Delta, including Slack, GitHub, and Figma.
*   OAuth using your own app’s client ID and optional client secret.
*   A bearer token you supply.
*   Custom request headers, such as a provider’s API-key header.

Local MCP servers are not supported.

### [Use a bearer token or custom headers](#use-a-bearer-token-or-custom-headers)

In **Settings > MCP > Available**, choose **Add Custom Server** and enter the server’s MCP endpoint URL.

*   For a bearer token, set **Authentication** to **Bearer token** and enter the token without the `Bearer` prefix.
*   For custom headers, expand **Additional Headers**, choose **Add Header**, and enter each header’s name and value.

Choose **Connect**. Delta stores the supplied credentials encrypted in your account. Supply secrets through these settings, not in project files.

### [Use your own OAuth app](#use-your-own-oauth-app)

If a server requires a manually registered OAuth app, you can supply its credentials when adding a custom server.

1.  Register an OAuth app with the server’s provider, or use an existing app.
2.  In **Settings > MCP > Available**, choose **Add Custom Server** and enter the server’s MCP endpoint URL.
3.  Set **Authentication** to **OAuth**.
4.  Enter your app’s **Client ID**. Enter its **Client secret** if the provider requires one; otherwise, leave it empty.
5.  If the provider requires a redirect URI, copy the URI shown in Delta and add it to your OAuth app’s allowed redirect URIs.
6.  Choose **Connect** and complete authorization in your browser.

## [Manage connections](#manage-connections)

*   Choose **Reauthorize** to authorize an OAuth connection again.
*   Choose **Remove** to remove a configured server from Delta.
*   Turn off the switch at the top of the MCP settings page to stop offering configured MCP tools to new agent turns without removing the servers.

For the GitHub MCP server, use **Configure Access** to manage which repositories it can access. If it shows **No repository access**, configure access and complete any requested sign-in.

## [Troubleshooting](#troubleshooting)

### [A server needs authorization](#a-server-needs-authorization)

For **Authorization required**, choose **Sign in**. For **Reconnect required**, choose **Reauthorize** and complete the browser flow.

For a rejected bearer token, remove the server and add it again with a new token instead of using OAuth reauthorization.

### [A custom server cannot connect](#a-custom-server-cannot-connect)

Check that you entered the MCP endpoint, that it is publicly reachable over HTTPS, and that it uses one of the supported authentication options listed above.

## [What’s next](#whats-next)

*   [Skills](/docs/agents/skills) teach agents reusable workflows, including how to use connected tools.
*   [Agentic Safety](/docs/privacy-and-security/agentic-safety) covers the risks of giving agents access to tools.