# Passive series spot-check

This page records the generator output for a fixed set of values so that future
changes can be compared. The generator makes resistors and capacitors for four
standard chip packages and uses the EIA value code in every part number.

The E-series used are E6, E12, E24 and E96. The part number pattern is
prefix-package-valuecode-tolerance where the prefix is RES or CAP, the value
code uses R K M or P N U markers and the tolerance is the percentage as an
integer. Every row in the table below is hand checked against the generator
output.

| Kind | Value | Package | Part number | Pads | Pins |
| --- | --- | --- | --- | --- | --- |
| resistor | 10 ohm | 0402 | RES-0402-10R-1 | 2 | 2 |
| resistor | 4.7 kohm | 0603 | RES-0603-4K7-1 | 2 | 2 |
| resistor | 100 kohm | 0805 | RES-0805-100K-1 | 2 | 2 |
| resistor | 1 Mohm | 1206 | RES-1206-1M0-1 | 2 | 2 |
| capacitor | 22 pF | 0402 | CAP-0402-22P-5 | 2 | 2 |
| capacitor | 100 nF | 0603 | CAP-0603-100N-5 | 2 | 2 |
| capacitor | 4.7 uF | 0805 | CAP-0805-4U7-10 | 2 | 2 |
| capacitor | 10 uF | 1206 | CAP-1206-10U-10 | 2 | 2 |

The automated check in passive_series.rs includes this page, parses the table
and re-generates every row. If a row is missing or the generated part number
differs the check fails.
