import { notifications } from "@mantine/notifications";
import { describeError } from "./errors";

/**
 * One error voice for every mutation. Screens read as `notifyError(error,
 * "Cannot archive thread")` instead of inventing their own toast copy.
 */
export function notifyError(error: unknown, fallbackTitle = "Action failed") {
  const described = describeError(error, fallbackTitle);
  notifications.show({
    title: described.title,
    message: described.description,
    color: "red",
  });
}

export function notifySuccess(message: string, title?: string) {
  notifications.show({ message, title, color: "green" });
}
