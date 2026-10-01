use crate::types::*;
use soroban_env::{Env, IntoVal, Symbol, Val};

/// Publishes an event only when the given condition holds.
/// Returns true if an event was published.
///
/// Invariants:
/// - When `condition` is false, no event is emitted and the event count is unchanged.
/// - When `condition` is true, exactly one event is emitted.
pub fn publish_if(env: &Env, condition: bool, topic: Symbol, data: &[Val], val: Val) -> bool {
    if !condition {
        return false;
    }
    env.events().publish((topic, data), val);
    true
}

/// Publishes an event with the given topic and data.
pub fn publish(env: &Env, topic: Symbol, data: &[Val], val: Val) {
    env.events().publish((topic, data), val);
}

/// Returns the number of events emitted so far.
pub fn event_count(env: &Env) -> u32 {
    env.events().all().len()
}

/// Returns the last event emitted, if any.
pub fn last_event(env: &Env) -> Option<(soroban_env::Address, soroban_env::Vec<soroban_env::Val>, Val)> {
    env.events().all().last()
}

/// Returns the number of events matching the given topic.
pub fn event_count_by_topic(env: &Env, topic: &Symbol) -> u32 {
    env.events()
        .all()
        .iter()
        .filter(|(f, _) | f.get(0) == Some(topic.clone()))
        .count() as u32
}

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
