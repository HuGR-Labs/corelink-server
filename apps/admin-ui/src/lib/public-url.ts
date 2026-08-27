export const CORELINK_BASE_PATH = "/corelink";
export const CORELINK_PUBLIC_ORIGIN = "https://humangr.com";
export const CORELINK_PUBLIC_BASE_URL = `${CORELINK_PUBLIC_ORIGIN}${CORELINK_BASE_PATH}`;

export function corelinkPath(path = "/"): string {
  if (path === "" || path === "/") return CORELINK_BASE_PATH;
  return `${CORELINK_BASE_PATH}${path.startsWith("/") ? path : `/${path}`}`;
}

export function corelinkUrl(path = "/"): string {
  return `${CORELINK_PUBLIC_ORIGIN}${corelinkPath(path)}`;
}
