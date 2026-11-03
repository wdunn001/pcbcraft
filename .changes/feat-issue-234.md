## feat: Add silk layers to fabrication bundle

Include front and back silkscreen Gerber files (pcb-F.Silk.gbr,
pcb-B.Silk.gbr) in the fabrication package alongside copper,
mask, paste, and drill files. The silk layers carry mandatory
X2 attributes (FileFunction, FilePolarity, SameCoordinates).
