declare module "jest-axe" {
  import type { AxeResults, RunOptions, Spec } from "axe-core";

  export interface JestAxe {
    (html: Element | string, options?: RunOptions): Promise<AxeResults>;
  }

  export function configureAxe(options?: RunOptions & { globalOptions?: Spec }): JestAxe;
  export const axe: JestAxe;

  export const toHaveNoViolations: Record<string, unknown>;
}
