# Privacy model

Nerfify is local-only by default. It has no telemetry, cloud sync, auto-updater, system tray, or remote account service in V1.

Stored values are aggregate measurements: token counts, normalized model IDs, cost/pricing status, quota percentages and reset metadata, timestamps, checkpoints, algorithm versions, and aggregate diagnostic reasons. Raw prompts, code, JSONL lines, account email addresses, raw identifiers, and full local paths are not stored or returned through DTOs.

Account identity is normalized in memory and converted to a salted SHA-256 key before persistence. Historical logs without an identifiable account stay unassigned. A log can be associated with a newly observed account only while that authenticated account is continuously observed.

The database lives under the platform-local Nerfify application data directory, uses WAL and foreign keys, and is restricted to the current user on Unix. If the database cannot be opened, it is moved to a timestamped recovery name before a clean database is created.
