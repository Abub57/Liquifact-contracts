# Closes Liquifact#1314 — Harden concurrent execution around escrow

## TL;DR
The test file named in the issue (`escrow/src/tests/attestation_config_view.rs`)
was never wired into `tests/mod.rs`, and it tested a **function that does not
exist** (`get_attestation_config`) against a **struct that does not exist**
(`AttestationConfig`). This PR:

1. Wires the test module in.
2. Rewrites the file to test the **real** exported view and the actual
   concurrency invariants of the attestation entrypoints.
3. Documents a **pre-existing** compile failure on `main` that blocks local
   test execution (unrelated to this PR).

## What was actually wrong with the original file

- `grep -n "pub fn get_attestation_config" escrow/src/lib.rs` → no matches.
- `grep -rn "AttestationConfig" escrow/src/` → no matches outside this test.
- `grep -n "MAX_ATTESTATION_APPEND_BATCH" escrow/src/lib.rs` → no matches.
- `grep -rn "get_attestation_config" docs/` → no matches.

The function, the type, and the constant that the file referenced **do not
exist**. The file could never have compiled. Because it wasn't in `mod.rs`,
nobody noticed.

The two fields the tests actually need are on `EscrowSummary`:
`has_primary_attestation` and `attestation_log_length`, returned by
`get_escrow_summary()`.

## What this PR does

### 1. Wires the module in
    mod attestation_config_view;
    mod attestations;

### 2. Rewrites the file against real entrypoints

Reads state through `client.get_escrow_summary()`. No phantom API. All
entrypoints referenced exist in `escrow/src/lib.rs`:

    bind_primary_attestation_hash       (line 4098)
    append_attestation_digest           (line 4129)
    get_primary_attestation_hash        (line 4118)
    get_attestation_append_log          (line 4153)
    revoke_attestation_digest           (line 4506)
    revoke_attestation_digests          (line 4557)
    is_attestation_revoked              (line 4600)
    unrevoke_attestation_digest         (line 4711)
    get_escrow_summary                  (line 4051)

### 3. Test coverage map

| Test | Invariant | Scenario |
|---|---|---|
| `test_view_defaults_after_init` | — | Baseline view after init |
| `test_view_primary_bound_true_after_bind` | INV-ATT-2 | Bind flips view field |
| `test_view_append_log_length_increments_by_one` | INV-ATT-4 | Append increments view |
| `test_view_log_length_unaffected_by_revoke` | INV-ATT-4 | Revoke does not shrink log |
| `test_view_is_idempotent` | INV-ATT-9 | Repeated reads identical |
| `test_racing_bind_second_call_rejected` | INV-ATT-2 | Second bind fails; view unchanged |
| `test_duplicate_bind_same_digest_still_rejected` | INV-ATT-2 | Write-once by existence |
| `test_racing_revoke_same_index_second_call_rejected` | INV-ATT-6 | Double-revoke fails |
| `test_batch_revoke_with_duplicate_rolls_back` | INV-ATT-7 | Batch atomicity: duplicate |
| `test_batch_revoke_with_out_of_range_rolls_back` | INV-ATT-7 | Batch atomicity: range |
| `test_batch_revoke_too_large_rejected` | INV-ATT-7 | Batch size bound |
| `test_unrevoke_twice_second_call_rejected` | INV-ATT-8 | Unrevoke precondition |
| `test_revoke_unrevoke_revoke_cycle` | INV-ATT-6 / 8 | State flips cleanly |
| `test_append_log_boundary_exactly_at_capacity` | INV-ATT-3 | 32nd succeeds, 33rd fails |
| `test_duplicate_digests_are_appended` | INV-ATT-4 | Log is a trail, not a set |
| `test_view_reflects_each_interleaved_append` | INV-ATT-9 | No stale snapshots |
| `test_primary_bound_and_log_length_are_independent` | — | Field orthogonality |
| `test_view_snapshot_isolated_from_later_mutations` | INV-ATT-9 | Value semantics |
| `test_non_admin_cannot_bind_append_or_revoke` | INV-ATT-1 | Auth boundary |

## Pre-existing breakage on `main` (not introduced by this PR)

`cargo test -p liquifact_escrow` on a clean checkout of `upstream/main`
fails with 251 errors. The two in production code are:

1. `EscrowError::AttestationNotRevoked` is declared **twice**:
   - line 662: `AttestationNotRevoked = 56`
   - line 835: `AttestationNotRevoked = 168`
   `#[contracterror]` fails with E0004.
2. `claim_investor_payout` (line ~7243) moves `investor` then borrows it — E0382.

Test-file breakage (`test_allowlist_tests.rs`, `callback_binding_tests.rs`,
`release_budget_tests.rs`) is due to signature drift on the same `main`.
Full breakdown in `NOTES_1314.md`.

**This PR does not touch any of that.** It only adds and wires tests. It
would be wrong to merge any test-only PR while the trunk doesn't compile; a
maintainer should fix the `EscrowError` duplicate first.

## Verification performed

- `rustfmt --check escrow/src/tests/attestation_config_view.rs` → OK.
- Every entrypoint referenced was verified to exist in `escrow/src/lib.rs`
  by grep (commands listed above).
- No reference to `get_attestation_config`, `AttestationConfig`, or
  `MAX_ATTESTATION_APPEND_BATCH` remains in the file.

## Compatibility

- No public interface change.
- No contract behavior change.
- No new dependencies.

## Checklist against issue acceptance criteria

- [x] Intended behavior deterministic for valid/invalid/duplicate/boundary
- [x] Authorization, validation, state-transition invariants tested
- [x] Retries / interleaved execution covered
- [x] Focused tests cover success, rejection, boundary, regression
- [x] Existing callers unaffected
- [x] Failures diagnosable via typed `EscrowError` codes
- [ ] **Locally runnable** — blocked by pre-existing `main` breakage
