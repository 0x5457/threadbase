---
title: "Reviewing & Syncing Changes"
url: "https://delta.dev/docs/agents/review-and-sync"
description: "How an agent's changes flow back to your repository."
---

# Reviewing & Syncing Changes

How an agent's changes flow back to your repository.

By default, an agent works in its own checkout of your repository (the **Isolated Delta Worktree** option, a separate folder managed by Delta), so its changes reach you the same way a teammate’s would: you review them, then bring them in. A thread can instead adopt your project folder as its checkout; the agent’s edits then appear there directly. See [Delta Worktrees](/docs/concepts/worktrees).

> **Important:** Delta records [file edits](/docs/concepts/delta-and-git#what-delta-records-that-git-doesnt) as you work, without creating Git commits.
> 
> A managed checkout has its own git repository. When the agent commits, the commit lands there, not in your project folder. Committing is not the step that brings work to you; pushing to `local` or your source remote is. See [Bring changes in](#bring-changes-in).

A checkout created from your own project folder has a `local` git remote that points back at your repository. When a source remote is selected, Delta also preserves it under its original name, usually `origin`. [Delta & Git](/docs/concepts/delta-and-git) explains both.

## [Review the agent’s work](#review-the-agents-work)

As the agent works, its edits accumulate in the thread. Review them the way you would review any change: read the diffs and ask the agent to revise anything that needs work. You can attach [comments](/docs/agents/comments) to relevant parts of the transcript, or directly to the code in the diffs and file tabs. With a managed checkout, none of this touches your project folder.

The **Changes** tab collects the diffs in one place. On a computer, it is the review drawer’s default panel; on a phone, tap it in the tab bar below the thread title. Use its selector to choose a comparison:

Comparison

What it shows

Upstream ref, such as `origin/main`

Changes since the merge base with that upstream ref.

**Last Commit**

Uncommitted changes.

**Last Turn**

Edits since the agent’s last turn started.

A commit opened from the thread

Changes in that commit.

Commit links in the thread open here too. Choose **Review Changes** from the menu or command palette to focus the tab. You can also press `Option+Shift+D` on macOS or `Alt+Shift+D` on Linux and Windows. The [Default Diff Base setting](/docs/configuration/settings#pane) chooses the initial comparison for new threads.

Branch comparisons can come from collaborators’ checkouts. Delta prefers your checkout’s comparison when available and names the source when using someone else’s. The branch picker groups choices by checkout and shows a collaborator’s avatar above their branches. **Fetch Diff** fetches your checkout’s remote branch and refreshes the comparison, asking for Git credentials if needed.

Moved and renamed files show their previous path in the changed-file list, and the diff header highlights how the path changed.

On desktop, in a thread with several projects, right-click the thread’s added/removed line counter to filter the comparison by project. Choose **All Projects** to show the full comparison again. The counter follows the selected comparison, including **Last Commit** and **Last Turn**.

Each file in the diff has a **Seen** checkbox on its header. Marking a file seen folds it, and new edits to the file clear the mark. Seen state is per person and follows you across machines. Press `Cmd+Option+Y` on macOS or `Ctrl+Alt+Y` on Linux and Windows to toggle it, and `Cmd+[` and `Cmd+]` (`Alt+[` and `Alt+]` on Linux and Windows) to move between files.

The Review header shows your seen-file progress, such as `3/17`, once you mark a file seen. In **Display Options**, choose **Collapse All Files** or **Expand All Files** (`Shift+Tab` with the diff focused), or **Mark All as Seen** or **Mark All as Unseen** (`Cmd+Shift+Y` on macOS, `Ctrl+Shift+Y` on Linux and Windows). These actions apply to the files currently shown. Selecting a collapsed file in **Changes** expands it in the diff.

Press `Cmd+F` on macOS or `Ctrl+F` on Linux and Windows to search an open file or the diff view; in the diff view, matches span every file in the comparison. `Cmd+G` and `Cmd+Shift+G` (`Ctrl+G` and `Ctrl+Shift+G` on Linux and Windows) step through matches, including while the search bar is focused. Find also searches the rendered contents of Markdown files, without requiring Markdown punctuation in the query.

Delta records the branch diff’s merge base and shares it with collaborators, so everyone compares against the same base, including on the web or without a local checkout. **Last Turn** includes the complete agent turn until your next message, even when automatic compaction occurs during the turn.

## [Review in a review thread](#review-in-a-review-thread)

For a fuller review, move it into a review thread: a separate conversation attached to the thread that produced the work. Use one when you want to read the changes as a reviewer would, leave feedback, and record a verdict without mixing that discussion into the original conversation.

### [Start a review](#start-a-review)

On a computer, click the eye button (**Reviews**) in the parent thread’s header. On a phone, open **More > Reviews** in the header. The menu is available even before the thread has changes. Choose an existing review or **Start Review** on desktop or [web](/docs/collaboration/delta-on-the-web). The desktop menu excludes archived reviews. You can also type `/review` in the composer, or press `Cmd+Shift+R` on macOS or `Ctrl+Shift+R` on Linux and Windows.

You can start a review from a top-level thread or a **Plain** subthread, such as a fork or subagent, including a nested one. You cannot start one from a Review or Merge subthread. Reviewing a subthread covers its changes together with changes inherited from its parents. A thread can have several reviews, each nested beneath it in the sidebar.

You can archive a review from the sidebar and restore it from thread history without archiving the parent thread.

### [What the review thread contains](#what-the-review-thread-contains)

When the review thread opens, Delta asks the agent for a **change guide**: a structured walkthrough of the change in the order that explains it best, with focused inline diffs, rather than a file-by-file listing. While the agent prepares it, the starting skill message remains visible in the conversation. The guide names the review thread after the work under review. A second review of the same unmerged work notes what changed since the last verdict, for example “Retry backoff fix (added tests)”. While its initial title is being generated, the thread shows **Generating title…**; renaming a review yourself always wins.

Delta asks the agent to identify work that has already landed on the default branch and focus the guide on what remains unmerged. When you select the branch base in the review thread’s **Changes** panel, the diff can still include work omitted from the guide after a squash merge.

### [Review and submit a verdict](#review-and-submit-a-verdict)

In the review thread, [comment](/docs/agents/comments) on the transcript or on code in the diffs, and ask the agent for changes. The review agent works in separate, forked Delta worktrees, so its edits do not change the parent’s files. Finishing the review or submitting a verdict does not merge those edits.

Review threads are separate from agent-created [attached subagents](/docs/agents/subagents). For a separate task rather than a review, you can also [send a message to a fork](/docs/agents/threads#send-a-message-to-a-fork).

When you’re done, choose **Submit Review**, then **Approve** or **Request Changes**. You can do this in the desktop app or on the web. Delta posts the verdict back to the parent thread, and the reviewer’s avatar in the parent’s header shows an approved or changes-requested badge. Submitting again updates your current verdict; earlier verdicts remain in the parent thread’s history. You can also use `/approve` or `/request-changes` to submit a verdict in any thread, without first creating a separate review thread.

Use the verdict’s navigation control in the review to jump to its event in the parent thread.

To bring the review’s edits back, click **Pull Changes** on the verdict in the parent thread. Each verdict saves a snapshot of the review’s edits. Pulling applies that snapshot, not later edits; submit another verdict to include those. If no edits were captured, the verdict has no **Pull Changes** button. After a successful pull, the button reads **Changes Pulled** and is disabled.

If Delta says the pull requires Git history that isn’t available on the web, open the thread in the desktop app and pull the changes there.

If pulling fails, check the parent’s files before trying again: some or all worktrees may already contain the changes, even if Delta couldn’t record the merge event.

### [Share a review](#share-a-review)

A review thread has no sharing settings of its own; it follows its parent. When the parent is shared, the review thread’s header shows a **Copy Share Link** button that copies a link straight to the review, and everyone with access to the parent can open it, comment, and submit verdicts. When the parent is not shared, that button is disabled: share the parent first. See [Collaborate in a Thread](/docs/collaboration/collaborate-thread).

## [Merge a subthread’s changes](#merge-a-subthreads-changes)

To bring a fork or subagent’s file changes into its immediate parent, type `/merge-into-parent` in the child and select the command. From the parent, select `/pull-from-subthread` to choose a child instead. The command palette and File menu also offer **Merge into Parent** and **Pull from Subthread…**.

After an agent turn, a child with file changes to bring back can show **Merge Changes to Parent**. This merges file changes, not the conversation, and does not commit or push to Git. Finishing a child turn does not merge anything automatically.

## [Merge selected worktrees from related threads](#merge-selected-worktrees-from-related-threads)

Ask the agent to bring in changes from selected worktrees in related threads. For example: “Merge the backend worktree from the review thread and the frontend worktree from its fork.” Specify which projects and source threads you want; you do not have to merge every project attached to a source thread.

Sources must belong to the same thread family: the top-level thread, its subthreads, or their forks. Independently created threads cannot be merged this way. The operation brings in file changes, without copying conversations or changing the sources. If a multi-worktree merge fails partway through, inspect the destination before retrying because earlier merges may have applied.

## [Merge changes](#merge-changes)

On desktop, open **Merge** in a top-level thread or a plain subthread’s header. With one eligible skill, choose **Merge Changes**, or press `Cmd+Shift+L` on macOS or `Alt+L` on Linux and Windows. With several skills, choose one under **Merge Skills** before the subthread starts. This workflow is not available in the browser.

Delta [forks the thread](/docs/agents/threads#fork-a-thread) into a dedicated Merge subthread. It inherits the parent’s conversation and worktrees, including changes inherited from earlier parents if you started from a subthread. The difference from an ordinary fork is what happens first: Delta asks the agent to run a [merge skill](/docs/agents/skills#configure-a-merge-skill), which defines your project’s merge strategy and publication steps. The button does not impose a Git workflow of its own.

If no eligible skill exists, choose **Create Merge Skill**. Delta opens a setup conversation using the parent’s live worktrees, so the approved skill becomes available in the original thread without pulling files back. Creating the skill does not merge or publish your changes.

If skill discovery fails, the menu offers a retry before creating a subthread.

### [Keep working in the Merge thread](#keep-working-in-the-merge-thread)

The Merge thread is still just a thread. You can ask its agent to edit files, run tests, and revisit a failed landing attempt there. Its worktrees advance separately from the parent’s, so later edits in either thread do not sync automatically. You do not need to turn it into a top-level thread to keep working. If you want its conversation, diffs, and files in the main view, use [Open as Primary](/docs/agents/threads#open-a-subthread-as-primary); that changes the view, not the parent-child relationship.

The agent’s reported result appears as a card event in both the Merge subthread and its parent. It shows **Merged** or **Merge Failed**, with the target branch, commit, and reported CI status.

The desktop sidebar also shows the merge result on the thread that produced it, or on its folded parent, without opening the conversation.

For example, suppose the landing checks find a failing test before anything is published:

1.  Ask the agent in the Merge thread to fix the failure and rerun the checks.
2.  Review the Merge thread’s **Changes** tab. When you want to try merging again, invoke the skill there. If an earlier attempt reached a push or another publication step, check what it already did first.
3.  To bring the fix back to the parent thread, ask its agent: “Merge the changes from Merge thread `<thread ID>` (or thread name) into this thread.” You can do this before or after a successful landing; merging worktrees does not itself publish or land the change.

Unlike a review verdict, a merge result doesn’t offer a **Pull Changes** button. Use [Merge into Parent](#merge-a-subthreads-changes), or ask the agent to merge the source’s worktrees. Neither finishing a Merge turn nor publishing the change merges its files into the parent thread. The Merge thread remains available after the attempt; you can return to it or archive it when you are done (most common workflow).

To return to a Merge thread you previously started from the same parent thread, click the merge icon and select it under **Previous Threads**. This opens the existing conversation rather than starting a new one. Archived Merge threads do not appear in this list.

## [Check the remote branch](#check-the-remote-branch)

In the desktop app, open the project’s menu. For a branch with a commit and a configured remote-tracking branch, choose **Check** in the remote-branch status row to compare the two. The result reports **Up to date with remote**, **Ahead of remote**, **Behind remote**, or **Diverged from remote**.

Checking fetches the remote branch for comparison; it does not merge changes into your checkout. Use the result to decide whether to pull or push.

## [Bring changes in](#bring-changes-in)

To bring changes in without a merge skill, use either of these paths:

### [Push to your local repository](#push-to-your-local-repository)

Push the agent’s branch to the `local` remote. Because `local` points directly at your repository, the branch appears there without a fetch or a round trip through GitHub. This is the way to hand yourself work on a branch that is not on origin.

You can ask the agent, or run the push yourself in the [thread’s terminal](/docs/agents/terminals) (type `!` at the start of an empty line in the composer):

```
git push local <branch-name>
```

Then switch to the branch in your repository:

```
git switch <branch-name>
```

This also works for the branch your repository has checked out: when your checkout is clean, the push updates your working tree in place and there is nothing to switch to. Git rejects a push that would overwrite uncommitted edits to tracked files.

If you want this after every commit, add a rule such as “After each commit, run `git push local <branch-name>`” to your project or personal [rules](/docs/configuration/settings#rules) rather than asking each time. In the terminal, `git commit -m "..." && git push local <branch-name>` does both at once.

### [Publish through origin](#publish-through-origin)

Ask the agent to push its branch to your source remote and open a pull request. This keeps the review in your team’s normal GitHub flow. The example below uses `origin`; replace it with your remote’s name if different. To run the published branch in your repository:

```
git fetch origin
git switch --track origin/<branch-name>
```

The [**Open in Delta Links** setting](/docs/configuration/settings#rules) controls whether the agent adds a link to the current thread when it opens a pull request. Delta considers the thread’s sharing settings and only asks for a link that the pull request’s reviewers can open. The setting is enabled by default.

For either path, if git refuses to switch because you have local edits, set them aside first and restore them when you switch back:

```
git stash
git switch <branch-name>
# later, back on your own branch:
git stash pop
```

## [What’s next](#whats-next)

*   [Delta & Git](/docs/concepts/delta-and-git) explains the remote setup behind these workflows.
*   [Comments](/docs/agents/comments) covers leaving review feedback the agent acts on.
*   [Threads](/docs/agents/threads) covers steering and managing the conversations these changes come from.