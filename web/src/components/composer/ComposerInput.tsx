import { Avatar } from "@mantine/core";
import { EditorContent, type Editor } from "@tiptap/react";
import { userColor as colorForUser } from "../../lib/draft";

function initials(name: string) {
  return name
    .split(/\s+/)
    .map((part) => part[0])
    .slice(0, 2)
    .join("")
    .toUpperCase();
}

interface ComposerInputProps {
  editor: Editor | null;
  hasText: boolean;
  writable: boolean;
  readOnlyAccount: boolean;
  agent: string;
  userId: string | null;
  userName: string | null;
  avatarUrl: string | null;
  userColor?: string;
}

export function ComposerInput({
  editor,
  hasText,
  writable,
  readOnlyAccount,
  agent,
  userId,
  userName,
  avatarUrl,
  userColor,
}: ComposerInputProps) {
  const accent = userColor ?? (userId ? colorForUser(userId) : undefined);

  return (
    <div className="composer-row">
      <Avatar
        size={18}
        radius="xl"
        color="brand"
        src={avatarUrl ?? undefined}
        styles={
          accent
            ? {
                root: {
                  backgroundColor: accent,
                  color: "#fff",
                },
              }
            : undefined
        }
      >
        {initials(userName ?? "You")}
      </Avatar>
      <div className="composer-input">
        <EditorContent editor={editor} />
        {!hasText ? (
          <span className="composer-placeholder" aria-hidden="true">
            {writable
              ? `Message ${agent}…`
              : readOnlyAccount
                ? "Read-only account — ask an admin for editing access"
                : "Read-only access to this thread"}
          </span>
        ) : null}
      </div>
    </div>
  );
}
