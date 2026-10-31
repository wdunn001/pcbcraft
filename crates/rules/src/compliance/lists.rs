//! Parsed compliance list snapshots bundled with the rules crate.
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct ListMeta {
    pub schema: u32,
    pub id: String,
    pub title: String,
    pub source_url: String,
    pub published: String,
    pub retrieved: String,
    pub licence: String,
    pub cadence_days: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CoveredCountriesList {
    #[serde(flatten)]
    pub meta: ListMeta,
    #[serde(default)]
    pub countries: Vec<CoveredCountry>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CoveredCountry {
    pub code: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FrameworkList {
    #[serde(flatten)]
    pub meta: ListMeta,
    #[serde(default)]
    pub part_numbers: Vec<FrameworkPart>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FrameworkPart {
    pub mpn: String,
    #[serde(default)]
    pub role: String,
    pub added: String,
    #[serde(default)]
    pub citation: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RadioBandsList {
    #[serde(flatten)]
    pub meta: ListMeta,
    #[serde(default)]
    pub market: Vec<RadioMarket>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RadioMarket {
    pub code: String,
    pub bands_mhz: Vec<[f64; 2]>,
}

pub fn parse_covered_countries(text: &str) -> Result<CoveredCountriesList, String> {
    toml::from_str(text).map_err(|e| e.to_string())
}

pub fn parse_framework(text: &str) -> Result<FrameworkList, String> {
    toml::from_str(text).map_err(|e| e.to_string())
}

pub fn parse_radio_bands(text: &str) -> Result<RadioBandsList, String> {
    toml::from_str(text).map_err(|e| e.to_string())
}

pub fn country_covered(list: &CoveredCountriesList, code: &str) -> bool {
    list.countries
        .iter()
        .any(|c| c.code.eq_ignore_ascii_case(code))
}

pub fn framework_entry(list: &FrameworkList, mpn: &str) -> Option<&FrameworkPart> {
    list.part_numbers
        .iter()
        .find(|p| p.mpn.eq_ignore_ascii_case(mpn))
}

pub fn band_allowed(list: &RadioBandsList, markets: &[String], mhz: f64) -> bool {
    if markets.is_empty() {
        return true;
    }
    markets.iter().any(|market| {
        list.market
            .iter()
            .find(|m| m.code.eq_ignore_ascii_case(market))
            .is_some_and(|m| m.bands_mhz.iter().any(|b| mhz >= b[0] && mhz <= b[1]))
    })
}
