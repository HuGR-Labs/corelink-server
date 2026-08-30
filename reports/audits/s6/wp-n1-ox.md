Two issues: (1) `res.contentLength` is optional so TS needs a fallback for a missing header; (2) vitest types aren't linked in my hand-built node_modules. Fixing both:
