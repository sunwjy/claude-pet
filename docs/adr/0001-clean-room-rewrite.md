# Clean-room rewrite, no upstream art

claude-pet is inspired by clawd-on-desk, but its code is AGPL-3.0 and all of its character art (Clawd, Calico, Cloudling, Hash Sage) is "All rights reserved" — and Clawd itself is Anthropic's mascot. We therefore read upstream only to learn behaviour, never copy or translate its source, license claude-pet as MIT/Apache-2.0, and ship an original default character; Clawd-like characters may only arrive as user-supplied themes, never bundled in this repo.

## Considered Options

- **AGPL fork/translation** — rejected: the only gain is reusing JS/Electron code that barely transfers to Rust, and it still gives no right to the art.
