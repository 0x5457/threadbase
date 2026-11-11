/** Status dot: the single status indicator used by every tree row. */
import { statusColor, statusLabel, type AnyStatus } from "./status";

export function StatusDot({
  status,
  pulse,
}: {
  status: AnyStatus;
  pulse?: boolean;
}) {
  return (
    <span
      title={statusLabel(status)}
      style={{
        display: "inline-block",
        width: 6,
        height: 6,
        flexShrink: 0,
        borderRadius: "50%",
        backgroundColor: statusColor(status),
        ...(pulse || status === "running" || status === "preparing"
          ? { animation: "sidebar-pulse 1.6s ease-in-out infinite" }
          : {}),
      }}
    />
  );
}
