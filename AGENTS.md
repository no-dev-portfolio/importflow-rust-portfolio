# ImportFlow maintenance

- Read README.md, docs/DESIGN.md and docs/REQUIREMENTS_MATRIX.md before edits.
- Preserve tenant isolation, permissions, atomic imports, payload-bound idempotency, queue admission, lease fencing and cancellation.
- Keep validated batches immutable and the unsafe-code prohibition.
- Run real PostgreSQL integration tests; never silently skip absent TEST_DATABASE_URL.
- Run fmt, clippy with warnings denied, affected tests, frontend build/tests and dependency audits as appropriate.
- Review the lockfile-only RSA advisory exception; fail if rsa enters the active build graph.
- Do not commit runtime credentials, actual customer CSV, DB dumps, target or node_modules. The documented demo keys and synthetic DB password are intentionally public.
- Do not claim professional Rust years, cloud deployment, external MQ operations or independent certification without evidence.
