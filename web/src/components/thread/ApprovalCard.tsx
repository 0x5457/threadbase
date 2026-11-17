import { Box, Button, Card, Group, Stack, Text } from "@mantine/core";
import type { Approval } from "../../api";

type ApprovalOption = Approval["options"][number];

const FALLBACK_OPTIONS: ApprovalOption[] = [
  { id: "deny", kind: "deny", label: "Reject" },
  {
    id: "allow_session",
    kind: "allow_session",
    label: "Always allow",
  },
  { id: "allow_once", kind: "allow_once", label: "Allow once" },
];

/**
 * A permission request is not always a yes/no dialog. pi uses the same
 * mechanism for multi-choice questions, where every answer has the adapter's
 * `allow_once` kind (for example "enabled" vs "disabled"). Showing those as a
 * row of identical primary buttons made them look interchangeable and sent
 * only the kind, so the adapter always picked the first one. Choices are
 * therefore rendered as an explicit single-select list, and the exact option
 * id travels with the decision.
 */
export function ApprovalCard({
  approval,
  busy,
  readOnly = false,
  onDecide,
}: {
  approval: Approval;
  busy: boolean;
  readOnly?: boolean;
  onDecide: (option: ApprovalOption) => void;
}) {
  const options =
    approval.options.length > 0 ? approval.options : FALLBACK_OPTIONS;
  const choices = options.filter((option) => option.kind !== "deny");
  const denyOptions = options.filter((option) => option.kind === "deny");
  const counts = new Map<string, number>();
  for (const option of choices) {
    counts.set(option.kind, (counts.get(option.kind) ?? 0) + 1);
  }
  const choiceMode = [...counts.values()].some((count) => count > 1);
  // One row reads best when destructive choices sit left and the direct
  // approval sits right, regardless of the adapter's emission order.
  const rank = (kind: ApprovalOption["kind"]) =>
    kind === "deny" ? 0 : kind === "allow_once" ? 2 : 1;
  const orderedOptions = options.toSorted((a, b) => rank(a.kind) - rank(b.kind));
  const fallbackDeny: ApprovalOption = {
    id: "deny",
    kind: "deny",
    label: "Cancel",
  };
  // Some adapters model "discard"/"cancel" as just another allow_once choice.
  // Repeating it as a separate escape hatch would present two cancels.
  const hasCancelChoice = choices.some((option) =>
    /\b(cancel|abort|discard|decline|reject)\b/i.test(option.label),
  );
  const showCancel = denyOptions.length > 0 || !hasCancelChoice;
  const question = approval.kind === "other";

  return (
    <div className="block">
      <div className="block-gutter" />
      <div className="block-body">
        <Card withBorder p={0} className="approval-card">
          <Group h={27} px={9} gap={6} className="approval-head" wrap="nowrap">
            <span className="dot waiting" />
            <Text size="sm" c="dimmed">
              {question ? "Decision requested" : "Permission requested"}
            </Text>
            <Box style={{ flex: 1 }} />
            <Text size="sm" c="dimmed">
              {approval.agent}
            </Text>
          </Group>
          <Box p={9}>
            <Text size="md" fw={500} mb={4}>
              {approval.title}
            </Text>
            {approval.paths.length ? (
              <Text size="sm" c="dimmed" className="approval-path">
                {approval.paths.join(", ")}
              </Text>
            ) : null}
            {approval.command ? (
              <pre className="cmd">{approval.command}</pre>
            ) : approval.input ? (
              <pre className="cmd">{approval.input}</pre>
            ) : null}
            {readOnly ? (
              <Text size="xs" c="dimmed" mt={6}>
                Your account has read-only access, so decisions are disabled.
                Ask an administrator to allow editing.
              </Text>
            ) : null}

            {choiceMode ? (
              <Stack gap={6} mt={10}>
                <Text size="xs" c="dimmed">
                  Choose one
                </Text>
                {choices.map((option) => (
                  <Button
                    key={option.id}
                    className="approval-choice"
                    variant="default"
                    justify="flex-start"
                    fullWidth
                    disabled={readOnly}
                    loading={busy}
                    onClick={() => onDecide(option)}
                  >
                    {option.label}
                  </Button>
                ))}
                {showCancel ? (
                  <Group justify="flex-end" mt={2}>
                    <Button
                      variant="subtle"
                      color="gray"
                      disabled={readOnly}
                      loading={busy}
                      onClick={() => onDecide(denyOptions[0] ?? fallbackDeny)}
                    >
                      {denyOptions[0]?.label ?? fallbackDeny.label}
                    </Button>
                  </Group>
                ) : null}
              </Stack>
            ) : (
              <Group justify="flex-end" gap={6} mt={8} className="approval-actions">
                {orderedOptions.map((option) => (
                  <Button
                    key={option.id}
                    loading={busy}
                    disabled={readOnly}
                    variant={
                      option.kind === "deny"
                        ? "subtle"
                        : option.kind === "allow_once"
                          ? "filled"
                          : "default"
                    }
                    color={
                      option.kind === "deny"
                        ? "gray"
                        : option.kind === "allow_once"
                          ? "brand"
                          : "gray"
                    }
                    onClick={() => onDecide(option)}
                  >
                    {option.label}
                  </Button>
                ))}
              </Group>
            )}
          </Box>
        </Card>
      </div>
    </div>
  );
}
