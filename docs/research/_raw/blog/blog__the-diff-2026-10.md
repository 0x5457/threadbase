---
title: "The Diff: October 2026 — Delta Blog"
url: "https://delta.dev/blog/the-diff-2026-10"
description: "Collaborate through Claude Code, bring teammates into threads with @mentions, connect remote MCP servers, and plenty more from October."
---

Blog

# The Diff: October 2026

Collaborate through Claude Code, bring teammates into threads with @mentions, connect remote MCP servers, and plenty more from October.

Joseph LyonsNovember 5, 2026

![A leaf-covered path through a green forest in early autumn.](https://images.zed.dev/blog/the-diff/2026-10/cover_photo.webp)

Murckowski Forest, Katowice, Poland. Photo by [Piotr Osiewicz](https://github.com/osiewicz).

It’s been an incredibly busy month for us here at Zed Industries. A central theme has been connecting Delta with the tools and workflows you already use: sharing threads through Claude Code, expanding the CLI to support other integrations, and giving agents access to external tools through remote MCP servers. We’ve also landed a handful of powerful features, such as nested subthreads and @mentions. Let’s go over these updates in a bit more detail.

## [Highlights](#highlights)

### [DeltaDB in your preferred tools](#deltadb-in-your-preferred-tools)

We heard from people who want Delta’s collaboration features in the tools they already use, so we expanded [Delta’s CLI](https://delta.dev/docs/cli/get-started) to allow for just that. Our first integration is a Claude Code plugin: share your session, and a teammate can pick up the work mid-task, from their own Claude Code session or from Delta.

[A walkthrough of the Delta CLI working in Claude Code as Anant & Joseph work together.](https://customer-snccc0j9v3kfzkif.cloudflarestream.com/baf9bd237710dc1cefd3ae7415227ff7/manifest/video.m3u8)

Getting set up is quick:

1.  Install the plugin:
    
    ```
    claude plugin marketplace add zed-industries/delta-claude
    claude plugin install delta@zed-industries
    ```
    
2.  Start a new Claude Code session, then run `/delta share` from your project’s Git checkout
    
3.  Send the link to your teammate
    

With the plugin installed, your teammate can join using `/delta join <link>`.

For more details, see the [announcement post](https://delta.dev/blog/claude-code-multiplayer), or the [plugin docs](https://delta.dev/docs/cli/claude-code) for the full setup and sharing guide.

NOTE

We’re working on plugins for [Zed](https://zed.dev), [Pi](https://pi.dev), and [Codex](https://developers.openai.com/codex) next. If your tool isn’t on that list, you can [build your own integration using the CLI](https://delta.dev/docs/cli/build-a-plugin).

### [@mentions](#mentions)

@mentions make it easy to invite teammates from [your organization](https://delta.dev/docs/account/authenticate#create-an-organization) straight into your thread. Type `@` followed by a teammate’s GitHub username in the thread or in a comment. They’ll receive an OS-level notification if system notifications are enabled, and the mention will also appear in Delta’s inbox so they can find it later.

[Joseph opens Anant's mention from his Inbox, asks an agent for a summary in a subthread, then returns to comment.](https://images.zed.dev/blog/the-diff/2026-10/at-mentions.mp4)

### [Remote MCP server support](#remote-mcp-server-support)

You can now [connect remote MCP servers](https://delta.dev/docs/agents/mcp#connect-an-mcp-server) to Delta, giving your agent access to tools from the services you use. Installing remote MCP servers is easy. In `Settings` > `MCP`, the `Available` tab includes popular preconfigured servers like Figma, Linear, Cloudflare, and GitHub.

If the server you want isn’t in Delta’s catalog, you can add it via `Add Custom Server` in the `Available` tab. Delta detects authentication requirements automatically by default, but custom OAuth configuration with your own client ID and optional client secret is also supported.

![Delta's MCP settings showing the Available tab with preconfigured servers, including Figma, and an \`Add Custom Server\` button.](https://images.zed.dev/blog/the-diff/2026-10/remote_mcp_servers.webp)

### [Subthreads within subthreads](#subthreads-within-subthreads)

Subthreads are no longer limited to one level. You can now use `/fork` inside a subthread to create another, and subthreads now behave more like top-level threads: you can start a review or `/land` changes from inside one. When an agent starts a subthread to delegate work, you can now talk to the agent there and steer its work.

![Delta's thread sidebar showing subthreads nested several levels deep under a mobile UI project.](https://images.zed.dev/blog/the-diff/2026-10/nested_subthreads.webp)

Subthreads are the perfect tool for:

*   exploring different approaches to your problem
*   getting up to speed on a teammate’s thread you’ve been mentioned in
*   running work in parallel

… all without polluting the main thread.

To see these workflows in action, check Danilo’s [subthread walkthrough](https://www.youtube.com/watch?v=i1IProLb_UM) or read Nathan’s [account of planning and building together in Delta](https://delta.dev/blog/planning-together-in-delta).

### [Multiple terminals per thread](#multiple-terminals-per-thread)

Typing `!` in the message composer opens or recalls an inline terminal. Now, `@!` opens the [terminal picker](https://delta.dev/docs/agents/terminals#open-a-terminal), where you can start another session (`1`) or bring an existing one back inline (`2`).

![Two terminals in a Delta thread with the @! picker open. Annotation 1 marks options to start a terminal in each worktree; annotation 2 marks existing sessions labeled pwd and ls.](https://images.zed.dev/blog/the-diff/2026-10/multiple_terminals_2.webp)

### [A better Delta Web on mobile](#a-better-delta-web-on-mobile)

Delta Web has a new layout for mobile. A single tab bar lets you switch between your conversation, changes, files, and open documents. Thread navigation, model selection, and other controls now open in touch-friendly panels. Together, these changes should make Delta feel much smoother on mobile.

![Delta Web on mobile showing a conversation with an inline code change.](https://images.zed.dev/blog/the-diff/2026-10/web_mobile_thread_view_2.webp)

Thread

![Delta Web on mobile with the message overview open in a bottom sheet.](https://images.zed.dev/blog/the-diff/2026-10/web_mobile_message_overview_2.webp)

Message overview

![Delta Web on mobile with the Inbox open in a bottom sheet.](https://images.zed.dev/blog/the-diff/2026-10/web_mobile_inbox_2.webp)

Inbox

TIP

Add [Delta Web](https://delta.dev/threads) to your [iOS or Android home screen](https://delta.dev/docs/collaboration/delta-on-the-web#install-delta-web) for quick access to your threads.

### [@delta, configure my settings](#delta-configure-my-settings)

You can always read the [settings docs](https://delta.dev/docs/configuration/settings) when you want to tweak something, but that’s boring and not very “hip hop.” Instead, mention `@delta` in your thread and ask it to make the change for you.

![A Delta thread showing an agent updating its settings to configure Ollama.](https://images.zed.dev/blog/the-diff/2026-10/delta_configures_self.webp)

The screenshot above comes from [my Ollama setup walkthrough](https://www.youtube.com/watch?v=Y4mpyFDb4xc), where I ask Delta to configure itself.

### [Thread colors and tags, simplified](#thread-colors-and-tags-simplified)

Thread colors and tags were introduced as separate attempts to give us tools to organize our threads. After spending some time with both, it became clear that the two had enough overlapping functionality that it made sense to merge them into one experience. Assigning a color to a thread applies a [tag](https://delta.dev/docs/agents/threads#tag-and-filter-threads) named after that color by default. To give it a custom name, open `More Tags…` in the thread’s right-click menu, then choose `Manage Tags…`.

![A thread's right-click menu in the Delta sidebar showing color swatches above the Tags submenu, with a hovered color labeled with its tag name, The Diff.](https://images.zed.dev/blog/the-diff/2026-10/merged_colors_and_tags.webp)

Note that subthreads no longer automatically inherit their parent thread’s color, since you may want to tag them differently. To tag all of a thread’s subthreads at once, right-click the thread and choose `Apply Tag to Subthreads`.

## [More from this month](#more-from-this-month)

*   **Synced settings and keys**: Your settings now follow your account across desktop and web. Provider API keys also sync between desktop installations.
*   **Share selections from files and diffs**: `Copy Link to Selection` now lets you link directly to selected text in files and diffs, as well as comments anywhere in a thread. Use `cmd-opt-l` on macOS or `ctrl-alt-l` on Windows and Linux.
*   **Better image support**: Images are now better supported throughout Delta. From a thread, you can open images in tabs where you can zoom and pan. You can also copy them to the clipboard on macOS, Windows, and the web. Images pasted into comments now show previews, matching the behavior of pasting images directly in the thread.

## [Quality Week](#quality-week)

Quality Week is a Zed Industries tradition that began in the Zed repository; a few times a year, we set feature work aside for a week and focus entirely on fixing bugs. This month, we brought that tradition to Delta.

We closed more than 70 issues during Quality Week. Here are a few highlights:

*   More responsive diffs when reviewing files with very long lines.
*   A fix for terminal programs that could freeze Delta when they stopped accepting input.
*   No more flickering around blank lines in terminal output.
*   Fixed settings recovery failures that could make saved preferences disappear after restarting Delta.
*   Comments on deleted code now open a historical version of the file or diff where that code still existed.

Thanks to everyone who reported a bug or helped us track one down. Keep the feedback coming through `/send-feedback` in Delta.

## [Delta at AI Engineer NYC 2026](#delta-at-ai-engineer-nyc-2026)

Just this summer, at AI Engineer World’s Fair, at most, we were vaguely _alluding_ to a new product we were building, as Delta was still heavily under wraps. Things looked and felt a bit different at AI Engineer NYC:

![The Zed booth and team at AI Engineer NYC 2026](https://images.zed.dev/blog/the-diff/2026-10/ai_engineer_team.webp)

With Delta out in the wild, we knew we had to “full send” it at AI Engineer NYC: **tons** of Delta merch, and demos showing our day-to-day Delta usage, and how it improves our ability to effectively work with agents across the team.

These tech conferences are great; we love hearing your feedback and being able to bring it back to the team. A few themes kept coming up:

*   People wanted shared conversations and code in their preferred harnesses. We’ve since [expanded the CLI and built our first plugin on top of it, for Claude Code](#deltadb-in-your-preferred-tools), with more integrations on the way.
*   People really loved commenting directly on agent replies! Nice! We’ve been loving it too! 🔥

These conferences are draining on the body and hard on the voice, but it’s always an honor to talk face to face with people who use our tools or simply want to learn more about them.

Here are some more moments from our time in NYC:

![Attendees gathered at the Zed booth in front of a screen reading "Delta brings meaning back to your code"](https://images.zed.dev/blog/the-diff/2026-10/ai_engineer_demoing_2.webp)

The Zed team demoing Delta for a group of people

![People watching a Delta demo beneath the AI Engineer Expo sign](https://images.zed.dev/blog/the-diff/2026-10/ai_engineer_demoing_1.webp)

The Zed team demoing Delta for another group of people

![Four people gather around a laptop during a Delta demo at AI Engineer NYC.](https://images.zed.dev/blog/the-diff/2026-10/ai_engineer_demoing_3.webp)

Anthony and Richard demoing Delta for WorkOS folks (hi Nick and Zack 👋)

![Close-up of a circular merch rack with cream and black shirts and socks around a Zed sign](https://images.zed.dev/blog/the-diff/2026-10/ai_engineer_merch.webp)

Zed's merch setup (the baseball socks were a 'grand slam!' ⚾)

_Shoutout to Esther, Kasey, Danilo, and Franciska for killing it with the merch and booth setup._

## [Further reading](#further-reading)

*   Antonio digs into [building fine-grained version control with Rust and Cloudflare Durable Objects](https://delta.dev/blog/fine-grained-version-control-for-agents). If you’re looking for a deep dive into DeltaDB’s architecture, from SQLite storage and synchronization to keeping the UI responsive, this is the one to read.
*   Conrad explains [how DeltaDB merges concurrent edits](https://delta.dev/blog/future-of-version-control), walking through how changes made in parallel resolve consistently and how earlier versions are stored efficiently.
*   In [Escaping the tyranny of worktrees](https://delta.dev/blog/escaping-the-tyranny-of-worktrees), Conrad explains how Delta worktrees differ from Git worktrees, with practical advice on preparing your project for parallel agents.
*   Franciska shares how working with her team and agent in the same thread means [no more copying drafts into Google Docs for review, then pasting feedback back into Claude](https://delta.dev/blog/please-stop-sending-me-google-docs).

## [Wrapping up](#wrapping-up)

That’s it from us in this month’s edition. If you’re curious about what’s to come, here’s a quick preview:

*   DeltaDB integrations for Zed, Pi, and Codex, so you can collaborate through shared threads without leaving your preferred tools.
*   ACP support, so you can use other harnesses within the Delta app.
*   Cloud Runners, allowing your agents to run independently of your local machine’s availability.
*   More ways to discover threads shared with your team or organization.
*   Syntax highlighting for many more languages, powered by Zed’s extension ecosystem.
*   Repository-level context, a way to trace code back to the conversations that produced it. More on this soon!

We’re incredibly stoked about this work and can’t wait to bring it to you. See you in November! 🍂🦃🥧

## More from the blog

*   ### [Claude Code is now multiplayer](/blog/claude-code-multiplayer)
    
    ![Franciska Dethlefsen](https://avatars.githubusercontent.com/u/10458212?v=4&s=32 "Franciska Dethlefsen")Oct 27, 2026
    
*   ### [Please Stop Sending Me Google Docs](/blog/please-stop-sending-me-google-docs)
    
    ![Franciska Dethlefsen](https://avatars.githubusercontent.com/u/10458212?v=4&s=32 "Franciska Dethlefsen")Oct 8, 2026
    
*   ### [Escaping the tyranny of worktrees](/blog/escaping-the-tyranny-of-worktrees)
    
    ![Conrad Irwin](https://avatars.githubusercontent.com/u/94272?v=4&s=32 "Conrad Irwin")Oct 6, 2026
    

[Download Delta Now](/download)[Open Delta](/threads)