# Minimal Swift/iOS contract fixture

This fixture is Gantry's public native-iOS source contract. It intentionally contains
only the release metadata needed to exercise first-party detection and governance:

- a concrete bundle identifier, marketing version, build number, and minimum OS;
- a valid Apple privacy manifest;
- an explicit application entitlement file; and
- a minimal Xcode project marker for native-iOS stack detection; and
- a committed Gantry vault/policy so CI evaluates clean Git inputs and cache identity.

The fixture is not an App Store application and does not require an Apple developer
account. On macOS, `scripts/ci/build-ios-contract-ipa.sh` packages these inputs around a
local Mach-O executable, applies an ad-hoc signature, and creates a disposable IPA. The
CI contract then proves Gantry can verify the signature and govern the exact artifact.

No generated IPA, signature, provisioning profile, or private signing material is
committed to the repository.
