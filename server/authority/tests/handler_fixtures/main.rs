//! Native fixture tests for the authority (#113 S0, grown by #114): every
//! acceptance criterion, against real SQLite (rusqlite in memory) behind
//! the same [`hummingbird_authority::Sql`] seam the Durable Object drives.
//! Zero live credentials.

mod rig;

mod blocked_by;
mod changes;
mod fog;
mod items;
mod projects_routes;
mod rebase;
mod routing;
mod schema;
mod steps;
