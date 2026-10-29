---
title: Install and open
area: ui-shell
kind: tutorial
story: 
shots: [start-page]
since: 0.1
---
Install PCBCraft and open it on the sample design.

## Before you start

You need Rust 1.95 or newer and the window libraries listed in [Getting started](../../../getting-started.md#install-and-run).

## Steps

1. Run `cargo run -p pcbcraft -- --sample` in the folder you cloned.
2. Wait for the window to open on the start page.
3. Open the {{ui:docks.workbench_tabs.schematic}} tab to see the sample circuit.
4. Open the {{ui:docks.workbench_tabs.layout}} tab to see the same circuit as a board.

![The start page](shot:start-page)

## What you should see

A window with the sample design. The tabs along the top switch between its views.

## If it did not work

Check that your Rust version is new enough. The [Getting started](../../../getting-started.md) page lists the window libraries for Ubuntu.

## Related pages

- [Getting started](../../../getting-started.md)
