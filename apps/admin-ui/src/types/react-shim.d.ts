/**
 * Shim to enforce a single @types/react@19 instance throughout the admin-ui
 * TypeScript compilation, preventing dual-version conflicts from the pnpm
 * workspace (docs app brings @types/react@18 via docusaurus peer deps).
 *
 * This file is NOT a module — it's an ambient declaration that augments the
 * global JSX namespace to use the already-loaded react@19 types.
 */
export {};
