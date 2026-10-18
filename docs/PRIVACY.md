# Privacy

Keel is designed to run **fully offline**.

- No telemetry, analytics, crash phone-home, or accounts
- No marketplace or cloud sync
- Profiles and settings are stored only under the OS app data directory (`state.json`)
- Network use only happens when **you** configure an action that opens a URL or an app that itself uses the network
- `run_command` spawns an explicit program path with arguments (no shell). Treat it as powerful local automation

HID access talks only to attached Elgato Stream Deck devices (USB VID `0x0FD9`).
