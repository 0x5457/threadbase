import { notifications } from "@mantine/notifications";
import { useCallback, useEffect, useRef, useState } from "react";
import { uploadAttachment } from "../../api";
import { imageMimeForFile, isPreviewableImage } from "../../lib/attachments";

/** Attachments accepted in one message; mirrors the server's cap. */
const MAX_COMPOSER_ATTACHMENTS = 8;
/** Decoded caps; mirror the server. */
const MAX_COMPOSER_IMAGE_BYTES = 8 * 1024 * 1024;
const MAX_COMPOSER_FILE_BYTES = 25 * 1024 * 1024;

export interface ComposerAttachment {
  key: string;
  name: string;
  mimeType: string;
  byteSize: number;
  isImage: boolean;
  /** Object URL while uploading, the served image URL once ready. */
  url?: string;
  attachmentId: string | null;
  status: "uploading" | "ready";
}

/**
 * Pasted/picked files for the composer. Each file uploads as soon as it is
 * accepted, so the prompt only carries attachment ids and the send never blocks
 * on bytes. Images keep a local preview until the server URL is ready.
 */
export function useComposerAttachments(
  threadId: string | undefined,
  writable: boolean,
) {
  const [items, setItems] = useState<ComposerAttachment[]>([]);
  const itemsRef = useRef<ComposerAttachment[]>([]);
  const objectUrls = useRef<Set<string>>(new Set());

  const commit = useCallback(
    (updater: (current: ComposerAttachment[]) => ComposerAttachment[]) => {
      itemsRef.current = updater(itemsRef.current);
      setItems(itemsRef.current);
    },
    [],
  );

  const revoke = useCallback((url: string | undefined) => {
    if (!url?.startsWith("blob:")) return;
    URL.revokeObjectURL(url);
    objectUrls.current.delete(url);
  }, []);

  const remove = useCallback(
    (key: string) => {
      commit((current) => {
        const removed = current.find((item) => item.key === key);
        if (removed) revoke(removed.url);
        return current.filter((item) => item.key !== key);
      });
    },
    [commit, revoke],
  );

  const clear = useCallback(() => {
    commit((current) => {
      for (const item of current) revoke(item.url);
      return [];
    });
  }, [commit, revoke]);

  // A thread switch starts a fresh prompt; previews from the old one must go.
  useEffect(() => {
    clear();
  }, [clear, threadId]);

  // Nothing left behind when the composer unmounts mid-upload.
  useEffect(
    () => () => {
      for (const url of objectUrls.current) URL.revokeObjectURL(url);
      objectUrls.current.clear();
    },
    [],
  );

  const accept = useCallback(
    async (files: File[]): Promise<boolean> => {
      if (!threadId || !writable) return false;
      const accepted: File[] = [];
      for (const file of files) {
        const image = imageMimeForFile(file) !== null;
        const limit = image
          ? MAX_COMPOSER_IMAGE_BYTES
          : MAX_COMPOSER_FILE_BYTES;
        if (file.size > limit) {
          notifications.show({
            message: `${file.name || "File"} is larger than ${
              image ? "8 MB" : "25 MB"
            }`,
            color: "red",
          });
          continue;
        }
        accepted.push(file);
      }
      const slots = Math.max(
        0,
        MAX_COMPOSER_ATTACHMENTS - itemsRef.current.length,
      );
      if (slots === 0 && accepted.length > 0) {
        notifications.show({
          message: `Up to ${MAX_COMPOSER_ATTACHMENTS} attachments per message`,
          color: "yellow",
        });
        return false;
      }
      if (accepted.length > slots) {
        notifications.show({
          message: `Up to ${MAX_COMPOSER_ATTACHMENTS} attachments per message`,
          color: "yellow",
        });
      }
      const batch = accepted.slice(0, slots);
      if (batch.length === 0) return false;
      let success = true;
      // Uploads are independent; running them together keeps a multi-file
      // paste from paying the round trip one file at a time.
      await Promise.all(
        batch.map(async (file) => {
          const key = crypto.randomUUID();
          const inferredImage = imageMimeForFile(file);
          const isImage = inferredImage !== null;
          const url = isImage
            ? // Tracked in objectUrls and revoked on success, removal, clear
              // and unmount.
              // react-doctor-disable-next-line react-doctor/no-create-object-url-without-revoke
              URL.createObjectURL(file)
            : undefined;
          if (url) objectUrls.current.add(url);
          commit((current) => [
            ...current,
            {
              key,
              name: file.name || "attachment",
              mimeType: inferredImage ?? (file.type || "application/octet-stream"),
              byteSize: file.size,
              isImage,
              url,
              attachmentId: null,
              status: "uploading",
            },
          ]);
          try {
            const attachment = await uploadAttachment(threadId, file);
            revoke(url);
            const readyIsImage = isPreviewableImage(attachment.mimeType);
            commit((current) =>
              current.map((item) =>
                item.key === key
                  ? {
                      ...item,
                      name: attachment.fileName ?? item.name,
                      mimeType: attachment.mimeType,
                      byteSize: attachment.byteSize,
                      isImage: readyIsImage,
                      url: readyIsImage
                        ? `/api/attachments/${attachment.id}`
                        : undefined,
                      attachmentId: attachment.id,
                      status: "ready",
                    }
                  : item,
              ),
            );
          } catch (cause) {
            success = false;
            remove(key);
            notifications.show({
              message:
                cause instanceof Error
                  ? cause.message
                  : `${file.name || "File"} could not be attached`,
              color: "red",
            });
          }
        }),
      );
      return success && accepted.length <= slots;
    },
    [commit, remove, revoke, threadId, writable],
  );

  const attachmentIds = items
    .filter((item) => item.status === "ready" && item.attachmentId !== null)
    .map((item) => item.attachmentId as string);
  const uploading = items.some((item) => item.status === "uploading");

  return { items, attachmentIds, uploading, accept, remove, clear };
}
