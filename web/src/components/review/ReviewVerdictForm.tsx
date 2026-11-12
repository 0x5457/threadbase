import {
  Button,
  Group,
  SegmentedControl,
  Stack,
  Textarea,
} from "@mantine/core";
import { SectionHead } from "./SectionHead";

export type ReviewVerdict = "approve" | "request_changes";

interface ReviewVerdictFormProps {
  verdict: ReviewVerdict;
  note: string;
  busy: boolean;
  canSubmit: boolean;
  onVerdictChange: (verdict: ReviewVerdict) => void;
  onNoteChange: (note: string) => void;
  onCancel: () => void;
  onSubmit: () => void;
}

export function ReviewVerdictForm({
  verdict,
  note,
  busy,
  canSubmit,
  onVerdictChange,
  onNoteChange,
  onCancel,
  onSubmit,
}: ReviewVerdictFormProps) {
  return (
    <div>
      <SectionHead label="Verdict" icon={null} />
      <Stack gap="xs" p={6}>
        <SegmentedControl
          size="xs"
          fullWidth
          aria-label="Review verdict"
          value={verdict}
          onChange={(value) => onVerdictChange(value as ReviewVerdict)}
          data={[
            { label: "Approve", value: "approve" },
            { label: "Request changes", value: "request_changes" },
          ]}
        />
        <Textarea
          label="Review note"
          description="Recorded as a thread comment for the agent."
          autosize
          minRows={2}
          maxRows={6}
          value={note}
          onChange={(event) => onNoteChange(event.currentTarget.value)}
        />
        <Group justify="flex-end">
          <Button variant="default" onClick={onCancel} disabled={busy}>
            Cancel
          </Button>
          <Button
            color="dark"
            loading={busy}
            disabled={!canSubmit || busy}
            onClick={onSubmit}
          >
            Submit review
          </Button>
        </Group>
      </Stack>
    </div>
  );
}
