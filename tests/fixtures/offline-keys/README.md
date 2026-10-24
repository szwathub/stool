# Disposable test identity

`test_ed25519` and its public companion are synthetic, unencrypted test-only
Ed25519 keys generated for the isolated integration tests. They identify
`stool-test-only@example.invalid`, are public fixtures, and must never be used
for any real account or authentication. Production keys are never test input.
