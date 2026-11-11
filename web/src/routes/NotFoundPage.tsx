import { ArrowLeft, FileQuestion } from "lucide-react";
import { Link } from "react-router-dom";
import { Button, Stack } from "@mantine/core";
import { EmptyState } from "@/components/app";

export function NotFoundPage() {
  return (
    <Stack align="center" justify="center" h="100%" p="lg">
      <EmptyState
        icon={FileQuestion}
        title="Page not found"
        description="This route does not exist in the workspace."
        action={
          <Button
            component={Link}
            to="/"
            variant="default"
            size="xs"
            leftSection={<ArrowLeft aria-hidden />}
          >
            Back to projects
          </Button>
        }
      />
    </Stack>
  );
}
