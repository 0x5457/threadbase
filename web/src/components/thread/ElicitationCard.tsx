import { HelpCircle, ShieldAlert } from "lucide-react";
import { useState } from "react";
import type { ElicitationAction } from "@/api/types/ElicitationAction";
import type { ElicitationDto } from "@/api/types/ElicitationDto";
import { Badge, Button, Input } from "@/components/ui";

interface SchemaProperty {
  type?: string;
  title?: string;
  description?: string;
  enum?: unknown[];
  default?: unknown;
}

interface FormSchema {
  properties?: Record<string, SchemaProperty>;
  required?: string[];
}

const SECRET_MARKERS = ["password", "token", "secret", "api_key", "apikey"];

export function ElicitationCard({
  elicitation,
  resolvedAction,
  onRespond,
}: {
  elicitation: ElicitationDto;
  resolvedAction: ElicitationAction | null;
  onRespond?: (
    id: string,
    action: ElicitationAction,
    values: Record<string, unknown>,
  ) => void;
}) {
  const schema = (elicitation.requestedSchema ?? {}) as FormSchema;
  const fields = Object.entries(schema.properties ?? {});
  const authorizationUrl =
    elicitation.mode === "url"
      ? (elicitation.requestedSchema as { url?: string } | null)?.url
      : undefined;
  const [values, setValues] = useState<Record<string, unknown>>(() => {
    const initial: Record<string, unknown> = {};
    for (const [key, property] of fields) {
      if (property.default !== undefined) {
        initial[key] = property.default;
      }
    }
    return initial;
  });
  const secret = fields.some(([key]) =>
    SECRET_MARKERS.some((marker) => key.toLowerCase().includes(marker)),
  );
  const pending = elicitation.status === "pending" && resolvedAction === null;

  return (
    <div className="flex gap-3">
      <div className="w-7 shrink-0" />
      <div className="min-w-0 flex-1 overflow-hidden rounded-lg border border-info/30 bg-info/5">
        <div className="flex items-center gap-2 px-3 py-2">
          <HelpCircle className="size-3.5 shrink-0 text-info" />
          <span className="text-[11.5px] font-medium text-fg">
            Agent needs input
          </span>
          <Badge
            variant={pending ? "info" : "neutral"}
            size="sm"
            className="ml-auto"
          >
            {resolvedAction ?? elicitation.status}
          </Badge>
        </div>
        <div className="space-y-2 border-t border-info/20 px-3 py-2">
          <p className="text-[11.5px] text-fg">{elicitation.message}</p>
          {authorizationUrl ? (
            <div className="space-y-1 rounded border border-border bg-canvas px-2 py-1.5">
              <p className="break-all font-mono text-[9.5px] text-muted">
                {authorizationUrl}
              </p>
              <a
                href={authorizationUrl}
                target="_blank"
                rel="noreferrer"
                className="inline-flex text-[10.5px] text-accent hover:underline"
              >
                Open authorization page
              </a>
            </div>
          ) : null}
          <p className="text-[10px] text-faint">
            {elicitation.agent}
            {elicitation.blocking ? "" : " · non-blocking"}
          </p>
          {secret ? (
            <p className="flex items-start gap-1.5 rounded border border-warning/30 bg-warning/5 px-2 py-1.5 text-[10.5px] text-warning">
              <ShieldAlert className="mt-0.5 size-3 shrink-0" />
              This form asks for a secret. Threadbase will not submit passwords
              or tokens through elicitation.
            </p>
          ) : null}
          {pending ? (
            <>
              {fields.map(([key, property]) => (
                <label key={key} className="block space-y-1">
                  <span className="text-[10.5px] text-muted">
                    {property.title ?? key}
                  </span>
                  {property.type === "boolean" ? (
                    <input
                      type="checkbox"
                      checked={Boolean(values[key])}
                      onChange={(event) =>
                        setValues((current) => ({
                          ...current,
                          [key]: event.target.checked,
                        }))
                      }
                      className="block size-4 accent-[var(--accent)]"
                    />
                  ) : property.enum ? (
                    <select
                      value={String(values[key] ?? "")}
                      onChange={(event) =>
                        setValues((current) => ({
                          ...current,
                          [key]: event.target.value,
                        }))
                      }
                      className="h-8 w-full rounded-md border border-border bg-surface px-2 text-[11px]"
                    >
                      <option value="">Select…</option>
                      {property.enum.map((option) => (
                        <option key={String(option)} value={String(option)}>
                          {String(option)}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <Input
                      type={property.type === "number" ? "number" : "text"}
                      value={String(values[key] ?? "")}
                      onChange={(event) =>
                        setValues((current) => ({
                          ...current,
                          [key]:
                            property.type === "number"
                              ? Number(event.target.value)
                              : event.target.value,
                        }))
                      }
                      disabled={secret}
                    />
                  )}
                  {property.description ? (
                    <span className="block text-[10px] text-faint">
                      {property.description}
                    </span>
                  ) : null}
                </label>
              ))}
              <div className="flex flex-wrap gap-1.5 border-t border-info/20 pt-2">
                <Button
                  size="sm"
                  variant="primary"
                  disabled={secret || !onRespond}
                  onClick={() => onRespond?.(elicitation.id, "accept", values)}
                >
                  Submit
                </Button>
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={() => onRespond?.(elicitation.id, "decline", {})}
                >
                  Decline
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => onRespond?.(elicitation.id, "cancel", {})}
                >
                  Cancel
                </Button>
              </div>
            </>
          ) : elicitation.respondedBy ? (
            <p className="text-[10.5px] text-faint">
              Answered by {elicitation.respondedBy.displayName}
            </p>
          ) : null}
        </div>
      </div>
    </div>
  );
}
