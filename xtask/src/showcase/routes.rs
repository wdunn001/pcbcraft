//! The planned routes of the showcase board, one block at a time.
//!
//! # Rules
//! - Local connections run on the top layer; long buses run on the bottom
//!   layer between vias, one lane per net with the farthest target turning
//!   first so no two lanes cross.
//!
//! # Dependencies
//! - xtask/src/showcase/copper.rs
//!
//! # Do not
//! - Route a plane net here; the pours and plane vias take those.
use super::copper::{W, lay};
use super::{Gen, Res};

const SIG: f64 = 0.2;
const PWR: f64 = 0.4;

/// Run every planned route and report the ones the checks refused.
pub fn all(g: &mut Gen) -> Res<usize> {
    let mut failed = 0;
    let mut run = |g: &mut Gen, net: &str, width: f64, w: &[W]| -> Res<()> {
        if !lay(g, net, width, w)? {
            failed += 1;
            println!("route not kept: {net}");
        }
        Ok(())
    };
    // USB power input.
    run(g, "VBUS_RAW", PWR, &[W::P("J1", "7"), W::P("F1", "1")])?;
    run(g, "VBUS_RAW", PWR, &[W::P("D2", "2"), W::Y(9.0)])?;
    run(
        g,
        "VBUS_RAW",
        PWR,
        &[W::P("J1", "2"), W::X(10.0), W::V, W::Y(9.0), W::V],
    )?;
    run(g, "CC1", SIG, &[W::P("J1", "3"), W::P("R1", "1")])?;
    run(g, "CC2", SIG, &[W::P("J1", "6"), W::P("R2", "1")])?;
    // Fuse to charger.
    run(
        g,
        "VBUS",
        PWR,
        &[W::P("F1", "2"), W::X(26.7), W::P("C1", "1")],
    )?;
    run(g, "VBUS", PWR, &[W::P("F1", "2"), W::P("U1", "1")])?;
    run(
        g,
        "VBUS",
        PWR,
        &[W::P("F1", "2"), W::X(32.98), W::P("R4", "1")],
    )?;
    run(
        g,
        "STAT",
        SIG,
        &[W::P("U1", "5"), W::X(27.4), W::Y(12.0), W::P("D1", "2")],
    )?;
    run(g, "CHG_A", SIG, &[W::P("D1", "1"), W::P("R4", "2")])?;
    run(
        g,
        "PROG",
        SIG,
        &[W::P("U1", "4"), W::X(27.72), W::P("R3", "1")],
    )?;
    // Battery rail: trunk on the bottom layer. Branches end on a straight run.
    run(
        g,
        "VBAT",
        PWR,
        &[
            W::P("U1", "3"),
            W::X(19.0),
            W::V,
            W::Y(47.5),
            W::V,
            W::X(5.0),
            W::P("J2", "1"),
        ],
    )?;
    run(g, "VBAT", PWR, &[W::P("C2", "1"), W::Y(16.45)])?;
    run(
        g,
        "VBAT",
        PWR,
        &[W::P("U2", "1"), W::X(14.0), W::Y(50.95), W::P("U2", "3")],
    )?;
    run(g, "VBAT", PWR, &[W::A(14.0, 50.0), W::Y(47.5), W::X(13.0)])?;
    run(g, "VBAT", PWR, &[W::P("C3", "1"), W::X(14.0), W::Y(50.0)])?;
    run(g, "VBAT", PWR, &[W::P("TP1", "1"), W::Y(47.5), W::X(8.0)])?;
    // USB pair, coupled at half a millimetre.
    run(
        g,
        "USB_DN",
        SIG,
        &[
            W::P("J1", "5"),
            W::X(10.0),
            W::Y(11.25),
            W::X(17.5),
            W::Y(28.75),
            W::P("U3", "6"),
        ],
    )?;
    run(
        g,
        "USB_DP",
        SIG,
        &[
            W::P("J1", "4"),
            W::X(10.0),
            W::Y(11.75),
            W::X(17.0),
            W::Y(29.25),
            W::P("U3", "7"),
        ],
    )?;
    // Crystal.
    run(
        g,
        "XOUT",
        SIG,
        &[W::P("C6", "1"), W::Y(21.0), W::X(33.4), W::P("Y1", "3")],
    )?;
    run(
        g,
        "XOUT",
        SIG,
        &[W::P("Y1", "3"), W::X(35.5), W::Y(26.75), W::P("U3", "2")],
    )?;
    run(
        g,
        "XIN",
        SIG,
        &[W::P("Y1", "1"), W::Y(27.25), W::P("U3", "3")],
    )?;
    run(g, "XIN", SIG, &[W::P("C5", "1"), W::X(31.2)])?;
    run(g, "3V3", PWR, &[W::P("U7", "1"), W::X(61.3), W::V])?;
    // LED bus: bottom-layer lanes, the farthest target turns first.
    for (k, (pin, vy, lane, target)) in [
        (9, 31.6, 34.0, 27.0),
        (10, 32.6, 35.0, 33.0),
        (11, 31.6, 0.0, 39.25),
        (12, 32.6, 36.0, 45.0),
        (13, 31.6, 35.0, 51.0),
        (14, 32.6, 34.0, 57.0),
    ]
    .into_iter()
    .enumerate()
    {
        let pin = pin.to_string();
        let pin: &'static str = Box::leak(pin.into_boxed_str());
        let r: &'static str = Box::leak(format!("R{}", 10 + k).into_boxed_str());
        let net = format!("LED{k}");
        let mut w = vec![W::P("U3", pin), W::Y(vy), W::V];
        if lane > 0.0 {
            w.extend([W::Y(lane), W::X(target)]);
        }
        w.extend([W::Y(40.4), W::V, W::P(r, "1")]);
        run(g, &net, SIG, &w)?;
    }
    // SPI to the flash, top layer, diagonals spread so none cross.
    for (net, pin, jog, pad) in [
        ("SPI_CS", "24", 47.5, "1"),
        ("SPI_SCK", "23", 47.0, "2"),
        ("SPI_MOSI", "22", 46.0, "3"),
        ("SPI_MISO", "21", 45.0, "4"),
    ] {
        let a = crate::showcase::board::pad_at(g, "U3", pin)?;
        let b = crate::showcase::board::pad_at(g, "U4", pad)?;
        let dy = b.y - a.y;
        run(
            g,
            net,
            SIG,
            &[
                W::P("U3", pin),
                W::X(jog),
                W::A(jog + dy.abs(), b.y),
                W::P("U4", pad),
            ],
        )?;
    }
    // Debug header: bottom-layer lanes to the through-hole pads.
    for (net, pin, via_y, lane, target, pad) in [
        ("SWDIO", "29", 24.1, 16.0, 43.04, "3"),
        ("SWCLK", "28", 23.0, 17.0, 44.31, "4"),
        ("UART_TX", "27", 24.1, 18.0, 45.58, "5"),
        ("UART_RX", "26", 23.0, 19.0, 46.85, "6"),
        ("NRST", "25", 24.1, 20.0, 48.12, "7"),
    ] {
        run(
            g,
            net,
            SIG,
            &[
                W::P("U3", pin),
                W::Y(via_y),
                W::V,
                W::Y(lane),
                W::X(target),
                W::Y(8.4),
                W::V,
                W::P("J3", pad),
            ],
        )?;
    }
    run(
        g,
        "NRST",
        SIG,
        &[W::B, W::A(48.12, 15.0), W::X(50.7), W::V, W::P("SW1", "1")],
    )?;
    run(
        g,
        "NRST",
        SIG,
        &[W::B, W::A(48.12, 10.6), W::X(49.9), W::V, W::P("R5", "2")],
    )?;
    for k in 0..6 {
        let n: &'static str = Box::leak(format!("R{}", 10 + k).into_boxed_str());
        let d: &'static str = Box::leak(format!("D{}", 3 + k).into_boxed_str());
        run(g, &format!("LED{k}_A"), SIG, &[W::P(n, "2"), W::P(d, "1")])?;
    }
    // Buttons, the sensor bus and the sensor enable: bottom-layer lanes, one per
    // net, each turning later than the lane above it.
    run(
        g,
        "BTN1",
        SIG,
        &[
            W::P("U3", "20"),
            W::X(43.4),
            W::V,
            W::X(51.7),
            W::Y(7.6),
            W::V,
            W::P("SW2", "1"),
        ],
    )?;
    run(
        g,
        "I2C_SCL",
        SIG,
        &[
            W::P("U3", "19"),
            W::X(44.2),
            W::V,
            W::X(61.02),
            W::Y(12.95),
            W::V,
            W::P("U5", "2"),
        ],
    )?;
    run(
        g,
        "I2C_SCL",
        SIG,
        &[W::B, W::A(61.02, 12.95), W::Y(7.6), W::V, W::P("R8", "2")],
    )?;
    run(
        g,
        "I2C_SCL",
        SIG,
        &[W::B, W::A(61.02, 26.65), W::V, W::P("U6", "1")],
    )?;
    run(
        g,
        "I2C_SDA",
        SIG,
        &[
            W::P("U3", "18"),
            W::X(43.4),
            W::V,
            W::X(63.82),
            W::Y(13.95),
            W::V,
            W::P("U5", "4"),
        ],
    )?;
    run(
        g,
        "I2C_SDA",
        SIG,
        &[W::B, W::A(63.82, 13.95), W::Y(7.6), W::V, W::P("R9", "2")],
    )?;
    run(
        g,
        "I2C_SDA",
        SIG,
        &[W::B, W::A(63.82, 28.55), W::V, W::P("U6", "3")],
    )?;
    run(
        g,
        "SENSOR_EN",
        SIG,
        &[
            W::P("U3", "17"),
            W::X(44.2),
            W::V,
            W::X(60.4),
            W::Y(42.95),
            W::X(63.6),
            W::V,
            W::P("U7", "3"),
        ],
    )?;
    run(
        g,
        "BTN2",
        SIG,
        &[
            W::P("U3", "5"),
            W::X(36.6),
            W::V,
            W::X(25.2),
            W::Y(26.2),
            W::V,
            W::P("SW3", "1"),
        ],
    )?;
    Ok(failed)
}
