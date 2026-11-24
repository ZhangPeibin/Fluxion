// Public managed remote endpoint.
//
// Set at build time with the VITE_MANAGED_REMOTE_URL environment variable
// (for GitHub Pages: repository variable MANAGED_REMOTE_URL). Empty means the
// homepage shows the Managed tab as "soon" and defaults to self-hosting.
export const MANAGED_REMOTE_URL = (import.meta.env.VITE_MANAGED_REMOTE_URL ?? "").trim();
