# Machine profile TOML schema

Machine profiles describe shop equipment used by fabrication artifacts. Each file is one profile. Profiles are copied into a design with `machine.add` or imported with `machine.import`.

## Header

| Field | Type | Notes |
| --- | --- | --- |
| `id` | string | Stable slug, 1 to 64 bytes, `[a-z0-9-]` only. |
| `name` | string | Display name, 1 to 128 bytes. |
| `kind` | string | One of `laser_galvo`, `laser_gantry`, `cnc_mill`, `pick_place`, `uv_printer`. |
| `vendor` | string | Up to 128 bytes. |
| `model` | string | Up to 128 bytes. |
| `notes` | string | Optional free text, up to 4096 bytes. |

## Work area

Table `[work_area]`:

| Field | Type | Notes |
| --- | --- | --- |
| `x`, `y` | number | Positive millimetres, required. |
| `z` | number | Optional positive depth in millimetres. |
| `origin` | string | Origin label, for example `lower-left` or `center`. |
| `y_up` | boolean | Whether positive Y points up on the machine bed. |

## Output

Table `[output]`:

| Field | Type | Notes |
| --- | --- | --- |
| `format` | string | Target file format for artifacts. |
| `dialect` | string | Writer dialect name. |
| `units` | string | Usually `mm`. |
| `line_ending` | string | `lf` or `crlf`. |
| `mirror_bottom` | boolean | Mirror bottom-side output. |
| `mirror_axis` | string | `x` or `y`. |
| `invert` | boolean | Invert coordinates when needed. |

Only the section that matches `kind` may be populated. Other kind sections must be omitted or left empty.

## Laser (`laser_galvo`, `laser_gantry`)

Table `[laser]` with optional `field_x_mm`, `field_y_mm`, `source_watts`, `wavelength_nm`.

Array of tables `[[laser.operation]]` (at most 64 rows):

| Field | Type | Notes |
| --- | --- | --- |
| `name` | string | Operation id, for example `fiducials`. |
| `mode` | string | `line` or `fill`. |
| `speed_mm_s` | number | Job speed. |
| `power_percent` | number | 0 to 100. |
| `passes` | integer | At least 1. |
| `frequency_khz`, `pulse_width_ns` | number | Source timing. |
| `line_interval_mm`, `hatch_angle_deg`, `angle_increment_deg`, `kerf_offset_mm` | number | Optional fill or line tuning. |
| `verified` | boolean | False when speeds are starting guesses. |

## Mill (`cnc_mill`)

Table `[mill.spindle]`: `speed_style`, `max_rpm`, `default_percent`, `min_percent`, `max_percent` (percent values stay between 50 and 100).

Table `[mill.gcode]`: header strings, `arcs_allowed`, `tool_change`, speeds, `preamble` and `postamble` string arrays.

Array `[[mill.tool]]` (at most 64): `id`, `diameter_mm`, `feed_mm_min`, `plunge_mm_min`, `spindle_percent` (50 to 100, feeds positive).

Table `[mill.material]`: `name`, `thickness_mm`, `safe_z_mm`.

## Pick and place (`pick_place`)

Tables `[pick_place.board]`, `[pick_place.rotation]`, `[pick_place.csv]`.

Arrays (each at most 64 rows): `[[pick_place.feeders]]`, `[[pick_place.nozzle]]`, `[[pick_place.rotation_offset]]`, `[[pick_place.footprint_alias]]`, `[[pick_place.feeder_map]]`, and up to 256 `[[pick_place.placement_rule]]` rows.

Keys of `[pick_place]` itself: `nozzle_slots` (nozzle types the machine runs in one go without an operator, 0 = unlimited) and `default_speed_pct`.

Each `[[pick_place.nozzle]]` has `id`, `diameter_mm` (inner), optional `outer_diameter_mm` and `packages` (package-name prefixes it is recommended for).

Each `[[pick_place.placement_rule]]` chooses a nozzle for a part from its short package name (`package` is a prefix; empty matches all), the largest pad dimension (`min_size_mm`, `max_size_mm`) and the body height from the part's `height` property (`min_height_mm`, `max_height_mm`); 0 means no bound. It also gives `nozzle`, `speed_pct`, `pick_height_mm`, `place_height_mm`, `skip` (leave the part out, for test points) and a `note`. The first matching row wins. A part overrides any value with the properties `Placement nozzle`, `Placement feeder`, `Placement speed`, `Placement pick height`, `Placement place height` and `Placement skip`. A rule's `nozzle` must be listed in `[[pick_place.nozzle]]`.

## UV printer (`uv_printer`)

Tables `[uv_printer.print]`, `[uv_printer.artwork]`, `[uv_printer.limits]` with bed size, DPI options, channels, bleed, pad clearance, and pixel caps.

## Built-in presets

| `id` | Kind |
| --- | --- |
| `neoden-yy1` | pick and place |
| `neoden-4` | pick and place (nozzle rules only; no job file yet) |
| `eufymake-e1` | uv printer |
| `snapmaker-2-a350t-200w-cnc` | mill |
| `snapmaker-artisan-200w-cnc` | mill |
| `commarker-b6-mopa` | laser galvo |
