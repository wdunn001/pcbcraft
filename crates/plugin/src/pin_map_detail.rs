//! Detailed pin map rows from the firmware design record.
//!
//! Builds one row per pin assignment with the net, peripheral instance, bus
//! family, chip-select and interrupt pin. Parts with no known driver are
//! listed as unmapped.
//!
//! # Rules
//! - Rows are sorted by pin name.
//! - Unmapped part names are sorted.
//! - Refuses more than 4096 assignments.
//!
//! # Dependencies
//! - crates/doc/src/firmware_design.rs `load_firmware_context`
//! - crates/library/src/mcu_record.rs `function_instance`
//!
//! # Do not
//! - Guess a driver for an unmapped part.
use pcbcraft_doc::Project;
use pcbcraft_doc::firmware_design::load_firmware_context;
use pcbcraft_library::mcu_record::function_instance;

/// One detailed pin row from the firmware design.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PinDetailRow {
    /// The microcontroller pin name.
    pub pin: String,
    /// The function assigned to the pin.
    pub function: String,
    /// The net the pin is wired to, or empty when it is not wired.
    pub net: String,
    /// The peripheral instance the function belongs to, for example SPI1, or empty.
    pub instance: String,
    /// The bus family of the instance, or empty when the function is not on a bus.
    pub bus: String,
    /// The chip-select pin of the device on an SPI instance, or empty.
    pub select: String,
    /// The interrupt pin for an external-interrupt function, or empty.
    pub interrupt: String,
}

/// Detailed pin map with unmapped parts listed separately.
#[derive(Clone, Debug)]
pub struct PinDetail {
    /// One row per pin assignment.
    pub rows: Vec<PinDetailRow>,
    /// Parts with no known driver, listed apart from the rows.
    pub unmapped: Vec<String>,
}

const MAX_ASSIGNMENTS: usize = 4096;

/// Look up the net name for a given pin on the MCU component.
fn pin_net_name(
    project: &Project,
    ctx: &pcbcraft_doc::firmware_design::FirmwareContext,
    pin: &str,
) -> String {
    let id = ctx.mcu_component.id;
    for component in &project.components {
        if component.id == id {
            for p in &component.pins {
                if (p.number == pin || p.name == pin)
                    && let Some(net_id) = p.net
                {
                    for net in &project.nets {
                        if net.id == net_id {
                            return net.name.clone();
                        }
                    }
                }
            }
        }
    }
    String::new()
}

/// Determine the bus family from a peripheral instance id.
fn bus_for_instance(instance: &str) -> String {
    if instance.starts_with("SPI")
        || instance.starts_with("I2C")
        || instance.starts_with("USART")
        || instance.starts_with("UART")
        || instance.starts_with("CAN")
    {
        instance.to_string()
    } else {
        String::new()
    }
}

/// Find the chip-select pin for an SPI instance.
fn select_for_instance(
    ctx: &pcbcraft_doc::firmware_design::FirmwareContext,
    instance: &str,
) -> String {
    for gyro in &ctx.design.gyro {
        if gyro.spi == instance {
            return gyro.cs.clone();
        }
    }
    for flash in &ctx.design.flash {
        if flash.spi == instance {
            return flash.cs.clone();
        }
    }
    String::new()
}

/// Build the detailed pin map from the firmware design.
///
/// # Rules
/// - Loads the firmware context; returns an error if the design is missing.
/// - Refuses when the assignment count exceeds `MAX_ASSIGNMENTS`.
///
/// # Dependencies
/// - crates/doc/src/firmware_design.rs
/// - crates/library/src/mcu_record.rs
///
/// # Do not
/// - Mutate the project.
pub fn pin_map_detail(p: &Project) -> Result<PinDetail, String> {
    let ctx = load_firmware_context(p)?;
    let design = &ctx.design;
    if design.assignment.len() > MAX_ASSIGNMENTS {
        return Err("firmware design has too many pin assignments".into());
    }
    let mut rows = Vec::with_capacity(design.assignment.len());
    for assignment in &design.assignment {
        let net = pin_net_name(p, &ctx, &assignment.pin);
        let instance = function_instance(&ctx.mcu, &assignment.function).unwrap_or_default();
        let bus = bus_for_instance(&instance);
        let select = if instance.starts_with("SPI") {
            select_for_instance(&ctx, &instance)
        } else {
            String::new()
        };
        let interrupt = if assignment.function.starts_with("EXTI") {
            assignment.pin.clone()
        } else {
            String::new()
        };
        rows.push(PinDetailRow {
            pin: assignment.pin.clone(),
            function: assignment.function.clone(),
            net,
            instance,
            bus,
            select,
            interrupt,
        });
    }
    rows.sort();
    let mut unmapped = design.unmapped.clone();
    unmapped.sort();
    Ok(PinDetail { rows, unmapped })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{Component, EmbeddedFile, Net, Pin, Project};

    fn fixture_project() -> Project {
        let design_text = include_str!("../../library/tests/fixtures/firmware-design.toml");
        let mcu_text = include_str!("../../library/tests/fixtures/stm32f405-fixture.mcu.toml");
        let mut project = Project::default();
        project.files.push(EmbeddedFile {
            name: "firmware-design.toml".into(),
            data: design_text.into(),
            encoding: String::new(),
        });
        project.files.push(EmbeddedFile {
            name: "stm32f405-fixture.mcu.toml".into(),
            data: mcu_text.into(),
            encoding: String::new(),
        });
        project.components.push(Component {
            id: 1,
            reference: "U1".into(),
            value: "STM32F405".into(),
            kind: "ic_mcu".into(),
            position: pcbcraft_geom::Point { x: 0.0, y: 0.0 },
            rotation: 0.0,
            pins: vec![
                Pin {
                    number: "PA5".into(),
                    name: "PA5".into(),
                    offset: pcbcraft_geom::Point { x: 0.0, y: 0.0 },
                    net: Some(10),
                    electrical_type: "bidirectional".into(),
                    direction: None,
                    unit: 1,
                    appearances: Default::default(),
                },
                Pin {
                    number: "PA4".into(),
                    name: "PA4".into(),
                    offset: pcbcraft_geom::Point { x: 0.0, y: 0.0 },
                    net: Some(11),
                    electrical_type: "input".into(),
                    direction: None,
                    unit: 1,
                    appearances: Default::default(),
                },
                Pin {
                    number: "PB12".into(),
                    name: "PB12".into(),
                    offset: pcbcraft_geom::Point { x: 0.0, y: 0.0 },
                    net: Some(12),
                    electrical_type: "input".into(),
                    direction: None,
                    unit: 1,
                    appearances: Default::default(),
                },
            ],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![],
            sheet: 0,
            compliance: Default::default(),
            part: None,
            sourcing: Default::default(),
            instance: Default::default(),
            prototype: Default::default(),
            include_in_bom: true,
            include_in_placement: true,
        });
        project.nets.push(Net {
            id: 10,
            name: "+5V".into(),
            ..Net::default()
        });
        project.nets.push(Net {
            id: 11,
            name: "GYRO_CS".into(),
            ..Net::default()
        });
        project.nets.push(Net {
            id: 12,
            name: "FLASH_CS".into(),
            ..Net::default()
        });
        project
    }

    #[test]
    fn detail_rows_list_every_assigned_pin_with_net_and_bus() -> Result<(), String> {
        let project = fixture_project();
        let detail = pin_map_detail(&project)?;
        assert_eq!(detail.rows.len(), 10, "expected 10 rows");
        let pa5 = detail
            .rows
            .iter()
            .find(|r| r.pin == "PA5")
            .ok_or("PA5 row missing")?;
        assert_eq!(pa5.function, "SPI1_SCK");
        assert_eq!(pa5.net, "+5V");
        assert_eq!(pa5.instance, "SPI1");
        assert_eq!(pa5.bus, "SPI1");
        let pa4 = detail
            .rows
            .iter()
            .find(|r| r.pin == "PA4")
            .ok_or("PA4 row missing")?;
        assert_eq!(pa4.select, "PA4", "gyro cs on SPI1");
        let pb12 = detail
            .rows
            .iter()
            .find(|r| r.pin == "PB12")
            .ok_or("PB12 row missing")?;
        assert_eq!(pb12.select, "PB12", "flash cs on SPI2");
        assert_eq!(detail.unmapped, vec!["UNKNOWN_PART"]);
        Ok(())
    }
}
