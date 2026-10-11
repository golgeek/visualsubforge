# macOS signing setup

This guide prepares credentials for VSF releases built on GitHub Actions. You
only create/export the signing identity on your Mac; release builds do not need
to run locally. The same identity can sign both Intel and Apple Silicon builds.

Status, 2026-10-11: the six repository secrets are configured. The manual macOS
packaging workflow now has a separate signing job that consumes them when
`sign` is selected on `main`. Hosted signing/notarization has not yet passed;
the build must first produce its validation artifact. This workflow does not
publish a release. See [release preparation](releases.md).

## 1. Choose the certificate

Use **Developer ID Application** for the planned app/DMG download outside the
Mac App Store. Apple Development is a different certificate. Developer ID
Installer is for `.pkg` installers and is not needed for this distribution.
This is Apple's signing and notarization process, not App Store review.
[Apple's Developer ID guide](https://developer.apple.com/help/account/certificates/create-developer-id-certificates/)
describes the certificate types and requires the Account Holder role for this
portal workflow.

First check for an existing identity:

```sh
security find-identity -v -p codesigning
```

If a valid `Developer ID Application: Your Name (TEAMID)` is already present
with its private key in Keychain Access, reuse it and go to step 4. Otherwise,
create one below. Obtain your **Team ID** from Membership details in your
[Apple Developer account](https://developer.apple.com/account/); do not infer it
from an Apple Development identity's name.

## 2. Prepare local storage and a certificate request

Keep the export outside the Git checkout. Our recommended location is:

```text
~/Library/Application Support/VisualSubForge/signing/
```

Create that directory in Terminal:

```sh
mkdir -p "$HOME/Library/Application Support/VisualSubForge/signing"
chmod 700 "$HOME/Library/Application Support/VisualSubForge/signing"
```

In **Keychain Access** (find it with Spotlight):

1. Select the **login** keychain.
2. Choose **Keychain Access → Certificate Assistant → Request a Certificate
   from a Certificate Authority**.
3. Enter your email and a descriptive Common Name, such as `VSF Developer ID`.
   Leave CA Email Address empty.
4. Select **Saved to disk** and save `VisualSubForge.certSigningRequest` in the
   directory above. In a file dialog, **Shift–Command–G** lets you paste its path.

This creates the key pair on this Mac. The request is uploaded to Apple; the
private key stays in Keychain. See [Apple's CSR instructions](https://developer.apple.com/help/account/certificates/create-a-certificate-signing-request/).

## 3. Obtain and install the certificate

1. Open [Certificates, Identifiers & Profiles](https://developer.apple.com/account/resources/certificates/list).
2. Add a certificate with **+** and choose **Developer ID Application**.
3. Upload `VisualSubForge.certSigningRequest`, continue, then download the `.cer`.
4. Double-click the `.cer` to install it on the same Mac. Keep a copy in the
   signing directory if desired.
5. In Keychain Access → **login → My Certificates**, expand the new certificate.
   It must have an associated **private key** underneath it.

Run the identity command from step 1 again. Copy the complete
`Developer ID Application: … (…)` text, without the surrounding quotes or the
leading hash. This will become `APPLE_SIGNING_IDENTITY`.

If the private key is missing, the downloaded `.cer` alone cannot sign: use the
Mac that created the CSR or import its certificate/private-key export.
[Apple documents installation here](https://developer.apple.com/help/account/certificates/create-developer-id-certificates/).

## 4. Export an encrypted copy for GitHub Actions

In **My Certificates**, select the Developer ID Application identity, including
its private key, and choose **File → Export Items**. Export as **Personal
Information Exchange (.p12)** to:

```text
~/Library/Application Support/VisualSubForge/signing/developer-id-application.p12
```

Set a strong, new **export password**, and retain it in your password manager.
macOS may separately ask for your login Keychain password to authorize the
export. Those are two different passwords. The export password becomes
`APPLE_CERTIFICATE_PASSWORD`; your login password is never uploaded.

```sh
chmod 600 "$HOME/Library/Application Support/VisualSubForge/signing/developer-id-application.p12"
```

The `.p12` contains the private key. Keep an encrypted backup and its password
so the identity is recoverable. A `.cer` export without the key is insufficient.
See [Tauri's certificate export instructions](https://v2.tauri.app/distribute/sign/macos/).

## 5. Prepare notarization credentials

At [account.apple.com](https://account.apple.com/), open **Sign-In and Security →
App-Specific Passwords** and create one named `VisualSubForge notarization`.
This requires two-factor authentication. Save the generated password in your
password manager; it becomes `APPLE_APP_SPECIFIC_PASSWORD`. Use this generated
password, **not your normal Apple Account password**.
[Apple's app-specific password instructions](https://support.apple.com/en-us/102654).

For optional local notarization and credential validation, store a profile:

```sh
xcrun notarytool store-credentials "visualsubforge-notary"
```

Choose the Apple ID/app-specific-password route when prompted. Supply your
developer account email, Team ID and generated app-specific password
interactively. Leave API-key fields empty if prompted. The command validates
the credentials before saving them by default; resolve any validation error
before continuing. Xcode's command-line tools must provide `notarytool`.

This profile lives in your local Keychain. Its name is safe to share, but
**GitHub-hosted runners cannot access the profile on your Mac**. They need the
separate secrets below. The profile is useful locally but is not a prerequisite
for hosted signing.

## 6. Set GitHub repository secrets

Open the repository's [Actions secrets settings](https://github.com/golgeek/visualsubforge/settings/secrets/actions):
**Settings → Secrets and variables → Actions → Repository secrets**.
Use **New repository secret**, not a repository variable.

These are the names used by the VSF signing workflow:

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE` | Base64-encoded contents of the encrypted `.p12`, including the private key. |
| `APPLE_CERTIFICATE_PASSWORD` | The export password chosen in step 4. |
| `APPLE_SIGNING_IDENTITY` | Full `Developer ID Application: Your Name (TEAMID)` identity, without quotes. |
| `APPLE_ID` | The email used for the developer account and notarization. |
| `APPLE_TEAM_ID` | Team ID from Apple Developer Membership details. |
| `APPLE_APP_SPECIFIC_PASSWORD` | The generated notarization password from step 5. |

For the certificate, use this command to send the encoded value directly to
GitHub without printing it or writing a second secret file:

```sh
base64 < "$HOME/Library/Application Support/VisualSubForge/signing/developer-id-application.p12" |
  gh secret set APPLE_CERTIFICATE --repo golgeek/visualsubforge
```

Base64 is an encoding, not encryption; treat that value like the `.p12` itself.
You have already authorized `gh`; no extra workflow-scope refresh is needed for
these instructions.

Set the remaining five through the web page or run these commands **one at a
time** and paste each value into the interactive prompt:

```sh
gh secret set APPLE_CERTIFICATE_PASSWORD --repo golgeek/visualsubforge
gh secret set APPLE_SIGNING_IDENTITY --repo golgeek/visualsubforge
gh secret set APPLE_ID --repo golgeek/visualsubforge
gh secret set APPLE_TEAM_ID --repo golgeek/visualsubforge
gh secret set APPLE_APP_SPECIFIC_PASSWORD --repo golgeek/visualsubforge
```

Avoid putting passwords in command arguments, shell history, `.env` files or
chat. Do not upload your login Keychain file or password. The hosted signing job
will create its own temporary Keychain and password. Repository secrets will
be referenced only by the signing job; ordinary build/test jobs do not need
them. This follows the temporary-Keychain approach in
[GitHub's signing guide](https://docs.github.com/en/actions/how-tos/deploy/deploy-to-third-party-platforms/sign-xcode-applications),
using VSF's secret names above.

Check that the names exist without displaying their values:

```sh
gh secret list --repo golgeek/visualsubforge
```

## 7. What to report back

You can report **“the six GitHub secrets are set”** and, if configured, the local
profile name `visualsubforge-notary`. The identity and Team ID are also safe to
share for troubleshooting. Do not send passwords, the `.p12`, its base64 contents
or a private key in chat.

The signing job checks the assembly hashes, imports the identity into a private
temporary Keychain, signs the fully assembled application and its native tools,
submits it to Apple, requires acceptance, staples the ticket and runs signature
and Gatekeeper checks. It then recreates the ZIP with the stapled app. The job
cleans up the Keychain/export and uploads a receipt with the submission ID and
final archive hash. Secrets are scoped to this step, not the build/test jobs.
Successful credential setup is not evidence that signing or clean-install checks
have passed. Signing also does not lower the minimum macOS version required by
the compiled dependencies.
