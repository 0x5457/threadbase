import { useEffect } from "react";
import { createPortal } from "react-dom";

/**
 * Full-screen in-app preview for tool and attachment images. Clicking the
 * backdrop, the close button or Escape closes it; the image itself never
 * navigates away or opens a browser tab.
 */
export function ImageLightbox({
  src,
  alt,
  onClose,
}: {
  src: string;
  alt?: string;
  onClose: () => void;
}) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  return createPortal(
    <div
      className="image-lightbox"
      role="dialog"
      aria-modal="true"
      aria-label="Image preview"
      onClick={onClose}
    >
      <button
        type="button"
        className="image-lightbox-close"
        aria-label="Close image preview"
        onClick={onClose}
      >
        ×
      </button>
      <img
        src={src}
        alt={alt ?? ""}
        onClick={(event) => event.stopPropagation()}
      />
    </div>,
    document.body,
  );
}
