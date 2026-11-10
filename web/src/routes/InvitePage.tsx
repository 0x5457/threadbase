import { useMutation } from "@tanstack/react-query";
import { useEffect } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { toast } from "sonner";
import { api } from "@/lib/api";
import { Button, EmptyState } from "@/components/ui";

export function InvitePage() {
  const { token } = useParams();
  const navigate = useNavigate();
  const accept = useMutation({
    mutationFn: () =>
      api(`/api/invites/${encodeURIComponent(token ?? "")}/accept`, {
        method: "POST",
      }),
    onSuccess: () => {
      toast.success("Invite accepted");
      navigate("/", { replace: true });
    },
  });

  useEffect(() => {
    if (token) {
      accept.mutate();
    }
    // The mutation object is intentionally not a dependency; it would re-run on every render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [token]);

  return (
    <div className="flex h-full items-center justify-center p-6">
      <div className="w-full max-w-md">
        {accept.isError ? (
          <EmptyState
            title="Invite could not be accepted"
            description={String(accept.error)}
            action={
              <Button size="sm" variant="secondary" onClick={() => navigate("/")}>
                Back to workspace
              </Button>
            }
          />
        ) : (
          <EmptyState
            title="Joining thread…"
            description="Checking the invite and adding you to the thread."
          />
        )}
      </div>
    </div>
  );
}
