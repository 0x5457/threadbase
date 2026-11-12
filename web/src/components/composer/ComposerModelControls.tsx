import { Text } from "@mantine/core";
import { ModelPicker, type ModelChoice } from "../ModelPicker";

interface ComposerModelControlsProps {
  modelLabel: string;
  effectiveModel: string | null;
  modelOptions: ModelChoice[];
  effectiveEffort: string | null;
  effortOptions: ModelChoice[];
  loading: boolean;
  writable: boolean;
  onModelChange?: (model: string) => void;
  onEffortChange?: (effort: string) => void;
  onOpen: () => void;
}

/** Model/effort pickers when the agent config is writable, label otherwise. */
export function ComposerModelControls({
  modelLabel,
  effectiveModel,
  modelOptions,
  effectiveEffort,
  effortOptions,
  loading,
  writable,
  onModelChange,
  onEffortChange,
  onOpen,
}: ComposerModelControlsProps) {
  if (!onModelChange) {
    return (
      <Text size="sm" c="dimmed" visibleFrom="sm" mr={4}>
        {modelLabel}
      </Text>
    );
  }

  return (
    <>
      <ModelPicker
        value={effectiveModel}
        options={modelOptions}
        loading={loading}
        disabled={!writable}
        onChange={onModelChange}
        onOpen={onOpen}
      />
      {onEffortChange ? (
        <ModelPicker
          label="Effort"
          placeholder="Default effort"
          value={effectiveEffort}
          options={effortOptions}
          loading={loading}
          disabled={!writable}
          onChange={onEffortChange}
          onOpen={onOpen}
        />
      ) : null}
    </>
  );
}
