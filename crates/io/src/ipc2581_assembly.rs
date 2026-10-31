//! Bill of materials, component instances and approved vendor list for IPC-2581C (issue #661).

use super::{sanitize_id, xml_escape, Writer};
use pcbcraft_doc::{Component, Footprint, Layer, Project};
use std::collections::{BTreeMap, BTreeSet};

const BOM_NAME: &str = "BOM";
const AVL_NAME: &str = "Primary_Vendor_List";
const STEP_NAME: &str = "Board";

pub(super) struct AssemblyMeta {
    pub write_bom: bool,
    pub write_avl: bool,
}

impl AssemblyMeta {
    pub fn plan(p: &Project) -> Self {
        let write_bom = p.board.footprints.iter().any(|f| {
            p.component(f.component)
                .is_some_and(|c| c.in_bom() && placed_mount(f).is_some())
        });
        let write_avl = write_bom && p.components.iter().any(|c| c.in_bom() && has_avl_facts(c));
        Self {
            write_bom,
            write_avl,
        }
    }
}

impl Writer {
    pub(super) fn write_content_assembly_refs(&mut self, meta: &AssemblyMeta) -> Result<(), String> {
        if meta.write_bom {
            self.push(&format!("<BomRef name=\"{BOM_NAME}\"/>\n"))?;
        }
        if meta.write_avl {
            self.push(&format!("<AvlRef name=\"{AVL_NAME}\"/>\n"))?;
        }
        Ok(())
    }

    pub(super) fn write_assembly_root(&mut self, p: &Project, meta: &AssemblyMeta) -> Result<(), String> {
        if meta.write_bom {
            self.write_bom(p)?;
        }
        if meta.write_avl {
            self.write_avl(p)?;
        }
        Ok(())
    }

    pub(super) fn write_step_assembly(&mut self, p: &Project, meta: &AssemblyMeta) -> Result<(), String> {
        if !meta.write_bom {
            return Ok(());
        }
        let refs: BTreeMap<u64, &Component> = p.components.iter().map(|c| (c.id, c)).collect();
        let mut packages: BTreeSet<String> = BTreeSet::new();
        for f in &p.board.footprints {
            let Some(c) = refs.get(&f.component) else {
                continue;
            };
            if !c.in_bom() || placed_mount(f).is_none() {
                continue;
            }
            let package = package_name(c);
            if packages.insert(package.clone()) {
                self.push(&format!(
                    "<Package name=\"{}\" type=\"OTHER\">\n</Package>\n",
                    xml_escape(&package)
                ))?;
            }
        }
        for f in &p.board.footprints {
            let Some(c) = refs.get(&f.component) else {
                return Err("footprint references an unknown component".into());
            };
            if !c.in_bom() {
                continue;
            }
            let Some(mount) = placed_mount(f) else {
                continue;
            };
            let back = f.board_side() == Layer::Back;
            let layer_ref = if back {
                p.layer_name(Layer::Back)
            } else {
                p.layer_name(Layer::Front)
            };
            let package = package_name(c);
            let part = oem_part_id(c, &package);
            let rotation = export_rotation(p, f);
            self.push(&format!(
                "<Component refDes=\"{}\" packageRef=\"{}\" layerRef=\"{}\" part=\"{}\" mountType=\"{mount}\">\n",
                xml_escape(&c.reference),
                xml_escape(&package),
                xml_escape(&layer_ref),
                xml_escape(&part),
            ))?;
            self.push(&format!(
                "<Location x=\"{:.6}\" y=\"{:.6}\"/>\n",
                f.position.x, f.position.y
            ))?;
            if rotation.abs() > 1e-9 {
                self.push(&format!("<Xform rotation=\"{rotation:.6}\"/>\n"))?;
            }
            self.push("</Component>\n")?;
        }
        Ok(())
    }

    fn write_bom(&mut self, p: &Project) -> Result<(), String> {
        let refs: BTreeMap<u64, &Component> = p.components.iter().map(|c| (c.id, c)).collect();
        let mut groups: BTreeMap<(String, String, String), BomGroup> = BTreeMap::new();
        for f in &p.board.footprints {
            let Some(c) = refs.get(&f.component) else {
                return Err("footprint references an unknown component".into());
            };
            if !c.in_bom() || placed_mount(f).is_none() {
                continue;
            }
            let package = package_name(c);
            let key = (c.value.clone(), c.kind.clone(), package.clone());
            let back = f.board_side() == Layer::Back;
            let layer_ref = if back {
                p.layer_name(Layer::Back)
            } else {
                p.layer_name(Layer::Front)
            };
            let entry = groups.entry(key).or_insert_with(|| BomGroup {
                oem_id: oem_part_id(c, &package),
                description: component_description(c).to_string(),
                package: package.clone(),
                pin_count: c.pins.len(),
                refdes: Vec::new(),
            });
            entry.refdes.push(BomRef {
                name: c.reference.clone(),
                layer_ref,
                populate: true,
            });
        }
        if groups.is_empty() {
            return Ok(());
        }
        let assembly = if p.name.trim().is_empty() {
            "assembly".to_string()
        } else {
            p.name.clone()
        };
        self.push(&format!("<Bom name=\"{BOM_NAME}\">\n"))?;
        self.push(&format!(
            "<BomHeader revision=\"1\" assembly=\"{}\">\n<StepRef name=\"{STEP_NAME}\"/>\n</BomHeader>\n",
            xml_escape(&assembly)
        ))?;
        for ((value, kind, footprint), group) in groups {
            let qty = group.refdes.len();
            self.push(&format!(
                "<BomItem OEMDesignNumberRef=\"{}\" quantity=\"{qty}\" pinCount=\"{}\" category=\"ELECTRICAL\"",
                xml_escape(&group.oem_id),
                group.pin_count
            ))?;
            if !group.description.is_empty() {
                self.push(&format!(
                    " description=\"{}\"",
                    xml_escape(&group.description)
                ))?;
            }
            self.push(">\n")?;
            for rd in &group.refdes {
                self.push(&format!(
                    "<RefDes name=\"{}\" packageRef=\"{}\" populate=\"{}\" layerRef=\"{}\"/>\n",
                    xml_escape(&rd.name),
                    xml_escape(&footprint),
                    if rd.populate { "true" } else { "false" },
                    xml_escape(&rd.layer_ref)
                ))?;
            }
            self.push("<Characteristics category=\"ELECTRICAL\">\n")?;
            if !value.is_empty() {
                self.write_textual("Value", &value)?;
            }
            if !kind.is_empty() {
                self.write_textual("Kind", &kind)?;
            }
            self.push("</Characteristics>\n")?;
            self.push("</BomItem>\n")?;
        }
        self.push("</Bom>\n")?;
        Ok(())
    }

    fn write_avl(&mut self, p: &Project) -> Result<(), String> {
        let mut enterprises = collect_enterprises(p, &AssemblyMeta { write_bom: true, write_avl: true });
        let refs: BTreeMap<u64, &Component> = p.components.iter().map(|c| (c.id, c)).collect();
        let mut items: BTreeMap<String, AvlFacts> = BTreeMap::new();
        for f in &p.board.footprints {
            let Some(c) = refs.get(&f.component) else {
                continue;
            };
            if !c.in_bom() || placed_mount(f).is_none() {
                continue;
            }
            let package = package_name(c);
            let oem = oem_part_id(c, &package);
            if !has_avl_facts(c) {
                continue;
            }
            items.entry(oem).or_insert_with(|| AvlFacts {
                manufacturer: component_manufacturer(c).to_string(),
                mpn: component_mpn(c).to_string(),
                moisture: component_moisture(c),
                supplier_parts: supplier_part_numbers(c),
            });
        }
        if items.is_empty() {
            return Ok(());
        }
        self.push(&format!("<Avl name=\"{AVL_NAME}\">\n"))?;
        self.push(
            "<AvlHeader title=\"BOM\" source=\"PCBCraft\" author=\"OWNER\" datetime=\"2020-01-01T00:00:00\" version=\"1\"/>\n",
        )?;
        for (oem, facts) in items {
            self.push(&format!(
                "<AvlItem OEMDesignNumber=\"{}\">\n",
                xml_escape(&oem)
            ))?;
            if !facts.mpn.is_empty() {
                self.write_avl_vmpn(&mut enterprises, &facts)?;
            }
            for (rank, (supplier, number)) in facts.supplier_parts.iter().enumerate() {
                self.push("      <AvlVmpn qualified=\"false\" chosen=\"false\">\n")?;
                self.push(&format!(
                    "        <AvlMpn name=\"{}\" rank=\"{}\"/>\n",
                    xml_escape(number),
                    rank + 1
                ))?;
                let enterprise = enterprise_id(supplier);
                self.ensure_enterprise(&mut enterprises, &enterprise, supplier)?;
                self.push(&format!(
                    "        <AvlVendor enterpriseRef=\"{}\"/>\n",
                    xml_escape(&enterprise)
                ))?;
                self.push("      </AvlVmpn>\n")?;
            }
            self.push("</AvlItem>\n")?;
        }
        self.push("</Avl>\n")?;
        Ok(())
    }

    fn write_avl_vmpn(
        &mut self,
        enterprises: &mut BTreeMap<String, String>,
        facts: &AvlFacts,
    ) -> Result<(), String> {
        self.push("      <AvlVmpn qualified=\"false\" chosen=\"true\">\n")?;
        let mut mpn_attrs = format!("        <AvlMpn name=\"{}\"", xml_escape(&facts.mpn));
        if let Some(ms) = facts.moisture.and_then(moisture_ipc) {
            mpn_attrs.push_str(&format!(" moistureSensitivity=\"{ms}\""));
        }
        mpn_attrs.push_str("/>\n");
        self.push(&mpn_attrs)?;
        if !facts.manufacturer.is_empty() {
            let enterprise = enterprise_id(&facts.manufacturer);
            self.ensure_enterprise(enterprises, &enterprise, &facts.manufacturer)?;
            self.push(&format!(
                "        <AvlVendor enterpriseRef=\"{}\"/>\n",
                xml_escape(&enterprise)
            ))?;
        }
        self.push("      </AvlVmpn>\n")?;
        Ok(())
    }

    fn write_textual(&mut self, name: &str, value: &str) -> Result<(), String> {
        self.push(&format!(
            "<Textual definitionSource=\"PCBCraft\" textualCharacteristicName=\"{}\" textualCharacteristicValue=\"{}\"/>\n",
            xml_escape(name),
            xml_escape(value)
        ))?;
        Ok(())
    }

    fn ensure_enterprise(
        &mut self,
        enterprises: &mut BTreeMap<String, String>,
        id: &str,
        name: &str,
    ) -> Result<(), String> {
        enterprises.entry(id.to_string()).or_insert_with(|| name.to_string());
        Ok(())
    }
}

pub(super) fn collect_enterprises(p: &Project, meta: &AssemblyMeta) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    if !meta.write_avl {
        return out;
    }
    let refs: BTreeMap<u64, &Component> = p.components.iter().map(|c| (c.id, c)).collect();
    for f in &p.board.footprints {
        let Some(c) = refs.get(&f.component) else {
            continue;
        };
        if !c.in_bom() || placed_mount(f).is_none() {
            continue;
        }
        let package = package_name(c);
        let oem = oem_part_id(c, &package);
        if !has_avl_facts(c) {
            continue;
        }
        if !component_manufacturer(c).is_empty() {
            let id = enterprise_id(component_manufacturer(c));
            out.entry(id).or_insert_with(|| component_manufacturer(c).to_string());
        }
        for (supplier, _) in supplier_part_numbers(c) {
            let id = enterprise_id(&supplier);
            out.entry(id).or_insert_with(|| supplier);
        }
        let _ = oem;
    }
    out
}

struct BomGroup {
    oem_id: String,
    description: String,
    package: String,
    pin_count: usize,
    refdes: Vec<BomRef>,
}

struct BomRef {
    name: String,
    layer_ref: String,
    populate: bool,
}

struct AvlFacts {
    manufacturer: String,
    mpn: String,
    moisture: Option<u8>,
    supplier_parts: Vec<(String, String)>,
}

fn component_manufacturer(c: &Component) -> &str {
    c.field("Manufacturer").unwrap_or("")
}

fn component_mpn(c: &Component) -> &str {
    c.field("MPN").unwrap_or("")
}

fn component_description(c: &Component) -> &str {
    c.field("Description").unwrap_or("")
}

fn component_moisture(c: &Component) -> Option<u8> {
    c.field("Moisture level")
        .and_then(|s| s.trim().parse::<u8>().ok())
        .filter(|m| (1..=6).contains(m))
}

fn supplier_part_numbers(c: &Component) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for f in &c.fields {
        let key = f.key.trim();
        let value = f.value.trim();
        if value.is_empty() {
            continue;
        }
        if let Some(name) = supplier_field_name(key) {
            out.push((name, value.to_string()));
        }
    }
    out
}

fn supplier_field_name(key: &str) -> Option<String> {
    let lower = key.to_ascii_lowercase();
    for tail in [" part number", " part no", " part #", " pn", " sku"] {
        if let Some(head) = lower.strip_suffix(tail) {
            let name = key[..head.len()].trim();
            if name.is_empty() {
                return None;
            }
            let plain: String = name
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
                .to_ascii_lowercase();
            if ["manufacturer", "mfr", "mfg", "maker"].contains(&plain.as_str()) {
                return None;
            }
            return Some(name.to_string());
        }
    }
    None
}

fn has_avl_facts(c: &Component) -> bool {
    !component_manufacturer(c).is_empty()
        || !component_mpn(c).is_empty()
        || !supplier_part_numbers(c).is_empty()
        || component_moisture(c).is_some()
}

fn package_name(c: &Component) -> String {
    match &c.library_footprint {
        Some(fp) => fp.definition.name.clone(),
        None => c.kind.clone(),
    }
}

fn oem_part_id(c: &Component, package: &str) -> String {
    let base = if !component_mpn(c).is_empty() {
        component_mpn(c).to_string()
    } else if !c.value.is_empty() {
        format!("{}_{}", c.kind, c.value)
    } else {
        format!("{}_{}", c.kind, package)
    };
    sanitize_id(&base)
}

fn enterprise_id(name: &str) -> String {
    sanitize_id(name)
}

fn placed_mount(f: &Footprint) -> Option<&'static str> {
    let tht = f.pads.iter().any(|pad| {
        pad.geometry
            .as_ref()
            .is_some_and(|g| g.pad_type == "thru_hole")
    });
    if tht {
        Some("THMT")
    } else if f.pads.iter().any(|pad| {
        pad.geometry.as_ref().is_some_and(|g| g.pad_type == "smd")
    }) {
        Some("SMT")
    } else if f.pads.is_empty() {
        None
    } else {
        Some("OTHER")
    }
}

fn export_rotation(p: &Project, f: &Footprint) -> f64 {
    let back = f.board_side() == Layer::Back;
    let mut rotation = f.rotation;
    if back {
        if let Some(profile) = p
            .machine_profiles
            .iter()
            .find(|m| m.kind == "pick_place")
        {
            if profile.output.mirror_bottom {
                rotation = -rotation;
            }
        }
    }
    rotation.rem_euclid(360.0)
}

fn moisture_ipc(level: u8) -> Option<&'static str> {
    match level {
        1 => Some("UNLIMITED"),
        2 => Some("1_YEAR"),
        3 => Some("4_WEEKS"),
        4 => Some("168_HOURS"),
        5 => Some("72_HOURS"),
        6 => Some("BAKE"),
        _ => None,
    }
}

trait ProjectExt {
    fn component(&self, id: u64) -> Option<&Component>;
}

impl ProjectExt for Project {
    fn component(&self, id: u64) -> Option<&Component> {
        self.components.iter().find(|c| c.id == id)
    }
}
