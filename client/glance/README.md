# The glance host

A headless core that publishes the Frontier and Today to Home Assistant over
MQTT, for the office e-paper in the nest repo. ADR-0041 is the decision;
`src/lib.rs` and `src/main.rs` document themselves. It runs on Kestrel only,
and `deploy.sh` doesn't build or install it.

## Install on Kestrel

1. Build: `cd client && cargo build --release -p hummingbird-glance`, then copy
   `target/release/hummingbird-glance` to `~/.local/bin/`.
2. Put both secrets in the login Keychain. Never put them in the plist or a file.
   - The device token: mint `device-glance-kestrel` through the admin lane
     (`scripts/mint-device-token.sh`) and keep it in 1Password `Private` as
     `hummingbird-glance-device-token`. Then add it with
     `security add-generic-password -s hummingbird-glance -a device-token -w`,
     which prompts for the value.
   - The MQTT password: 1Password `dev` item `nest-mqtt-hummingbird` (username
     `hummingbird-glance`, already a Mosquitto login on the nest box). Add it
     with `security add-generic-password -s hummingbird-glance -a mqtt-password -w`.
3. Copy `net.twinion.hummingbird.glance.plist` to `~/Library/LaunchAgents/`,
   replacing `JOHN_HOME` with your home directory. Then run
   `launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/net.twinion.hummingbird.glance.plist`.
4. Check it: `tail -f ~/Library/Logs/hummingbird-glance.log` should show
   `mqtt connected`. In Home Assistant, the device "Hummingbird Glance" should
   appear with three sensors.

The host keeps its mirror in `~/Library/Application Support/hummingbird-glance/`.
Deleting that folder costs only a fresh sweep, because the host has no queue
worth keeping. The LaunchAgent keeps the process alive and sets no interval:
the host's own loop owns the five-minute cadence.

To revoke the token, use Signed-in devices in the app, or the admin lane.
