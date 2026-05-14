/**
 * Static metadata about the SDK how-to guides produced under WI-S18-003.
 *
 * Tests in tests/sdk-guides.test.ts read this contract and walk the docs/
 * tree to assert every required page is present + frontmatter-valid + has
 * a "When to use this guide" section (Diataxis how-to principle).
 */

export const SDK_LANGUAGES = ["python", "go", "js", "cli"] as const;
export type SdkLanguage = (typeof SDK_LANGUAGES)[number];

export const SDK_HOWTO_SLUGS = [
  "01-authenticate",
  "02-upload-blob",
  "03-download-blob",
  "04-action-cache",
  "05-batch-operations",
  "06-ci-integration",
  "07-troubleshooting",
] as const;

export const CLI_REFERENCE_PAGES = [
  "index",
  "ls",
  "get",
  "put",
  "stat",
  "bench",
  "doctor",
  "version",
  "config",
] as const;

export const REQUIRED_HOWTO_HEADING = "When to use this guide";

/** Map language slug to canonical MDX code-fence language tag. */
export const CODE_FENCE_LANG: Record<SdkLanguage, string> = {
  python: "python",
  go: "go",
  js: "typescript",
  cli: "bash",
};
