//! Native fixture tests for the authority (#113 S0, grown by #114): every
//! acceptance criterion, against real SQLite (rusqlite in memory) behind
//! the same [`hummingbird_authority::Sql`] seam the Durable Object drives.
//! Zero live credentials.

mod rig;

mod admin_tokens;
mod alerts;
mod auth;
mod blocked_by;
mod calendar_token;
mod changes;
mod delivery;
mod fcm;
mod fog;
mod grills;
mod items;
mod project_links;
mod projects_routes;
mod push_targets;
mod rebase;
mod resolution;
mod routing;
mod rules;
mod schema;
mod settings;
mod skills;
mod snapshots;
mod steps;
mod sweep;
mod webhook_delivery;
