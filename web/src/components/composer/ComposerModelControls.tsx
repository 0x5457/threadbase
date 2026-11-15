import { Text } from "@mantine/core";
import { ModelPicker, type ModelChoice } from "../ModelPicker";
import type { ComposerDelivery } from "./useComposerSession";

interface ComposerModelControlsProps {
  modelLabel: string;
  effectiveModel: string | null;
  modelOptions: ModelChoice[];
  effectiveEffort: string | null;
  effortOptions: ModelChoice[];
  delivery: ComposerDelivery;
  onDeliveryChange: (delivery: ComposerDelivery) => void;
  loading: boolean;
  writable: boolean;
  onModelChange?: (model: string) => void;
  onEffortChange?: (effort: string) => void;
  onOpen: () => void;
}

/** Agents prefix effort names with the setting ("Thinking: high"); the row is
 *  already labelled Effort, so the prefix is noise. */
function stripEffortPrefix(value: string) {
  return value.replace(/^(thinking|reasoning|effort|mode)\s*:\s*/i, "");
}

/** One "agent settings" surface: the model picker owns effort and delivery in
 *  its footer, so the composer has one control instead of three. */
export function ComposerModelControls({
  modelLabel,
  effectiveModel,
  modelOptions,
  effectiveEffort,
  effortOptions,
  delivery,
  onDeliveryChange,
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

  const efforts = effortOptions.map((option) => ({
    value: option.value,
    name: stripEffortPrefix(option.name?.trim() || option.value),
  }));
  // The thread can carry an effort the agent no longer advertises (or the
  // config has not loaded yet); keep the current value visible either way.
  if (
    effectiveEffort &&
    !efforts.some((option) => option.value === effectiveEffort)
  ) {
    efforts.unshift({
      value: effectiveEffort,
      name: stripEffortPrefix(effectiveEffort),
    });
  }
  const showEffort = Boolean(onEffortChange) && efforts.length > 0;

  const footer = (
    <>
      {showEffort ? (
        <div className="model-picker-setting">
          <Text size="xs" c="dimmed" className="model-picker-setting-label">
            Effort
          </Text>
          <div className="model-picker-chips">
            {efforts.map((option) => (
              <button
                key={option.value}
                type="button"
                className="model-picker-chip"
                data-active={
                  effectiveEffort === option.value ? "true" : undefined
                }
                onClick={() => onEffortChange?.(option.value)}
              >
                {option.name}
              </button>
            ))}
          </div>
        </div>
      ) : null}
      <div className="model-picker-setting">
        <Text size="xs" c="dimmed" className="model-picker-setting-label">
          Delivery
        </Text>
        <div className="model-picker-chips">
          <button
            type="button"
            className="model-picker-chip"
            data-active={delivery === "auto" ? "true" : undefined}
            title="Send immediately"
            onClick={() => onDeliveryChange("auto")}
          >
            Send now
          </button>
          <button
            type="button"
            className="model-picker-chip"
            data-active={delivery === "queue" ? "true" : undefined}
            title="Queue input for the next turn"
            onClick={() => onDeliveryChange("queue")}
          >
            Queue
          </button>
        </div>
      </div>
    </>
  );

  return (
    <ModelPicker
      value={effectiveModel}
      options={modelOptions}
      loading={loading}
      disabled={!writable}
      onChange={onModelChange}
      onOpen={onOpen}
      triggerLabel={`Agent settings: ${modelLabel}`}
      triggerTitle="Model, effort and delivery"
      badge={effectiveEffort}
      footer={footer}
      queue={delivery === "queue"}
    />
  );
}
