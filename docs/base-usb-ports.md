# Base USB ports

The Master Hub base exposes one USB port per pogo site. A module enumerates on the port for the site it is sitting on. These ids are macOS USB location IDs from `ioreg -p IOUSB`. The base itself stays on `@02114400`.

The OpenDeck layout uses the port path after the bus byte and the Mac port nibble. `@02112400` is site `124`. The same location can be a different rectangle for a different module size, because each module's USB contact sits in a different place on its body.

The grid below is the base in each orientation. Horizontal is the 6×4 side. Vertical is the base turned so that side is 4×6. Captured on 2026-10-13 with one module at a time.

## 2×2

Module: 1xDial (`1xDial w/ IPS, 2x2`). Fourteen placements, fourteen distinct ports. The two extra bottom sites were captured with the knob module, which has the same footprint.

Horizontal:

| Site | Location |
| --- | --- |
| Top left | `@02112400` |
| Top center | `@02111100` |
| Top right | `@02114300` |
| Bottom left | `@02144000` |
| Bottom, left of center | `@02143000` |
| Bottom center | `@02123000` |
| Bottom, right of center | `@02111200` |
| Bottom right | `@02113400` |

Vertical:

| Site | Location |
| --- | --- |
| Top left | `@02134000` |
| Top right | `@02111300` |
| Middle left | `@02131000` |
| Middle right | `@02142000` |
| Bottom left | `@02112300` |
| Bottom right | `@02112200` |

## 4×3

Module: keypad (`15xKey w/ IPS, 4x3`). Twelve placements, twelve distinct ports. The horizontal bottom row has a center column. The top row does not. Three sites leave one row of the keypad above the base.

Horizontal:

| Site | Location |
| --- | --- |
| Above, left | `@02111100` |
| Above, right | `@02114300` |
| Top left | `@02122000` |
| Top right | `@02113200` |
| Bottom left | `@02123000` |
| Bottom center | `@02111200` |
| Bottom right | `@02113400` |

Vertical, one column:

| Site | Location |
| --- | --- |
| Above the top | `@02111300` |
| Top | `@02124000` |
| Upper middle | `@02142000` |
| Lower middle | `@02141000` |
| Bottom | `@02112200` |

## 4×1

Module: rollers (`2xRoller, 4x1`). The horizontal left-top site did not enumerate. The other thirteen placements each have a distinct port.

Horizontal. Left side from top to bottom, then the right side from top to bottom:

| Site | Location |
| --- | --- |
| Left top | not captured |
| Left upper middle | `@02132000` |
| Left lower middle | `@02121000` |
| Left bottom | `@02111400` |
| Right top | `@02114100` |
| Right upper middle | `@02114200` |
| Right lower middle | `@02113100` |
| Right bottom | `@02113300` |

Vertical, one column, top to bottom:

| Site | Location |
| --- | --- |
| 1 | `@02113400` |
| 2 | `@02111200` |
| 3 | `@02123000` |
| 4 | `@02143000` |
| 5 | `@02144000` |
| 6 | `@02112100` |

## 4×2

Module: sliders (`5xFader, 4x2`). Five horizontal sites, then five in one vertical column.

Horizontal:

| Site | Location |
| --- | --- |
| Top left | `@02111100` |
| Top right | `@02114300` |
| Bottom left | `@02123000` |
| Bottom center | `@02111200` |
| Bottom right | `@02113400` |

Vertical, one column, top to bottom:

| Site | Location |
| --- | --- |
| 1 | `@02111300` |
| 2 | `@02124000` |
| 3 | `@02142000` |
| 4 | `@02141000` |
| 5 | `@02112200` |
