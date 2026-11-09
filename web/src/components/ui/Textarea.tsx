import { forwardRef, type TextareaHTMLAttributes } from "react";
import { cn } from "@/lib/utils";

export interface TextareaProps
  extends TextareaHTMLAttributes<HTMLTextAreaElement> {
  invalid?: boolean;
}

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaProps>(
  ({ className, invalid, rows = 3, ...props }, ref) => (
    <textarea
      ref={ref}
      rows={rows}
      aria-invalid={invalid || undefined}
      className={cn(
        "w-full resize-y rounded-md border border-border bg-canvas px-2.5 py-2 text-[12.5px] leading-relaxed text-fg transition-colors placeholder:text-faint hover:border-border-strong focus:border-accent/70 focus:outline-none disabled:cursor-not-allowed disabled:opacity-60",
        invalid && "border-danger/60 focus:border-danger",
        className,
      )}
      {...props}
    />
  ),
);
Textarea.displayName = "Textarea";
