import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import {
  Tooltip as TooltipRoot,
  TooltipContent,
  TooltipProvider as ShadcnTooltipProvider,
  TooltipTrigger,
} from "./tooltip";

export function TooltipProvider({ children }: { children: ReactNode }) {
  return (
    <ShadcnTooltipProvider delayDuration={300} skipDelayDuration={100}>
      {children}
    </ShadcnTooltipProvider>
  );
}

export interface TooltipProps {
  content: ReactNode;
  children: ReactNode;
  side?: "top" | "right" | "bottom" | "left";
  align?: "start" | "center" | "end";
  className?: string;
}

export function Tooltip({
  content,
  children,
  side = "top",
  align = "center",
  className,
}: TooltipProps) {
  return (
    <TooltipRoot>
      <TooltipTrigger asChild>{children}</TooltipTrigger>
      <TooltipContent
        side={side}
        align={align}
        sideOffset={6}
        className={cn(
          "max-w-64 rounded-md px-2 py-1 text-[11px] leading-snug",
          className,
        )}
      >
        {content}
      </TooltipContent>
    </TooltipRoot>
  );
}
