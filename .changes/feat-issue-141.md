## Issue #141: Copper layer model for any number of layers

### Summary
Replaced the two-value `Layer` enum with a stackup-tied index model that supports any number of copper layers.

### Changes
- Updated `track.cleanup` schema from `enum: ["F.Cu","B.Cu"]` to accept any copper layer name
- Updated `track.route_pair` schema from `enum: ["F.Cu","B.Cu"]` to accept any copper layer name
- The `Layer` type is now a newtype over `u8` with bounded index validation against the stackup
- Existing files load unchanged (F.Cu/B.Cu map to outer layers)
- `Project::validate` rejects objects on non-existent layers
- `board.set_stackup` refuses to remove copper layer with objects on it

### Tests
- Migration tests load all examples/ and verify backward compatibility
- Unit tests verify two-layer files load/save byte-equal (except format version)
- Unit tests verify tracks on inner layers of 4-layer boards validate correctly
- Unit tests verify same tracks on 2-layer boards are refused
