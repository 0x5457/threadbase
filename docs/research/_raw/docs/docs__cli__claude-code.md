---
title: "Claude Code Plugin"
url: "https://delta.dev/docs/cli/claude-code"
description: "Install the Delta plugin, share a Claude Code conversation, and join a teammate's thread."
---

# Claude Code Plugin

Install the Delta plugin, share a Claude Code conversation, and join a teammate's thread.

> **Beta:** The Claude Code plugin is in beta. Its behavior can change between releases.

Share a Claude Code conversation and its Git checkout with your Delta organization, or with people you invite by email. A teammate can join the shared thread from their own Claude Code session and continue the work.

The plugin runs the [Delta CLI](/docs/cli/get-started) for you, so you don’t need to learn its commands.

## [Quick start](#quick-start)

### [1\. Install the plugin](#1-install-the-plugin)

Everyone sharing or joining needs the plugin. In a terminal, first add the Delta marketplace:

```
claude plugin marketplace add zed-industries/delta-claude
```

Then install the plugin from it:

```
claude plugin install delta@zed-industries
```

Start a new Claude Code session, or run `/reload-plugins` in one that’s already open. Using the Claude desktop app? See [Install in the Claude desktop app](#install-in-the-claude-desktop-app).

The plugin installs Delta for you the first time it needs it. On Windows, it asks first: choose **Install Delta**.

> **Tip:** After you install the plugin, it offers to set up the repository. Choose **Set up now** to sync every conversation automatically. See [Set up automatic syncing](#set-up-automatic-syncing). To try sharing one conversation first, choose **Not now**.

### [2\. Share your conversation](#2-share-your-conversation)

Open Claude Code in a Git checkout and run:

```
/delta share
```

Choose who should see the conversation. To share with your team, choose **Share with** and your team’s name. If you don’t have a team, select **Type something** and enter your teammate’s email address.

The first time, the plugin installs Delta if it’s missing and asks before opening your browser to sign in. Once the conversation is shared, select **copy link** and send the link to your teammate.

### [3\. Your teammate joins](#3-your-teammate-joins)

Your teammate installs the plugin, starts a new Claude Code session in any folder, and runs `/delta join` with your link:

```
/delta join https://delta.dev/join/thread_...
```

When Claude Code asks whether to trust the new folder, they choose **Yes, move here**. Claude moves into a checkout of your files with the conversation as context, and your teammate can continue the work.

## [Install in the Claude desktop app](#install-in-the-claude-desktop-app)

1.  In the Code tab, select **+** next to the prompt box, then **Add plugins**. Typing `/plugin` in the prompt box opens the same screen.
2.  Select **Add**, then **Add marketplace**.
3.  Choose **Add from a repository**.
4.  In **URL**, paste `https://github.com/zed-industries/delta-claude.git`, then select **Sync**.
5.  Under **Yours**, find **Delta** by Zed Industries and select **Add**.
6.  Start a new session.

Installing the plugin in the desktop app also installs it for Claude Code in a terminal, and the other way around.

To check that the plugin works, run `/delta` in the new session. The plugin opens a menu of its actions, such as sharing this conversation or joining a shared one.

## [Set up automatic syncing](#set-up-automatic-syncing)

Once sharing works, set up the repository so you don’t need to run `/delta share` in every session. Delta syncs each new conversation, and you choose whether your team sees it.

1.  Open Claude Code in the repository’s Git checkout and run `/delta setup`.
2.  Choose the team conversations sync to. You can also create a team here, and invite people to it by email. To keep conversations to yourself, choose **Skip this step**.
3.  Choose which sessions sync:
    *   **Private sync:** every new session syncs, and only you can see it until you share it.
    *   **Team sharing:** every new session is shared with your team.
    *   **Manual:** sessions sync only when you run `/delta share`. This is how the plugin works until you set it up.

Setup saves your choice to `.delta/settings.json` at the checkout root and keeps that file out of Git, so each teammate chooses for themselves. The setting applies to new sessions, including ones started with `/clear`. After saving, setup offers to sync your current session too.

You can also write the file yourself. `mode` is `manual`, `private`, or `organization`, and `organization` is a team ID from `/delta org list`:

```
{
  "threads": {
    "mode": "private",
    "organization": "ORGANIZATION_ID"
  }
}
```

If the file isn’t valid, the plugin syncs nothing automatically and says what’s wrong.

In a session that syncs automatically:

*   To share a private conversation with your team, run `/delta share`.
*   Claude keeps working offline. The conversation syncs when Delta reconnects.
*   If Delta can’t sync your offline changes because a teammate changed the thread, syncing pauses. Choose **Fork & Restore** to keep your changes in a new thread, or **Discard** to drop them.

## [Share a conversation](#share-a-conversation)

You can share at any point in a session, including before your first prompt. If Claude is responding, wait for it to finish, then run `/delta share` and choose who should see the conversation:

*   **Share with** your team: everyone on the team can open it.
*   **Just me (sync privately):** the conversation syncs, and only you can see it. You can share it later.
*   To invite specific people, select **Type something** and enter their email addresses, separated by commas. Only you and the people you invite can open the conversation.

To invite people without the question, add their addresses to the command:

```
/delta share ada@example.com sam@example.com
```

Sharing uses the team you chose when you [set up the repository](#set-up-automatic-syncing), or your selected team otherwise. After you share, the plugin keeps syncing new activity in the session. A conversation that already syncs can’t become private again, so the question offers **Keep current sharing** instead of **Just me**.

### [What sharing includes](#what-sharing-includes)

*   Your messages and Claude’s replies, including thinking.
*   Tool calls with their inputs and outputs.
*   Images in the conversation.
*   Your checkout’s files, including staged, unstaged, and untracked changes.

### [What sharing doesn’t include](#what-sharing-doesnt-include)

*   Prompt text you haven’t sent.
*   Files Git ignores, such as build output, and local Git state such as branches.
*   Extra folders you added with `--add-dir`.
*   Earlier Claude sessions, and messages that compaction removed. The plugin exports up to the newest 4,096 messages of the current conversation.

## [Join a conversation](#join-a-conversation)

1.  Start a new Claude Code session. Any folder works, because joining moves Claude into the thread’s files.
2.  Run `/delta join` with the link you received. The link’s last part, starting with `thread_`, also works on its own.
3.  When Claude asks whether to trust the new folder, choose **Yes, move here**. **No, stay put** pauses the join until you run `/delta join` again.
4.  Once the join finishes, send your next prompt.

To join, you need to be on the thread’s team or invited to the thread. Joining starts a fresh Claude session in a checkout of the thread’s files that Delta creates. Your previous session and folder stay as they were, and you can resume that session later. Joining doesn’t add your previous conversation to the thread or start a Claude turn.

You can also join from an existing session. Claude must be idle, with no background agents or agent team members running.

> **Tip:** In the Delta app, open a shared thread and choose **Edit > Open in Claude Code**. Claude Code opens with `/delta join` and the link already typed.

### [Work in a joined conversation](#work-in-a-joined-conversation)

The joined session gets the thread’s conversation as context. It’s a new Claude session, not a copy of the original one. For long threads, Claude gets the most recent messages, plus a note explaining how to read older ones with `delta thread show`. The transcript shows only the latest few messages when you join.

While you’re connected:

*   Messages from teammates appear in your transcript but don’t start a Claude turn. Claude sees them with your next prompt.
*   Comments made in Delta reach Claude once someone sends them with a message.
*   Another agent’s thinking appears in your transcript, but Claude doesn’t receive it.
*   Sending a prompt takes over any turn that’s running in the thread, including a teammate’s. Coordinate before you start work.

Delta chooses where checkouts go. With one worktree, Claude works in its checkout. With several, Claude works in a folder containing a link to each one. The plugin blocks prompts until every checkout is ready.

When you exit a joined session, Delta uploads your changes and then deletes its checkouts. Ignored files, such as build output, and local Git branches are deleted with them, so don’t keep anything there that you need later.

## [Manage your team](#manage-your-team)

A team is a Delta organization, the same one you see in the Delta app.

```
/delta org list
/delta org current
/delta org switch ORGANIZATION_ID
/delta org invite ada@example.com sam@example.com
/delta org manage
```

*   `list` shows your teams and their IDs, and `current` shows the selected one.
*   `switch` selects a different team. Repositories you set up keep the team you chose in setup.
*   `invite` invites people to the selected team. Members can open every conversation shared with the team. To give someone access to one conversation only, use [`/delta share`](#share-a-conversation) with their email address.
*   `manage` opens the team’s members page in your browser.

To create a team in the Delta app, open the user menu and choose **Create Organization**, which opens the form on dashboard.zed.dev. See [Create an organization](/docs/account/authenticate#create-an-organization).

## [Update Delta](#update-delta)

When Claude Code starts, the plugin tells you if a newer Delta is available. To install it, run `/delta update`. A running Delta app restarts to install the update. Your synced and shared sessions disconnect briefly and reconnect on their own. On Windows, approve the Windows prompt if one appears. If an agent turn or a background terminal is running in Delta, nothing is stopped: run `/delta update` again once the work finishes.

## [Update the plugin](#update-the-plugin)

The plugin checks for a newer version of itself once an hour and offers to update. To update it yourself, run `/delta plugin-update`. To turn the checks off, run `/delta plugin-update notifications off`, and `/delta plugin-update notifications on` turns them back on. Checks need Claude Code’s `claude` command on your `PATH`.

## [Troubleshooting](#troubleshooting)

**The plugin says Delta is too old:** the plugin reuses a Delta that’s already installed, even an old one. Update it from the Delta app with **Delta > Check for Updates**, then run your command again.

**The plugin says Delta is newer than the plugin:** run `/delta plugin-update`, then run your command again.

**Sharing failed:** fix the problem Delta reports, then run `/delta share` again in the same session. If the conversation already syncs, choose **Keep current sharing** to reconnect it without changing who can see it.

**An automatic sync failed:** fix the problem Delta reports and send your next prompt. The plugin tries again.

**Joining failed, or you chose No, stay put:** run `/delta join` again with the same link. It continues where it stopped.

**A joined session disconnected:** run `/delta join` again with the same link. It reconnects without starting another new session. In a joined session, always recover with `/delta join`, not `/delta share`.

**You resumed a joined session:** wait for the plugin to restore the checkouts and move Claude back to them. Prompts are blocked until then.

## [Requirements](#requirements)

*   Claude Code 2.1.303 or newer, in a terminal or the Code tab of the Claude desktop app. Older versions show a warning, and some features may not work. Plugins are enabled by default, but your company’s Claude Code policy can turn them off.
*   Delta 0.22.0 or newer.
*   To share: a Git checkout with a remote. If the repository has several remotes, one must be named `origin`.
*   To join: membership in the thread’s team, or an invitation to the thread.

The plugin doesn’t support Claude Code cloud sessions, print mode (`claude -p`), or SDK sessions yet.

## [Command reference](#command-reference)

Command

What it does

`/delta`

Open a menu of the plugin’s actions

`/delta share [EMAIL…]`

Share this conversation, or invite people by email

`/delta join LINK`

Join a shared conversation in a new session

`/delta setup`

Choose this repository’s team and which sessions sync

`/delta update`

Update Delta

`/delta plugin-update`

Update the plugin

`/delta org list`

List your teams

`/delta org current`

Show the selected team

`/delta org switch ID`

Select a different team

`/delta org invite EMAIL…`

Invite people to the selected team

`/delta org manage`

Open the team’s members page in your browser

Claude Code’s command list also shows each one as its own command, such as `/delta-share` and `/delta-join`. They do the same thing.

## [Next steps](#next-steps)

*   [Collaborate on a thread](/docs/collaboration/collaborate-thread): share, invite, and join from the Delta app.
*   [Delta on the web](/docs/collaboration/delta-on-the-web): use the shared link in a browser.
*   [Get Started with Delta CLI](/docs/cli/get-started#read-a-conversation): read older messages from your terminal.
*   [Build a Coding Agent Plugin](/docs/cli/build-a-plugin): connect another coding agent to Delta.