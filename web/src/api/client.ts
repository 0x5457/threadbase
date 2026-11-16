/** Same-origin JSON client shared by every endpoint module. */

interface ErrorBody {
  error?: { code?: string; message?: string };
}

export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly code: string | null,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

async function failure(response: Response): Promise<never> {
  const body = (await response.json().catch(() => null)) as ErrorBody | null;
  throw new ApiError(
    body?.error?.message ?? `Request failed (${response.status})`,
    response.status,
    body?.error?.code ?? null,
  );
}

export async function request<T>(
  path: string,
  init?: RequestInit,
): Promise<T> {
  const response = await fetch(path, {
    credentials: "same-origin",
    ...init,
    headers: {
      "x-requested-with": "threadbase",
      ...(init?.body ? { "content-type": "application/json" } : {}),
      ...init?.headers,
    },
  });

  if (!response.ok) return failure(response);

  return (await response.json()) as T;
}

/** POST raw bytes (an attachment) and parse the JSON reply. */
export async function binaryRequest<T>(
  path: string,
  body: Blob,
  headers?: Record<string, string>,
): Promise<T> {
  const response = await fetch(path, {
    method: "POST",
    credentials: "same-origin",
    headers: {
      "x-requested-with": "threadbase",
      "content-type": body.type || "application/octet-stream",
      ...headers,
    },
    body,
  });

  if (!response.ok) return failure(response);

  return (await response.json()) as T;
}

/** POST/PATCH/DELETE with a JSON body. */
export function jsonRequest<T>(
  path: string,
  method: "POST" | "PATCH" | "PUT" | "DELETE",
  body?: unknown,
): Promise<T> {
  return request<T>(path, {
    method,
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}
