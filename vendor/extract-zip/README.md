# CoreLink `extract-zip` substitute

This package keeps the `extract-zip` API used by Puppeteer while rejecting
absolute, parent-traversing, and symlink-mediated paths. It is pinned as a
local override because the upstream `2.0.1` line has no published fix.
