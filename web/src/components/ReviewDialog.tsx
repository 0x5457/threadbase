import { Group, Loader, Modal, Stack, Text } from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { ApprovalSummary } from "./review/ApprovalSummary";
import { ChangesSummary } from "./review/ChangesSummary";
import { CommentSummary } from "./review/CommentSummary";
import { ReviewVerdictForm } from "./review/ReviewVerdictForm";
import { useReviewDialog } from "./review/useReviewDialog";

export function ReviewDialog({
  threadId,
  opened,
  onClose,
  onOpenChanges,
}: {
  threadId: string | null;
  opened: boolean;
  onClose: () => void;
  onOpenChanges?: () => void;
}) {
  const phone = useMediaQuery("(max-width: 47.99em)");
  const review = useReviewDialog(threadId, opened, onClose);

  return (
    <Modal
      opened={opened}
      onClose={onClose}
      title="Review thread"
      size="lg"
      centered
      fullScreen={phone}
    >
      {review.bundle.isPending && threadId ? (
        <Group justify="center" py="xl">
          <Loader size="sm" />
        </Group>
      ) : review.bundle.isError ? (
        <Text size="sm" c="red">
          {review.bundle.error instanceof Error
            ? review.bundle.error.message
            : "Cannot load thread"}
        </Text>
      ) : review.data ? (
        <Stack gap="md">
          <ChangesSummary
            changes={review.data.changes}
            onOpenChanges={onOpenChanges}
          />
          <ApprovalSummary approvals={review.pending} />
          <CommentSummary comments={review.data.comments} />
          <ReviewVerdictForm
            verdict={review.verdict}
            note={review.note}
            busy={review.busy}
            canSubmit={Boolean(threadId)}
            onVerdictChange={review.setVerdict}
            onNoteChange={review.setNote}
            onCancel={onClose}
            onSubmit={() => void review.submit()}
          />
        </Stack>
      ) : (
        <Text size="sm" c="dimmed">
          Open a thread to review it
        </Text>
      )}
    </Modal>
  );
}
