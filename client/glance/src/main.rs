//! `hummingbird-glance <mirror-dir>`: the glance host's process (ADR-0041).
//!
//! Wiring only. Every decidable part is in `lib.rs`, under test. This file
//! reads the two secrets from the macOS login Keychain, runs one sync cycle
//! every [`CYCLE_INTERVAL_SECS`], and publishes the result to Home
//! Assistant's broker.
//!
//! - **Secrets** rest in the login Keychain, service `hummingbird-glance`,
//!   accounts `device-token` and `mqtt-password`. Never in the plist, the
//!   environment or a file. With no token the host idles and re-reads the
//!   Keychain every cycle, so adding one needs no restart.
//! - **Settings** come from the environment: `HB_AUTHORITY_URL` (default
//!   `https://hb.twinion.net`), `GLANCE_MQTT_HOST` (default
//!   `192.168.1.134`), `GLANCE_MQTT_PORT` (1883) and `GLANCE_MQTT_USER`
//!   (`nest-mqtt-hummingbird`).
//! - **Publishing:** nothing until a cycle has completed in this process,
//!   so a fresh start never publishes an empty list it hasn't pulled. After
//!   that, a retained state after every attempt, carrying the time of the
//!   last completed cycle, so a failing pull reads as stale, not as empty.
//!   A refused token (401) publishes `offline` and holds until the Keychain
//!   holds a different token.

use std::process::Command;
use std::time::Duration;

use chrono::{Local, SecondsFormat};
use hummingbird_core::sync::{sync_client, CycleOutcome, ReqwestSyncTransport, Trigger};
use hummingbird_core::{Core, CoreCycleOutcome};
use hummingbird_glance::{
    discovery, frontier_in_display_order, state_payload, RefusingWrite, AVAILABILITY_TOPIC,
    CYCLE_INTERVAL_SECS, STATE_TOPIC,
};
use rumqttc::{AsyncClient, Event, LastWill, MqttOptions, Packet, QoS};

const KEYCHAIN_SERVICE: &str = "hummingbird-glance";

fn log(message: &str) {
    eprintln!("{} {message}", Local::now().to_rfc3339_opts(SecondsFormat::Secs, false));
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

/// One generic password from the login Keychain, or `None`. The value goes
/// straight into memory and is never logged.
fn keychain(account: &str) -> Option<String> {
    let output = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", KEYCHAIN_SERVICE, "-a", account, "-w"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!value.is_empty()).then_some(value)
}

async fn publish(client: &AsyncClient, topic: &str, payload: String) {
    if let Err(error) = client.publish(topic, QoS::AtLeastOnce, true, payload).await {
        log(&format!("mqtt publish to {topic} failed: {error}"));
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let Some(mirror_dir) = std::env::args().nth(1) else {
        eprintln!("usage: hummingbird-glance <mirror-dir>");
        std::process::exit(64);
    };
    let authority = env_or("HB_AUTHORITY_URL", "https://hb.twinion.net");
    let host = env_or("GLANCE_MQTT_HOST", "192.168.1.134");
    let port: u16 = env_or("GLANCE_MQTT_PORT", "1883").parse().unwrap_or(1883);
    let user = env_or("GLANCE_MQTT_USER", "nest-mqtt-hummingbird");
    let Some(mqtt_password) = keychain("mqtt-password") else {
        log("no MQTT password in the Keychain (service hummingbird-glance, account mqtt-password)");
        std::process::exit(78);
    };

    let mut core = match Core::init(mirror_dir).await {
        Ok(core) => core,
        Err(error) => {
            log(&format!("core init failed: {error}"));
            std::process::exit(74);
        }
    };

    let mut options = MqttOptions::new("hummingbird-glance", host, port);
    options.set_credentials(user, mqtt_password);
    options.set_keep_alive(Duration::from_secs(60));
    options.set_last_will(LastWill::new(AVAILABILITY_TOPIC, "offline", QoS::AtLeastOnce, true));
    let (client, mut eventloop) = AsyncClient::new(options, 16);

    // The event loop drives the connection. Discovery goes out on every
    // connect, so Home Assistant has it after either side restarts.
    let announcer = client.clone();
    tokio::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    log("mqtt connected");
                    for message in discovery() {
                        publish(&announcer, &message.topic, message.payload.to_string()).await;
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    log(&format!("mqtt: {error}; reconnecting"));
                    tokio::time::sleep(Duration::from_secs(10)).await;
                }
            }
        }
    });

    let read = ReqwestSyncTransport::new(sync_client(), authority);
    let write = RefusingWrite::default();
    let mut pushed_token: Option<String> = None;
    let mut last_completed: Option<String> = None;
    let mut first = true;
    let mut tick = tokio::time::interval(Duration::from_secs(CYCLE_INTERVAL_SECS));

    loop {
        tokio::select! {
            _ = tick.tick() => {}
            _ = tokio::signal::ctrl_c() => break,
        }

        // A token appearing, or changing after a 401, is picked up here.
        match keychain("device-token") {
            Some(token) if pushed_token.as_deref() != Some(token.as_str()) => {
                core.push_api_key(token.clone());
                pushed_token = Some(token);
                first = true;
            }
            Some(_) => {}
            None if pushed_token.is_none() => {
                core.report_no_api_key();
                log("no device token in the Keychain (service hummingbird-glance, account device-token)");
                continue;
            }
            None => {}
        }

        let now_ms = Local::now().timestamp_millis();
        let trigger = if first { Trigger::User } else { Trigger::Timer };
        let outcome = core.run(&read, &write, now_ms, trigger, first, 0.0).await;
        first = false;
        if write.refused() > 0 {
            log(&format!("refused {} mutation(s): the glance host never writes", write.refused()));
        }

        match outcome {
            CoreCycleOutcome::Held | CoreCycleOutcome::Cycle(CycleOutcome::CredentialNeeded { .. }) => {
                log("the authority refused the device token; publishing offline until it changes");
                publish(&client, AVAILABILITY_TOPIC, "offline".into()).await;
                continue;
            }
            CoreCycleOutcome::Cycle(CycleOutcome::Completed { .. }) => {
                last_completed = Some(Local::now().to_rfc3339_opts(SecondsFormat::Secs, false));
            }
            other => log(&format!("cycle did not complete: {other:?}")),
        }

        let Some(last_cycle) = last_completed.as_deref() else {
            continue;
        };
        let now = Local::now().format("%Y-%m-%dT%H:%M").to_string();
        let frontier = frontier_in_display_order(core.frontier());
        let today = core.today(&now);
        let state = state_payload(&frontier, &today, &now, last_cycle);
        match serde_json::to_string(&state) {
            Ok(json) => {
                publish(&client, STATE_TOPIC, json).await;
                publish(&client, AVAILABILITY_TOPIC, "online".into()).await;
            }
            Err(error) => log(&format!("state did not serialise: {error}")),
        }
    }

    publish(&client, AVAILABILITY_TOPIC, "offline".into()).await;
    let _ = client.disconnect().await;
}
