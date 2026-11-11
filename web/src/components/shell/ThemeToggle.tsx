import { ActionIcon, Tooltip } from "@mantine/core";
import { Moon, Sun } from "lucide-react";
import { toggleTheme, useTheme } from "@/lib/theme";

/**
 * Both icons stay mounted so the swap has an exit as well as an enter state.
 * CSS drives opacity/scale/blur; reduced motion keeps the static button state.
 */
export function ThemeToggle({
  size = "md",
  hitArea,
}: {
  size?: "sm" | "md" | "lg";
  hitArea?: "32" | "40";
}) {
  const theme = useTheme();
  const next = theme === "dark" ? "light" : "dark";
  const label = `Switch to ${next} theme`;

  return (
    <Tooltip label={label}>
      <ActionIcon
        variant="default"
        size={size}
        className={hitArea ? `tb-hit-${hitArea}` : undefined}
        aria-label={label}
        onClick={toggleTheme}
      >
        <span
          aria-hidden
          style={{
            position: "relative",
            display: "inline-flex",
            width: 16,
            height: 16,
          }}
        >
          <Sun
            className="tb-theme-icon"
            data-active={theme === "dark"}
            size={16}
            style={{ position: "absolute", inset: 0 }}
          />
          <Moon
            className="tb-theme-icon"
            data-active={theme !== "dark"}
            size={16}
            style={{ position: "absolute", inset: 0 }}
          />
        </span>
      </ActionIcon>
    </Tooltip>
  );
}
