import { Route, Routes } from "react-router-dom";
import { Toaster } from "sonner";
import { TooltipProvider } from "@/components/ui";
import { useTheme } from "@/lib/theme";
import { ComparePage } from "./routes/ComparePage";
import { InboxPage } from "./routes/InboxPage";
import { InvitePage } from "./routes/InvitePage";
import { LoginPage } from "./routes/LoginPage";
import { NotFoundPage } from "./routes/NotFoundPage";
import { ProjectPage } from "./routes/ProjectPage";
import { RequireAuth } from "./routes/RequireAuth";
import { SettingsPage } from "./routes/SettingsPage";
import { SubagentComparePage } from "./routes/SubagentComparePage";
import { ThreadPage } from "./routes/ThreadPage";
import { WorkspaceLayout } from "./routes/WorkspaceLayout";
import { WorkspacePage } from "./routes/WorkspacePage";

export default function App() {
  const theme = useTheme();

  return (
    <TooltipProvider>
      <Routes>
        <Route path="/login" element={<LoginPage />} />
        <Route element={<RequireAuth />}>
          <Route element={<WorkspaceLayout />}>
            <Route index element={<WorkspacePage />} />
            <Route path="p/:projectId" element={<ProjectPage />} />
            <Route
              path="p/:projectId/t/:threadId"
              element={<ThreadPage />}
            />
            <Route
              path="p/:projectId/t/:threadId/s/:subagentId"
              element={<ThreadPage />}
            />
            <Route path="p/:projectId/compare" element={<ComparePage />} />
            <Route
              path="p/:projectId/t/:threadId/subagents/compare"
              element={<SubagentComparePage />}
            />
            <Route path="inbox" element={<InboxPage />} />
            <Route path="invite/:token" element={<InvitePage />} />
            <Route path="settings" element={<SettingsPage />} />
            <Route path="*" element={<NotFoundPage />} />
          </Route>
        </Route>
      </Routes>
      <Toaster
        theme={theme}
        position="bottom-right"
        toastOptions={{
          style: {
            background: "var(--panel)",
            border: "1px solid var(--border)",
            color: "var(--fg)",
          },
        }}
      />
    </TooltipProvider>
  );
}
