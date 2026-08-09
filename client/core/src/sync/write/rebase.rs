//! The rebase decision (#101): the pure field-diff that makes a 409
//! computable rather than merely detected.
//!
//! Three JSON values, three roles: `base` is the entity as this client last
//! knew it (what the patch was composed against); `patch` is the touched-
//! field body this client sent (`expected_version` plus absolute-value
//! sets — [`crate`]'s domain patch DTOs, e.g. `ItemPatch`, all share this
//! shape, so this diff is generic over every entity in ADR-0009's write
//! vocabulary); `current` is the entity the 409 carried back.
//!
//! For each field the patch touched (every key but `expected_version`):
//! - `current[field] == patch[field]` — already achieved. Either this
//!   exact write already landed (a crash-then-replay saw its own prior
//!   success) or an intervening write coincidentally set the same value.
//!   Either way there is nothing left to do for this field.
//! - `current[field] == base[field]` — untouched since this client last
//!   read it. Safe to reapply at the new version.
//! - otherwise — an intervening write set this field to something this
//!   client neither intended nor already knew about. A genuine collision;
//!   named so the conflict journal (client-side, not this adapter's job)
//!   has material to work with.
//!
//! No collisions across every touched field ⇒ [`RebaseDecision::Safe`]: the
//! same patch, reissued at `current`'s version, is guaranteed to apply
//! cleanly (nothing it touches has moved to a value it doesn't already
//! reconcile with). Any collision ⇒ [`RebaseDecision::Collision`], naming
//! every colliding field — never "merely that a 409 happened".

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum RebaseDecision {
    /// Every touched field is either already at its intended value or
    /// untouched by the intervening write — safe to reissue at the new
    /// version.
    Safe,
    /// At least one touched field collided; every colliding field's name.
    Collision(Vec<String>),
}

/// Decide whether a 409 auto-resolves. `patch` must be a JSON object (every
/// domain patch DTO serializes to one); a non-object degenerates to `Safe`
/// rather than panicking, since there is nothing to diff.
pub fn decide(patch: &Value, base: &Value, current: &Value) -> RebaseDecision {
    let Some(fields) = patch.as_object() else {
        return RebaseDecision::Safe;
    };

    let mut collisions = Vec::new();
    for (field, intended) in fields {
        if field == "expected_version" {
            continue;
        }
        let current_value = current.get(field).unwrap_or(&Value::Null);
        if current_value == intended {
            continue; // already achieved
        }
        let base_value = base.get(field).unwrap_or(&Value::Null);
        if current_value == base_value {
            continue; // untouched by the intervening write
        }
        collisions.push(field.clone());
    }

    if collisions.is_empty() {
        RebaseDecision::Safe
    } else {
        RebaseDecision::Collision(collisions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_disjoint_field_change_is_safe() {
        // This client's patch touches only `priority`; the intervening
        // write bumped `context` — a different field entirely.
        let base = json!({"priority": 1, "context": "@calls", "version": 5});
        let current = json!({"priority": 1, "context": "@computer", "version": 6});
        let patch = json!({"expected_version": 5, "priority": 2});

        assert_eq!(decide(&patch, &base, &current), RebaseDecision::Safe);
    }

    #[test]
    fn a_same_field_change_is_a_named_collision() {
        // Both this client and the intervening write touched `priority`,
        // to different values.
        let base = json!({"priority": 1, "version": 5});
        let current = json!({"priority": 3, "version": 6});
        let patch = json!({"expected_version": 5, "priority": 2});

        assert_eq!(
            decide(&patch, &base, &current),
            RebaseDecision::Collision(vec!["priority".to_string()])
        );
    }

    #[test]
    fn a_replay_that_already_landed_is_safe() {
        // The intervening write is this client's own prior success (a
        // crash swallowed the ack): current already holds exactly what
        // this patch intends.
        let base = json!({"priority": 1, "version": 5});
        let current = json!({"priority": 2, "version": 6});
        let patch = json!({"expected_version": 5, "priority": 2});

        assert_eq!(decide(&patch, &base, &current), RebaseDecision::Safe);
    }

    #[test]
    fn multiple_touched_fields_report_every_collision() {
        let base = json!({"title": "old", "priority": 1, "version": 5});
        let current = json!({"title": "someone else's title", "priority": 1, "version": 6});
        let patch = json!({"expected_version": 5, "title": "my title", "priority": 2});

        assert_eq!(
            decide(&patch, &base, &current),
            RebaseDecision::Collision(vec!["title".to_string()])
        );
    }

    #[test]
    fn a_clear_to_null_is_diffed_like_any_other_value() {
        // Double-`Option` patch fields serialize `Some(None)` as an
        // explicit JSON null — the diff must treat that like any other
        // value, not specially skip it.
        let base = json!({"description": "notes", "version": 5});
        let current = json!({"description": "notes", "version": 6});
        let patch = json!({"expected_version": 5, "description": null});

        assert_eq!(decide(&patch, &base, &current), RebaseDecision::Safe);
    }

    #[test]
    fn expected_version_itself_is_never_diffed_as_a_field() {
        let base = json!({"version": 5});
        let current = json!({"version": 6});
        let patch = json!({"expected_version": 5});

        assert_eq!(decide(&patch, &base, &current), RebaseDecision::Safe);
    }
}
