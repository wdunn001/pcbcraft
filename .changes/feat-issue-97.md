---
"pcbcraft-engine": minor
"pcbcraft-io": minor
"pcbcraft-library": minor
"pcbcraft-mcp": minor
---

## Library export core

- Add a multi-symbol library file writer that serializes many symbols into one file.
- Add a footprint folder writer that writes one file per footprint.
- Unknown source nodes are retained on read so an unmodified definition survives a read, write, read cycle with unknown nodes intact.
- The typed library.export engine command writes project-library symbols to one file or footprints to a folder through pcbcraft_io::atomic_write without editing the design.
- MCP library export tools delegate onto that shared path.
