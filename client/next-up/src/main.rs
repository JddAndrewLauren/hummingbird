//! `next-up-rank`: stdin, stdout, serde. Nothing decidable lives here.
//!
//! The same split every poller in this repo keeps (`server/city-waste`,
//! `server/gmail-poll`, `server/calendar-poll`): everything worth testing
//! is in the lib and natively tested, and the binary holds only the I/O
//! that cannot be. It makes no HTTP call and holds no credential — the
//! survey fetch is `.claude/skills/next-up-hb/scripts/next-up.sh`'s job, and
//! the runner arm hands the payload straight in.

use std::io::{self, Read, Write};

use hummingbird_next_up::{run, Envelope};

fn main() {
    if let Err(message) = real_main() {
        eprintln!("next-up-rank: {message}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<(), String> {
    let mut raw = String::new();
    io::stdin()
        .read_to_string(&mut raw)
        .map_err(|e| format!("could not read stdin: {e}"))?;

    let envelope: Envelope =
        serde_json::from_str(&raw).map_err(|e| format!("could not parse the envelope: {e}"))?;

    let output = run(&envelope).map_err(|problem| problem.to_string())?;

    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &output)
        .map_err(|e| format!("could not write the result: {e}"))?;
    writeln!(stdout).map_err(|e| format!("could not write the result: {e}"))?;
    Ok(())
}
