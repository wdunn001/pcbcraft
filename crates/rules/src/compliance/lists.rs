use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct ListSnapshot {
    pub schema: u32,
    pub id: String,
    pub title: String,
    pub source_url: String,
    pub published: String,
    pub retrieved: String,
    pub licence: String,
    pub cadence_days: u32,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub substances: Vec<SubstanceEntry>,
    #[serde(default)]
    pub exemptions: Vec<ExemptionEntry>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SubstanceEntry {
    pub cas: String,
    #[serde(default)]
    pub ec: String,
    pub name: String,
    pub added: String,
    #[serde(default)]
    pub citation: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ExemptionEntry {
    pub number: String,
    #[serde(default)]
    pub description: String,
    pub expires: String,
    #[serde(default)]
    pub citation: String,
}

impl ListSnapshot {
    pub fn exemption_active(&self, number: &str, today: &str) -> bool {
        self.exemptions.iter().any(|e| {
            e.number == number && pcbcraft_doc::valid_date(&e.expires) && e.expires >= today
        })
    }

    pub fn stale_for_cadence(&self, today: &str) -> bool {
        if !pcbcraft_doc::valid_date(&self.retrieved) || !pcbcraft_doc::valid_date(today) {
            return false;
        }
        let days = self.cadence_days as i64;
        let retrieved = parse_day(&self.retrieved);
        let now = parse_day(today);
        now - retrieved > days
    }
}

fn parse_day(text: &str) -> i64 {
    let y: i64 = text[0..4].parse().unwrap_or(0);
    let m: i64 = text[5..7].parse().unwrap_or(0);
    let d: i64 = text[8..10].parse().unwrap_or(0);
    y * 372 + m * 31 + d
}

pub fn load_list_snapshot(toml_text: &str) -> Result<ListSnapshot, String> {
    let snap: ListSnapshot = toml::from_str(toml_text).map_err(|e| e.to_string())?;
    if snap.schema != 1 {
        return Err(format!("unsupported list snapshot schema {}", snap.schema));
    }
    if snap.id.is_empty() || snap.source_url.is_empty() || snap.licence.is_empty() {
        return Err("list snapshot missing required metadata".into());
    }
    if !pcbcraft_doc::valid_date(&snap.retrieved) {
        return Err("list snapshot retrieved date is invalid".into());
    }
    Ok(snap)
}
