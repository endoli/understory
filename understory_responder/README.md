<div align="center">

# Understory Responder

**Deterministic responder chain for UI: capture → target → bubble**

[![Latest published version.](https://img.shields.io/crates/v/understory_responder.svg)](https://crates.io/crates/understory_responder)
[![Documentation build status.](https://img.shields.io/docsrs/understory_responder.svg)](https://docs.rs/understory_responder)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/endoli/understory/ci.yml?logo=github&label=CI)](https://github.com/endoli/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_responder
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Understory Responder: a deterministic, `no_std` router for UI events.

## Overview

This crate builds the responder chain sequence — capture → target → bubble — from pre‑resolved hits.
It does not perform hit testing.
Instead, feed it [`ResolvedHit`](https://docs.rs/understory_responder/latest/understory_responder/types/struct.ResolvedHit.html) items (for example from a box tree or a 3D ray cast), and it emits a deterministic propagation sequence you can dispatch.

## Inputs

Provide one or more [`ResolvedHit`](https://docs.rs/understory_responder/latest/understory_responder/types/struct.ResolvedHit.html) values for candidate targets.
A [`ResolvedHit`](https://docs.rs/understory_responder/latest/understory_responder/types/struct.ResolvedHit.html) contains the node key, an optional root→target `path`, a [`DepthKey`](https://docs.rs/understory_responder/latest/understory_responder/types/enum.DepthKey.html) used for ordering,
a [`Localizer`](https://docs.rs/understory_responder/latest/understory_responder/types/struct.Localizer.html) for coordinate conversion, and an optional `meta` payload (e.g., text or ray‑hit details).
You may also provide a [`ParentLookup`](https://docs.rs/understory_responder/latest/understory_responder/types/trait.ParentLookup.html) source to reconstruct a path when `path` is absent.

## Ordering

Candidates are ranked by [`DepthKey`](https://docs.rs/understory_responder/latest/understory_responder/types/enum.DepthKey.html).
For `Z`, higher is nearer. For `Distance`, lower is nearer. When kinds differ, `Z` ranks above `Distance` by default.
Equal‑depth ties are stable and the router selects the last.

## Pointer capture

If capture is set, the router routes to the captured node regardless of fresh hits.
It uses the matching hit’s path and `meta` if present, otherwise reconstructs a path with [`ParentLookup`](https://docs.rs/understory_responder/latest/understory_responder/types/trait.ParentLookup.html) or falls back to a singleton path.
Capture bypasses scope filtering.

## Layering

The router only computes the traversal order. A higher‑level dispatcher can execute handlers, honor cancelation, and apply toolkit policies.

## Workflow

1) Pick candidates — e.g., from a 2D box tree or a 3D ray cast — and build
   one or more [`ResolvedHit`](https://docs.rs/understory_responder/latest/understory_responder/types/struct.ResolvedHit.html) values (with optional root→target paths).
2) Route — [`Router`](https://docs.rs/understory_responder/latest/understory_responder/router/struct.Router.html) ranks candidates by [`DepthKey`](https://docs.rs/understory_responder/latest/understory_responder/types/enum.DepthKey.html) and selects
   exactly one target. It emits a capture→target→bubble sequence for that target’s path.
   - Overlapping siblings: only the topmost/nearest candidate is selected; siblings do not receive the target.
   - Equal‑depth ties: deterministic and stable; the last candidate wins unless you pre‑order your hits or set a policy.
   - Pointer capture: overrides selection until released.
3) Hover — derive the path from the dispatch via [`path_from_dispatch`](https://docs.rs/understory_responder/latest/understory_responder/hover/fn.path_from_dispatch.html)
   and feed it to [`HoverState`](https://docs.rs/understory_responder/latest/understory_responder/hover/struct.HoverState.html). `HoverState` emits leave (inner→outer)
   and enter (outer→inner) events for the minimal transition between old and new paths.

## Dispatcher sketch

The snippet below shows how a higher‑level layer could walk the router’s sequence and honor stop/cancel rules.
It groups contiguous entries by phase and allows a handler to stop within a phase or stop‑and‑consume the event entirely.

```rust
use understory_responder::types::{Dispatch, Outcome, Phase};

/// Deliver a single dispatch item to your toolkit and return
/// whether to continue propagation or stop.
fn deliver<K, W, M>(_d: &Dispatch<K, W, M>) -> Outcome {
    Outcome::Continue
}

/// Walk the dispatch sequence produced by the router.
/// Returns true if the event was consumed (e.g., default prevented).
fn run_dispatch<K, W, M>(seq: &[Dispatch<K, W, M>]) -> bool {
    let mut consumed = false;
    let mut i = 0;
    while i < seq.len() {
        let phase = seq[i].phase;
        // Process contiguous entries for the same phase.
        while i < seq.len() && seq[i].phase == phase {
            match deliver(&seq[i]) {
                Outcome::Continue => {}
                Outcome::Stop => {
                    // Skip remaining entries in this phase.
                    while i + 1 < seq.len() && seq[i + 1].phase == phase {
                        i += 1;
                    }
                }
                Outcome::StopAndConsume => {
                    consumed = true;
                    // Abort remaining phases.
                    return consumed;
                }
            }
            i += 1;
        }
    }
    consumed
}

```

This crate is `no_std` and uses `alloc`.


<!-- cargo-rdme end -->

## Examples

- Router basics.
  - `cargo run -p understory_examples --example responder_basics`
- Hover transitions.
  - `cargo run -p understory_examples --example responder_hover`
- Box tree integration.
  - `cargo run -p understory_examples --example responder_box_tree`

## Minimum supported Rust Version (MSRV)

This crate has been verified to compile with **Rust 1.88** and later.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE] or <http://www.apache.org/licenses/LICENSE-2.0>), or
- MIT license ([LICENSE-MIT] or <http://opensource.org/licenses/MIT>),

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you,
as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

## Contribution

Contributions are welcome by pull request. The [Rust code of conduct] applies.
Please feel free to add your name to the [AUTHORS] file in any substantive pull request.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be licensed as above, without any additional terms or conditions.

[Rust Code of Conduct]: https://www.rust-lang.org/policies/code-of-conduct
[AUTHORS]: ../AUTHORS
[LICENSE-APACHE]: LICENSE-APACHE
[LICENSE-MIT]: LICENSE-MIT
