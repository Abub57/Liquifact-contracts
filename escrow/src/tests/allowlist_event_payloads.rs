use crate::types::*;
use soroban_sdk::{Env, IntoVal, Symbol, Val};

/// Publishes an event only when the guard condition holds.
///
/// This is the mindimal contract that every event-emitting entrypoint in this
/// test tree relies on: a false guard must not emit anything, and a true
/// guard must emit exactly one event with the caller-supplied topic and
/// payload. Keeping this helper deterministic is what allows the assertions
/// below to be meaningful across empty, duplicate, and boundary inputs.
///
///  Invariants
///  - Returns `true` iff the event was actually published.
///  - Never publishes when `guard` is false.
///  - Never publishes more than one event per call.
///  - Preserves the exact topic and payload passed by the caller.
///
/// The `guard` parameter is deliberately a plain `bool` so that the callsite
/// cannot accidentally pass a non-boolean truthy value and silently change
/// emission behavior.
pub fn publish_if(env: &Env, guard: bool, topic: Symbol, data: &[Val], payload: Val) -> bool {
    if !guard {
        return false;
    }
    env.events().publish((topic,), (data, payload));
    true
}

/// Unconditional publish wrapper used by tests that are not exercising the
/// guard logic. This keeps the callsite shape identical to `publish_if` with
/// a `true` guard so behavior differences are attributable to the guard alone.
pub fn publish(env: &Env, topic: Symbol, data: &[Val], payload: Val) -> bool {
    publish_if(env, true, topic, data, payload)
}

#[test]
fn no_event_on_noop() {
    let env = Env::default();
    let n = env.events().all().len();
    let published = publish_if(
        &env,
        false,
        Symbol::new(&env, "noop"),
        &([]),
        ().into_val(&env),
    );
    assert!(!published);
    assert_eq!(env.events().all().len(), n);
}

#[test]
fn publish_if_true_emits_one_event() {
    let env = Env::default();
    let n = env.events().all().len();
    let published = publish_if(
        &env,
        true,
        Symbol::new(&env, "one"),
        &([]),
        ().into_val(&env),
    );
    assert!(published);
    assert_eq!(env.events().all().len(), n + 1);
}

#[test]
fn multiple_events() {
    let env = Env::default();
    publish(&env, Symbol::new(&env, "a"), &[], ().into_val(&env));
    publish(&env, Symbol::new(&env, "b"), &[], ().into_val(&env));
    assert_eq(env.events().all().len(), 2);
}

#[test]
fn publish_if_false_is_idempotent_under_repeated() {
    let env = Env::default();
    let n0 = env.events().all().len();
    for _ in 0..16 {
        let published = publish_if(
            &env,
            false,
            Symbol::new(&env, "noop"),
            &([]),
            ().into_val(&env),
        );
        assert!(!published);
    }
    assert_eq!(env.events().all().len(), n0);
}

#[test]
fn publish_if_true_repeated_emits_one_per_call() {
    let env = Env::default();
    let n0 = env.events().all().len();
    for _ in 0..8 {
        let published = publish_if(
            &env,
            true,
            Symbol::new(&env, "dup"),
            &([]),
            ().into_val(&env),
        );
        assert!(published);
    }
    assert_eq!(env.events().all().len(), n0 + 8);
}

/// Empty data and empty payload are valid inputs. The helper must not
/// drop the event or attempt to interpret the payload.
#[test]
fn empty_data_and_payload_still_emits() {
    let env = Env::default();
    let n0 = env.events().all().len();
    let published = publish_if(
        &env,
        true,
        Symbol::new(&env, "empty"),
        &([]),
        ().into_val(&env),
    );
    assert!(published);
    assert_eq!(env.events().all().len(), n0 + 1);
}

/// The guard is the only thing that can suppress an event: for a fixed
/// topic and payload, `false` must emit zero events and `true` must emit
/// exactly one. This is the regression guard for the compatibility
/// contract of the helper.
#[test]
fn guard_is the_only_difference() {
    let env = Env::default();
    let topic = Symbol::new(&env, "guard");
    let n0 = env.events().all().len();
    assert!(!publish_if(&env, false, topic.clone(), &([]), ().into_val(&env)));
    assert_eq!(env.events().all().len(), n0);
    assert!(publish_if(&env, true, topic.clone(), &([]), ().into_val(&env)));
    assert_eq!(env.events().all().len(), n0 + 1);
}
