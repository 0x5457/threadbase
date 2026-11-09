/* eslint-disable react-refresh/only-export-components */
import { cva, type VariantProps } from "class-variance-authority";
import type { HTMLAttributes } from "react";
import { cn } from "@/lib/utils";

export const badgeVariants = cva(
  "inline-flex items-center gap-1 rounded-full border font-medium leading-none [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        neutral: "border-border bg-raised text-muted",
        outline: "border-border-strong bg-transparent text-muted",
        accent: "border-accent/25 bg-accent-soft text-accent",
        success: "border-success/25 bg-success/10 text-success",
        warning: "border-warning/30 bg-warning/10 text-warning",
        danger: "border-danger/30 bg-danger/10 text-danger",
        info: "border-info/30 bg-info/10 text-info",
      },
      size: {
        sm: "h-4 px-1.5 text-[10px] [&_svg]:size-2.5",
        md: "h-5 px-2 text-[10.5px] [&_svg]:size-3",
      },
    },
    defaultVariants: {
      variant: "neutral",
      size: "md",
    },
  },
);

export interface BadgeProps
  extends HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof badgeVariants> {}

export function Badge({ className, variant, size, ...props }: BadgeProps) {
  return (
    <span
      className={cn(badgeVariants({ variant, size }), className)}
      {...props}
    />
  );
}
