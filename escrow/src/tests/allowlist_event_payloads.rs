use crate::types::*;
use soroban_sdk::{Env, IntoVal, Symbol, Val};

/// Test helper: publish an event only when `should_publish` is true.
/// Returns whether the event was actually published.
fn publish_if(env: &Env, should_publish: bool, topic: Symbol, data: Val) -> bool {
    if should_publish {
        env.events().publish((topic,), data);
        true
    } else {
        false
    }
}

/// Test helper: unconditionally publish an event.
fn publish(env: &Env, topic: Symbol, data: Val) {
    env.events().publish((topic,), data);
}

#[test]
fn no_event_on_noop() {
    let env = Env::default();
    let n = env.events().all().len();
    let published = publish_if(&env, false, Symbol::new(&env, "noop"), ().into_val(&env));
    assert!(!published);
    assert_eq!(env.events().all().len(), n);
}

#[test]
fn publish_if_true_emits_event() {
    let env = Env::default();
    let n = env.events().all().len();
    let published = publish_if(&env, true, Symbol::new(&env, "ok"), ().into_val(&env));
    assert!(published);
    assert_eq(env.events().all().len(), n + 1);
}

#[test]
fn multiple_events() {
    let env = Env::default();
    publish(&env, Symbol::new(&env, "a"), ().into_val(&env));
    publish(&env, Symbol::new(&env, "b"), ().into_val(&env));
    assert_eq!(env.events().all().len(), 2);
}

#[test]
fn duplicate_events_are_both_recorded() {
    let env = Env::default();
    let topic = Symbol::new(&env, "dup");
    publish(&env, topic.clone(), ().into_val(&env));
    publish(&env, topic.clone(), ().into_val(&env));
    assert_eq!(env.events().all().len(), 2);
}

#[test]
fn boundary_empty_data_is_valid() {
    let env = Env::default();
    let n = env.events().all().len();
    publish(&env, Symbol::new(&env, "bound"), ().into_val(&env));
    assert_eq(env.events().all().len(), n + 1);
}
