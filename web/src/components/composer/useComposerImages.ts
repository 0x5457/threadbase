import { notifications } from "@mantine/notifications";
import { useCallback, useEffect, useRef, useState } from "react";
import { uploadAttachment } from "../../api";

/** Images accepted in one message; mirrors the server's cap. */
export const MAX_COMPOSER_IMAGES = 4;
/** Decoded cap per image; mirrors the server's cap. */
export const MAX_COMPOSER_IMAGE_BYTES = 8 * 1024 * 1024;

export interface ComposerImage {
  key: string;
  /** Object URL while uploading, the served attachment URL once ready. */
  url: string;
  attachmentId: string | null;
  status: "uploading" | "ready";
}

/**
 * Pasted images for the composer. Each file uploads as soon as it is pasted, so
 * the prompt only carries attachment ids and the send never blocks on bytes.
 */
export function useComposerImages(
  threadId: string | undefined,
  writable: boolean,
) {
  const [images, setImages] = useState<ComposerImage[]>([]);
  const imagesRef = useRef<ComposerImage[]>([]);
  const objectUrls = useRef<Set<string>>(new Set());

  const commit = useCallback(
    (updater: (current: ComposerImage[]) => ComposerImage[]) => {
      imagesRef.current = updater(imagesRef.current);
      setImages(imagesRef.current);
    },
    [],
  );

  const revoke = useCallback((url: string) => {
    if (!url.startsWith("blob:")) return;
    URL.revokeObjectURL(url);
    objectUrls.current.delete(url);
  }, []);

  const remove = useCallback(
    (key: string) => {
      commit((current) => {
        const removed = current.find((image) => image.key === key);
        if (removed) revoke(removed.url);
        return current.filter((image) => image.key !== key);
      });
    },
    [commit, revoke],
  );

  const clear = useCallback(() => {
    commit((current) => {
      for (const image of current) revoke(image.url);
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
    async (files: File[]) => {
      if (!threadId || !writable) return;
      const accepted: File[] = [];
      for (const file of files) {
        if (!file.type.startsWith("image/")) continue;
        if (file.size > MAX_COMPOSER_IMAGE_BYTES) {
          notifications.show({
            message: `${file.name || "Image"} is larger than 8 MB`,
            color: "red",
          });
          continue;
        }
        accepted.push(file);
      }
      const slots = Math.max(0, MAX_COMPOSER_IMAGES - imagesRef.current.length);
      if (slots === 0 && accepted.length > 0) {
        notifications.show({
          message: `Up to ${MAX_COMPOSER_IMAGES} images per message`,
          color: "yellow",
        });
        return;
      }
      if (accepted.length > slots) {
        notifications.show({
          message: `Up to ${MAX_COMPOSER_IMAGES} images per message`,
          color: "yellow",
        });
      }
      // Uploads are independent; running them together keeps a four-image
      // paste from paying the round trip four times over.
      await Promise.all(
        accepted.slice(0, slots).map(async (file) => {
          const key = crypto.randomUUID();
          // Tracked in objectUrls and revoked on success, removal, clear and
          // unmount.
          // react-doctor-disable-next-line react-doctor/no-create-object-url-without-revoke
          const url = URL.createObjectURL(file);
          objectUrls.current.add(url);
          commit((current) => [
            ...current,
            { key, url, attachmentId: null, status: "uploading" },
          ]);
          try {
            const attachment = await uploadAttachment(threadId, file);
            revoke(url);
            commit((current) =>
              current.map((image) =>
                image.key === key
                  ? {
                      ...image,
                      url: `/api/attachments/${attachment.id}`,
                      attachmentId: attachment.id,
                      status: "ready",
                    }
                  : image,
              ),
            );
          } catch (cause) {
            remove(key);
            notifications.show({
              message:
                cause instanceof Error
                  ? cause.message
                  : "Could not upload image",
              color: "red",
            });
          }
        }),
      );
    },
    [commit, remove, revoke, threadId, writable],
  );

  const attachmentIds = images
    .filter((image) => image.status === "ready" && image.attachmentId !== null)
    .map((image) => image.attachmentId as string);
  const uploading = images.some((image) => image.status === "uploading");

  return { images, attachmentIds, uploading, accept, remove, clear };
}
