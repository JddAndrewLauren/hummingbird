//! THE PORTABLE BIT. Pure functions over plain data — no I/O, no printing,
//! no clock of its own. The TUI in `main.rs` is the throwaway shell; this
//! module is the shape that would be lifted into the real `city-waste`
//! adapter (#120) if it survives being driven by hand.
//!
//! The question it answers, in one line: **given a daily poll of a static
//! city page, what does the snapshot payload have to carry, and what exactly
//! counts as a material change, so that a cadence roll-forward stays silent
//! and a holiday slide rings exactly once?**
//!
//! Three claims are encoded here, each of which the TUI is meant to attack:
//!
//! 1. **Materiality is deviation from cadence, not a diff against the
//!    previous snapshot.** [`judge`] never sees the last poll. That is the
//!    whole reason a roll-forward is silent: after Monday's pickup the page
//!    starts advertising next Monday, which is a *large* diff and a *zero*
//!    deviation. It also means the adapter is correct on its very first poll,
//!    with no snapshot to compare against, and stays correct across a lost or
//!    wiped snapshot.
//! 2. **The alert's identity is the scheduled date, and re-minting it must
//!    not re-stamp `raised_at`.** The poll is daily and the slide persists
//!    for days, so a naive upsert would re-raise every morning and undo the
//!    human's dismissal — ADR-0014's `is_live` compares `raised_at` against
//!    `dismissed_at`. [`mint`] therefore reports whether to restamp, and only
//!    a *change in the change itself* (a correction: Tue → Wed) restamps.
//! 3. **The pane joins by `source`, and computes at read time.** [`pane`]
//!    takes the snapshot and the live alerts and returns a view; nothing it
//!    derives is ever written back.

use hummingbird_domain::{city_waste_v1_key, Alert};
use std::collections::BTreeMap;

use crate::date::Date;

/// Both the snapshot's `source` and every alert's `source` — ADR-0009's join
/// constraint is that these are one string, so there is one constant.
///
/// `/v2` because `city-waste/v1` is retired in the frozen registry
/// (`server/domain/src/sources.rs`). Note that `/v2` is *not registered yet*:
/// the prototype is exactly the thing that decides what its payload and key
/// recipe are, and registering it is part of the real slice, not of this.
pub const SOURCE: &str = "city-waste/v2";

/// The `context_snapshots.key` for the one row this source owns. The whole
/// answer is one row, replaced wholesale by each poll.
pub const SNAPSHOT_KEY: &str = "collection";

/// A stream's collection rhythm: an anchor occurrence and a period. The
/// weekday is not stored — it is `anchor`'s weekday, and storing it
/// separately would let the two disagree.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cadence {
    pub anchor: Date,
    pub every_n_weeks: i64,
}

impl Cadence {
    pub fn period_days(&self) -> i64 {
        7 * self.every_n_weeks
    }

    pub fn next_on_or_after(&self, d: Date) -> Date {
        let p = self.period_days();
        let delta = d.days() - self.anchor.days();
        let mut k = delta.div_euclid(p);
        if delta.rem_euclid(p) != 0 {
            k += 1;
        }
        Date::from_days(self.anchor.days() + k * p)
    }

    pub fn latest_on_or_before(&self, d: Date) -> Date {
        let p = self.period_days();
        let delta = d.days() - self.anchor.days();
        Date::from_days(self.anchor.days() + delta.div_euclid(p) * p)
    }

    pub fn describe(&self) -> String {
        match self.every_n_weeks {
            1 => format!("every {}", self.anchor.weekday()),
            n => format!("every {n} weeks on {}", self.anchor.weekday()),
        }
    }
}

/// One stream as the page reports it. **Cadence and next date, both** — the
/// cadence is what makes the pane able to say "and then the week after," and
/// it is what makes materiality decidable without a previous snapshot.
#[derive(Clone, Debug)]
pub struct StreamReading {
    pub stream: String,
    pub cadence: Cadence,
    pub next_date: Date,
}

/// The whole `context_snapshots.payload`, replaced wholesale each poll.
#[derive(Clone, Debug)]
pub struct Payload {
    pub streams: Vec<StreamReading>,
}

impl Payload {
    /// Hand-rolled so the prototype can show the reader the literal TEXT
    /// column the client would parse. The real adapter uses serde.
    pub fn to_json(&self) -> String {
        let streams: Vec<String> = self
            .streams
            .iter()
            .map(|s| {
                format!(
                    r#"{{"stream":"{}","cadence":{{"anchor":"{}","every_n_weeks":{}}},"next_date":"{}"}}"#,
                    s.stream,
                    s.cadence.anchor.iso(),
                    s.cadence.every_n_weeks,
                    s.next_date.iso()
                )
            })
            .collect();
        format!(r#"{{"streams":[{}]}}"#, streams.join(","))
    }
}

/// What one reading deviates by. Computed from the reading alone plus today —
/// deliberately never from the previous snapshot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Deviation {
    /// The page's next date is exactly where the cadence puts it. Includes
    /// the roll-forward: the morning after a pickup the page jumps a whole
    /// week, which is a big diff and no deviation at all.
    OnCadence,
    /// The classic holiday slide: this cycle's collection moved later in the
    /// same period. `scheduled` is the fixed coordinate the key is built from.
    Slide { scheduled: Date, slides_to: Date },
    /// The page skipped a whole cycle — the next date is two or more periods
    /// out. A deviation-from-cadence rule that only looked at the offset
    /// inside a period would call this "on cadence" and say nothing, which is
    /// the loudest possible silence. `missed` is the collection that is not
    /// happening.
    SkippedCycle { missed: Date, next_seen: Date },
}

/// The materiality judgment. `today` is the caller's clock — injected, never
/// read here, same discipline as the rest of the server.
pub fn judge(reading: &StreamReading, today: Date) -> Deviation {
    let cadence = reading.cadence;
    let observed = reading.next_date;
    let scheduled = cadence.latest_on_or_before(observed);
    let offset = observed.days() - scheduled.days();

    // Which cycle is this, relative to the cycle we are currently inside?
    // 0 = this one (possibly slid past today), 1 = the ordinary roll-forward
    // after a completed pickup. 2+ = a cycle went missing.
    let current = cadence.latest_on_or_before(today);
    let cycles_ahead = (scheduled.days() - current.days()) / cadence.period_days();

    if cycles_ahead >= 2 {
        return Deviation::SkippedCycle {
            missed: cadence.next_on_or_after(today),
            next_seen: observed,
        };
    }
    if offset == 0 {
        return Deviation::OnCadence;
    }
    Deviation::Slide {
        scheduled,
        slides_to: observed,
    }
}

/// What an alert upsert would carry. Returned rather than performed, so the
/// decision is testable and the write stays in one place.
#[derive(Clone, Debug, PartialEq)]
pub struct AlertUpsert {
    pub source: &'static str,
    pub source_key: String,
    pub title: String,
    pub body: String,
    /// True only when this is genuinely a new occurrence *or* the change
    /// itself changed. False on every one of the daily re-polls in between —
    /// which is what keeps a dismissal dismissed.
    pub restamp_raised_at: bool,
    /// End of the *affected* collection date. Note which one that is: the
    /// later of scheduled and slid-to. Expiring at the end of the originally
    /// scheduled Monday would take the holiday text off the pane on the
    /// Tuesday morning it exists to warn about.
    pub expires_at: i64,
}

/// The `source_key` this deviation would land on, so a caller can look up
/// the existing row *before* deciding anything. ADR-0014's v1 recipe carried
/// forward: the ORIGINALLY scheduled date, so a correction to the slid-to
/// date lands on the same row rather than minting a second alert about one
/// holiday. `None` for an on-cadence reading — there is no occurrence.
pub fn occurrence_key(stream: &str, deviation: Deviation) -> Option<String> {
    let scheduled = match deviation {
        Deviation::OnCadence => return None,
        Deviation::Slide { scheduled, .. } => scheduled,
        Deviation::SkippedCycle { missed, .. } => missed,
    };
    Some(city_waste_v1_key(stream, &scheduled.iso()))
}

/// The alert decision for one stream's deviation, given whatever row already
/// exists under its key. `None` means "there is nothing to say" — which is
/// only ever the on-cadence case. An unchanged slide still returns a
/// `Some(..)`, because the daily poll must keep the row's content current;
/// what it does *not* do is set `restamp_raised_at`.
pub fn mint(
    stream: &str,
    deviation: Deviation,
    existing: Option<&Alert>,
    today: Date,
) -> Option<AlertUpsert> {
    let (scheduled, title, body, affected) = match deviation {
        Deviation::OnCadence => return None,
        Deviation::Slide {
            scheduled,
            slides_to,
        } => (
            scheduled,
            // The alert states the change itself, never that one happened.
            format!(
                "{stream}: {} → {} {}",
                scheduled.weekday(),
                slides_to.weekday(),
                when_phrase(scheduled, today)
            ),
            format!(
                "Collection normally {}. This cycle moves from {} to {}.",
                cadence_phrase(scheduled),
                scheduled.iso(),
                slides_to.iso()
            ),
            if slides_to > scheduled {
                slides_to
            } else {
                scheduled
            },
        ),
        Deviation::SkippedCycle { missed, next_seen } => (
            missed,
            format!(
                "{stream}: no {} collection {}",
                missed.weekday(),
                when_phrase(missed, today)
            ),
            format!(
                "The page shows no pickup for {}; next collection is {}.",
                missed.iso(),
                next_seen.iso()
            ),
            missed,
        ),
    };

    let source_key = city_waste_v1_key(stream, &scheduled.iso());

    let restamp = match existing {
        // No row yet: a new occurrence.
        None => true,
        // The change itself changed (a Tue → Wed correction): a new thing to
        // say, so it earns a fresh raise even over a dismissal. Anything
        // else — the same slide re-read on each of the next four mornings —
        // must not restamp, or the dismissal is undone daily.
        Some(row) => row.title != title || row.body.as_deref() != Some(body.as_str()),
    };

    Some(AlertUpsert {
        source: SOURCE,
        source_key,
        title,
        body,
        restamp_raised_at: restamp,
        expires_at: affected.epoch_end(),
    })
}

fn when_phrase(scheduled: Date, today: Date) -> String {
    let delta = scheduled.days() - today.days();
    if delta < 7 {
        "this week".to_string()
    } else if delta < 14 {
        "next week".to_string()
    } else {
        format!("the week of {}", scheduled.iso())
    }
}

fn cadence_phrase(scheduled: Date) -> String {
    format!("on {}", scheduled.weekday())
}

// --- the pane ----------------------------------------------------------

/// One can in the three-can graphic.
#[derive(Clone, Debug)]
pub struct Can {
    pub stream: String,
    pub next_date: Date,
    pub days_away: i64,
    /// Part of the next pickup — the cans that actually go to the kerb next.
    pub goes_out_next: bool,
    pub cadence: String,
}

/// Everything the pane renders, computed fresh at read time and never stored.
#[derive(Clone, Debug)]
pub struct PaneView {
    pub cans: Vec<Can>,
    pub next_date: Option<Date>,
    /// The join: titles of alerts that are live right now and carry the same
    /// `source` string as the snapshot.
    pub holiday_notes: Vec<String>,
    pub stale_minutes: i64,
}

/// The standing question, answered over the snapshot + the live alerts.
pub fn pane(payload: &Payload, alerts: &BTreeMap<String, Alert>, today: Date, now: i64, fetched_at: i64) -> PaneView {
    let next_date = payload.streams.iter().map(|s| s.next_date).min();

    let cans = payload
        .streams
        .iter()
        .map(|s| Can {
            stream: s.stream.clone(),
            next_date: s.next_date,
            days_away: s.next_date.days() - today.days(),
            goes_out_next: Some(s.next_date) == next_date,
            cadence: s.cadence.describe(),
        })
        .collect();

    let holiday_notes = alerts
        .values()
        .filter(|a| a.source == SOURCE && a.is_live(now))
        .map(|a| a.title.clone())
        .collect();

    PaneView {
        cans,
        next_date,
        holiday_notes,
        stale_minutes: (now - fetched_at) / 60,
    }
}
