import { Modal, Stack, Text } from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { ApprovalSummary } from "./review/ApprovalSummary";
import { ChangesSummary } from "./review/ChangesSummary";
import { CommentSummary } from "./review/CommentSummary";
import { ReviewVerdictForm } from "./review/ReviewVerdictForm";
import { useReviewDialog } from "./review/useReviewDialog";
import { useDelayedFlag } from "../lib/use-delayed-flag";
import { LoadError } from "./loading/LoadError";
import { ReviewSkeleton } from "./loading/Skeleton";

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
  const showSkeleton = useDelayedFlag(
    review.bundle.isPending && Boolean(threadId),
    300,
    500,
  );

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
        showSkeleton ? (
          <ReviewSkeleton />
        ) : null
      ) : review.bundle.isError ? (
        <LoadError
          title="Cannot load thread"
          message={
            review.bundle.error instanceof Error
              ? review.bundle.error.message
              : undefined
          }
          onRetry={() => void review.bundle.refetch()}
        />
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
