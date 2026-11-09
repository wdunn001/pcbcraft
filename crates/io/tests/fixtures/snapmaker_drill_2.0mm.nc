;Header Start
;header_type: cnc
;tool_head: levelTwoCNCToolheadForSM2
;machine: Snapmaker 2.0 A350
;gcode_flavor: marlin
;renderMethod: line
;file_total_lines: 35
;estimated_time(s): 4
;is_rotate: false
;max_x(mm): 30
;max_y(mm): 20
;max_z(mm): 5
;min_x(mm): 30
;min_y(mm): 20
;min_z(mm): -1.6
;work_speed(mm/minute): 300
;jog_speed(mm/minute): 1500
;work_size_x: 320
;work_size_y: 325
;origin: lower-left
;Header End
G90
G21
G17
;PCBCraft drill
G0 Z5 F1500
;tool T3 2.000 mm
M3 P100
G0 Z5 F1500
G0 X30 Y20 F1500
G1 Z-1.6 F100
G0 Z2 F1500
G0 Z5 F1500
M400
M5
