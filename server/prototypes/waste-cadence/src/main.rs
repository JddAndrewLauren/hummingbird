//! PROTOTYPE — throwaway. Delete or absorb; see `NOTES.md` next to this file.
//!
//! A hand-drivable model of #120: the which-cans pane + the holiday-slide
//! alert. You are the clock and you are the city. Advance days, slide a
//! pickup for a holiday, correct the slide, dismiss the alert, and watch what
//! the daily poll does to the snapshot, the alert row and the pane.
//!
//! Everything decidable lives in `waste.rs`; this file is the shell.

mod date;
mod waste;

use date::Date;
use hummingbird_domain::Alert;
use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};
use waste::{
    judge, mint, occurrence_key, pane, Cadence, Deviation, Payload, StreamReading, SNAPSHOT_KEY,
    SOURCE,
};

const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const OFF: &str = "\x1b[0m";

/// The day starts at 06:00 local and every action costs a minute. A real
/// clock, however coarse, is load-bearing here: ADR-0014's `is_live` compares
/// `raised_at` against `dismissed_at`, so a re-raise stamped with the poll's
/// *nominal schedule slot* (a cron bucket, "today at 06:00") rather than the
/// actual write time lands at or before a dismissal made later that morning
/// and is silently swallowed. Stamp raises with the write clock.
const DAY_START_HOUR: i64 = 6;

struct StreamDef {
    name: &'static str,
    cadence: Cadence,
}

struct World {
    today: Date,
    streams: Vec<StreamDef>,
    /// The city page's mutations, keyed by (stream, originally scheduled
    /// date). This is the *page*, not our model of it — the adapter never
    /// sees this map, only the date the page prints.
    slides: BTreeMap<(String, i64), Date>,
    snapshot: Option<(Payload, i64)>,
    alerts: BTreeMap<String, Alert>,
    log: Vec<String>,
    /// Minutes elapsed since 06:00 today — the shell's whole clock.
    minutes: i64,
}

impl World {
    fn new() -> World {
        // Monday 2026-08-03 anchors trash (weekly) and recycling (fortnightly);
        // yard waste runs Wednesdays.
        let monday = Date::ymd(2026, 8, 3);
        let wednesday = Date::ymd(2026, 8, 5);
        World {
            today: Date::ymd(2026, 8, 10),
            streams: vec![
                StreamDef {
                    name: "trash",
                    cadence: Cadence {
                        anchor: monday,
                        every_n_weeks: 1,
                    },
                },
                StreamDef {
                    name: "recycling",
                    cadence: Cadence {
                        anchor: monday,
                        every_n_weeks: 2,
                    },
                },
                StreamDef {
                    name: "yard",
                    cadence: Cadence {
                        anchor: wednesday,
                        every_n_weeks: 1,
                    },
                },
            ],
            slides: BTreeMap::new(),
            snapshot: None,
            alerts: BTreeMap::new(),
            log: Vec::new(),
            minutes: 0,
        }
    }

    fn note(&mut self, s: String) {
        self.log.push(s);
        while self.log.len() > 6 {
            self.log.remove(0);
        }
    }

    /// The write/read clock. Monotone: each action costs a minute, each day
    /// resets to 06:00.
    fn now(&self) -> i64 {
        self.today.epoch_start() + DAY_START_HOUR * 3600 + self.minutes * 60
    }

    /// What the city page prints for one stream today.
    fn page_next(&self, s: &StreamDef) -> Date {
        let current = s.cadence.latest_on_or_before(self.today);
        let upcoming = s.cadence.next_on_or_after(self.today);
        if let Some(&to) = self.slides.get(&(s.name.to_string(), current.days())) {
            if to >= self.today {
                return to;
            }
        }
        if let Some(&to) = self.slides.get(&(s.name.to_string(), upcoming.days())) {
            return to;
        }
        upcoming
    }

    /// One daily poll: fetch, replace the snapshot wholesale, judge each
    /// stream, upsert whatever the judgment asks for.
    fn poll(&mut self) {
        let streams: Vec<StreamReading> = self
            .streams
            .iter()
            .map(|s| StreamReading {
                stream: s.name.to_string(),
                cadence: s.cadence,
                next_date: self.page_next(s),
            })
            .collect();
        let payload = Payload { streams };
        let now = self.now();

        let mut said = Vec::new();
        for reading in &payload.streams {
            let deviation = judge(reading, self.today);
            let key = occurrence_key(&reading.stream, deviation);
            let existing = key.as_ref().and_then(|k| self.alerts.get(k));
            let Some(up) = mint(&reading.stream, deviation, existing, self.today) else {
                continue;
            };

            match self.alerts.get_mut(&up.source_key) {
                Some(row) => {
                    // Value-identical upsert = no write at all. A daily poll
                    // of an unchanged slide would otherwise bump `version`
                    // every morning and push a meaningless delta to every
                    // device — the same no-op rule #221 landed for rules.
                    let identical = row.title == up.title
                        && row.body.as_deref() == Some(up.body.as_str())
                        && row.expires_at == Some(up.expires_at);
                    if identical && !up.restamp_raised_at {
                        continue;
                    }
                    row.title = up.title.clone();
                    row.body = Some(up.body.clone());
                    row.expires_at = Some(up.expires_at);
                    row.version += 1;
                    if up.restamp_raised_at {
                        row.raised_at = now;
                        said.push(format!("re-raised: {}", up.title));
                    } else {
                        said.push(format!("updated {} quietly", up.source_key));
                    }
                }
                None => {
                    self.alerts.insert(
                        up.source_key.clone(),
                        Alert {
                            id: format!("{}|{}", up.source, up.source_key),
                            source: up.source.to_string(),
                            source_key: up.source_key.clone(),
                            title: up.title.clone(),
                            body: Some(up.body.clone()),
                            url: None,
                            severity: Some("warn".to_string()),
                            raised_at: now,
                            resolved_at: None,
                            dismissed_at: None,
                            expires_at: Some(up.expires_at),
                            version: 1,
                        },
                    );
                    said.push(format!("raised {}", up.title));
                }
            }
        }

        self.snapshot = Some((payload, now));
        if said.is_empty() {
            self.note("poll: snapshot replaced, nothing to say".to_string());
        } else {
            self.note(format!("poll: {}", said.join("; ")));
        }
    }
}

fn stream_index(world: &World, arg: Option<&str>) -> Vec<usize> {
    match arg.and_then(|a| a.parse::<usize>().ok()) {
        Some(i) if i >= 1 && i <= world.streams.len() => vec![i - 1],
        _ => (0..world.streams.len()).collect(),
    }
}

fn slide(world: &mut World, which: Vec<usize>, extra_days: i64) {
    for i in which {
        let name = world.streams[i].name.to_string();
        let cadence = world.streams[i].cadence;
        // The cycle the page is CURRENTLY advertising, not the next cadence
        // date: once a slide has carried this week's pickup past its own
        // Monday, `next_on_or_after(today)` is next week — so pressing `c`
        // again would quietly move a different cycle instead of correcting
        // the one on screen, and the correction would never reach the live
        // alert.
        let current = cadence.latest_on_or_before(world.today);
        let sched = match world.slides.get(&(name.clone(), current.days())) {
            Some(&pending) if pending >= world.today => current,
            _ => cadence.next_on_or_after(world.today),
        };
        let base = *world
            .slides
            .get(&(name.clone(), sched.days()))
            .unwrap_or(&sched);
        world
            .slides
            .insert((name.clone(), sched.days()), base.add_days(extra_days));
        world.note(format!(
            "city page: {name} {} → {}",
            sched.short(),
            base.add_days(extra_days).short()
        ));
    }
}

fn skip(world: &mut World, which: Vec<usize>) {
    for i in which {
        let name = world.streams[i].name.to_string();
        let cadence = world.streams[i].cadence;
        let sched = cadence.next_on_or_after(world.today);
        let to = sched.add_days(cadence.period_days());
        world.slides.insert((name.clone(), sched.days()), to);
        world.note(format!(
            "city page: {name} skips {}, next {}",
            sched.short(),
            to.short()
        ));
    }
}

fn render(world: &World) {
    print!("\x1b[2J\x1b[H");
    println!(
        "{BOLD}which-cans prototype{OFF}  {DIM}#120 · source {SOURCE} · snapshot key {SNAPSHOT_KEY}{OFF}"
    );
    println!(
        "\n{BOLD}Today{OFF}  {}   {DIM}clock {} · one action = one minute{OFF}",
        world.today.short(),
        stamp(Some(world.now()))
            .split_once(' ')
            .map(|(_, t)| t.to_string())
            .unwrap_or_default()
    );

    println!("\n{BOLD}City page (you control this){OFF}");
    for (i, s) in world.streams.iter().enumerate() {
        let next = world.page_next(s);
        let on_cadence = s.cadence.latest_on_or_before(next) == next;
        println!(
            "  {DIM}{}{OFF} {:<10} next {}  {DIM}{}{}{OFF}",
            i + 1,
            s.name,
            next.short(),
            s.cadence.describe(),
            if on_cadence { "" } else { "  ← off cadence" }
        );
    }

    println!("\n{BOLD}context_snapshots{OFF}");
    match &world.snapshot {
        None => println!("  {DIM}(no snapshot yet — press p){OFF}"),
        Some((payload, fetched_at)) => {
            println!(
                "  {DIM}fetched_at {} ({}m ago){OFF}",
                fetched_at,
                (world.now() - fetched_at) / 60
            );
            println!("  {DIM}{}{OFF}", payload.to_json());
            print!("  judgments: ");
            let parts: Vec<String> = payload
                .streams
                .iter()
                .map(|r| match judge(r, world.today) {
                    Deviation::OnCadence => format!("{}=on-cadence", r.stream),
                    Deviation::Slide { .. } => format!("{BOLD}{}=slide{OFF}", r.stream),
                    Deviation::SkippedCycle { .. } => format!("{BOLD}{}=skipped{OFF}", r.stream),
                })
                .collect();
            println!("{}", parts.join("  "));
        }
    }

    println!("\n{BOLD}alerts{OFF} {DIM}(live = ADR-0014 is_live at read time){OFF}");
    if world.alerts.is_empty() {
        println!("  {DIM}(none){OFF}");
    }
    for a in world.alerts.values() {
        let live = a.is_live(world.now());
        println!(
            "  {}{:<24}{OFF} {}",
            if live { BOLD } else { DIM },
            a.source_key,
            a.title
        );
        println!(
            "    {DIM}raised {}  dismissed {}  expires {}  v{}  →{OFF} {}",
            stamp(Some(a.raised_at)),
            stamp(a.dismissed_at),
            stamp(a.expires_at),
            a.version,
            if live { "LIVE" } else { "quiet" }
        );
    }

    println!("\n{BOLD}the pane{OFF} {DIM}(read-time, never stored){OFF}");
    match &world.snapshot {
        None => println!("  {DIM}no data yet{OFF}"),
        Some((payload, fetched_at)) => {
            let view = pane(
                payload,
                &world.alerts,
                world.today,
                world.now(),
                *fetched_at,
            );
            for can in &view.cans {
                println!(
                    "  {}{:<10}{OFF} {}  {DIM}in {}d · {}{OFF}",
                    if can.goes_out_next { BOLD } else { DIM },
                    can.stream,
                    can.next_date.short(),
                    can.days_away,
                    can.cadence
                );
            }
            let out: Vec<&str> = view
                .cans
                .iter()
                .filter(|c| c.goes_out_next)
                .map(|c| c.stream.as_str())
                .collect();
            match view.next_date {
                Some(d) => println!(
                    "  {BOLD}→ out next: {} on {} ({}){OFF}",
                    out.join(" + "),
                    d.short(),
                    match d.days() - world.today.days() {
                        0 => "today".to_string(),
                        1 => "tomorrow".to_string(),
                        n => format!("in {n} days"),
                    }
                ),
                None => println!("  {DIM}nothing scheduled{OFF}"),
            }
            for note in &view.holiday_notes {
                println!("  {BOLD}! {note}{OFF}");
            }
            if view.holiday_notes.is_empty() {
                println!("  {DIM}(no holiday note — no live alert on this source){OFF}");
            }
            println!("  {DIM}as of {}m ago{OFF}", view.stale_minutes);
        }
    }

    if !world.log.is_empty() {
        println!("\n{DIM}{}{OFF}", world.log.join("\n"));
    }

    println!(
        "\n{BOLD}n{OFF}{DIM} next day (polls){OFF}  {BOLD}p{OFF}{DIM} poll again{OFF}  \
         {BOLD}h[i]{OFF}{DIM} holiday slide +1d{OFF}  {BOLD}c[i]{OFF}{DIM} correct +1d{OFF}  \
         {BOLD}s[i]{OFF}{DIM} skip cycle{OFF}"
    );
    println!(
        "{BOLD}r{OFF}{DIM} revert page{OFF}  {BOLD}d{OFF}{DIM} dismiss live{OFF}  \
         {BOLD}q{OFF}{DIM} quit{OFF}   {DIM}i = 1 trash, 2 recycling, 3 yard (blank = all){OFF}"
    );
    print!("> ");
    let _ = io::stdout().flush();
}

fn stamp(t: Option<i64>) -> String {
    match t {
        None => "-".to_string(),
        Some(t) => format!(
            "{} {:02}:{:02}",
            Date::from_days(t.div_euclid(86_400)).iso(),
            t.rem_euclid(86_400) / 3600,
            (t.rem_euclid(86_400) % 3600) / 60
        ),
    }
}

fn main() {
    let mut world = World::new();
    world.poll();

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    loop {
        render(&world);
        let Some(Ok(line)) = lines.next() else { break };
        let mut parts = line.split_whitespace();
        let cmd = parts.next().unwrap_or("");
        let arg = parts.next();
        let which = stream_index(&world, arg);

        world.minutes += 1;
        match cmd {
            "q" => break,
            "n" | "" => {
                world.today = world.today.add_days(1);
                world.minutes = 0;
                world.poll();
            }
            "p" => world.poll(),
            "h" | "c" => slide(&mut world, which, 1),
            "s" => skip(&mut world, which),
            "r" => {
                world.slides.clear();
                world.note("city page: reverted to cadence".to_string());
            }
            "d" => {
                let now = world.now();
                let mut n = 0;
                for a in world.alerts.values_mut() {
                    if a.is_live(now) {
                        a.dismissed_at = Some(now);
                        n += 1;
                    }
                }
                world.note(format!("human dismissed {n} live alert(s)"));
            }
            other => world.note(format!("unknown command {other:?}")),
        }
    }
}
