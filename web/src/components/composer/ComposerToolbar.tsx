import {
  ActionIcon,
  Box,
  Group,
  Kbd,
  Menu,
  Text,
  Tooltip,
} from "@mantine/core";
import {
  IconAdjustmentsHorizontal,
  IconArrowUp,
  IconAt,
  IconCheck,
  IconCommand,
  IconEye,
  IconMessagePlus,
} from "@tabler/icons-react";
import type { ModelChoice } from "../ModelPicker";
import { ComposerModelControls } from "./ComposerModelControls";
import type { ComposerDelivery } from "./useComposerSession";

interface ComposerToolbarProps {
  agent: string;
  watching?: number;
  draftActive: boolean;
  draftSynced: boolean;
  draftConnected: boolean;
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
  onSend: () => void;
}

export function ComposerToolbar({
  agent,
  watching,
  draftActive,
  draftSynced,
  draftConnected,
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
      <Kbd visibleFrom="sm" mr={4}>
        ⌘↵
      </Kbd>
      <Menu position="top-end" width={230} shadow="md">
        <Menu.Target>
          <ActionIcon
            variant="subtle"
            color="gray"
            size={22}
            aria-label="Effort and delivery"
          >
            <IconAdjustmentsHorizontal size={13} stroke={1.5} />
          </ActionIcon>
        </Menu.Target>
        <Menu.Dropdown>
          <Menu.Label>Agent</Menu.Label>
          <Menu.Item disabled>{agent}</Menu.Item>
          <Menu.Divider />
          <Menu.Label>Delivery</Menu.Label>
          <Menu.Item
            onClick={() => onDeliveryChange("auto")}
            rightSection={
              delivery === "auto" ? (
                <IconCheck size={12} stroke={1.5} />
              ) : null
            }
          >
            Send now
          </Menu.Item>
          <Menu.Item
            onClick={() => onDeliveryChange("queue")}
            rightSection={
              delivery === "queue" ? (
                <IconCheck size={12} stroke={1.5} />
              ) : null
            }
          >
            Queue for next turn
          </Menu.Item>
        </Menu.Dropdown>
      </Menu>
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
              draftSynced && draftConnected
                ? "online"
                : draftConnected
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
            size={24}
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
