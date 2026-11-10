import { Link } from "react-router-dom";
import { ArrowLeft, FileQuestion } from "lucide-react";
import { buttonVariants, EmptyState } from "@/components/ui";
import { cn } from "@/lib/utils";

export function NotFoundPage() {
  return (
    <div className="flex h-full items-center justify-center p-6">
      <div className="w-full max-w-md">
        <EmptyState
          icon={FileQuestion}
          title="Page not found"
          description="This route does not exist in the workspace."
          action={
            <Link
              to="/"
              className={cn(buttonVariants({ variant: "secondary", size: "sm" }))}
            >
              <ArrowLeft aria-hidden />
              Back to projects
            </Link>
          }
        />
      </div>
    </div>
  );
}
