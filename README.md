# Vibestar

Project NorthStar workspace with XR50 + Ultraleap integration experiments.

## Current Runtime Policy

- **XR50 SLAM runtime:** run on **Linux or Windows**.
- **macOS:** development/UI environment only for now.
- **Why:** macOS receives XR50 packets at high rate, but tracking stays non-functional (`confidence ~0.001`, translation fixed at zero) across tested startup sequences/backends.

See `xvisio-rs/SETUP.md` for the detailed status and commands.

## Hardware Status (Latest)

| Component | Model | Status |
|-----------|-------|--------|
| 6DOF Tracking | XVisio SeerSense XR50 | ✅ Working on Linux/Windows, ❌ not production-ready on macOS |
| Hand Tracking | Ultraleap Stereo IR 170 | ✅ Working |
| Display | 2880×1600 @ 89Hz | ✅ Detected |

## Recommended Setup

1. Connect XR50 runtime host (Linux/Windows) to XR50 and run `xvisio-rs`.
2. Keep macOS for rendering/dev tooling.
3. Stream pose data from runtime host to macOS app over network.

## Repository Pointers

- `xvisio-rs/` — active cross-platform XR50 SDK work.
- `visual-test/` — frontend visualizations.
- `MACOS_FIXES.md` — archived macOS investigation notes and findings.
