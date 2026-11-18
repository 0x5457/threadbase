-- Guest links and the MCP endpoint were removed. Their tables go with them:
-- 0029 and 0030 stay in place because applied migrations are checksummed.
DROP TABLE IF EXISTS guest_messages;
DROP TABLE IF EXISTS guest_sessions;
DROP TABLE IF EXISTS guest_links;
DROP TABLE IF EXISTS mcp_tokens;
