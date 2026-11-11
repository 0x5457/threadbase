import { createBrowserRouter } from "react-router-dom";
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

/**
 * Data router: Settings needs `useBlocker` to protect unsaved changes on
 * in-app navigation, which is only available from a data router.
 */
export const router = createBrowserRouter([
  { path: "/login", element: <LoginPage /> },
  {
    element: <RequireAuth />,
    children: [
      {
        element: <WorkspaceLayout />,
        children: [
          { index: true, element: <WorkspacePage /> },
          { path: "p/:projectId", element: <ProjectPage /> },
          { path: "p/:projectId/t/:threadId", element: <ThreadPage /> },
          {
            path: "p/:projectId/t/:threadId/s/:subagentId",
            element: <ThreadPage />,
          },
          { path: "p/:projectId/compare", element: <ComparePage /> },
          {
            path: "p/:projectId/t/:threadId/subagents/compare",
            element: <SubagentComparePage />,
          },
          { path: "inbox", element: <InboxPage /> },
          { path: "invite/:token", element: <InvitePage /> },
          { path: "settings", element: <SettingsPage /> },
          { path: "*", element: <NotFoundPage /> },
        ],
      },
    ],
  },
]);

export default router;
