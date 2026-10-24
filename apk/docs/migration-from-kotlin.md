# Migrating the pairing from the Kotlin app (v0.1.x)

Anyone who had the Kotlin app installed (same `applicationId` and signing key) and
updates to the Flutter app keeps the data directory, so the new app can detect the
previous pairing and offer to reuse it instead of asking to pair again.

What the app reads (read-only, never modified):

- `files/datastore/pairing.preferences_pb` — base URL and device id.
- `herdr_recovery` (EncryptedSharedPreferences) — the recovery code, only when re-enrolling.

Flow:

1. On start, if the app has no pairing of its own and legacy data exists, it shows
   "Found the pairing from the previous version with <computer>. Use it?".
2. "Use" re-enrolls with the recovery code through `POST /v1/pair`, using the Flutter
   app's own key. The host keeps the `device_id` and permissions and only replaces the
   public key (`use_recovery_code`); no host change is needed.
3. The legacy data is deleted only after a signed `GET /v1/state` succeeds. If anything
   fails, it is kept and the user can pair again. Declining is remembered and the app
   does not ask again.

Known limits: clearing the legacy data is not atomic (preferences, SSE state, Keystore),
and the host rotates the recovery code before the app stores the new one, so a crash in
between leaves the legacy pairing unusable. A dry-run endpoint on the host would remove
that window.
