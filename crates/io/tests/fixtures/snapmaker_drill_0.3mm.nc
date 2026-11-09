;Header Start
;header_type: cnc
;tool_head: levelTwoCNCToolheadForSM2
;machine: Snapmaker 2.0 A350
;gcode_flavor: marlin
;renderMethod: line
;file_total_lines: 35
;estimated_time(s): 4
;is_rotate: false
;max_x(mm): 10
;max_y(mm): 10
;max_z(mm): 5
;min_x(mm): 10
;min_y(mm): 10
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
;tool T1 0.300 mm
M3 P100
G0 Z5 F1500
G0 X10 Y10 F1500
G1 Z-1.6 F100
G0 Z2 F1500
G0 Z5 F1500
M400
M5
