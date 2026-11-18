import {
  ActionIcon,
  Avatar,
  Box,
  Button,
  Card,
  Group,
  Menu,
  Text,
  Textarea,
} from "@mantine/core";
import {
  IconCheck,
  IconDots,
  IconSend,
  IconTrash,
} from "@tabler/icons-react";
import { useState } from "react";
import type { Comment } from "../../api";
import { useDelayedFlag } from "../../lib/use-delayed-flag";
import { can } from "../../lib/machine";
import { initials, timeOf } from "./format";

/**
 * Inline comment composer. The draft lives here (not in the thread view) so
 * typing does not re-render the transcript and the composer on every key.
 */
export function CommentDraft({
  onSave,
  onCancel,
}: {
  onSave: (body: string) => Promise<boolean>;
  onCancel: () => void;
}) {
  const [value, setValue] = useState("");
  const [saving, setSaving] = useState(false);
  const [failed, setFailed] = useState(false);
  const showSaving = useDelayedFlag(saving, 300, 500);

  const submit = () => {
    if (saving || !value.trim()) return;
    setFailed(false);
    setSaving(true);
    void onSave(value.trim())
      .then((saved) => {
        setSaving(false);
        if (saved) setValue("");
        else setFailed(true);
      })
      .catch(() => {
        setSaving(false);
        setFailed(true);
      });
  };

  return (
    <CommentEditor
      value={value}
      onChange={(next) => {
        setValue(next);
        if (failed) setFailed(false);
      }}
      saving={saving}
      showSaving={showSaving}
      failed={failed}
      onCancel={onCancel}
      onSubmit={submit}
    />
  );
}

export function CommentCard({
  comment,
  onResolve,
  onSend,
  onDelete,
  busy,
  pending = false,
  saved = false,
  error = null,
}: {
  comment: Comment;
  onResolve?: () => void;
  onSend?: () => void;
  onDelete?: () => void;
  busy?: boolean;
  /** An optimistic comment waiting for the server to assign its real id. */
  pending?: boolean;
  saved?: boolean;
  error?: string | null;
}) {
  const showBusy = useDelayedFlag(Boolean(busy), 300, 500);
  return (
    <div className="comment-row">
      <Card withBorder p={5} className="comment-card">
        <Group gap={6} align="flex-start" wrap="nowrap">
          <Avatar
            size={14}
            radius="xl"
            color="brand"
            src={comment.author.avatarUrl ?? undefined}
          >
            {initials(comment.author.displayName)}
          </Avatar>
          <div className="comment-text">
            <Group gap={4} className="comment-head" wrap="nowrap">
              <Text size="sm" fw={500}>
                {comment.author.displayName}
              </Text>
              <Text size="xs" c="dimmed">
                {timeOf(comment.createdAt)}
              </Text>
              {pending ? (
                <Text size="xs" c="dimmed">
                  · Saving…
                </Text>
              ) : saved ? (
                <Text size="xs" c="dimmed" className="comment-saved">
                  · Saved
                </Text>
              ) : null}
              {comment.status === "resolved" ? (
                <Text size="xs" c="dimmed">
                  · resolved
                </Text>
              ) : null}
              <Box style={{ flex: 1 }} />
              {pending ? null : (
                <Menu position="bottom-end" width={160} shadow="md">
                  <Menu.Target>
                    <ActionIcon
                      aria-label="Comment actions"
                      className="comment-menu"
                      size={16}
                      loading={showBusy}
                      disabled={busy}
                    >
                      <IconDots size={12} stroke={1.5} />
                    </ActionIcon>
                  </Menu.Target>
                  <Menu.Dropdown>
                    {can("comment", comment.status, "deliver") && onSend ? (
                      <Menu.Item
                        leftSection={<IconSend size={12} />}
                        onClick={onSend}
                      >
                        Send now
                      </Menu.Item>
                    ) : null}
                    {can("comment", comment.status, "resolve") && onResolve ? (
                      <Menu.Item
                        leftSection={<IconCheck size={12} />}
                        onClick={onResolve}
                      >
                        Resolve
                      </Menu.Item>
                    ) : null}
                    {onDelete ? (
                      <>
                        <Menu.Divider />
                        <Menu.Item
                          color="red"
                          leftSection={<IconTrash size={12} />}
                          onClick={onDelete}
                        >
                          Delete
                        </Menu.Item>
                      </>
                    ) : null}
                  </Menu.Dropdown>
                </Menu>
              )}
            </Group>
            <Text size="sm" className="comment-body">
              {comment.body}
            </Text>
            {error ? (
              <Text size="xs" c="red" className="comment-error">
                {error}
              </Text>
            ) : null}
          </div>
        </Group>
      </Card>
    </div>
  );
}

function CommentEditor({
  value,
  onChange,
  onSubmit,
  onCancel,
  saving,
  showSaving,
  failed,
}: {
  value: string;
  onChange: (value: string) => void;
  onSubmit: () => void;
  onCancel: () => void;
  saving: boolean;
  showSaving: boolean;
  failed: boolean;
}) {
  return (
    <div className="comment-row">
      <Card withBorder p={6} className="comment-card">
        <Textarea
          autoFocus
          autosize
          minRows={2}
          maxRows={6}
          placeholder="Leave a comment for the next turn…"
          value={value}
          onChange={(event) => onChange(event.currentTarget.value)}
          onKeyDown={(event) => {
            if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
              event.preventDefault();
              onSubmit();
            }
          }}
        />
        {failed ? (
          <Text size="xs" c="red" mt={6}>
            Couldn't save. Your comment is still here.
          </Text>
        ) : null}
        <Group justify="flex-end" gap="xs" mt={6}>
          <Button
            variant="subtle"
            size="compact-xs"
            color="gray"
            onClick={onCancel}
          >
            Cancel
          </Button>
          <Button
            size="compact-xs"
            color="dark"
            loading={showSaving}
            disabled={saving || !value.trim()}
            onClick={onSubmit}
          >
            Save comment
          </Button>
        </Group>
      </Card>
    </div>
  );
}
