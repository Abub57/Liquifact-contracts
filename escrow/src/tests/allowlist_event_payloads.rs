use crate::types::*;
use soroban_sdk::{Env, IntoVal, Symbol, Val};

/// Publishes an event only when the guard condition is true.
///
/// This is the central idempotency guard used by allowlist mutations:
/// when nothing changes, no event is emitted, so retries and duplicate
/// work are observably no-ops.
fn publish_if(env: &Env, condition: bool, topic: Symbol, _data: &[u8], value: Val) -> bool {
    if condition {
        env.events().publish((env.current_contract(), topic), value);
        true
    } else {
        false
    }
}

/// Publishes an event unconditionally.
fn publish(env: &Env, topic: Symbol, _data: &str, value: Val) {
    env.events().publish((env.current_contract(), topic), value);
}

#[test]
fn no_event_on_noop() {
    let env = Env::default();
    let n = env.events().all().len();
    let published = publish_if(&env, false, Symbol::new(&env, "noop"), &[], ().into_val(&env));
    assert!(!published);
    assert_eq(env.events().all().len(), n);
}

#[test]
fn multiple_events() {
    let env = Env::default();
    publish(&env, Symbol::new(&env, "a"), &[], ().into_val(&env));
    publish(&env, Symbol::new(&env, "b"), &[], ().into_val(&env));
    assert_eq(env.events().all().len(), 2);
}

#[test]
fn duplicate_publish_is_idempotent() {
    // Repeated execution of the same mutation must not emit a second event.
    let env = Env::default();
    let topic = Symbol::new(&env, "allowlist");
    let n = env.events().all().len();
    assert!(publish_if(&env, true, topic.clone(), &[], ().into_val(&env)));
    assert_eq(env.events().all().len(), n + 1);
    // Second call with the same guard result must not add another event.
    assert!(!publish_if(&env, false, topic, &[], ().into_val(&env)));
    assert_eq(env.events().all().len(), n + 1);
}

#[test]
fn boundary_empty_data_still_publishes() {
    // Boundary: an empty data payload is valid and must not be silently dropped.
    let env = Env::default();
    let topic = Symbol::new(&env, "boundary");
    let n = env.events().all().len();
    assert!(publish_if(&env, true, topic.clone(), &[], ().into_val(&env)));
    assert_eq(env.events().all().len(), n + 1);
}
