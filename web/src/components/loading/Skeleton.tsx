import type { CSSProperties, ReactNode } from "react";

/** One placeholder shape. Widths may be percentages so rows do not look ruled. */
export function Bone({
  width = "100%",
  height = 10,
  circle = false,
  className = "",
  style,
}: {
  width?: number | string;
  height?: number | string;
  circle?: boolean;
  className?: string;
  style?: CSSProperties;
}) {
  return (
    <span
      className={`skeleton-bone${circle ? " is-circle" : ""}${
        className ? ` ${className}` : ""
      }`}
      style={{ width, height, ...style }}
      aria-hidden="true"
    />
  );
}

function SkeletonText({
  lines = 2,
  widths = ["100%", "72%", "86%"],
  height = 12,
  gap = 8,
}: {
  lines?: number;
  widths?: string[];
  height?: number;
  gap?: number;
}) {
  return (
    <span className="skeleton-text" style={{ gap }}>
      {Array.from({ length: lines }, (_, index) => (
        <Bone
          key={index}
          width={widths[index % widths.length]}
          height={height}
        />
      ))}
    </span>
  );
}

/** A busy region with one accessible label, wrapped around placeholder shapes. */
export function LoadingRegion({
  label,
  className = "",
  children,
}: {
  label: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <div
      className={`skeleton-region skeleton-band${className ? ` ${className}` : ""}`}
      role="status"
      aria-live="polite"
      aria-busy="true"
      aria-label={label}
    >
      {children}
    </div>
  );
}

const WIDTHS = [64, 78, 58, 84, 70, 88, 61, 76];

/** Generic list skeleton: inbox, sessions, files, history and switchers. */
export function ListSkeleton({
  label,
  rows = 5,
  avatar = false,
  avatarSize = 26,
  circle = false,
  lines = 2,
  meta = false,
  className = "",
  rowClassName = "",
  header,
}: {
  label: string;
  rows?: number;
  avatar?: boolean;
  avatarSize?: number;
  circle?: boolean;
  lines?: number;
  meta?: boolean;
  className?: string;
  rowClassName?: string;
  header?: ReactNode;
}) {
  return (
    <LoadingRegion label={label} className={`skeleton-list ${className}`}>
      {header}
      {Array.from({ length: rows }, (_, index) => (
        <div
          key={index}
          className={`skeleton-row${rowClassName ? ` ${rowClassName}` : ""}`}
        >
          {avatar ? (
            <Bone
              width={avatarSize}
              height={avatarSize}
              circle={circle}
              className="skeleton-row-avatar"
            />
          ) : null}
          <span className="skeleton-row-body">
            <Bone width={`${WIDTHS[index % WIDTHS.length]}%`} height={12} />
            {lines > 1 ? (
              <Bone
                width={`${WIDTHS[(index + 3) % WIDTHS.length] - 12}%`}
                height={10}
              />
            ) : null}
            {lines > 2 ? (
              <Bone
                width={`${WIDTHS[(index + 5) % WIDTHS.length] - 22}%`}
                height={10}
              />
            ) : null}
          </span>
          {meta ? <Bone width={52} height={10} /> : null}
        </div>
      ))}
    </LoadingRegion>
  );
}

/** The transcript's first paint: real gutter, row rhythm and avatar size. */
export function ThreadSkeleton() {
  return (
    <LoadingRegion
      label="Loading thread"
      className="doc timeline-doc thread-skeleton"
    >
      {[0, 1, 2, 3].map((row) => (
        <div key={row} className="block">
          <div className="block-gutter">
            <Bone width={32} height={32} circle />
          </div>
          <div className="block-body">
            <Bone width={`${28 + row * 6}%`} height={10} />
            <Bone width={`${94 - row * 5}%`} height={12} />
            <Bone
              width={`${72 + (row % 2) * 12}%`}
              height={12}
              style={{ marginBottom: row === 3 ? 0 : 14 }}
            />
          </div>
        </div>
      ))}
    </LoadingRegion>
  );
}

/** First paint before the app shell itself is known to be usable. */
export function ShellSkeleton() {
  return (
    <LoadingRegion label="Loading workspace" className="shell-skeleton">
      <div className="shell-skeleton-head">
        <Bone width={108} height={14} />
        <Bone width={160} height={14} />
      </div>
      <div className="shell-skeleton-body">
        <div className="shell-skeleton-nav">
          <Bone width="64%" height={12} />
          {[0, 1, 2, 3, 4, 5].map((row) => (
            <Bone
              key={row}
              width={`${68 + (row % 3) * 9}%`}
              height={11}
            />
          ))}
        </div>
        <div className="shell-skeleton-main">
          <Bone width={160} height={16} />
          <SkeletonText lines={2} widths={["86%", "68%"]} />
          <Bone width="100%" height={120} />
          <SkeletonText lines={3} widths={["92%", "78%", "58%"]} />
        </div>
      </div>
    </LoadingRegion>
  );
}

/** Row skeletons for the agents/people sections inside Settings. */
export function SettingsRowsSkeleton({
  label = "Loading settings",
}: {
  label?: string;
}) {
  const widths = [
    ["42%", "66%"],
    ["50%", "71%"],
    ["58%", "75%"],
  ];
  return (
    <LoadingRegion label={label} className="settings-row-skeleton">
      {widths.map(([title, detail], row) => (
        <span key={row} className="settings-row-skeleton-line">
          <Bone width={title} height={11} />
          <Bone width={detail} height={9} />
        </span>
      ))}
    </LoadingRegion>
  );
}

/** Settings cards keep their real padding and rhythm. */
export function SettingsSkeleton() {
  return (
    <LoadingRegion label="Loading settings" className="settings-view">
      <div className="settings-scroll">
        <div className="settings-head">
          <Bone width={88} height={16} />
          <Bone width={220} height={11} />
        </div>
        <div className="settings-body">
          {[0, 1, 2].map((card) => (
            <div key={card} className="skeleton-card">
              <Bone width={110} height={13} />
              <div className="skeleton-card-row">
                <span className="skeleton-card-copy">
                  <Bone width="46%" height={11} />
                  <Bone width="72%" height={10} />
                </span>
                <Bone width={86} height={24} />
              </div>
              <div className="skeleton-card-row">
                <span className="skeleton-card-copy">
                  <Bone width="52%" height={11} />
                  <Bone width="64%" height={10} />
                </span>
                <Bone width={124} height={24} />
              </div>
            </div>
          ))}
        </div>
      </div>
    </LoadingRegion>
  );
}

export function SessionsSkeleton() {
  return (
    <div className="sessions-view">
      <div className="sessions-head">
        <Bone width={92} height={16} />
        <Bone width={320} height={11} />
        <div className="sessions-tools">
          <Bone width={260} height={28} />
        </div>
      </div>
      <ListSkeleton
        label="Loading sessions"
        rows={6}
        avatar
        avatarSize={14}
        lines={2}
        className="sessions-scroll sessions-list"
      />
    </div>
  );
}

export function AsideSkeleton() {
  return (
    <LoadingRegion
      label="Loading changes"
      className="changes changes-skeleton"
    >
      <div className="changes-head changes-head-stacked">
        <div className="changes-head-main">
          <Bone width={64} height={13} />
          <Bone width={44} height={12} />
        </div>
        <div className="changes-head-meta">
          <Bone width="52%" height={10} />
        </div>
      </div>
      <ListSkeleton
        label="Loading changed files"
        rows={7}
        lines={1}
        meta
        className="changes-scroll changes-files changes-skeleton-list"
        rowClassName="skeleton-change-row"
      />
    </LoadingRegion>
  );
}

export function DiffSkeleton() {
  return (
    <LoadingRegion label="Loading diff" className="skeleton-diff">
      {[0, 1, 2, 3, 4, 5, 6].map((row) => (
        <div key={row} className="skeleton-diff-row">
          <Bone width={22} height={10} />
          <Bone
            width={`${44 + (row % 4) * 11}%`}
            height={11}
            className={row % 3 === 0 ? "skeleton-diff-add" : undefined}
          />
        </div>
      ))}
    </LoadingRegion>
  );
}

export function ReviewSkeleton() {
  return (
    <LoadingRegion label="Loading review" className="review-skeleton">
      {[0, 1, 2].map((section) => (
        <div key={section} className="skeleton-review-section">
          <Bone width={94} height={12} />
          <SkeletonText lines={2} widths={["92%", "64%"]} />
        </div>
      ))}
    </LoadingRegion>
  );
}
