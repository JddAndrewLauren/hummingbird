//! The council's HTML, read into a [`PageReading`].
//!
//! This is the one module written against a saved sample of a real page
//! rather than against a specification, so it is isolated behind a typed
//! error and every failure names what it was looking for: everything else in
//! the crate — cadence, judgement, body, alert — is decidable without ever
//! having seen the page.
//!
//! # What the page actually says
//!
//! San Diego's Get It Done "Schedule Detail" page is server-rendered
//! Visualforce (no JavaScript is needed to see the answer) and states, per
//! stream, three things: a frequency word, the **standing collection day**,
//! and the next collection's date.
//!
//! ```text
//! Trash          Recyclables    Organics
//! Weekly         Biweekly       Weekly
//! Monday         Monday         Monday
//! 08/10/2026     08/10/2026     08/10/2026
//! ```
//!
//! That is the best of the three shapes this module's original note weighed:
//! the page states the normal collection day outright, so the [`Cadence`] is
//! observed rather than inferred, and nothing has to widen `city-waste-page`
//! from a URL into a second binding value.
//!
//! # From three streams to one collection
//!
//! The corrected domain has **one** collection day, with the bins going out
//! that day as a property of it ([`crate::cadence`]) — and the page agrees:
//! every stream names the same day. The streams differ only in *which weeks*
//! they are out, which is exactly the which-cans question. So:
//!
//! * `collected_on` is the **earliest** date any stream advertises, and
//!   `streams` is the set of streams sharing it — on a week the biweekly bin
//!   stays in, that set is smaller, and that is the whole answer;
//! * `every_n_weeks` is the **shortest** period across the streams, because
//!   a weekly stream makes collection days weekly whatever the others do;
//! * the anchor is `collected_on` snapped to the nearest stated collection
//!   day, so a holiday week — whose observed date is off the lattice — still
//!   yields the lattice it moved away from.
//!
//! # The one assumption a real holiday week will settle
//!
//! The cadence's weekday is read from the page's stated collection day, on
//! the reading that "Weekly / Monday" is the standing arrangement and the
//! date below it is this cycle's instance. **No holiday week has been
//! observed yet to confirm it.** If the page instead moves that label with
//! the slide — printing "Tuesday" on the week it is collected on a Tuesday —
//! then the anchor moves with it, the observed date lands back on the
//! lattice, and the lane reads the holiday as an ordinary week: `scheduled`
//! equals `collected_on`, the pane says nothing unusual and no alert rings.
//! That failure is **quiet**, which is the one thing this design otherwise
//! avoids everywhere — and it cannot be fixed from the page alone, because
//! distinguishing "moved this week" from "moved permanently" would need the
//! previous snapshot, which [`crate::judge`] deliberately never sees. It is
//! recorded here to be checked against the first real holiday.
//!
//! # Why there is no HTML parser dependency
//!
//! Three fields, read off three ids the page has given its own names
//! (`trash-date`, `recycle-date`, `organics-date`), which is a far better
//! anchor than any tree walk: the surrounding element ids are Visualforce's
//! generated `j_id0:j_id74` sort, which change whenever the page is
//! recompiled and must never be depended on. A tag-stripping scan between
//! two known markers is the whole job, and it keeps a crate that already
//! carries an HTTP client and a tzdb from also carrying an HTML5 tree
//! builder. Every marker it looks for is a [`PageError::Missing`] when
//! absent, so a page that has genuinely been redesigned fails loudly on the
//! first poll rather than writing something plausible.

use std::fmt;

use crate::cadence::Cadence;
use crate::date::{Date, DateError};

/// Everything one fetch of the council's page yields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageReading {
    /// The IANA zone the address's days are civil in. From the page if it
    /// says so; otherwise a constant this module owns, because the address
    /// is fixed and its zone is not a per-poll fact.
    pub zone: String,
    /// The next collection the page advertises. **Observed** — never
    /// adjusted, never reconciled with the cadence here; that is
    /// [`crate::judge`]'s job and keeping them apart is what makes a holiday
    /// derivable.
    pub collected_on: Date,
    /// Which bins go out with it. A property of the collection, never part
    /// of its identity.
    pub streams: Vec<String>,
    pub cadence: Cadence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageError {
    /// The fetch succeeded but the page did not contain what this parser
    /// needs. Carries what was looked for, so the log names it.
    Missing(&'static str),
    /// A date on the page could not be read.
    BadDate(DateError),
    /// The page's dates do not describe a regular rhythm, so no cadence can
    /// be derived — a real change to the council's page, and loud.
    NoDerivableCadence,
}

impl fmt::Display for PageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PageError::Missing(what) => write!(f, "the page does not state {what}"),
            PageError::BadDate(e) => write!(f, "the page carries an unreadable date: {e}"),
            PageError::NoDerivableCadence => {
                f.write_str("the page's dates do not describe a regular collection rhythm")
            }
        }
    }
}

impl std::error::Error for PageError {}

/// The address's zone. The page does not state one — it is a single-city
/// service — and the address is fixed by the binding, so this is a constant
/// rather than a per-poll reading. Named here because every day-shaped
/// question downstream resolves against it, and a silent UTC would be the
/// wrong answer dressed as an answer.
const ZONE: &str = "America/Los_Angeles";

/// The page's own element ids, and the pane's stream name for each.
///
/// These ids are hand-written into the page's markup, unlike the generated
/// `j_id0:*` ones around them. The stream names on the right are
/// `waste.ts`'s closed vocabulary (`trash | recycling | yard`) — note that
/// the city's "Organics" is the pane's `yard`, the green bin; the two words
/// name the same can, and `tests/contract.rs` is what keeps this side inside
/// the vocabulary the pane will accept.
///
/// Order is kerb order, which is also the order the page prints them in.
const BLOCKS: &[(&str, &str)] =
    &[("trash-date", "trash"), ("recycle-date", "recycling"), ("organics-date", "yard")];

/// One stream's row, as the page states it.
struct Block {
    stream: &'static str,
    /// The **standing** collection day, not the observed date's weekday.
    /// Days since the epoch, mod 7 — the same encoding [`Date`] uses
    /// internally, so snapping is one subtraction.
    weekday: i64,
    collected_on: Date,
    every_n_weeks: i64,
}

/// Reads one fetched page. `html` is the response body verbatim.
pub fn parse(html: &str) -> Result<PageReading, PageError> {
    let mut blocks = Vec::new();
    for (anchor, stream) in BLOCKS {
        // A stream can be legitimately absent: the page wraps the Organics
        // column in a conditional, so an address without that service simply
        // has no such block. Absence is skipped, not an error — the loud
        // case is having none at all.
        if let Some(block) = read_block(html, anchor, stream)? {
            blocks.push(block);
        }
    }
    let Some(first) = blocks.first() else {
        return Err(PageError::Missing("any collection schedule this parser can read"));
    };

    // One collection day for the address is the domain's own claim
    // (`cadence.rs`); the page agreeing with it is not something to assume.
    let weekday = first.weekday;
    if blocks.iter().any(|b| b.weekday != weekday) {
        return Err(PageError::NoDerivableCadence);
    }

    // The soonest collection is the one being answered, and the bins sharing
    // it are the answer.
    let collected_on = blocks.iter().map(|b| b.collected_on).min().expect("non-empty");
    let streams: Vec<String> = blocks
        .iter()
        .filter(|b| b.collected_on == collected_on)
        .map(|b| b.stream.to_string())
        .collect();

    // A weekly stream makes the collection day weekly whatever the others
    // do, so the rhythm of *collection days* is the shortest of them.
    let every_n_weeks = blocks.iter().map(|b| b.every_n_weeks).min().expect("non-empty");
    let anchor = snap_to_weekday(collected_on, weekday);

    // With a longer period the lattice has a phase as well as a weekday, and
    // the streams have to agree about it. Each stream's own scheduled date
    // must land on the anchor's lattice; one that does not means the page is
    // describing something this cadence cannot express, and inventing a
    // rhythm for it would put a wrong bin day on the pane.
    let period_days = 7 * every_n_weeks;
    for block in &blocks {
        let scheduled = snap_to_weekday(block.collected_on, weekday);
        if (scheduled.days() - anchor.days()).rem_euclid(period_days) != 0 {
            return Err(PageError::NoDerivableCadence);
        }
    }

    Ok(PageReading {
        zone: ZONE.to_string(),
        collected_on,
        streams,
        cadence: Cadence { anchor, every_n_weeks },
    })
}

/// The nearest date with the given weekday, in either direction. Ties cannot
/// happen — seven is odd, so one direction is always strictly closer.
fn snap_to_weekday(d: Date, weekday: i64) -> Date {
    let forward = (weekday - d.days()).rem_euclid(7);
    if forward <= 3 {
        d.add_days(forward)
    } else {
        d.add_days(forward - 7)
    }
}

/// Reads one stream's column, or `None` if the page has no such column.
fn read_block(html: &str, anchor: &str, stream: &'static str) -> Result<Option<Block>, PageError> {
    let marker = format!("id=\"{anchor}\"");
    let Some(at) = html.find(&marker) else {
        return Ok(None);
    };

    // The frequency word sits between this column's heading and its dated
    // block. Bounding it by the heading rather than by a class is what keeps
    // it off the generated ids: `<h3>` is the page's own structure, the
    // `<span id="j_id0:j_id71">` wrapped around "Biweekly" is not.
    let heading_end = html[..at]
        .rfind("</h3>")
        .ok_or(PageError::Missing("a heading above its collection frequency"))?;
    let every_n_weeks = read_frequency(&html[heading_end..at])?;

    // The column ends at the next heading, or at the end of the page body
    // for the last one. Bounding it matters: without an end, a column
    // missing its own date would silently read the *next* column's.
    let rest = &html[at..];
    let end = [rest.find("<h3"), rest.find("</main")]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(rest.len());
    let column = &rest[..end];

    let day_at = column
        .find("class=\"day\"")
        .ok_or(PageError::Missing("a collection day for one of its bins"))?;
    let date_at = column
        .find("class=\"date\"")
        .ok_or(PageError::Missing("a collection date for one of its bins"))?;
    if date_at < day_at {
        return Err(PageError::Missing("its collection day before its collection date"));
    }
    let date_end = column[date_at..]
        .find("</p>")
        .map(|i| date_at + i)
        .ok_or(PageError::Missing("a closed collection date"))?;

    Ok(Some(Block {
        stream,
        weekday: read_weekday(after_tag(&column[day_at..date_at]))?,
        collected_on: read_date(after_tag(&column[date_at..date_end]))?,
        every_n_weeks,
    }))
}

/// Both fields are found by their `class="…"` attribute, so the fragment
/// starts *inside* the opening tag. Dropping everything up to that tag's own
/// `>` is what keeps the attribute text out of the reading — without it the
/// weekday reads as `class="day" Monday`.
fn after_tag(fragment: &str) -> &str {
    match fragment.find('>') {
        Some(i) => &fragment[i + 1..],
        None => fragment,
    }
}

/// "Weekly" / "Biweekly", as the page prints them. Matched case-insensitively
/// and on a contains, because the fragment carries the surrounding markup's
/// whitespace and the page has moved this word in and out of a `<span>`
/// before.
fn read_frequency(fragment: &str) -> Result<i64, PageError> {
    let text = visible_text(fragment).to_ascii_lowercase();
    // Checked before "weekly", which is a substring of it.
    if text.contains("biweekly") || text.contains("bi-weekly") || text.contains("every other week")
    {
        return Ok(2);
    }
    if text.contains("weekly") {
        return Ok(1);
    }
    Err(PageError::Missing("a collection frequency this parser recognises"))
}

fn read_weekday(fragment: &str) -> Result<i64, PageError> {
    let text = visible_text(fragment);
    // Compared against `Date`'s own names rather than a second list, so the
    // two can never disagree about which day a number is.
    for offset in 0..7 {
        let day = Date::from_days(offset);
        if text.eq_ignore_ascii_case(day.weekday()) {
            return Ok(offset);
        }
    }
    Err(PageError::Missing("a collection day this parser recognises"))
}

/// `MM/DD/YYYY`, as the page prints it, through [`Date::parse`] — which is
/// what rejects a well-shaped non-day like `02/30/2026` rather than quietly
/// normalising it into March.
fn read_date(fragment: &str) -> Result<Date, PageError> {
    let text = visible_text(fragment);
    let bad = || PageError::BadDate(DateError::NotIso(text.clone()));
    let (m, rest) = text.split_once('/').ok_or_else(bad)?;
    let (d, y) = rest.split_once('/').ok_or_else(bad)?;
    if m.len() != 2 || d.len() != 2 || y.len() != 4 {
        return Err(bad());
    }
    Date::parse(&format!("{y}-{m}-{d}")).map_err(PageError::BadDate)
}

/// Everything outside the tags, with runs of whitespace collapsed to one
/// space and the result trimmed. Enough for three short fields between two
/// known markers, and deliberately not an HTML parser.
fn visible_text(fragment: &str) -> String {
    let mut out = String::with_capacity(fragment.len());
    let mut inside_tag = false;
    for ch in fragment.chars() {
        match ch {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if inside_tag => {}
            _ if ch.is_whitespace() => {
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            }
            _ => out.push(ch),
        }
    }
    // The page separates fields with `&nbsp;` runs in places; treating the
    // entity as a space keeps a trailing one from reaching `Date::parse`.
    out.replace("&nbsp;", " ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixtures are reduced and sanitised captures — see the header
    /// comment inside each one. Read at runtime rather than through
    /// `include_str!` so the sample stays a test input rather than something
    /// baked into the binary.
    fn fixture(name: &str) -> String {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path}: {e}"))
    }

    #[test]
    fn an_ordinary_week_reads_the_next_date_and_a_cadence() {
        let reading = parse(&fixture("city-page-ordinary.html")).expect("the ordinary page parses");
        assert_eq!(reading.collected_on.weekday(), reading.cadence.anchor.weekday());
        assert!(!reading.streams.is_empty());
        assert!(!reading.zone.is_empty());
    }

    /// The real capture, pinned exactly: every stream out together on the
    /// Monday, weekly, in the address's zone.
    #[test]
    fn the_captured_page_reads_the_collection_it_actually_advertises() {
        let reading = parse(&fixture("city-page-ordinary.html")).unwrap();
        assert_eq!(reading.collected_on.iso(), "2026-08-10");
        assert_eq!(reading.streams, ["trash", "recycling", "yard"]);
        assert_eq!(reading.cadence, Cadence { anchor: reading.collected_on, every_n_weeks: 1 });
        assert_eq!(reading.zone, "America/Los_Angeles");
    }

    /// **The which-cans case, and the reason the lane exists.** On a week the
    /// biweekly bin stays in, the page's three columns disagree about the
    /// date — and the answer is the soonest one, with only the bins sharing
    /// it. The collection day stays weekly, because two of the three streams
    /// are.
    #[test]
    fn a_week_the_biweekly_bin_stays_in_answers_with_the_smaller_set() {
        let reading = parse(&fixture("city-page-recycling-off-week.html")).unwrap();
        assert_eq!(reading.collected_on.iso(), "2026-08-17");
        assert_eq!(reading.streams, ["trash", "yard"], "the recyclables are not out this week");
        assert_eq!(reading.cadence.every_n_weeks, 1, "collection days are still weekly");
    }

    #[test]
    fn a_holiday_week_reads_a_date_off_the_cadence() {
        let reading = parse(&fixture("city-page-holiday.html")).expect("the holiday page parses");
        assert_ne!(
            reading.cadence.latest_on_or_before(reading.collected_on),
            reading.collected_on,
            "a holiday week's observed date is off the lattice"
        );
    }

    /// The whole point of reading the *stated* day rather than the observed
    /// date's weekday: the anchor is the Monday the collection moved away
    /// from, so `judge` sees a one-day slide of a real cycle.
    #[test]
    fn a_holiday_week_anchors_on_the_day_it_moved_away_from() {
        use crate::judge::{judge, Deviation};

        let reading = parse(&fixture("city-page-holiday.html")).unwrap();
        assert_eq!(reading.collected_on.iso(), "2026-09-08", "the Tuesday it is collected");
        assert_eq!(reading.cadence.anchor.iso(), "2026-09-07", "Labor Day, the Monday it left");
        assert_eq!(
            judge(reading.cadence, reading.collected_on, Date::parse("2026-09-05").unwrap()),
            Deviation::Slide {
                scheduled: Date::parse("2026-09-07").unwrap(),
                slides_to: reading.collected_on,
            }
        );
    }

    /// The snap is the anchor's whole derivation, so it is tested as
    /// arithmetic rather than only through a fixture — including the
    /// backward slide, where the nearest stated day is *ahead* of the
    /// observed one.
    #[test]
    fn a_date_snaps_to_the_nearest_stated_collection_day_in_either_direction() {
        let monday = Date::parse("2026-08-17").unwrap();
        let weekday = monday.days().rem_euclid(7);
        for (observed, expected) in [
            ("2026-08-17", "2026-08-17"), // the day itself
            ("2026-08-18", "2026-08-17"), // one late
            ("2026-08-20", "2026-08-17"), // three late, the far arm
            ("2026-08-21", "2026-08-24"), // four late is three early of the next
            ("2026-08-15", "2026-08-17"), // two early
        ] {
            assert_eq!(
                snap_to_weekday(Date::parse(observed).unwrap(), weekday).iso(),
                expected,
                "{observed}"
            );
        }
    }

    /// A redesigned page must fail by name on the first poll, never write a
    /// plausible answer. The binary exits on any of these having written
    /// nothing.
    #[test]
    fn a_page_without_the_schedule_refuses_by_name() {
        assert_eq!(
            parse("<html><body><h1>Service unavailable</h1></body></html>"),
            Err(PageError::Missing("any collection schedule this parser can read"))
        );
        assert!(
            PageError::Missing("a collection day this parser recognises")
                .to_string()
                .contains("collection day"),
            "the log must say what is missing"
        );
    }

    /// One column losing its date must not silently borrow the next
    /// column's — the reason each column is bounded at the following
    /// heading.
    #[test]
    fn a_column_missing_its_date_does_not_read_the_next_columns() {
        let html = fixture("city-page-recycling-off-week.html").replace(
            "<p class=\"date\"> 08/17/2026",
            "<p class=\"pending\"> 08/17/2026",
        );
        assert_eq!(
            parse(&html),
            Err(PageError::Missing("a collection date for one of its bins"))
        );
    }

    /// The streams have to agree about the collection day. If the page ever
    /// starts naming two, this cadence cannot express it, and inventing one
    /// would put a wrong bin day on the pane.
    #[test]
    fn columns_naming_different_collection_days_are_loud() {
        let html = fixture("city-page-ordinary.html")
            .replace("<p class=\"day\"> Monday", "<p class=\"day\"> Thursday");
        assert_eq!(parse(&html), Err(PageError::NoDerivableCadence));
    }

    #[test]
    fn an_unreadable_date_is_named_rather_than_normalised() {
        let html = fixture("city-page-ordinary.html").replace("08/10/2026", "02/30/2026");
        assert_eq!(
            parse(&html),
            Err(PageError::BadDate(DateError::NotACalendarDay("2026-02-30".into())))
        );
    }

    #[test]
    fn a_frequency_this_parser_cannot_read_is_refused() {
        let html = fixture("city-page-ordinary.html").replace("<p>Weekly</p>", "<p>Occasionally</p>");
        assert_eq!(
            parse(&html),
            Err(PageError::Missing("a collection frequency this parser recognises"))
        );
    }

    /// Both spellings of the fortnightly bin, and the whitespace the page
    /// actually wraps them in.
    #[test]
    fn the_frequency_words_the_page_uses_are_read_through_their_markup() {
        assert_eq!(read_frequency("</h3>\n <p>\n Weekly\n </p>\n").unwrap(), 1);
        assert_eq!(
            read_frequency("</h3><p><span id=\"j_id0:j_id71\">\n Biweekly</span>\n</p>").unwrap(),
            2
        );
        assert_eq!(read_frequency("</h3><p>Every other week</p>").unwrap(), 2);
    }
}
