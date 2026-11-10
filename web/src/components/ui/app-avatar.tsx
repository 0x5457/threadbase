import { cn } from "@/lib/utils";
import { initials } from "@/lib/format";
import {
  Avatar as AvatarRoot,
  AvatarFallback,
  AvatarImage,
} from "./avatar";

const AVATAR_COLORS = [
  "#e08b53",
  "#d46a8e",
  "#8fbd5d",
  "#9a83e8",
  "#d7b34b",
  "#df7566",
  "#6fa8dd",
  "#8a949d",
];

function colorFor(seed: string): string {
  let hash = 0;
  for (let index = 0; index < seed.length; index += 1) {
    hash = (hash * 31 + seed.charCodeAt(index)) | 0;
  }
  return AVATAR_COLORS[Math.abs(hash) % AVATAR_COLORS.length];
}

export interface AvatarPerson {
  id: string;
  displayName: string;
  avatarUrl?: string | null;
  color?: string | null;
  online?: boolean;
}

export interface AvatarProps {
  person: AvatarPerson;
  size?: "xs" | "sm" | "md" | "lg";
  className?: string;
  online?: boolean;
}

const sizeClasses = {
  xs: "size-4 text-[8px] [&_[data-slot=avatar-fallback]]:text-[8px]",
  sm: "size-5 text-[9px] [&_[data-slot=avatar-fallback]]:text-[9px]",
  md: "size-6 text-[10px] [&_[data-slot=avatar-fallback]]:text-[10px]",
  lg: "size-8 text-[11px] [&_[data-slot=avatar-fallback]]:text-[11px]",
} as const;

export function Avatar({ person, size = "md", className, online }: AvatarProps) {
  const fallbackColor = person.color || colorFor(person.id || person.displayName);
  return (
    <span className={cn("relative inline-flex shrink-0", className)}>
      <AvatarRoot
        className={cn(
          "rounded-full border border-black/20 ring-1 ring-black/10",
          sizeClasses[size],
        )}
        title={person.displayName}
      >
        {person.avatarUrl ? (
          <AvatarImage src={person.avatarUrl} alt={person.displayName} />
        ) : null}
        <AvatarFallback
          className="font-semibold text-white"
          style={{ backgroundColor: fallbackColor }}
        >
          {initials(person.displayName)}
        </AvatarFallback>
      </AvatarRoot>
      {online ? (
        <span
          aria-label="Online"
          className="absolute -bottom-px -right-px size-1.5 rounded-full border border-panel bg-success"
        />
      ) : null}
    </span>
  );
}

export interface AvatarStackProps {
  people: AvatarPerson[];
  max?: number;
  size?: AvatarProps["size"];
  className?: string;
  onSelect?: (person: AvatarPerson) => void;
}

export function AvatarStack({
  people,
  max = 3,
  size = "sm",
  className,
  onSelect,
}: AvatarStackProps) {
  const visible = people.slice(0, max);
  const hidden = people.slice(max);
  if (people.length === 0) {
    return null;
  }
  return (
    <span className={cn("inline-flex items-center", className)}>
      <span className="flex -space-x-1.5">
        {visible.map((person) => (
          <button
            key={person.id}
            type="button"
            className="rounded-full focus-visible:outline-offset-2"
            title={onSelect ? `Follow ${person.displayName}` : person.displayName}
            onClick={() => onSelect?.(person)}
            disabled={!onSelect}
          >
            <Avatar
              person={person}
              size={size}
              online={person.online}
              className="rounded-full ring-2 ring-panel"
            />
          </button>
        ))}
      </span>
      {hidden.length > 0 ? (
        <span
          className="ml-1 text-[10px] text-faint"
          title={hidden.map((person) => person.displayName).join(", ")}
        >
          +{hidden.length}
        </span>
      ) : null}
    </span>
  );
}
