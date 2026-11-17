import { useEffect, useRef } from "react";
import { createPortal } from "react-dom";

/**
 * Full-screen in-app preview for tool and attachment images. Clicking the
 * backdrop, the close button or Escape closes it; the image itself never
 * navigates away or opens a browser tab.
 *
 * This is a native modal `<dialog>`: `showModal()` puts it in the top layer and
 * the browser owns focus containment, the `::backdrop`, `Esc`/back-gesture
 * dismissal and focus return to the thumbnail that opened it. Rebuilding those
 * by hand is what the previous fixed overlay did, and it left the page behind
 * reachable by keyboard and screen readers.
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
  const dialogRef = useRef<HTMLDialogElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    // `show()` and the `open` attribute leave the page interactive; only
    // `showModal()` makes it inert and gives the top layer, focus trap and
    // Esc handling that a lightbox needs.
    if (!dialog.open) dialog.showModal();
    closeRef.current?.focus();

    // Safari ignores `closedby`, so a backdrop click would not dismiss the
    // preview there. Match the platform rule: the press and the release both
    // have to land outside, so a text selection dragged onto the backdrop
    // never closes the image.
    if ("closedBy" in HTMLDialogElement.prototype) return;
    let pressedOutside = false;
    const outside = (event: PointerEvent) => {
      const rect = dialog.getBoundingClientRect();
      return (
        event.clientX < rect.left ||
        event.clientX > rect.right ||
        event.clientY < rect.top ||
        event.clientY > rect.bottom
      );
    };
    const onPointerDown = (event: PointerEvent) => {
      pressedOutside = outside(event);
    };
    const onPointerUp = (event: PointerEvent) => {
      if (pressedOutside && outside(event)) dialog.close();
      pressedOutside = false;
    };
    dialog.addEventListener("pointerdown", onPointerDown);
    dialog.addEventListener("pointerup", onPointerUp);
    return () => {
      dialog.removeEventListener("pointerdown", onPointerDown);
      dialog.removeEventListener("pointerup", onPointerUp);
    };
  }, []);

  return createPortal(
    <dialog
      ref={dialogRef}
      className="image-lightbox"
      aria-label="Image preview"
      // Light dismiss for the browsers that ship the attribute; the effect
      // above covers the rest.
      closedby="any"
      // The `close` event is the single exit for backdrop clicks, Escape, the
      // back gesture and the close button, so state cannot drift out of sync.
      onClose={onClose}
    >
      <button
        ref={closeRef}
        type="button"
        className="image-lightbox-close"
        aria-label="Close image preview"
        onClick={() => dialogRef.current?.close()}
      >
        ×
      </button>
      <img src={src} alt={alt ?? ""} />
    </dialog>,
    document.body,
  );
}
