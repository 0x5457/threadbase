/* eslint-disable react-refresh/only-export-components */
import { LoaderCircle } from "lucide-react";
import { forwardRef, type ComponentProps } from "react";
import { cn } from "@/lib/utils";
import {
  Button as ShadcnButton,
  buttonVariants as shadcnButtonVariants,
} from "./button";

export type ButtonVariant =
  | "primary"
  | "secondary"
  | "ghost"
  | "subtle"
  | "danger"
  | "link";
export type ButtonSize = "sm" | "md" | "lg" | "icon" | "icon-sm";

const VARIANT_MAP = {
  primary: "default",
  secondary: "outline",
  ghost: "ghost",
  subtle: "secondary",
  danger: "destructive",
  link: "link",
} as const;

const SIZE_MAP = {
  sm: "sm",
  md: "default",
  lg: "lg",
  icon: "icon",
  "icon-sm": "icon-sm",
} as const;

export interface ButtonProps
  extends Omit<ComponentProps<typeof ShadcnButton>, "variant" | "size"> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  loading?: boolean;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  (
    {
      className,
      variant = "secondary",
      size = "md",
      loading = false,
      disabled,
      children,
      ...props
    },
    ref,
  ) => (
    <ShadcnButton
      ref={ref}
      variant={VARIANT_MAP[variant]}
      size={SIZE_MAP[size]}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      className={cn("font-medium", className)}
      {...props}
    >
      {loading ? <LoaderCircle className="animate-spin" aria-hidden /> : null}
      {children}
    </ShadcnButton>
  ),
);
Button.displayName = "Button";

export const buttonVariants = shadcnButtonVariants;
