# Sparkle update signing

Keel uses Sparkle 2 EdDSA keys under Keychain account **`keel-suite`**.

## Keys (already generated on this Mac)

| Piece | Where |
| --- | --- |
| **Public** | [`public_ed_key.txt`](public_ed_key.txt) (committed; also baked into `.app` Info.plist as `SUPublicEDKey`) |
| **Private** | macOS Keychain account `keel-suite`, exported copy at `~/.config/keel/sparkle_private_ed_key.txt` (**never commit**) |

Appcast URL in bundles:

`https://github.com/Thravok/keel-suite/releases/latest/download/appcast.xml`

## Add the GitHub Actions secret

```bash
# From this machine (uses the exported private key file):
gh secret set SPARKLE_PRIVATE_KEY --repo Thravok/keel-suite < ~/.config/keel/sparkle_private_ed_key.txt
```

Optional override of the public key in CI (usually not needed once committed):

```bash
gh secret set SPARKLE_PUBLIC_ED_KEY --repo Thravok/keel-suite < packaging/sparkle/public_ed_key.txt
```

## Regenerate / export later

```bash
packaging/macos/fetch-sparkle.sh

# Print public key
packaging/sparkle/bin/generate_keys --account keel-suite -p

# Export private key (overwrite local export)
packaging/sparkle/bin/generate_keys --account keel-suite -x ~/.config/keel/sparkle_private_ed_key.txt

# Sign a DMG locally
packaging/sparkle/bin/sign_update --account keel-suite -p path/to/KeelDeck-….dmg
# or from file / CI secret:
echo "$SPARKLE_PRIVATE_KEY" | packaging/sparkle/bin/sign_update --ed-key-file - -p path/to/file.dmg
```

## New machine

1. Copy `~/.config/keel/sparkle_private_ed_key.txt` securely (or pull from the GitHub secret once).
2. Import: `packaging/sparkle/bin/generate_keys --account keel-suite -f ~/.config/keel/sparkle_private_ed_key.txt`
