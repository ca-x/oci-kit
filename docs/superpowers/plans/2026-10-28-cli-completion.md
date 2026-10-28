# CLI completion before image packaging

Specification: `SPEC.md`. The user requires every CLI's agreed functionality to
be implemented and verified before work continues on Pond image packaging. Do
not dispatch the image workflow or change the Pond Dockerfile during this gate.

## Tasks

1. Complete archive-inspection failure semantics and process-level acceptance
   for the generic tools. Cover source validation, verified cache reuse, failed
   downloads, archive metadata/text/compression, and cleanup dry-run/apply/refusal.
   A requested text file that is absent must not silently produce success.
   Inspection must catch truncated payloads and decoder errors, not report
   success after reading headers alone.
2. Audit and finish the LightOS adapter against the observed platform contract.
   Preserve platform ownership of credentials/accounts/SSH; do not invent seed
   fields. Test offline install, planning, prerequisites, failure retry,
   concurrency, changed configuration and unchanged-run idempotence. Verify the
   CLI output and exit behavior, not only private helpers. Unresolved agreed
   initialization functionality blocks the completion gate.
3. Verify the declared Rust minimum and usable Linux x86-64 static binaries in
   CI. Run the binaries, inspect ELF linkage, and exercise runtime initialization
   in disposable fixtures with no host credentials or host service changes.
   Tool acceptance is not the Pond image project and publishes no rootfs/image.
4. Obtain an independent whole-change review. Resolve important findings using
   observed red-to-green tests, then run full GitHub Actions gates. Record the
   per-command acceptance matrix and platform limits. Only then may the next
   task start image packaging automatically.

## Execution rulings

- Ruling: reuse the clean, user-authorized `main` checkout and push small verified
  phases over SSH. The review agent is read-only; there is no concurrent writer.
- Ruling: the CLI processes and existing public archive API are the agreed test
  seams. Acceptance tests may pin existing behavior without manufacturing a
  failure; each behavior correction requires a meaningful failing test.
- Ruling: the physical Niri/VT session belongs to platform/image acceptance, not
  to inventing a general initializer. The CLI gate must still verify everything
  the tools promise; source-only CI is insufficient.
- Ruling: an unavailable reviewer is pending, not an approved review.

Commit and push each verified phase and continue directly to the next task.
Preserve the third-party redistribution boundary.
