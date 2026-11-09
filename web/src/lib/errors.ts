import { ApiError } from "./api";

export interface ErrorDescription {
  title: string;
  description: string;
  retryable: boolean;
}

export function describeError(
  error: unknown,
  fallbackTitle = "Something went wrong",
): ErrorDescription {
  if (error instanceof ApiError) {
    if (error.status === 401) {
      return {
        title: "Sign in required",
        description: "Your session expired. Sign in again to continue.",
        retryable: false,
      };
    }
    if (error.status === 403) {
      return {
        title: "Access denied",
        description:
          error.message || "You do not have permission to view this resource.",
        retryable: false,
      };
    }
    if (error.status === 404) {
      return {
        title: "Not found",
        description:
          error.message || "The requested resource is not available yet.",
        retryable: false,
      };
    }
    if (error.status === 429) {
      return {
        title: "Too many requests",
        description: error.message || "Wait a moment and try again.",
        retryable: true,
      };
    }
    return {
      title: fallbackTitle,
      description: error.message || `Request failed with status ${error.status}.`,
      retryable: error.status >= 500,
    };
  }
  if (error instanceof TypeError) {
    return {
      title: "Cannot reach Threadbase",
      description:
        "The local service did not respond. Check that the Threadbase server is running.",
      retryable: true,
    };
  }
  if (error instanceof Error) {
    return {
      title: fallbackTitle,
      description: error.message,
      retryable: true,
    };
  }
  return {
    title: fallbackTitle,
    description: "An unexpected error occurred.",
    retryable: true,
  };
}
