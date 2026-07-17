import { useEffect, useRef, type RefObject } from "react";

const focusableSelector = [
  "button:not([disabled])",
  "a[href]",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

export function useDialogFocus<T extends HTMLElement>(
  active: boolean,
  onEscape: () => void,
  initialFocus?: RefObject<HTMLElement | null>,
) {
  const dialog = useRef<T>(null);
  const escapeHandler = useRef(onEscape);
  escapeHandler.current = onEscape;

  useEffect(() => {
    if (!active) return;
    const previouslyFocused = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const container = dialog.current;
    const focusable = () =>
      Array.from(container?.querySelectorAll<HTMLElement>(focusableSelector) ?? []).filter(
        (element) => element.getAttribute("aria-hidden") !== "true",
      );
    (initialFocus?.current ?? focusable()[0])?.focus();

    const handleKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        escapeHandler.current();
        return;
      }
      if (event.key !== "Tab") return;
      const elements = focusable();
      if (elements.length === 0) {
        event.preventDefault();
        return;
      }
      const first = elements[0];
      const last = elements[elements.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first?.focus();
      }
    };

    window.addEventListener("keydown", handleKey);
    return () => {
      window.removeEventListener("keydown", handleKey);
      previouslyFocused?.focus();
    };
  }, [active, initialFocus]);

  return dialog;
}
