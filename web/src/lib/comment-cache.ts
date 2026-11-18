import type { QueryClient } from "@tanstack/react-query";
import type { Comment } from "../api";

type BundleData = {
  bundle: {
    comments: Comment[];
    [key: string]: unknown;
  };
};

function updateComments(
  queryClient: QueryClient,
  threadId: string,
  update: (comments: Comment[]) => Comment[],
) {
  for (const [key] of queryClient.getQueriesData<BundleData>({
    queryKey: ["thread", threadId],
  })) {
    if (key[2] !== undefined && key[2] !== "meta") continue;
    queryClient.setQueryData<BundleData>(key, (data) => {
      if (!data) return data;
      return {
        ...data,
        bundle: {
          ...data.bundle,
          comments: update(data.bundle.comments),
        },
      };
    });
  }
}

export function insertOptimisticComment(
  queryClient: QueryClient,
  threadId: string,
  comment: Comment,
) {
  updateComments(queryClient, threadId, (comments) => [...comments, comment]);
}

export function replaceOptimisticComment(
  queryClient: QueryClient,
  threadId: string,
  tempId: string,
  comment: Comment,
) {
  updateComments(queryClient, threadId, (comments) =>
    comments.map((entry) => (entry.id === tempId ? comment : entry)),
  );
}

export function removeOptimisticComment(
  queryClient: QueryClient,
  threadId: string,
  tempId: string,
) {
  updateComments(queryClient, threadId, (comments) =>
    comments.filter((entry) => entry.id !== tempId),
  );
}
