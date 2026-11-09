export interface NewThreadOptions {
  agent?: string | null;
  model?: string | null;
  baseBranch?: string | null;
}

export interface WorkspaceOutletContext {
  openNewProject: () => void;
  openNewThread: (projectId: string, options?: NewThreadOptions) => void;
}
