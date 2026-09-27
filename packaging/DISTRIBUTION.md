# Getting a macOS app onto someone else's machine

Everything else in this pipeline works without Apple. This step does not, and
the measurements below say why.

## What Gatekeeper actually does, measured

macOS 26.5.1, on the packaged app from `pack.sh`.

| The app was signed with | `codesign --verify` | `spctl` (Gatekeeper) |
|---|---|---|
| Ad-hoc (`--sign -`) | passes | **rejected** — `no usable signature` |
| Apple **Distribution** certificate | passes | **rejected** |
| Developer ID + notarisation | — | not yet tested, no certificate |

Two things follow.

**A valid signature is not the same as an accepted one.** The Apple Distribution
certificate signs the bundle correctly — `codesign --verify --strict` passes —
and Gatekeeper still refuses it. Distribution certificates are for the App Store
and enterprise deployment. Direct download is judged on a different certificate
type entirely.

**Quarantine changes nothing here.** The app is rejected with or without the
`com.apple.quarantine` attribute a browser attaches. Rejection is about the
signature, not the download.

So on current macOS this is not friction to warn users about. Without a
**Developer ID Application** certificate and notarisation, the app does not open
on someone else's Mac.

## What is needed

A "Developer ID Application" certificate. This machine currently has:

- `Apple Development: …` — for running on your own registered devices
- `Apple Distribution: A-SAFE (UK) LIMITED (4Q7UXAX886)` — App Store and enterprise

Neither is the one. The good news is that Developer ID is a **different
certificate type from the same Apple Developer Program membership**, not a
separate product, so an account that can issue the Distribution certificate
above can generally issue this one too — subject to the account holder's role,
since Developer ID issuance is usually restricted to the Account Holder.

Once it exists:

```bash
xcrun notarytool store-credentials notary \
  --apple-id you@example.com --team-id TEAMID --password <app-specific-password>

packaging/notarize.sh --app dist/Ledger.app \
  --identity "Developer ID Application: … (TEAMID)" --keychain-profile notary
```

`notarize.sh` re-signs inside-out with a real timestamp, submits, staples, and
then checks with `spctl` rather than assuming success.

## The entitlements question this reopens

The packaged app needs two entitlements,
`disable-library-validation` and `allow-unsigned-executable-memory`, because
Ruby `dlopen`s every C extension. Both were established against an **ad-hoc**
signature, and ad-hoc signing gives every binary a different Team ID — which is
precisely what library validation rejects.

A real Developer ID signs every binary under one team, so
`disable-library-validation` may prove unnecessary. That matters beyond
tidiness: `allow-unsigned-executable-memory` is the kind of entitlement the
notary service scrutinises, so carrying one you do not need is worth avoiding.

**Re-run the entitlement matrix with the real certificate** before settling on
what to ship. `spike06` in the spikes repository does exactly that, and takes an
identity as an argument.

## The disk image

`packaging/dmg.sh` builds one with no certificate involved. A 122 MB app
compresses to a 66 MB image. The image itself is also assessed by Gatekeeper and
is rejected today for the same reason.

## Still untested

Notarisation itself, the stapled result, and first launch of a downloaded copy
on a machine that has never seen the app. All three are gated on the
certificate.
