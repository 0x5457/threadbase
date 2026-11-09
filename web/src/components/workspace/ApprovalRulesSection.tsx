import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ShieldCheck, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { deleteApprovalRule, fetchApprovalRules } from "@/api/endpoints";
import { Badge, Button, EmptyState, ErrorState, Skeleton } from "@/components/ui";

export function ApprovalRulesSection() {
  const queryClient = useQueryClient();
  const rules = useQuery({
    queryKey: ["approval-rules"],
    queryFn: fetchApprovalRules,
  });
  const remove = useMutation({
    mutationFn: (id: string) => deleteApprovalRule(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["approval-rules"] });
      toast.success("Allow rule revoked");
    },
    onError: (error) => toast.error(String(error)),
  });
  if (rules.isLoading) {
    return <Skeleton className="h-20 w-full" />;
  }
  if (rules.isError) {
    return (
      <ErrorState
        error={rules.error}
        title="Cannot load allow rules"
        onRetry={() => void rules.refetch()}
        compact
      />
    );
  }
  const items = rules.data?.rules ?? [];
  if (items.length === 0) {
    return (
      <EmptyState
        icon={ShieldCheck}
        title="No standing allow rules"
        description="Rules created with “Allow for thread” appear here and can be revoked."
        compact
      />
    );
  }
  return (
    <ul className="divide-y divide-border overflow-hidden rounded-md border border-border">
      {items.map((rule) => (
        <li key={rule.id} className="flex items-center gap-2 bg-surface px-3 py-2">
          <div className="min-w-0 flex-1">
            <p className="truncate text-[11.5px] text-fg">
              {rule.agent} · {rule.tool}
              {rule.commandPrefix ? (
                <span className="ml-1 font-mono text-[10px] text-muted">
                  {rule.commandPrefix}
                </span>
              ) : null}
            </p>
            <p className="text-[9.5px] text-faint">
              {rule.scope} · created by {rule.createdBy.displayName}
            </p>
          </div>
          <Badge size="sm" variant="outline">
            allow
          </Badge>
          <Button
            size="icon-sm"
            variant="ghost"
            aria-label="Revoke allow rule"
            loading={remove.isPending}
            onClick={() => remove.mutate(rule.id)}
          >
            <Trash2 aria-hidden />
          </Button>
        </li>
      ))}
    </ul>
  );
}
