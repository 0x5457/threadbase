import { notifications } from "@mantine/notifications";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import {
  addMember,
  createInvite,
  getThreadBundle,
  removeMember,
  revokeInvite,
  setShareLevel,
  updateMember,
  type ShareInvite,
  type ShareMember,
  type ShareState,
} from "../../api";

function inviteUrl(token: string | null) {
  if (!token) return null;
  return `${window.location.origin}/invite/${token}`;
}

export function useShareDialog(threadId: string | null, opened: boolean) {
  const queryClient = useQueryClient();
  const [level, setLevel] = useState<ShareState["shareLevel"]>("invite_only");
  const [username, setUsername] = useState("");
  const [memberRole, setMemberRole] =
    useState<ShareMember["role"]>("editor");
  const [email, setEmail] = useState("");
  const [inviteRole, setInviteRole] = useState<ShareMember["role"]>("editor");
  const [inviteLink, setInviteLink] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const bundle = useQuery({
    queryKey: ["thread", threadId],
    queryFn: () => getThreadBundle(threadId as string),
    enabled: Boolean(threadId),
    staleTime: 2_000,
  });
  const share = bundle.data?.bundle.share;

  useEffect(() => {
    if (share) setLevel(share.shareLevel);
  }, [share]);

  useEffect(() => {
    if (!opened) return;
    setUsername("");
    setEmail("");
    setInviteLink(null);
    setBusy(null);
    setMemberRole("editor");
    setInviteRole("editor");
  }, [opened, threadId]);

  const refresh = () => {
    if (!threadId) return;
    void queryClient.invalidateQueries({ queryKey: ["thread", threadId] });
  };

  const run = async (key: string, action: () => Promise<unknown>) => {
    setBusy(key);
    try {
      await action();
      refresh();
    } catch (cause) {
      notifications.show({
        message: cause instanceof Error ? cause.message : "Request failed",
        color: "red",
      });
    } finally {
      setBusy(null);
    }
  };

  const changeLevel = (next: ShareState["shareLevel"]) => {
    if (!share) return;
    setLevel(next);
    void run("level", () => setShareLevel(share.threadId, next));
  };

  const add = () => {
    if (!threadId || !username.trim()) return;
    void run("add", async () => {
      await addMember(threadId, username.trim(), memberRole);
      setUsername("");
      notifications.show({ message: "Member added", color: "teal" });
    });
  };

  const invite = () => {
    if (!threadId) return;
    void run("invite", async () => {
      const result = await createInvite(threadId, {
        email: email.trim() || undefined,
        role: inviteRole,
      });
      setEmail("");
      setInviteLink(inviteUrl(result.token));
      notifications.show({ message: "Invite created", color: "teal" });
    });
  };

  const updateRole = (member: ShareMember, role: ShareMember["role"]) => {
    if (!threadId) return;
    void run(`role:${member.user.id}`, async () => {
      await updateMember(threadId, member.user.id, role);
    });
  };

  const remove = (member: ShareMember) => {
    if (!threadId) return;
    void run(`remove:${member.user.id}`, () =>
      removeMember(threadId, member.user.id),
    );
  };

  const revoke = (invite: ShareInvite) => {
    if (!threadId) return;
    void run(`revoke:${invite.id}`, () => revokeInvite(threadId, invite.id));
  };

  return {
    bundle,
    share,
    level,
    changeLevel,
    username,
    setUsername,
    memberRole,
    setMemberRole,
    email,
    setEmail,
    inviteRole,
    setInviteRole,
    inviteLink,
    busy,
    add,
    invite,
    updateRole,
    remove,
    revoke,
  };
}
