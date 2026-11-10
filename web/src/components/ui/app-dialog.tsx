import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import {
  Dialog as DialogRoot,
  DialogClose,
  DialogContent as ShadcnDialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "./dialog";

export const Dialog = DialogRoot;
export { DialogClose, DialogTrigger };

export interface DialogContentProps {
  title: string;
  description?: string;
  children: ReactNode;
  footer?: ReactNode;
  className?: string;
  size?: "sm" | "md" | "lg";
}

const dialogSizes = {
  sm: "sm:max-w-sm",
  md: "sm:max-w-md",
  lg: "sm:max-w-xl",
} as const;

export function DialogContent({
  title,
  description,
  children,
  footer,
  className,
  size = "md",
}: DialogContentProps) {
  return (
    <ShadcnDialogContent
      className={cn("gap-0 overflow-hidden p-0", dialogSizes[size], className)}
    >
      <DialogHeader className="gap-1 border-b border-border px-4 py-3 pr-10">
        <DialogTitle className="text-[13px] font-semibold">{title}</DialogTitle>
        {description ? (
          <DialogDescription className="text-[11.5px] leading-relaxed">
            {description}
          </DialogDescription>
        ) : null}
      </DialogHeader>
      <div className="px-4 py-4">{children}</div>
      {footer ? (
        <div className="flex items-center justify-end gap-2 border-t border-border bg-muted/30 px-4 py-3">
          {footer}
        </div>
      ) : null}
    </ShadcnDialogContent>
  );
}
