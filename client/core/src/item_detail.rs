//! Everything the system knows about one item, gathered in one place —
//! `CONTEXT.md`'s **Item detail**, assembled here rather than in a seam
//! (ADR-0027).
//!
//! **Why the assembly is core's and not the seam's.** Which blockers count,
//! which alert is *the* alert, how a project name resolves, what an unseen
//! blocker id does — these are answers two clients must not disagree about,
//! which is ADR-0025's whole test, and they are testable in this crate's
//! native harness instead of only through generated bindings. The seam maps
//! [`ItemDetail`] to a record and adds nothing; the web's item detail, when
//! it comes, inherits the same answers.
//!
//! **Derived at read time, never stored** — like the Frontier and the
//! Ledger. Nothing here is a new mirror entity; every field is a read the
//! mirror already served, joined.
//!
//! This is the first place the task lane and the alert lane meet on one
//! record. The join is deliberately one-directional and narrow: an item
//! detail knows the live alert about it, and no alert record grows an item
//! field.

use hummingbird_domain::{Alert, Item, Step};

use crate::decisions::skills::MicrotaskAffordance;
use crate::ItemAction;

/// The project an item belongs to, named if the mirror has seen it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRef {
    pub id: String,
    /// `None` when this device's mirror has no row for `id` yet — the
    /// caller renders the bare id rather than pretending there is no
    /// project.
    pub name: Option<String>,
}

/// One open blocker holding the item back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockerEntry {
    pub item_id: String,
    /// `None` for an id this device's mirror has not seen. The row is still
    /// listed — see [`ItemDetail::open_blockers`].
    pub title: Option<String>,
}

/// One item, with everything the mirror knows joined onto it.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemDetail {
    /// The item itself, overlay applied — a not-yet-confirmed edit shows
    /// here the instant it is enqueued. Read from the *retained* roster, so
    /// an archived item still has a detail to open.
    pub item: Item,
    pub project: Option<ProjectRef>,
    /// The checklist, position order. **Read-only and deliberately not
    /// overlaid**: no step mutation exists on this seam yet, so there is no
    /// optimistic step to overlay. Step-ticking, when it lands, finds an
    /// overlay seam already built for items and extends it — it does not
    /// discover this line was load-bearing.
    pub steps: Vec<Step>,
    /// Every *open* blocker: live (not archived) and not `Done`.
    ///
    /// **An id the mirror has not seen is listed, never dropped.** A count
    /// that silently understates what holds an item back is worse than a
    /// row rendering as a bare id: the reader can chase an id, but cannot
    /// chase a blocker they were never shown.
    pub open_blockers: Vec<BlockerEntry>,
    /// The live `item-threshold/v1` alert about this item, if one is
    /// ringing. At most one by construction: that source is a *state*
    /// source (ADR-0014), so one item has exactly one such row across its
    /// whole life.
    pub live_alert: Option<Alert>,
    pub is_archived: bool,
    /// Whether this item may be edited — `CONTEXT.md`'s **Recall** rule
    /// (#478) reached through a second door: history stays readable, never
    /// editable. Acking is not gated on it, because silencing something
    /// still ringing is not editing history.
    pub is_editable: bool,
    /// The act vocabulary this item currently offers.
    /// **Empty for an archived item**, whatever stage its row still
    /// carries: `available_actions` answers "what can be done to a live
    /// item in this stage", and an archived row is not one. A client that
    /// re-derived this from `stage` alone would offer Complete on a
    /// cancelled item.
    pub available_actions: Vec<ItemAction>,
    /// Whether this item may be marked Done in one click —
    /// [`crate::decisions::can_mark_done`] verbatim.
    ///
    /// **It rides separately from [`ItemDetail::available_actions`] for the
    /// same reason it does on `TriageItemRecord` (ffi-mobile):**
    /// `available_actions` answers `&[]` for Triage and Grilling ("neither
    /// is action yet"), so a surface that derived the checkmark from that
    /// vocabulary would never offer it on the two stages whose detail pane
    /// most needs it. `can_mark_done` is the wider, deliberately-separate
    /// rule. The one consequence: a client rendering both must filter
    /// `Complete` out of `available_actions` so the affordance is not drawn
    /// twice.
    pub can_mark_done: bool,
    /// #539's applied result: which microtask gesture (`Break`/`Rewrite`)
    /// this item's own steps make legal, decided by
    /// [`crate::decisions::skills::microtask_affordance`]. `None` for a
    /// non-[`ItemDetail::is_editable`] item — an archived item has no live
    /// gesture to offer, the same rule [`ItemDetail::available_actions`]
    /// already applies to itself. **Kotlin is told whether to offer the
    /// affordance, not how to decide** (ADR-0025): no eligibility logic of
    /// its own may re-derive this.
    pub microtask_affordance: Option<MicrotaskAffordance>,
}
