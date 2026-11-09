import { forwardRef, type InputHTMLAttributes } from "react";
import { cn } from "@/lib/utils";

export interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  invalid?: boolean;
}

export const Input = forwardRef<HTMLInputElement, InputProps>(
  ({ className, invalid, type = "text", ...props }, ref) => (
    <input
      ref={ref}
      type={type}
      aria-invalid={invalid || undefined}
      className={cn(
        "h-8 w-full rounded-md border border-border bg-canvas px-2.5 text-[12.5px] text-fg shadow-[inset_0_1px_0_rgb(0_0_0/0.06)] transition-colors placeholder:text-faint hover:border-border-strong focus:border-accent/70 focus:outline-none disabled:cursor-not-allowed disabled:opacity-60",
        invalid && "border-danger/60 focus:border-danger",
        className,
      )}
      {...props}
    />
  ),
);
Input.displayName = "Input";
