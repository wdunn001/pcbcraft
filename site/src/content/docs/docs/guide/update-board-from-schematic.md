---
title: Update the board from the schematic
description: Copy schematic changes onto the board when automatic sync is off.
sidebar:
  order: 5
---

The schematic and the board share one design. With automatic sync on, every schematic edit reaches the board at once. Turn it off when you want to batch edits, then update the board yourself.

## Before you start

Open the sample with `cargo run -p pcbcraft -- --sample`. Stay on the **Schematic** tab.

## Turn automatic sync off

1. Open **Edit**, then **Preferences**.
2. Find **Keep the board in sync with the schematic automatically**.
3. Turn the switch off if it is on.

You can turn it on again at any time from the same place, or from **Edit**, then **Keep board in sync automatically**.

## Change the schematic

1. Click **Move**, then drag a part a little.
2. Click **Layout**. The board may still show the old positions until you sync.

   ![The sample schematic. J1, R1 and D1 are joined by green wires. Small circles mark the pins.](../../../../assets/guide/start-schematic.png)

## Update the board

1. Open **Edit**.
2. Pick **Update board from schematic**.
3. Click **Layout**. The parts and nets match the schematic again.

The status strip tells you what happened. The board view opens after a manual update.

## What you should see

- After **Update board from schematic**, the board matches the schematic.
- With automatic sync on, you do not need the command; the board updates with each edit.

## Related

- [Draw on the schematic](../draw-schematic/)
- [Route the board](../route-board/)
