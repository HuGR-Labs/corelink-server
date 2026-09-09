// Canonical route inventory for the baseline promotion guard. It mirrors the
// Playwright sweep; promotion rejects missing, duplicate, or unexpected routes.
import ROUTE_PATHS from "./a11y-routes.json";

const BASE_PATH = process.env.DOCS_BASE_PATH ?? "/corelink/docs";

export const A11Y_ROUTES = ROUTE_PATHS.map((route) =>
  route === "/" ? `${BASE_PATH}/` : `${BASE_PATH}${route}`,
);
