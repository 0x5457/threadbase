import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export function Kbd({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <kbd
      className={cn(
        "inline-flex h-[18px] min-w-[18px] items-center justify-center rounded border border-border bg-raised px-1 font-sans text-[10px] font-medium text-faint",
        className,
      )}
    >
      {children}
    </kbd>
  );
}

export function KbdGroup({ children }: { children: ReactNode }) {
  return <span className="inline-flex items-center gap-0.5">{children}</span>;
}
