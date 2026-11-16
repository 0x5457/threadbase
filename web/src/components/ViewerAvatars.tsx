import type { UserBrief } from "../api/types/UserBrief";
import { initials } from "../lib/initials";

/**
 * Compact, phrasing-content avatars for rows and the terminal header. It uses
 * spans rather than Mantine Avatar so it can live inside a <button> without
 * nesting interactive elements.
 */
export function ViewerAvatars({
  users,
  max = 3,
  size = 16,
  className,
}: {
  users: UserBrief[];
  max?: number;
  size?: number;
  className?: string;
}) {
  if (users.length === 0) return null;
  const shown = users.slice(0, max);
  const extra = users.length - shown.length;
  const fontSize = Math.max(8, Math.round(size * 0.42));
  return (
    <span
      className={`viewer-avatars${className ? ` ${className}` : ""}`}
      aria-label={`${users.length} viewer${users.length === 1 ? "" : "s"}`}
    >
      {shown.map((user) => (
        <span
          key={user.id}
          className="viewer-avatar"
          style={{ width: size, height: size, background: user.color, fontSize }}
          title={user.displayName || user.username}
        >
          {user.avatarUrl ? (
            <img src={user.avatarUrl} alt="" loading="lazy" />
          ) : (
            initials(user.displayName || user.username)
          )}
        </span>
      ))}
      {extra > 0 ? (
        <span
          className="viewer-avatar viewer-avatar-more"
          style={{ width: size, height: size, fontSize }}
          title={`${extra} more`}
        >
          +{extra}
        </span>
      ) : null}
    </span>
  );
}
