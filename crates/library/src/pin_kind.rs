use serde::{Deserialize, Serialize};

/// Closed electrical pin kinds for rules and verification.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinKind {
    PowerIn,
    PowerOut,
    Input,
    Output,
    Bidirectional,
    OpenDrain,
    OpenCollector,
    TriState,
    #[default]
    Passive,
    Analog,
    NoConnect,
    Mechanical,
    Thermal,
}

impl PinKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PowerIn => "power_in",
            Self::PowerOut => "power_out",
            Self::Input => "input",
            Self::Output => "output",
            Self::Bidirectional => "bidirectional",
            Self::OpenDrain => "open_drain",
            Self::OpenCollector => "open_collector",
            Self::TriState => "tri_state",
            Self::Passive => "passive",
            Self::Analog => "analog",
            Self::NoConnect => "nc",
            Self::Mechanical => "mechanical",
            Self::Thermal => "thermal",
        }
    }
}

/// Map an interchange-format electrical type string to a pin kind.
/// Returns `(kind, unknown)` where `unknown` is true when the string was not recognized.
pub fn pin_kind_from_library_string(raw: &str) -> (PinKind, bool) {
    let key = raw.trim().to_ascii_lowercase();
    let kind = match key.as_str() {
        "input" | "in" => PinKind::Input,
        "output" | "out" => PinKind::Output,
        "bidirectional" | "bidir" | "io" => PinKind::Bidirectional,
        "tri_state" | "tristate" | "tri-state" => PinKind::TriState,
        "passive" => PinKind::Passive,
        "power_in" | "powerin" | "pwr_in" => PinKind::PowerIn,
        "power_out" | "powerout" | "pwr_out" => PinKind::PowerOut,
        "open_collector" | "opencollector" => PinKind::OpenCollector,
        "open_emitter" | "open_drain" | "opendrain" | "openemitter" => PinKind::OpenDrain,
        "unspecified" | "free" => PinKind::Passive,
        "no_connect" | "not_connected" | "nc" => PinKind::NoConnect,
        "mechanical" | "mounting" => PinKind::Mechanical,
        "thermal" => PinKind::Thermal,
        "analog" => PinKind::Analog,
        _ => return (PinKind::Passive, true),
    };
    (kind, false)
}
