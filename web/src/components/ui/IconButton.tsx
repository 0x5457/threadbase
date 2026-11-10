import { forwardRef, type ButtonHTMLAttributes } from "react";
import { cn } from "@/lib/utils";
import { Button, type ButtonVariant } from "./app-button";

export interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  label: string;
  variant?: ButtonVariant;
  size?: "sm" | "md";
  pressed?: boolean;
}

export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(
  (
    {
      className,
      label,
      variant = "ghost",
      size = "md",
      pressed,
      children,
      type = "button",
      ...props
    },
    ref,
  ) => (
    <Button
      ref={ref}
      type={type}
      aria-label={label}
      title={label}
      aria-pressed={pressed}
      variant={variant}
      size={size === "sm" ? "icon-sm" : "icon"}
      className={cn("p-0", pressed && "bg-active text-fg", className)}
      {...props}
    >
      {children}
    </Button>
  ),
);
IconButton.displayName = "IconButton";
