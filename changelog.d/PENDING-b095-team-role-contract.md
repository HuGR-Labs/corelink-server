### Fixed

- **B-095: team invites now expose only roles the membership store can persist.** The customer UI and API use the canonical `admin`, `member`, and `viewer` values; unsupported roles are rejected instead of being silently changed, and responses report the effective stored role.
