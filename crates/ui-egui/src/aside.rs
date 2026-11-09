//! How wide the left and right asides are, as a share of the window.
//!
//! Context: the left aside (Layers tree or Library) and the right aside
//! (Properties and Assistant) used to have fixed point widths, so a window with
//! few points gave most of its width to the asides. This module is the one place
//! that decides their default, minimum and maximum widths from the window width.

/// Window width in points under which side docks give way to the drawing (WCAG
/// 1.4.10 reflow at 320 points).
pub(crate) const NARROW_WINDOW: f32 = 640.0;

/// The drawing keeps this share of the window.
const CENTRE_SHARE: f32 = 0.40;
/// The drawing keeps at least this many points.
const CENTRE_FLOOR: f32 = 360.0;

/// Share of the window and range in points for one width.
#[derive(Clone, Copy)]
struct Rule {
    share: f32,
    low: f32,
    high: f32,
}

impl Rule {
    fn at(self, window: f32) -> f32 {
        (window * self.share).clamp(self.low, self.high)
    }
}

const LEFT_DEFAULT: Rule = Rule {
    share: 0.22,
    low: 240.0,
    high: 380.0,
};
const LEFT_LIBRARY_DEFAULT: Rule = Rule {
    share: 0.26,
    low: 240.0,
    high: 400.0,
};
const RIGHT_DEFAULT: Rule = Rule {
    share: 0.23,
    low: 240.0,
    high: 360.0,
};
/// The Layers tree and the Library cards both need 240 points to keep their rows whole.
const LEFT_MIN: Rule = Rule {
    share: 0.14,
    low: 240.0,
    high: 260.0,
};
/// The Properties page is 234 points wide at its narrowest; narrower and it pushes the
/// whole window layout out past the right edge.
const RIGHT_MIN: Rule = Rule {
    share: 0.14,
    low: 240.0,
    high: 260.0,
};
/// The widest a person can drag each aside, before the drawing's share is applied.
const LEFT_DRAG_MAX: f32 = 480.0;
const RIGHT_DRAG_MAX: f32 = 440.0;

/// Widths in points for the two asides at one window width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AsideWidths {
    pub(crate) left_default: f32,
    pub(crate) left_min: f32,
    pub(crate) left_max: f32,
    pub(crate) right_default: f32,
    pub(crate) right_min: f32,
    pub(crate) right_max: f32,
    /// The least the drawing keeps between the asides.
    pub(crate) centre_min: f32,
    /// Window width minus the drawing's share: what the two asides may share.
    room: f32,
}

impl AsideWidths {
    /// The most the right aside may take when the left aside is `left_width` wide.
    pub(crate) fn right_max_beside(&self, left_width: f32) -> f32 {
        (self.room - left_width).clamp(self.right_min, self.right_max.max(self.right_min))
    }
}

/// The width rule for the asides.
///
/// # Rules
/// The drawing keeps the larger of 40 percent of the window and 360 points.
/// When the default widths do not leave that, the right aside shrinks first
/// and the left aside second, never below their minimums. When even the
/// minimums leave the drawing under 40 percent (`is_narrow`) the asides give way
/// to the drawing altogether. `library` picks the wider default of the left
/// Library tab.
pub(crate) fn aside_widths(window: f32, library: bool) -> AsideWidths {
    let centre_min = (window * CENTRE_SHARE).max(CENTRE_FLOOR);
    let room = (window - centre_min).max(0.0);
    let left_min = LEFT_MIN.at(window);
    let right_min = RIGHT_MIN.at(window);
    let mut left = if library {
        LEFT_LIBRARY_DEFAULT
    } else {
        LEFT_DEFAULT
    }
    .at(window)
    .max(left_min);
    let mut right = RIGHT_DEFAULT.at(window).max(right_min);
    if left + right > room {
        right = (room - left).max(right_min);
    }
    if left + right > room {
        left = (room - right).max(left_min);
    }
    AsideWidths {
        left_default: left,
        left_min,
        left_max: (room - right_min).clamp(left_min, LEFT_DRAG_MAX.max(left_min)),
        right_default: right,
        right_min,
        right_max: (room - left_min).clamp(right_min, RIGHT_DRAG_MAX.max(right_min)),
        centre_min,
        room,
    }
}

/// True when the window is too narrow to put the asides beside the drawing.
/// That is under 640 points, or when the asides at their minimums leave the
/// drawing under 40 percent. Above that and until the drawing can keep 360
/// points, the asides sit at their minimums.
pub(crate) fn is_narrow(window: f32) -> bool {
    let w = aside_widths(window, false);
    window < NARROW_WINDOW || w.left_min + w.right_min + window * CENTRE_SHARE > window + 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_leave_the_drawing_its_share() -> Result<(), String> {
        for window in [
            820.0, 840.0, 830.0, 1000.0, 1280.0, 1440.0, 1600.0, 1920.0, 3000.0,
        ] {
            for library in [false, true] {
                let w = aside_widths(window, library);
                let centre = window - w.left_default - w.right_default;
                let wanted = if window >= 840.0 {
                    w.centre_min
                } else {
                    window * 0.4
                };
                if centre + 0.5 < wanted {
                    return Err(format!("{window}: centre {centre} under {wanted}"));
                }
                if w.left_default < w.left_min || w.right_default < w.right_min {
                    return Err(format!("{window}: default under minimum"));
                }
                if w.left_default > w.left_max || w.right_default > w.right_max {
                    return Err(format!("{window}: default over maximum"));
                }
            }
        }
        Ok(())
    }

    #[test]
    fn a_regular_monitor_gets_the_old_widths() -> Result<(), String> {
        let w = aside_widths(1440.0, false);
        if (w.left_default - 316.8).abs() > 0.1 || (w.right_default - 331.2).abs() > 0.1 {
            return Err(format!("{w:?}"));
        }
        Ok(())
    }

    #[test]
    fn narrow_windows_use_the_stacked_layout() -> Result<(), String> {
        if !is_narrow(639.0) || !is_narrow(700.0) || is_narrow(800.0) {
            return Err("narrow breakpoint moved".to_owned());
        }
        Ok(())
    }
}
