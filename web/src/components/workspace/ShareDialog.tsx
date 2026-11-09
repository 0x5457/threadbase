import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Check, Copy, Link2, UserPlus } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import {
  addMember,
  createInvite,
  removeMember,
  revokeInvite,
  setShareLevel,
} from "@/api/endpoints";
import { queryKeys, useMe } from "@/api/queries";
import type { Role } from "@/api/types/Role";
import type { ThreadBundleDto } from "@/api/types/ThreadBundleDto";
import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  Input,
} from "@/components/ui";
import { cn } from "@/lib/utils";

interface ShareDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  bundle: ThreadBundleDto;
}

export function ShareDialog({ open, onOpenChange, bundle }: ShareDialogProps) {
  const queryClient = useQueryClient();
  const me = useMe();
  const [username, setUsername] = useState("");
  const [role, setRole] = useState<Role>("editor");
  const [inviteLink, setInviteLink] = useState<string | null>(null);
  const thread = bundle.thread;
  const owner =
    bundle.members.find((member) => member.user.id === me.data?.user.id)
      ?.role === "owner" || me.data?.user.isAdmin === true;
  const refresh = () => {
    void queryClient.invalidateQueries({
      queryKey: queryKeys.threadBundle(thread.id),
    });
    void queryClient.invalidateQueries({ queryKey: queryKeys.threads });
  };
  const level = useMutation({
    mutationFn: (value: "invite_only" | "workspace") =>
      setShareLevel(thread.id, value),
    onSuccess: refresh,
    onError: (error) => toast.error(String(error)),
  });
  const member = useMutation({
    mutationFn: () => addMember(thread.id, { username: username.trim(), role }),
    onSuccess: () => {
      setUsername("");
      refresh();
      toast.success("Member added");
    },
    onError: (error) => toast.error(String(error)),
  });
  const invite = useMutation({
    mutationFn: () =>
      createInvite(thread.id, {
        username: username.trim() || null,
        email: null,
        role,
      }),
    onSuccess: (response) => {
      setUsername("");
      refresh();
      if (response.token) {
        const link = `${window.location.origin}/invite/${response.token}`;
        setInviteLink(link);
        void navigator.clipboard.writeText(link).catch(() => undefined);
        toast.success("Invite link copied");
      } else {
        toast.success("Member added");
      }
    },
    onError: (error) => toast.error(String(error)),
  });
  const revoke = useMutation({
    mutationFn: (inviteId: string) => revokeInvite(thread.id, inviteId),
    onSuccess: refresh,
    onError: (error) => toast.error(String(error)),
  });
  const remove = useMutation({
    mutationFn: (userId: string) => removeMember(thread.id, userId),
    onSuccess: refresh,
    onError: (error) => toast.error(String(error)),
  });

  const copyLink = async () => {
    await navigator.clipboard.writeText(window.location.href);
    toast.success("Thread link copied");
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title="Share thread"
        footer={
          <>
            <Button size="sm" variant="ghost" onClick={() => onOpenChange(false)}>
              Done
            </Button>
            <Button size="sm" variant="secondary" onClick={() => void copyLink()}>
              <Link2 aria-hidden />
              Copy link
            </Button>
          </>
        }
      >
        <div className="space-y-4">
          <div className="space-y-1.5">
            <p className="text-[11px] font-medium text-muted">Access</p>
            <div className="grid grid-cols-2 gap-1.5">
              {(
                [
                  ["invite_only", "Only invited"],
                  ["workspace", "Whole workspace"],
                ] as const
              ).map(([value, label]) => (
                <button
                  key={value}
                  type="button"
                  disabled={!owner}
                  onClick={() => level.mutate(value)}
                  className={cn(
                    "rounded-md border px-2.5 py-2 text-left text-[11.5px] transition-colors",
                    thread.shareLevel === value
                      ? "border-accent/50 bg-accent/10 text-fg"
                      : "border-border bg-surface text-muted hover:bg-hover",
                  )}
                >
                  <span className="flex items-center gap-1.5">
                    {thread.shareLevel === value ? (
                      <Check className="size-3 text-accent" aria-hidden />
                    ) : null}
                    {label}
                  </span>
                </button>
              ))}
            </div>
          </div>

          <div className="space-y-1.5">
            <p className="text-[11px] font-medium text-muted">
              Members <span className="text-faint">({bundle.members.length})</span>
            </p>
            <ul className="divide-y divide-border overflow-hidden rounded-md border border-border">
              {bundle.members.map((entry) => (
                <li
                  key={entry.user.id}
                  className="flex items-center gap-2 bg-surface px-2.5 py-1.5"
                >
                  <span
                    className="size-2 rounded-full"
                    style={{ background: entry.user.color }}
                  />
                  <span className="min-w-0 flex-1 truncate text-[11.5px] text-fg">
                    {entry.user.displayName}
                    <span className="ml-1 text-[10px] text-faint">
                      @{entry.user.username}
                    </span>
                  </span>
                  <Badge size="sm" variant="neutral">
                    {entry.role}
                  </Badge>
                  {owner &&
                  entry.user.id !== me.data?.user.id &&
                  entry.role !== "owner" ? (
                    <button
                      type="button"
                      className="text-[10px] text-faint hover:text-danger"
                      onClick={() => remove.mutate(entry.user.id)}
                    >
                      remove
                    </button>
                  ) : null}
                </li>
              ))}
            </ul>
          </div>

          {bundle.share.pendingInvites.length > 0 ? (
            <div className="space-y-1.5">
              <p className="text-[11px] font-medium text-muted">Pending links</p>
              <ul className="divide-y divide-border overflow-hidden rounded-md border border-border">
                {bundle.share.pendingInvites.map((invite) => (
                  <li
                    key={invite.id}
                    className="flex items-center gap-2 bg-surface px-2.5 py-1.5 text-[10.5px]"
                  >
                    <span className="min-w-0 flex-1 truncate text-muted">
                      {invite.email ?? "invite link"} · {invite.role}
                    </span>
                    {owner ? (
                      <button
                        type="button"
                        className="text-faint hover:text-danger"
                        onClick={() => revoke.mutate(invite.id)}
                      >
                        revoke
                      </button>
                    ) : null}
                  </li>
                ))}
              </ul>
            </div>
          ) : null}

          {owner ? (
            <div className="space-y-1.5">
              <p className="text-[11px] font-medium text-muted">Invite</p>
              <div className="flex gap-1.5">
                <Input
                  value={username}
                  onChange={(event) => setUsername(event.target.value)}
                  placeholder="username or email"
                  className="h-8"
                />
                <select
                  value={role}
                  onChange={(event) => setRole(event.target.value as Role)}
                  className="h-8 rounded-md border border-border bg-surface px-2 text-[11px] text-muted"
                >
                  <option value="editor">Editor</option>
                  <option value="viewer">Viewer</option>
                  <option value="owner">Owner</option>
                </select>
                <Button
                  size="sm"
                  variant="primary"
                  loading={member.isPending || invite.isPending}
                  disabled={!username.trim()}
                  onClick={() => member.mutate()}
                >
                  <UserPlus aria-hidden />
                  Add
                </Button>
              </div>
              <button
                type="button"
                className="text-[10.5px] text-accent hover:underline"
                onClick={() => invite.mutate()}
              >
                Create a 14-day invite link instead
              </button>
              {inviteLink ? (
                <div className="flex items-center gap-1.5 rounded-md border border-border bg-raised px-2 py-1.5">
                  <code className="min-w-0 flex-1 truncate text-[10px] text-fg">
                    {inviteLink}
                  </code>
                  <button
                    type="button"
                    className="text-faint hover:text-fg"
                    onClick={() => void navigator.clipboard.writeText(inviteLink)}
                  >
                    <Copy className="size-3" aria-hidden />
                  </button>
                </div>
              ) : null}
            </div>
          ) : (
            <p className="text-[10.5px] text-faint">
              Only the thread owner can change access or invite people.
            </p>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
