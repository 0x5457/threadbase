import { Notifications } from "@mantine/notifications";
import { useMediaQuery } from "@/lib/hooks";

/**
 * Toasts sit above the composer on desktop, but on a phone that hides the
 * Send button while a message is being sent. Phones get them below the title
 * bar instead (52px clears the 44px bar), desktop keeps bottom-right.
 */
export function AppNotifications() {
  const isPhone = useMediaQuery("(max-width: 48em)");
  return (
    <Notifications
      className="tb-notifications"
      position={isPhone ? "top-center" : "bottom-right"}
    />
  );
}
