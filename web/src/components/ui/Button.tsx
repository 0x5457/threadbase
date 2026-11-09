import { cva, type VariantProps } from "class-variance-authority";
import { forwardRef, type ButtonHTMLAttributes } from "react";
import { LoaderCircle } from "lucide-react";
import { cn } from "@/lib/utils";

export const buttonVariants = cva(
  "inline-flex shrink-0 select-none items-center justify-center gap-1.5 whitespace-nowrap rounded-md font-medium transition-colors disabled:pointer-events-none disabled:opacity-50 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        primary:
          "bg-accent text-canvas shadow-[inset_0_1px_0_rgb(255_255_255/0.14)] hover:bg-accent/90",
        secondary:
          "border border-border bg-surface text-fg hover:border-border-strong hover:bg-hover",
        ghost: "text-muted hover:bg-hover hover:text-fg",
        subtle: "bg-raised text-fg hover:bg-active",
        danger:
          "border border-danger/30 bg-danger/10 text-danger hover:bg-danger/20",
        link: "h-auto rounded-none p-0 text-accent underline-offset-4 hover:underline",
      },
      size: {
        sm: "h-7 px-2 text-[12px] [&_svg]:size-3.5",
        md: "h-8 px-3 text-[12.5px] [&_svg]:size-4",
        lg: "h-9 px-4 text-[13px] [&_svg]:size-4",
        icon: "size-8 [&_svg]:size-4",
        "icon-sm": "size-7 [&_svg]:size-3.5",
      },
    },
    defaultVariants: {
      variant: "secondary",
      size: "md",
    },
  },
);

export type ButtonVariant = NonNullable<
  VariantProps<typeof buttonVariants>["variant"]
>;
export type ButtonSize = NonNullable<
  VariantProps<typeof buttonVariants>["size"]
>;

export interface ButtonProps
  extends ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  loading?: boolean;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  (
    { className, variant, size, loading = false, disabled, children, ...props },
    ref,
  ) => (
    <button
      ref={ref}
      className={cn(buttonVariants({ variant, size }), className)}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      {...props}
    >
      {loading ? <LoaderCircle className="animate-spin" aria-hidden /> : null}
      {children}
    </button>
  ),
);
Button.displayName = "Button";
