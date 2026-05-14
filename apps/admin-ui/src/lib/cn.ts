import clsx, { type ClassValue } from "clsx";

/**
 * Compose CSS class names. Tailwind-friendly classname helper.
 */
export function cn(...inputs: ClassValue[]): string {
  return clsx(inputs);
}
