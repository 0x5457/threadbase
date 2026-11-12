import { Drawer, Text } from "@mantine/core";
import { useEffect, useState, type ReactNode } from "react";
import { useOverlayBack } from "../lib/overlay-back";
import { SheetGrabber } from "./SheetGrabber";

export interface ActionSheetAction {
  label: string;
  icon?: ReactNode;
  onSelect: () => void;
  destructive?: boolean;
}

/**
 * Bottom action sheet for touch context menus. The grabber, scrim, Cancel and
 * the back button are all equally valid ways out, so a gesture is never the
 * only way to dismiss it.
 */
export function ActionSheet({
  opened,
  onClose,
  title,
  subtitle,
  actions,
}: {
  opened: boolean;
  onClose: () => void;
  title?: string;
  subtitle?: string;
  actions: ActionSheetAction[];
}) {
  useOverlayBack(opened, onClose);

  // The release that ends a long press is followed by a click. Without this
  // short guard that click lands on the freshly rendered scrim and closes the
  // sheet immediately.
  const [allowOutside, setAllowOutside] = useState(false);
  useEffect(() => {
    if (!opened) {
      setAllowOutside(false);
      return;
    }
    const timer = window.setTimeout(() => setAllowOutside(true), 320);
    return () => window.clearTimeout(timer);
  }, [opened]);

  return (
    <Drawer
      opened={opened}
      onClose={onClose}
      closeOnClickOutside={allowOutside}
      position="bottom"
      size="auto"
      padding={0}
      withCloseButton={false}
      className="action-sheet"
    >
      <SheetGrabber onClose={onClose} />
      <div className="action-sheet-scroll">
        {title || subtitle ? (
          <div className="action-sheet-head">
            {title ? (
              <Text size="sm" fw={500} className="action-sheet-title">
                {title}
              </Text>
            ) : null}
            {subtitle ? (
              <Text size="xs" c="dimmed" className="action-sheet-subtitle">
                {subtitle}
              </Text>
            ) : null}
          </div>
        ) : null}
        <div className="action-sheet-list" role="menu">
          {actions.map((action) => (
            <button
              key={action.label}
              type="button"
              role="menuitem"
              className={`action-sheet-item${action.destructive ? " destructive" : ""}`}
              onClick={() => {
                onClose();
                action.onSelect();
              }}
            >
              {action.icon ? (
                <span className="action-sheet-icon">{action.icon}</span>
              ) : null}
              <span>{action.label}</span>
            </button>
          ))}
        </div>
      </div>
      <button
        type="button"
        className="action-sheet-cancel"
        onClick={onClose}
      >
        Cancel
      </button>
    </Drawer>
  );
}
