//! **The one unbuilt module, and it is deliberately isolated.** Reading the
//! council's HTML into a [`PageReading`] is the only part of this crate that
//! cannot be written without a sample of the real page, so it sits behind a
//! typed error with everything else — cadence, judgement, body, alert — built
//! and tested around it.
//!
//! What the rest of the crate needs from here is small and fixed: the zone,
//! the observed next collection date, which bins go out with it, and a
//! [`Cadence`]. The cadence is the part that depends on what the page
//! actually states, and there are three shapes it can arrive in:
//!
//! 1. **The page states the normal collection day.** Observe it directly.
//! 2. **The page lists several upcoming dates.** Derive the anchor and period
//!    from their weekday and spacing, fixture-tested.
//! 3. **The page gives only the next date.** Then the cadence cannot come
//!    from the page at all and has to come from a second binding value —
//!    which widens `city-waste-page` from a URL to a JSON object and reaches
//!    into `client/web/src/screens/bindings.ts`. **That case must be escalated
//!    before it is built**, because it would make the delivered client half a
//!    lie about being complete.
//!
//! Until a sample lands, [`parse`] answers [`PageError::NotImplemented`], the
//! binary exits on it without writing anything, and the fixture tests below
//! are `#[ignore]`d rather than deleted so the shape they expect is on
//! record.

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
    /// No sample of the real page yet — see this module's header.
    NotImplemented,
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
            PageError::NotImplemented => f.write_str(
                "the council page parser is not built yet — it needs a saved sample of the \
                 real page (see src/page.rs)",
            ),
            PageError::Missing(what) => write!(f, "the page does not state {what}"),
            PageError::BadDate(e) => write!(f, "the page carries an unreadable date: {e}"),
            PageError::NoDerivableCadence => {
                f.write_str("the page's dates do not describe a regular collection rhythm")
            }
        }
    }
}

impl std::error::Error for PageError {}

/// Reads one fetched page. `_html` is the response body verbatim.
pub fn parse(_html: &str) -> Result<PageReading, PageError> {
    Err(PageError::NotImplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Left `#[ignore]`d, not deleted: it records the shape `parse` owes the
    /// rest of the crate, so whoever lands the sample has the assertion
    /// waiting rather than inventing one. Un-ignore it with the fixture.
    #[test]
    #[ignore = "needs tests/fixtures/city-page-*.html — the one thing this slice is blocked on"]
    fn an_ordinary_week_reads_the_next_date_and_a_cadence() {
        // Read at runtime, not `include_str!`: a missing fixture must not
        // break the *build* of a test that is ignored precisely because the
        // fixture does not exist yet.
        let html = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/city-page-ordinary.html"),
        )
        .expect("the saved sample");
        let reading = parse(&html).expect("the ordinary page parses");
        assert_eq!(reading.collected_on.weekday(), reading.cadence.anchor.weekday());
        assert!(!reading.streams.is_empty());
        assert!(!reading.zone.is_empty());
    }

    #[test]
    #[ignore = "needs tests/fixtures/city-page-*.html"]
    fn a_holiday_week_reads_a_date_off_the_cadence() {
        let html = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/city-page-holiday.html"),
        )
        .expect("the saved sample");
        let reading = parse(&html).expect("the holiday page parses");
        assert_ne!(
            reading.cadence.latest_on_or_before(reading.collected_on),
            reading.collected_on,
            "a holiday week's observed date is off the lattice"
        );
    }

    /// This one runs today, because it is about the seam and not the HTML:
    /// until a sample lands, the parser must fail loudly and namefully, and
    /// the binary must exit on it without writing anything.
    #[test]
    fn until_a_sample_lands_the_parser_refuses_by_name() {
        assert_eq!(parse("<html></html>"), Err(PageError::NotImplemented));
        assert!(
            PageError::NotImplemented.to_string().contains("saved sample"),
            "the log must say what is missing"
        );
    }
}
