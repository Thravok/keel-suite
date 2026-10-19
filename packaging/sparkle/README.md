# Sparkle update signing

1. Download [Sparkle](https://github.com/sparkle-project/Sparkle/releases) and run `./bin/generate_keys`.
2. Put the **public** EdDSA key (single line) into `public_ed_key.txt`.
3. Store the **private** key as GitHub Actions secret `SPARKLE_PRIVATE_KEY` (contents of the private key file).
4. Release workflow signs DMG updates with `sign_update` and publishes `appcast.xml`.

Appcast URL used in bundles:

`https://github.com/Thravok/keel-suite/releases/latest/download/appcast.xml`
