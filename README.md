# PCBCraft

One native window for schematic, board layout, libraries and checks. Every edit
goes through a typed engine command. The desktop, the CLI and MCP call the same
commands.

MIT OR Apache-2.0. UI toolkit: pinned `craft-ui` from `storytold/craft-libs`.
GPL-3.0-or-later. UI toolkit: pinned `craft-ui` from `wdunn001/craft-libs`.

This is a working foundation: a schematic, assigned footprints, basic routing,
checks, and fabrication output. It is not a finished production suite. Open
items live in [ROADMAP.md](ROADMAP.md).

**Start here:** [docs/getting-started.md](docs/getting-started.md)

## Run

Rust 1.95 or newer. On Ubuntu, install window libraries first:

```sh
sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev \
  libx11-dev libxi-dev libxrandr-dev libxcursor-dev libgl1-mesa-dev
```

Then:

```sh
git clone https://github.com/wdunn001/pcbcraft.git
cd pcbcraft
cargo run -p pcbcraft -- --sample
```

Same `cargo run` line on native Windows. Headless MCP:

```sh
cargo run -p pcbcraft-cli -- mcp
```

Fork of record: [wdunn001/pcbcraft](https://github.com/wdunn001/pcbcraft). Tagged
builds, when published, appear under
[Releases](https://github.com/wdunn001/pcbcraft/releases).

## First circuit

Load **LED example**. **A** places a symbol; click to drop it. **W**, then two
pin circles, draws a wire. If neither pin has a net, one is created; two
different existing nets are refused. **Sync to PCB** creates footprints for
assigned parts. **X**, then two same-net pads, routes an orthogonal track.
**Checks** lists what is still wrong.

Save a `.pcbcraft` project. The route tool does not dodge obstacles. Moving a
footprint does not drag its tracks. Crossing wires do not join nets.

Default keys: [docs/getting-started.md](docs/getting-started.md#keyboard).

## Docs

| Page | Contents |
| --- | --- |
| [Getting started](docs/getting-started.md) | Clone, first run, MCP, fork workflow |
| [What is implemented](docs/capabilities.md) | Editing, libraries, checks, export |
| [Roadmap](ROADMAP.md) | Open work by milestone |
| [Architecture](docs/architecture.md) | Crate layers and MCP-first rules |
| [MCP](docs/mcp.md) | Client config and request shape |
| [CI](docs/ci.md) | GitHub Actions jobs; do not run the full suite on a laptop |
| [CONTRIBUTING](CONTRIBUTING.md) | Issue tags through merge |
| [AGENTS](AGENTS.md) | Parallel agents, UI lane, honesty rules |

Doc index: [docs/README.md](docs/README.md).

## License

MIT OR Apache-2.0, at your option; see [LICENSE-MIT](LICENSE-MIT),
[LICENSE-APACHE](LICENSE-APACHE) and [NOTICE](NOTICE). Third-party crates and
assets keep their own licences ([THIRD-PARTY-LICENSES.txt](THIRD-PARTY-LICENSES.txt)). Provenance of adapted code and library fixtures:
[ATTRIBUTION.md](ATTRIBUTION.md).
