# Bundle paste file honours the footprint paste margin

The Gerber bundle writer computed the paste margin but ignored it when
flashing paste apertures, so F.Paste and B.Paste used the unmodified pad
size. The paste aperture is now grown or shrunk by the footprint paste
margin, matching the library rule and the behaviour of the separate
gerber_paste writer.

A 1.0 x 0.5 mm pad with paste_margin -0.02 now produces a 0.960 x 0.460 mm
paste opening. A zero or absent margin produces the pad size, as before.
