# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Rust developers building Dioxus applications who need a node-graph UI —
workflow editors, data pipelines, visual programming surfaces, diagram tools.
They embed `dioxus-flow` as a crate and render their own node/edge content
inside it. Many arrive knowing react-flow and expect its concepts to exist
here. Confirmed: the Dioxus community at large is the primary audience, not
any single internal app.

## Product Purpose

A react-flow-style node graph library for Dioxus: a pannable/zoomable canvas
with draggable nodes, connectable handles, animated edges, automatic layout,
and fully customizable rendering. Success (confirmed): becoming the go-to
node-graph answer in the Dioxus ecosystem — crates.io publication, adoption,
documentation, and examples all matter.

## Positioning

The react-flow equivalent for Dioxus, in pure Rust. No JavaScript interop or
JS dependencies anywhere — interaction, animation, layout, and measurement
are all built on idiomatic Dioxus primitives, which a JS-wrapper approach
could not truthfully claim.

## Operating Context

- Consumed as a library crate; developers evaluate it through the README
  quickstart, the demo app (`cd demo && dx serve`), and eventually docs.rs.
- Workspace layout: `dioxus-flow/` (library), `demo/` (dx-template-style demo
  app with Tailwind).
- Dev environment is a Nix flake (rust + wasm target, dioxus-cli,
  tailwindcss); the nixpkgs `dx` is no-downloads and `wasm-bindgen` is pinned
  in `demo/Cargo.toml` to match the CLI. Release builds need
  `--debug-symbols false` (binaryen 131 crashes on dx's DWARF).
- Tests: `cargo test -p dioxus-flow`; interaction changes are verified with a
  headless-Chrome CDP harness (see project memory / README Development).

## Capabilities and Constraints

Shipped capabilities (see README for detail): interactive canvas (pan, zoom
to cursor, selection, group drag, keyboard delete), connectable handles with
snap radius and connection preview, three edge kinds (bezier, straight,
smooth-step) with markers/labels/animated mode, built-in Sugiyama-style auto
layout with animated transitions, fully replaceable node/edge rendering
(`node_view`/`edge_view`, generic `Node<T>` payloads), `Background`,
`Controls`, and `MiniMap` overlays, `use_flow()`/`FlowHandle` programmatic
API, CSS-custom-property theming with automatic dark mode.

Binding constraint (confirmed): **pure Rust/Dioxus — no JS interop or JS
dependencies, ever.**

Current design characteristics, valued but not user-declared as binding:
react-flow-familiar naming (nodes, edges, handles, fit view, minimap),
narrow-reactive-scope per-frame performance architecture, headless
customization with overridable default styles.

Explicitly planned but undecided (roadmap): box selection, edge
reconnection, configurable connection validation, sub-flows/node grouping,
viewport-culled rendering for very large graphs.

## Brand Commitments

The name `dioxus-flow`, deliberately echoing react-flow. No logo or visual
identity exists yet.

## Evidence on Hand

- Working demo app at `demo/` exercising custom card nodes, toolbar-driven
  auto layout, all three overlays, and dark mode.
- README with quickstart, customization guides, and an architecture section.
- Unit tests in the library crate (`cargo test -p dioxus-flow`).
- Absences future work must not fabricate: no published benchmarks, no
  third-party users or testimonials, not yet published to crates.io
  (as of 2026-08-17).

## Product Principles

1. **Community-first generality.** API and feature decisions serve arbitrary
   Dioxus apps, not one internal use case.
2. **Pure Rust/Dioxus is non-negotiable.** New capabilities are built on
   idiomatic Dioxus primitives; JS interop is never the shortcut.
3. **Adoption is earned through demonstration.** The demo, quickstart, and
   examples are first-class product surfaces; a newcomer should reach a
   working flow in minutes.
4. **Claims stay demonstrable.** Performance and feature claims are backed by
   runnable code and tests, never asserted ahead of evidence.
