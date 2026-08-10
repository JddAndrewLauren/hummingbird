//! Native fixture tests for the authority (#113 S0, grown by #114): every
//! acceptance criterion, against real SQLite (rusqlite in memory) behind
//! the same [`hummingbird_authority::Sql`] seam the Durable Object drives.
//! Zero live credentials.

mod rig;

mod admin_tokens;
mod alerts;
mod auth;
mod blocked_by;
mod changes;
mod delivery;
mod fog;
mod items;
mod projects_routes;
mod push_targets;
mod rebase;
mod routing;
mod rules;
mod schema;
mod settings;
mod steps;
