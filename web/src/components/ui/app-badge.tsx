import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";
import { Badge as ShadcnBadge } from "./badge";

export type BadgeVariant =
  | "neutral"
  | "outline"
  | "accent"
  | "success"
  | "warning"
  | "danger"
  | "info";

const VARIANT_CLASS: Record<BadgeVariant, string> = {
  neutral: "border-border/80 bg-raised/70 text-muted-foreground",
  outline: "border-border-strong text-muted-foreground",
  accent: "border-primary/25 bg-primary/10 text-primary",
  success: "border-success/25 bg-success/10 text-success",
  warning: "border-warning/30 bg-warning/10 text-warning",
  danger: "border-danger/30 bg-danger/10 text-danger",
  info: "border-info/30 bg-info/10 text-info",
};

export interface BadgeProps
  extends Omit<ComponentProps<typeof ShadcnBadge>, "variant"> {
  variant?: BadgeVariant;
  size?: "sm" | "md";
}

export function Badge({
  className,
  variant = "neutral",
  size = "md",
  ...props
}: BadgeProps) {
  return (
    <ShadcnBadge
      variant="outline"
      className={cn(
        "gap-1 font-medium leading-none [&_svg]:shrink-0",
        size === "sm"
          ? "h-4 px-1.5 text-[10px] [&_svg]:size-2.5"
          : "h-5 px-2 text-[10.5px] [&_svg]:size-3",
        VARIANT_CLASS[variant],
        className,
      )}
      {...props}
    />
  );
}
