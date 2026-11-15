import { ActionIcon, Box, Group, Kbd, Text, Tooltip } from "@mantine/core";
import {
  IconArrowUp,
  IconAt,
  IconCommand,
  IconEye,
  IconTerminal2,
  IconMessagePlus,
} from "@tabler/icons-react";
import type { ModelChoice } from "../ModelPicker";
import type { DraftPhase } from "../../api/types/DraftPhase";
import type { Usage } from "../../api/types/Usage";
import { ComposerModelControls } from "./ComposerModelControls";
import { ContextMeter } from "./ContextMeter";
import type { ComposerDelivery } from "./useComposerSession";

interface ComposerToolbarProps {
  usage?: Usage | null;
  watching?: number;
  terminalUsers?: string[];
  draftActive: boolean;
  draftPhase: DraftPhase;
  liveLabel: string;
  onThreadComment?: () => void;
  onOpenTrigger: (kind: "command" | "file") => void;
  delivery: ComposerDelivery;
  onDeliveryChange: (delivery: ComposerDelivery) => void;
  modelLabel: string;
  effectiveModel: string | null;
  modelOptions: ModelChoice[];
  effectiveEffort: string | null;
  effortOptions: ModelChoice[];
  configLoading: boolean;
  writable: boolean;
  onModelChange?: (model: string) => void;
  onEffortChange?: (effort: string) => void;
  onConfigOpen: () => void;
  hasText: boolean;
  sending: boolean;
  sendBlocked: boolean;
  sendLabel: string;
  sendWithModifier: boolean;
  onSend: () => void;
}

export function ComposerToolbar({
  usage,
  watching,
  terminalUsers,
  draftActive,
  draftPhase,
  liveLabel,
  onThreadComment,
  onOpenTrigger,
  delivery,
  onDeliveryChange,
  modelLabel,
  effectiveModel,
  modelOptions,
  effectiveEffort,
  effortOptions,
  configLoading,
  writable,
  onModelChange,
  onEffortChange,
  onConfigOpen,
  hasText,
  sending,
  sendBlocked,
  sendLabel,
  sendWithModifier,
  onSend,
}: ComposerToolbarProps) {
  return (
    <div className="composer-tools">
      {onThreadComment ? (
        <Tooltip label="Comment on thread">
          <ActionIcon
            aria-label="Comment on thread"
            size={22}
            onClick={onThreadComment}
          >
            <IconMessagePlus size={14} stroke={1.5} />
          </ActionIcon>
        </Tooltip>
      ) : null}
      <Tooltip label="Reference a file">
        <ActionIcon
          aria-label="Reference a file"
          size={22}
          onClick={() => onOpenTrigger("file")}
        >
          <IconAt size={14} stroke={1.5} />
        </ActionIcon>
      </Tooltip>
      <Tooltip label="Slash commands and skills">
        <ActionIcon
          aria-label="Slash commands and skills"
          size={22}
          onClick={() => onOpenTrigger("command")}
        >
          <IconCommand size={14} stroke={1.5} />
        </ActionIcon>
      </Tooltip>
      <div className="spacer" />
      <ContextMeter usage={usage ?? null} />
      {sendWithModifier ? (
        <Kbd visibleFrom="sm" mr={4}>
          ⌘↵
        </Kbd>
      ) : null}
      {terminalUsers?.length ? (
        <Tooltip
          label={`${terminalUsers.join(", ")} in the terminal`}
          openDelay={200}
        >
          <Group gap={3} className="watching-label" wrap="nowrap">
            <IconTerminal2 size={12} stroke={1.5} />
            <Text size="xs" c="dimmed">
              {terminalUsers.length}
            </Text>
          </Group>
        </Tooltip>
      ) : null}
      {watching ? (
        <Tooltip
          label={`${watching} ${watching === 1 ? "person" : "people"} watching`}
        >
          <Group gap={3} className="watching-label" wrap="nowrap">
            <IconEye size={12} stroke={1.5} />
            <Text size="xs" c="dimmed">
              {watching}
            </Text>
          </Group>
        </Tooltip>
      ) : null}
      {draftActive ? (
        <Tooltip label={liveLabel}>
          <span
            className={`live-dot ${
              draftPhase === "live"
                ? "online"
                : draftPhase === "syncing"
                  ? "syncing"
                  : ""
            }`}
          />
        </Tooltip>
      ) : null}
      <ComposerModelControls
        modelLabel={modelLabel}
        effectiveModel={effectiveModel}
        modelOptions={modelOptions}
        effectiveEffort={effectiveEffort}
        effortOptions={effortOptions}
        delivery={delivery}
        onDeliveryChange={onDeliveryChange}
        loading={configLoading}
        writable={writable}
        onModelChange={onModelChange}
        onEffortChange={onEffortChange}
        onOpen={onConfigOpen}
      />
      <Tooltip label={sendLabel}>
        <Box component="span" style={{ display: "inline-flex" }}>
          <ActionIcon
            aria-label="Send"
            variant="filled"
            color="dark"
            size={26}
            radius="xl"
            disabled={!hasText || sending || sendBlocked || !writable}
            loading={sending}
            onClick={onSend}
            className="send-btn"
          >
            <IconArrowUp size={14} stroke={2} />
          </ActionIcon>
        </Box>
      </Tooltip>
    </div>
  );
}
