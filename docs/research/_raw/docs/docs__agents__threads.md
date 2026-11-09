---
title: "Threads"
url: "https://delta.dev/docs/agents/threads"
description: "Start, steer, and manage agent threads."
---

# Threads

Start, steer, and manage agent threads.

A thread is a conversation with an agent and the basic unit of work in Delta. It can stand alone or include one or more projects. Each project it uses has a [Delta worktree](/docs/concepts/worktrees).

## [Start a thread](#start-a-thread)

Choose **File > New Empty Thread** to start without a project, or open a project to start a thread with its files. Describe what you want in plain language. In a project thread, the agent can read files, make edits, and run commands in the checkout selected for your project.

When you start a project thread in the desktop app using **Isolated Delta Worktree**, Delta automatically attempts to fetch the repository’s remote and starts from your configured worktree branch, or the repository’s default branch. You don’t need to pull changes into your primary checkout first. See [how Delta chooses your starting code](/docs/concepts/worktrees#where-new-threads-start).

Press `Tab` to indent ordinary composer text to the next two-column stop and `Shift+Tab` to outdent it. Markdown lists keep their structural indentation behavior.

### [Insert symbols](#insert-symbols)

Type `:` followed by a name in a message or comment to choose an emoji or mathematical symbol. Attached projects can also supply [custom emoji](/docs/configuration/project-setup#custom-emoji).

## [Fork a thread](#fork-a-thread)

A fork lets you take work in another direction without changing the parent’s files. It opens as a subthread with a copy of the parent’s conversation and [Delta worktrees](/docs/concepts/worktrees) at the point you fork. From then on, the two threads advance independently. The inherited conversation gives the agent context, but is not an instruction to finish the parent’s task.

### [Start a fork](#start-a-fork)

Type `/fork` and select the command to open a fork without sending a message. The fork starts with the parent’s history; write your task there. You can also press `Cmd+Shift+Enter` on macOS or `Ctrl+Shift+Enter` on Linux and Windows with an empty composer to create one.

To send a task directly to a new fork, write it in the parent or an existing fork and use the same shortcut. Delta runs the request in the new fork, not the parent; the sent request remains in the parent’s history. You can also choose **File > Send Message to Fork**. These gestures work on desktop and web. If [Send With Modifier](/docs/configuration/settings#thread) is off, use `Cmd+Enter` on macOS or `Ctrl+Enter` on Linux and Windows instead, or hold those keys while clicking the send button. You can still find `/fork` by typing `/subthread` in the command picker.

You can include a [skill](/docs/agents/skills) when sending to a fork. If the skill picker is open, the shortcut accepts the highlighted skill and sends the draft. Pressing `Enter` in the picker only inserts the skill.

### [Work in a fork](#work-in-a-fork)

You can ask for edits, run checks, and review the fork’s **Changes** tab as you would in the parent. To continue the parent’s task, ask for that explicitly. You can also fork a fork, and close its tab without stopping its work. Click the fork’s title in its tab toolbar to rename it.

File changes do not sync between the fork and parent after the fork starts. To bring edits back, use [Merge into Parent](/docs/agents/review-and-sync#merge-a-subthreads-changes) or ask the parent agent to merge the fork’s changes. Finishing work in the fork does not merge them automatically. New [isolated subagents](/docs/agents/subagents#profiles) also need an explicit merge.

To read the conversation inherited from the parent, place the cursor on the **Forked from** row and press `Tab`, or use its hover controls. The inherited text is read-only, but you can comment on it in the fork. Those comments do not change the parent’s conversation. Press `Tab` on the row again to hide it.

Forks you create do not automatically report progress or blockers to the parent. Ask the agent to [send a message to the parent](#send-information-to-another-agent) if you need an update there. Agent-created subagents report their final result automatically; [Merge threads](/docs/agents/review-and-sync#merge-changes) also report their outcome in the parent.

### [Open a subthread as primary](#open-a-subthread-as-primary)

Open a subthread as primary when you want to work in it rather than keep it beside the parent in a tab. In the desktop sidebar, click the subthread. From an open subthread tab, run **Open as Primary** in the command palette on desktop or web. On desktop, this also works for subagents.

From a subthread tab, you can also press `Cmd+Option+Enter` on macOS or `Ctrl+Alt+Enter` on Linux and Windows.

The main conversation area now shows the subthread, and its **Changes** tab and files let you inspect the work there. You can keep asking for edits in that thread. This only changes which thread is in the main view: the subthread remains a child, and isolated worktrees do not merge into the parent. New subthreads have a **Forked from** row linking back to their parent; use it to return, or use the desktop shortcut below.

Subthreads can contain nested subthreads. A browser turn still has the [browser’s tool limitations](/docs/collaboration/delta-on-the-web#choose-where-to-run-a-request).

### [Find and open child threads](#find-and-open-child-threads)

On desktop and web, run **View All Subthreads** or press `Cmd+J` on macOS or `Ctrl+J` on Linux and Windows to browse the parent’s children. Filter by kind, and press `Tab` to show or hide archived children.

In the desktop sidebar, hold `Cmd` on macOS or `Ctrl` on Linux and Windows while clicking a subthread to open it in a tab instead of as primary. You can also right-click a fork and choose **Open in Tab**, **Rename**, **Mark as Read**, or **Mark as Unread**. Double-click its name to rename it. Press `Cmd+1` on macOS or `Ctrl+1` on Linux and Windows to focus the parent without closing a subthread tab. When a subthread is primary, press `Cmd+Option+U` on macOS or `Ctrl+Alt+U` on Linux and Windows to return to its parent.

## [Ask the agent to create a thread](#ask-the-agent-to-create-a-thread)

In the desktop app, you can ask the agent to start a separate thread for an independent task. This creates a top-level thread with a fresh conversation and isolated Delta worktrees; it does not inherit the current thread’s edits. To disable this capability, turn off **Agent Thread Creation** in **Settings > Thread > Rules**; subagent delegation remains separate.

You can also ask the agent to fork an existing conversation by giving its thread ID or shared link. The new thread inherits that source’s recorded conversation and worktree state, then advances independently. Unlike `/fork`, it appears as a top-level thread rather than beneath the source, and does not merge its changes back automatically.

## [Resume work on a feature](#resume-work-on-a-feature)

Use **Go to Thread** to find the thread where you started the work. The search includes [archived threads](#archive-and-reopen-threads). Open the thread’s [Changes tab](/docs/agents/review-and-sync#review-the-agents-work) to see where you left off, or ask the agent for a summary.

The thread keeps the conversation and recorded file changes together, including edits you haven’t committed. Starting a new isolated thread on the same branch does not resume that work or bring over its uncommitted edits or unpushed commits. See [Why can two workspaces use the same branch name?](/docs/concepts/worktrees#checkouts-a-delta-worktrees-files-on-a-machine).

To make the thread easier to find next time, use **Rename Thread** to give it a feature or issue name. You can match the branch name, but Delta doesn’t require them to match. On desktop, you can also double-click the thread’s name in the sidebar to rename it.

Start a new isolated thread for independent work. To collaborate on the existing work, [share the original thread](/docs/collaboration/collaborate-thread).

## [Organize threads](#organize-threads)

In the desktop sidebar’s filter menu, choose **Group by** to group threads by **Project**, **Time**, **State**, **Owner**, or **Tags**. Tag grouping uses each thread’s first assigned tag. The sidebar separates projects and threads by organization; threads you joined as a guest appear under **Shared with You**. Choose **Overview** beneath the organization name to browse and open threads in that organization.

Under **Sort by**, choose **Creation Time**, **Last Prompted**, **Last Response**, or **Manual**. **Last Response** puts the most recent agent responses first. Manual sorting is not available with Time grouping or in thread history. Time grouping and thread history group older threads by month.

Under **Filter by > Subthreads**, choose which children to show: **Plain**, **Reviews**, **Merge**, and **Subagents**. **Plain** includes forks and new subagents; **Subagents** shows older attached subagents. An expanded parent shows the ten newest matching children by default. Choose **Show N more** to reveal the rest. Under **Display**, use **Subthreads Per Thread** or **Threads Per Project** to choose 10, 25, 50, or **All**.

Use **Filter by > Projects** to hide projects from the sidebar in any grouping. To show one again, select it from the sidebar’s **Add Project** menu. The same sidebar preferences are available in **Settings > Sidebar**.

Choose **Show Thread History** to browse threads chronologically. Use **Filter History by Project** or the archived-only toggle to narrow the list.

To bookmark the current thread on desktop, press `Cmd+D` on macOS or `Ctrl+D` on Linux and Windows. Choose **Show Bookmarked Threads** from the sidebar or menu bar to find bookmarked threads, including archived ones. On macOS, you can also press `Cmd+Option+B`; Linux and Windows have no default shortcut for this view.

Right-click a thread and choose a color from its context menu to make it easier to recognize. Choosing a color adds its associated tag; choosing it again removes that tag. You can also apply or remove a color tag for several selected threads at once. Subthreads and subagents can have colors independent of their parents. Colors organize your sidebar; they are not shared with collaborators.

### [Tag and filter threads](#tag-and-filter-threads)

On desktop, right-click a thread and open **Tags**, or run **Edit Thread Tags** from the command palette. Assign tags to organize related work, then use **Filter by > Tags** in the sidebar filter menu to narrow the list. The Bookmarks view also has filters for tags, ownership, and which subthreads to show.

To let Delta choose a tag, right-click a single thread and select **Auto-tag Thread** beside the color dots. Delta applies a fitting existing tag or creates a new named, colored tag.

Turn on **Show Tag Names** in the sidebar filter menu to display all assigned tags beneath thread titles. This is off by default. You can also change it in **Settings > Sidebar > Show Tag Names**.

Colored tags control a thread’s color. If a thread has several colored tags, the first one assigned determines its color.

Subagents have their own right-click **Tags** menu, so you can tag them independently of their parent thread.

To add a thread’s tags to its existing subthreads, right-click the parent and choose **Apply Tag to Subthreads**. This also adds the tags to nested descendants without removing their existing tags. The command appears when a descendant is missing one of the parent’s tags.

When the parent has several tags, the command is **Apply Tags to Subthreads**.

## [Send information to another agent](#send-information-to-another-agent)

Ask the agent to send a message to another thread and `@`\-mention the destination, for example: “Send your findings to @Investigate sync errors.” From a subthread, you can ask it to send the findings to the parent.

The message appears as a **Sent Agent Message** in the source and a **Received Agent Message** in the destination. Expand the event to read it. The receiving agent handles it during its current response or starts a new response if idle. Sending a message does not merge file changes between threads.

## [Edit a sent message](#edit-a-sent-message)

You can edit one of your earlier messages in place. Sending the edit replaces the conversation after that message: later responses and follow-ups are discarded, and the agent answers the edited message fresh. Use this to rework a prompt that sent the agent in the wrong direction.

## [Revert a conversation](#revert-a-conversation)

Place the cursor at the point you want to return to, then choose **Edit > Revert Conversation to Cursor**. Delta asks for confirmation before removing later messages. It restores attached Delta worktrees that still match that point; worktrees that now use a different checkout are left unchanged.

Messages that remain in the conversation stay in sent history rather than becoming drafts.

## [Mention files and folders](#mention-files-and-folders)

Type `@` in the composer and choose **Files & Directories** to mention a file or folder from an attached project. Selecting an entry inserts a link to it. A folder mention points the agent to that folder; it does not attach all its contents.

By default, `@` searches people and threads. Choose a category to search **Files & Directories**, **Threads**, **Users**, or **Terminals**, or type `@file:`, `@thread:`, `@user:`, or `@terminal:`. Unique abbreviations such as `@th:` also work. Matching categories stay visible while you search; files and folders appear only in a file-scoped search.

Opening a folder link reveals it in the project panel. In a narrow browser window, widen the window to open folders.

## [Open files from the transcript](#open-files-from-the-transcript)

When the agent reads, edits, or patches a file, its output links back to the file. Place the cursor on the relevant line in a Read File, Edit File, or Apply Patch block, or in a change-guide diff, and press `Option+Enter` on macOS or `Alt+Enter` on Linux and Windows to open that location in the file pane.

The same shortcut opens a link under the cursor in the conversation, whether **Send With Modifier** is on or off.

File paths in **External Changes** reports also open with a click or this shortcut. For Markdown files, use the pencil button in the file toolbar to switch between **Show Markdown Source** and **Show Markdown Preview**.

The file pane’s version selector switches between **Latest** and a read-only **Before Last Turn** view without reverting your files. Split the tab to compare both versions side by side. This also works in the browser.

Image files open in tabs with zoom controls. In the file pane, drag over line numbers to select whole lines. With the pane focused, use `Cmd+Option+Left` and `Cmd+Option+Right` on macOS or `Ctrl+Alt+Left` and `Ctrl+Alt+Right` on Linux and Windows to switch tabs.

You can also use `Ctrl+Tab` and `Ctrl+Shift+Tab` to cycle file tabs while the file pane has focus. Outside the file pane, those keys switch recent threads.

To find a file directly, press `Cmd+P` on macOS or `Ctrl+P` on Linux and Windows. Enter `path:line` or `path:line:column` to open a specific position.

To collapse an image preview in a message, use its chevron or press `Tab` with the image selected. To collapse previews by default, turn on **Settings > Thread > Messages > Collapse Images by Default**.

When the agent reads another page of a long tool output, the page’s heading identifies the source. When the source name is linked, click it to return to the original output.

## [Search and navigate](#search-and-navigate)

Press `Cmd+F` on macOS or `Ctrl+F` on Linux and Windows to search the rendered contents of a thread. Search matches visible text across Markdown formatting, so you do not need to include Markdown punctuation in the query.

To find a recent conversation in the desktop app, open the command palette with `Cmd+K` on macOS or `Ctrl+K` on Linux and Windows. **Recent Threads** prioritizes threads you prompted most recently, not those you merely opened.

With the cursor inside a code block, press `Cmd+A` on macOS or `Ctrl+A` on Linux and Windows to select its code. Press again to expand the selection to the current message, then again to select the whole thread.

In a long thread, hold `Option` on macOS or `Alt` on Linux and Windows while scrolling to move between sent user turns and review events. Moving past the latest turn jumps to the end of the thread.

Press `Cmd+[` and `Cmd+]` on macOS, or `Alt+[` and `Alt+]` on Linux and Windows, to move between navigation stops, including final responses in long agent turns.

To pin or unpin the current thread, press `Cmd+Option+P` on macOS or `Ctrl+Alt+P` on Linux and Windows.

### [Bookmark a turn](#bookmark-a-turn)

Hold `Cmd` on macOS or `Ctrl` on Linux and Windows and click a user-turn marker in the rail at the edge of the conversation to bookmark it. Repeat the gesture to remove the bookmark. Turn bookmarks persist in the thread and are visible to everyone with access, on desktop and web.

## [Thread lifecycle](#thread-lifecycle)

Threads persist. You can:

*   **Leave and return.** A thread keeps its full history and any Delta worktrees.
*   **Run several at once.** Conversations stay separate. When threads use projects, each normally gets separate managed Delta worktrees, so their project changes do not overlap. This isolates project files; it does not sandbox the agent from the rest of the selected machine. Threads that reuse an existing local checkout are an exception; see [Delta Worktrees](/docs/concepts/worktrees#using-the-local-clone-with-delta).
*   **Switch its model.** The conversation and any Delta worktrees carry on; the new model picks up where the last one left off. See [Models & Providers](/docs/agents/models-and-providers).
*   **Abandon one.** With a managed checkout, a thread’s changes, including its commits, do not touch your project folder unless you bring them in. See [Bring changes in](/docs/agents/review-and-sync#bring-changes-in).
*   **Share one.** Invite others into the conversation. See [Collaborate in a Thread](/docs/collaboration/collaborate-thread).
*   **Open one in a browser.** Browse and read synced threads without the desktop app. See [Delta on the Web](/docs/collaboration/delta-on-the-web).
*   **Review one.** Read the agent’s work and record a verdict in a separate review thread. See [Review in a review thread](/docs/agents/review-and-sync#review-in-a-review-thread).

## [Organize projects in the sidebar](#organize-projects-in-the-sidebar)

On desktop, right-click a project header and choose **Rename Project** to give it a custom local name. Before sending a thread’s first message, drag that thread onto a project header to make that project primary.

## [Archive and reopen threads](#archive-and-reopen-threads)

Archiving removes a thread from the active thread list without deleting its conversation or Delta worktrees. Choose **File > Archive Thread**, run **Archive Thread** from the command palette, or press `Cmd+W` on macOS or `Ctrl+W` on Linux and Windows. In the thread list, `Cmd+W` archives every selected thread. When the file pane is focused, the shortcut closes its active tab; when the project or Changes panel is focused, it closes that panel. Closing an untitled thread that has never been sent deletes it locally instead of archiving it; the sidebar button is labeled **Delete Empty Thread**.

In the desktop app, type `/archive` in the thread’s composer and select the command to archive that thread.

To find any archived thread, search for its title in the command palette and select it; `Cmd+T` on macOS or `Ctrl+T` on Linux and Windows opens the palette with the thread filter already applied. You can also choose **File > Reopen Archived Thread**. `Cmd+Shift+T` on macOS or `Ctrl+Shift+T` on Linux and Windows reopens a recently archived thread.

You can also archive subthreads from the sidebar and restore them from thread history without archiving their parent.

Signing in on another computer does not automatically populate its desktop thread list or sync your sidebar organization. To continue a shared thread there, [open its link](/docs/collaboration/collaborate-thread#join-a-thread). You can also [browse accessible threads on the web](/docs/collaboration/delta-on-the-web#browse-threads).

Archiving organizes your thread list without deleting the server’s copy of a synced thread. To free disk space, Delta removes the local history of an archived thread three days after you archive or last open it, once the server holds a full copy. Opening the thread downloads it again, so an old archived thread needs a connection to open. Threads that were never synced stay on your machine. See [Data Storage & Deletion](/docs/privacy-and-security/data-storage#delete-individual-threads) for the separate deletion behavior and its current limitations.

## [Skills](#skills)

Threads can draw on [skills](/docs/agents/skills): reusable instructions that teach the agent project-specific or personal workflows.

## [Subagents](#subagents)

An agent can delegate parts of a task to subagents, which appear nested under the parent in the thread sidebar. See [Subagents](/docs/agents/subagents) for when to use them, how to follow their work, and how their changes reach the parent’s Delta worktrees.

## [What’s next](#whats-next)

*   [Review & Sync Changes](/docs/agents/review-and-sync) covers how the agent’s work reaches your repository.
*   [Collaborate in a Thread](/docs/collaboration/collaborate-thread) covers sharing a thread with collaborators.
*   [Terminals](/docs/agents/terminals) covers interactive and background terminals.
*   [Models & Providers](/docs/agents/models-and-providers#context-windows-and-compaction) covers context windows and compaction.