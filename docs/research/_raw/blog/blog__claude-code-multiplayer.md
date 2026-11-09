---
title: "Claude Code is now multiplayer — Delta Blog"
url: "https://delta.dev/blog/claude-code-multiplayer"
description: "Share your Claude Code conversation and unfinished code changes with teammates."
---

Blog

# Claude Code is now multiplayer

Franciska DethlefsenOctober 27, 2026

You can now share your Claude Code work with a teammate and have them join from their own Claude Code session. The conversation and unfinished code changes are shared, so they can see what you’ve tried, ask questions, and contribute without waiting for a commit or working from a pasted summary.

This is available in beta through the [Delta CLI](https://delta.dev/docs/cli/get-started) and our [Claude Code plugin](https://delta.dev/docs/cli/claude-code).

[A walkthrough of the Delta CLI working in Claude Code as Anant & Joseph work together.](https://customer-snccc0j9v3kfzkif.cloudflarestream.com/baf9bd237710dc1cefd3ae7415227ff7/manifest/video.m3u8)

## [Share the work, keep using Claude Code](#share-the-work-keep-using-claude-code)

Run `/delta share` to get a link, then send it to a teammate. They run `/delta join <link>` in Claude Code to join the shared thread.

The plugin shares your Claude Code conversation and worktree through Delta. The CLI keeps the files synchronized across machines. Each person uses their own Claude session, with the shared conversation available as context.

## [When you need another human](#when-you-need-another-human)

We collaborate in Delta all day. Being able to join a teammate’s session and talk with their agent directly has changed how we build together. But every developer is choosing their own agents and tools right now, and we don’t think collaborating should require everyone to switch.

Our team spans time zones, so one person’s evening is often another’s morning. One of us can close our laptop, and the next person open theirs to pick up the thread.

Now you can do the same from Claude Code. Sharing a session lets you hand over a task before it’s ready for a PR, with context a diff alone wouldn’t give your teammate.

And if you already use Delta but have colleagues who don’t want to give up Claude Code, you can bring them into the same thread. They stay in Claude Code, you stay in Delta, and you work together.

## [More harnesses to come](#more-harnesses-to-come)

We’re building support for more harnesses, starting with [Pi](https://pi.dev/), and we’d welcome help adding others. The [Claude Code plugin is open source](https://github.com/zed-industries/delta-claude) and provides a reference implementation for connecting your preferred harness. Our [Build your own plugin guide](https://delta.dev/docs/cli/build-a-plugin) explains where to start.

To start sharing Claude Code sessions, install the [Delta CLI](https://delta.dev/docs/cli/get-started) and the [Claude Code plugin](https://delta.dev/docs/cli/claude-code).

## More from the blog

*   ### [Please Stop Sending Me Google Docs](/blog/please-stop-sending-me-google-docs)
    
    ![Franciska Dethlefsen](https://avatars.githubusercontent.com/u/10458212?v=4&s=32 "Franciska Dethlefsen")Oct 8, 2026
    
*   ### [Your plan.md is missing the conversation](/blog/planning-together-in-delta)
    
    ![Nathan Sobo](https://avatars.githubusercontent.com/u/1789?v=4&s=32 "Nathan Sobo")Oct 1, 2026
    
*   ### [Introducing Delta](https://zed.dev/blog/introducing-delta)
    
    Featured![Nathan Sobo](https://avatars.githubusercontent.com/u/1789?v=4&s=32 "Nathan Sobo")Aug 12, 2026
    

[Download Delta Now](/download)[Open Delta](/threads)