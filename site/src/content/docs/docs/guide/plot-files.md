---
title: Copper and drill plot files
description: Write factory copper and drill files, then preview them on the Manufacture tab.
sidebar:
  order: 9
---

Plot files are the copper layers, board outline and drill data a board factory reads. PCBCraft writes them beside your design unless you name another folder.

## Before you start

Open the Showcase board so the design is ready to make:

```sh
cargo run -p pcbcraft -- --project examples/showcase.pcbcraft
```

Route the nets first if you use the sample instead.

## Write the files

1. Open **Edit**, then **Preferences**.
2. Scroll to **MAKE FILES**.
3. Read **Plain files for the board factory, written beside your design unless you name another folder**.
4. Leave the folder box blank to use a folder named `outputs` next to the design file. Or type another path.
5. Click **Copper & drill files**.

   ![The Layout tab of the Showcase board. Orange tracks on the top layer and blue tracks on the bottom layer join the parts across a 72 by 60 mm board.](../../../../assets/guide/board-routed.png)

The status strip confirms the write. The files land in the folder you chose.

## Preview on Manufacture

1. Click the **Manufacture** tab beside **Layout**.
2. Read **Plot folder** and **Path to copper and drill files**. The path fills from your make-folder setting when it was empty.
3. Click **Load**. The layer list shows names such as `pcb-F.Cu.gbr` and `pcb-B.Cu.gbr`.
4. Click a layer name to hide it on the canvas. Click again to show it.
5. Click **Fit view** to frame every visible layer.

The left panel lists **Outputs** and **Machines**. The plot preview fills the centre. A capture of a loaded folder is not shown here; the steps match the app on fork main.

## What you should see

- Files appear in the outputs folder on disk.
- After **Load**, layers draw on the Manufacture canvas.
- Hidden layers show `(hidden)` beside their name in the list.

## Related

- [Route the board](../route-board/)
- [Save your design](../saving/)
