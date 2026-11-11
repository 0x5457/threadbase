import { Box } from "@mantine/core";
import type { ReactNode } from "react";

export interface PageContainerProps {
  /** Maximum content width in pixels; defaults to the project list width. */
  maxWidth?: number;
  children: ReactNode;
}

/**
 * The shared page gutter. Every route body uses the same 768px column and
 * 16px gutter as the thread timeline, so page content left edges line up.
 */
export function PageContainer({
  maxWidth = 768,
  children,
}: PageContainerProps) {
  return (
    <Box
      maw={maxWidth}
      mx="auto"
      w="100%"
      px={{ base: "md", sm: "md" }}
      pt={{ base: "md", sm: "xl" }}
      pb={{ base: "xl", sm: 64 }}
    >
      {children}
    </Box>
  );
}
