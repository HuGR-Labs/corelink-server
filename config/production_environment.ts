/** Canonical production markers shared by the root and signup Workers. */
export const PRODUCTION_ENVIRONMENTS = new Set([
  "prod",
  "production",
  "prod-sam",
  "prod-lhr",
  "prod-nrt",
  "prod-syd",
]);

export interface ProductionEnvironmentInput {
  ENVIRONMENT?: string | null;
  NODE_ENV?: string | null;
}

/** Treat only documented Wrangler production markers as production. */
export function isProductionEnvironment(env: ProductionEnvironmentInput): boolean {
  const marker = env.ENVIRONMENT?.trim().toLowerCase();
  return (
    (marker !== undefined && PRODUCTION_ENVIRONMENTS.has(marker)) ||
    env.NODE_ENV?.trim().toLowerCase() === "production"
  );
}
