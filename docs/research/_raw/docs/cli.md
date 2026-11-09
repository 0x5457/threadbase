---
title: "Delta CLI — Sync and share agent sessions from your terminal"
url: "https://delta.dev/cli"
description: "The Delta CLI syncs your agent sessions to Delta, so your team can join one, see the conversation and uncommitted changes, and continue the work. Works with Claude Code today."
---

Delta CLIBeta

# Bring Delta to the agent you already use

The Delta CLI syncs your agent sessions to DeltaDB. Share one from Claude Code, and your teammate joins from their own session with the conversation and your uncommitted changes in front of them.

[Get started](/docs/cli/claude-code)[Learn more](/blog/claude-code-multiplayer)

```
curl -fsSL https://delta.dev/install.sh -o delta-install.sh
sh delta-install.sh
```

## Connect your preferred harness

Starting with Claude Code. More coming soon.

[Read the docs](/docs/cli/get-started)[Set up Claude Code](/docs/cli/claude-code)

1.  1
    
    ### Share from Claude Code
    
    Install the plugin and send a teammate the link, or join the thread yourself on another machine.
    
    You
    
    ```
    # Install the plugin
    claude plugin marketplace add zed-industries/delta-claude
    claude plugin install delta@zed-industries
    
    # Share the session you're in
    /delta share
    ```
    
2.  2
    
    ### Your teammate joins
    
    Your teammate's agent gets what you asked, what Claude tried, and your uncommitted changes. No summary required.
    
    Your teammate
    
    ```
    # Pick up where you left off
    /delta join <link>
    ```
    
3.  3
    
    ### Ask about any change, any time
    
    Search past threads, then get the reasoning from the conversation that produced it.
    
    Anyone on the team
    
    ```
    # Search messages, summaries, and the files an agent edited
    delta thread search sign-in
    
    # Get an answer from past transcripts
    delta thread query --keyword sign-in "What did we decide?"
    ```
    
4.  4
    
    ### Build on it
    
    Search, query, and fork threads from your shell. Scripts and agents can run the same commands.
    
    You or your agent
    
    ```
    # Try another approach without touching the original
    delta thread fork $THREAD_ID
    
    # Get a local checkout you can edit with any tool
    delta worktree mount $REFERENCE ~/checkout
    
    # Publish a thread to your organization
    delta thread share $THREAD_ID --visibility organization
    
    # JSON on stdout, diagnostics on stderr
    delta --json worktree list
    ```
    
5.  +
    
    ### Bring your own harness
    
    Pi support is next. The plugin is open source, so you can build your own.
    
    Claude CodePi (Soon)Your Harness
    
    ```
    # Start from the Claude Code plugin
    git clone https://github.com/zed-industries/delta-claude
    ```
    
    [Build a plugin](/docs/cli/build-a-plugin)[View the source](https://github.com/zed-industries/delta-claude)
    

## Learn More

[

### Get started with the Delta CLI

Install it, sign in, and run your first commands.

](/docs/cli/get-started)[

### Claude Code plugin

Share a Claude Code session and join a teammate's.

](/docs/cli/claude-code)[

### Build a coding agent plugin

Connect another harness to Delta.

](/docs/cli/build-a-plugin)[

### Scripts and automation

JSON output, exit codes, and background mode.

](/docs/cli/get-started#use-the-cli-in-scripts)

## Built on DeltaDB

Git records a snapshot each time you commit. DeltaDB, the version control underneath Delta, records every step in between, so a teammate can join work you haven't committed yet.

[Learn about DeltaDB](/deltadb)

Git stops at the last commit. DeltaDB keeps every step after it, so a teammate can run /delta join and pick up work that isn't committed yet.

## Frequently asked questions

Do I need the Delta app?

The CLI runs on Delta, so the installer downloads it into your home folder: ~/Applications on macOS and ~/.local/delta.app on Linux. You never have to open it. The CLI starts Delta in the background when a command needs it. If you already use Delta on a Mac, choose Delta > Install CLI… from the menu bar instead.

Which agents does it work with?

Claude Code, in a terminal or in the Code tab of the Claude desktop app. Support for Pi is in progress. Claude Code cloud sessions, print mode, and SDK sessions aren't supported yet.

Can teammates join if they don't use Claude Code?

No. Teammates who use Delta can join the same thread from the app or a browser, and work with the same conversation and files.

Who can see what I share?

Sharing publishes the thread and its files to your whole Delta organization, including people you didn't send the link to. You can also set up a repository so sessions upload privately and stay visible only to you until you share them.

What gets shared?

Your messages and the agent's replies, tool calls with their inputs and outputs, images, and your checkout's files, including staged, unstaged, and untracked changes. Delta doesn't share extra \`--add-dir\` folders, Git-ignored files, local branches, unsent prompts, or messages from earlier or compacted sessions.

How does Delta work with Git?

Sharing starts from a Git checkout with a remote, and you keep committing and opening pull requests as you do today. Delta covers the work that happens before a change is ready to commit.

[Download Delta Now](/download)[Open Delta](/threads)