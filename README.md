# NordenVault Desktop

In the recommended Private mode, desktop backups use the bundled restic engine to encrypt file contents, names and snapshot metadata before upload. Each new device can create an independent repository in its organisation’s bucket. Customers can reconnect to an existing repository with its recovery key.

## Back up and restore

1. Sign in, register the device and choose **Private backup**. Save the key somewhere accessible without this device, then confirm it. The app keeps its local copy in macOS Keychain, Windows Credential Manager or Linux Secret Service. Linux needs a running, unlocked credential store.
2. Select folders. The app takes an encrypted snapshot after setup when files change and also checks the enabled folders every five minutes while running. An upgrade requires a mode choice before the next backup. Existing ordinary uploads retain their original protection and remain accessible through the dashboard.
3. On a replacement device, sign in to the same storage organisation. Select **Restore a backup**, enter the saved key, select a snapshot and choose a destination. The app restores into a new folder and verifies the recovered data. Losing all copies of the key prevents decryption. Resetting the NordenVault sign-in password restores account access only.

**Standard backup** is an explicit alternative. It uploads ordinary files to the organisation’s bucket over HTTPS for access through the web dashboard. Authorised storage administrators can read their contents and names. Switching from Private to Standard clears the folder selection so each folder to upload as ordinary files must be chosen again. Earlier copies keep their original protection.

The storage service can observe the account, bucket, encrypted object sizes, request times and repository structure. Sharing the recovery key with someone who has storage access gives them the ability to decrypt the backup. The privacy guarantee applies to Private mode snapshots. Browser uploads and earlier client releases have their own behaviour.

## Development

Install the Tauri system dependencies, Rust, Node.js and Python 3. Run `python3 scripts/prepare-restic.py` to download the pinned engine for the current target, verify its checksum and place it in the Tauri sidecar directory. Run `npm ci`, `npm run tauri dev` and `cargo test --manifest-path src-tauri/Cargo.toml --lib`. The release workflow builds signed update packages for Windows, Linux and both Mac architectures.

[Restic encryption and repository format](https://restic.readthedocs.io/en/stable/100_references.html)
