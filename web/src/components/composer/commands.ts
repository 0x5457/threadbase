/** Slash commands advertised per agent. */

interface Command {
  command: string;
  description: string;
}

export interface SuggestionOption {
  id: string;
  kind: "command" | "skill" | "file";
  label: string;
  description?: string;
  source?: string;
  insert: string;
}

export function commandsFor(agent: string): Command[] {
  const name = agent.toLowerCase();
  const claude: Command[] = [
    { command: "help", description: "Show help and keyboard shortcuts" },
    { command: "clear", description: "Clear the conversation history" },
    {
      command: "compact",
      description: "Summarize the conversation to free context",
    },
    { command: "config", description: "Open configuration" },
    { command: "context", description: "Show context window usage" },
    { command: "cost", description: "Show token usage and cost" },
    { command: "doctor", description: "Check the Claude Code installation" },
    { command: "init", description: "Create a CLAUDE.md for this repository" },
    { command: "login", description: "Switch Anthropic accounts" },
    { command: "mcp", description: "Manage MCP servers" },
    { command: "memory", description: "Edit memory files" },
    { command: "model", description: "Choose the model" },
    { command: "permissions", description: "Review tool permissions" },
    { command: "resume", description: "Resume an earlier conversation" },
    { command: "review", description: "Review a pull request" },
    { command: "status", description: "Show session status" },
    { command: "vim", description: "Toggle vim keybindings" },
    { command: "agents", description: "Manage subagents" },
    { command: "skills", description: "List available skills" },
    { command: "export", description: "Export the conversation" },
    { command: "usage", description: "Show plan usage" },
  ];
  const codex: Command[] = [
    { command: "new", description: "Start a new conversation" },
    { command: "init", description: "Create an AGENTS.md for this repository" },
    {
      command: "compact",
      description: "Summarize the conversation to free context",
    },
    { command: "model", description: "Choose the model" },
    { command: "approvals", description: "Review approval settings" },
    { command: "status", description: "Show session status" },
    { command: "mcp", description: "Manage MCP servers" },
    { command: "diff", description: "Show the current diff" },
    { command: "mention", description: "Mention a file or directory" },
    { command: "plan", description: "Switch to plan mode" },
    { command: "review", description: "Review the current changes" },
    { command: "skills", description: "List available skills" },
    { command: "vim", description: "Toggle vim keybindings" },
    { command: "help", description: "Show help" },
    { command: "quit", description: "Quit the session" },
  ];
  const pi: Command[] = [
    // pi-goal-x: conversational goal planning with persistent progress.
    // Only the commands that start a run belong here: pi-acp never settles a
    // prompt whose extension command answers without running a turn, so
    // read-only goal commands (status/list/focus/settings) wedge the adapter.
    // Pause and resume live in the goal bar, which restarts the adapter after.
    { command: "goal", description: "Draft a goal, then work on it persistently" },
    { command: "goal-direct", description: "Create a goal immediately from an objective" },
    { command: "sisyphus", description: "Draft an ordered, step-by-step goal" },
    { command: "sisyphus-direct", description: "Create an ordered goal immediately" },
    { command: "goal-tweak", description: "Revise the goal objective or budget" },
    { command: "goal-resume", description: "Continue a paused goal" },
    { command: "help", description: "Show help" },
    { command: "model", description: "Choose the model" },
    { command: "compact", description: "Summarize the conversation" },
    { command: "new", description: "Start a new conversation" },
    { command: "resume", description: "Resume an earlier session" },
    { command: "settings", description: "Open settings" },
    { command: "tree", description: "Show the conversation tree" },
    { command: "share", description: "Share this session" },
    { command: "export", description: "Export the session" },
    { command: "init", description: "Initialize project instructions" },
    { command: "memory", description: "Edit memory files" },
    { command: "skills", description: "List available skills" },
    { command: "quit", description: "Quit the session" },
  ];
  const generic: Command[] = [
    { command: "help", description: "Show help" },
    { command: "compact", description: "Summarize the conversation" },
    { command: "new", description: "Start a new conversation" },
    { command: "model", description: "Choose the model" },
    { command: "status", description: "Show session status" },
  ];

  if (name.includes("claude") || name === "cc") return claude;
  if (name.includes("codex")) return codex;
  if (name === "pi" || name.includes("pi ")) return pi;
  return generic;
}
