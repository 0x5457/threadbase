import { Modal, useMantineTheme, type ModalProps } from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";

/**
 * Mantine modal that becomes a full-screen sheet on phones. Breakpoints come
 * from the Mantine theme, so there is no second breakpoint table to drift.
 */
export function ResponsiveModal({ fullScreen, ...props }: ModalProps) {
  const theme = useMantineTheme();
  const isPhone = useMediaQuery(
    `(max-width: calc(${theme.breakpoints.sm} - 0.0625em))`,
  );
  return <Modal fullScreen={fullScreen ?? isPhone} {...props} />;
}
