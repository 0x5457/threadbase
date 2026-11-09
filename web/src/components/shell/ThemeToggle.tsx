import { Moon, Sun } from "lucide-react";
import { toggleTheme, useTheme } from "@/lib/theme";
import { IconButton, Tooltip } from "@/components/ui";

export function ThemeToggle() {
  const theme = useTheme();
  const next = theme === "dark" ? "light" : "dark";
  return (
    <Tooltip content={`Switch to ${next} theme`}>
      <IconButton
        label={`Switch to ${next} theme`}
        variant="ghost"
        onClick={toggleTheme}
      >
        {theme === "dark" ? <Sun /> : <Moon />}
      </IconButton>
    </Tooltip>
  );
}
