//! MCU pin-mux and peripheral conflict checks from the firmware design record.
use pcbcraft_doc::{Id, Project};
use pcbcraft_library::firmware_design::{load_firmware_context, FirmwareContext};
use pcbcraft_library::mcu::{function_instance, McuPin};
use std::collections::{BTreeMap, BTreeSet};

use crate::Finding;

fn net_name(project: &Project, net: Option<Id>) -> Option<String> {
    net.and_then(|id| project.nets.iter().find(|n| n.id == id).map(|n| n.name.clone()))
}

fn pin_on_net(project: &Project, component_id: Id, pin_ref: &str) -> Option<Id> {
    project
        .components
        .iter()
        .find(|c| c.id == component_id)
        .and_then(|c| {
            c.pins
                .iter()
                .find(|p| p.number == pin_ref || p.name == pin_ref)
        })
        .and_then(|p| p.net)
}

fn is_high_rail_net(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper.contains("5V") || upper.contains("+5") || upper.starts_with("VBUS")
}

fn push(
    project: &Project,
    out: &mut Vec<Finding>,
    code: &'static str,
    default: &str,
    message: String,
    objects: Vec<Id>,
) {
    if project.rule_disabled(code) {
        return;
    }
    if project.rule_excluded(code, &objects) || project.rule_region_excluded(code, &objects) {
        return;
    }
    out.push(Finding {
        code,
        severity: project.rule_severity(code, default),
        message,
        objects,
    });
}

fn check_invalid_function(ctx: &FirmwareContext, project: &Project, out: &mut Vec<Finding>) {
    let id = ctx.mcu_component.id;
    for assign in &ctx.design.assignment {
        let Some(pin) = ctx.mcu.pin_by_name(&assign.pin) else {
            push(
                project,
                out,
                "peripheral_unknown_pin",
                "error",
                format!(
                    "{} pin {} is not in the MCU package table",
                    ctx.mcu_component.reference, assign.pin
                ),
                vec![id],
            );
            continue;
        };
        if !pin.functions.iter().any(|f| f == &assign.function) {
            push(
                project,
                out,
                "peripheral_invalid_function",
                "error",
                format!(
                    "{} pin {} cannot take {} (allowed: {})",
                    ctx.mcu_component.reference,
                    assign.pin,
                    assign.function,
                    pin.functions.join(", ")
                ),
                vec![id],
            );
        }
    }
}

fn check_instance_conflicts(ctx: &FirmwareContext, project: &Project, out: &mut Vec<Finding>) {
    let id = ctx.mcu_component.id;
    let mut by_signal: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for assign in &ctx.design.assignment {
        by_signal
            .entry(assign.function.clone())
            .or_default()
            .push(&assign.pin);
    }
    for (signal, pins) in &by_signal {
        if pins.len() > 1 {
            push(
                project,
                out,
                "peripheral_instance_conflict",
                "error",
                format!(
                    "nets claim {} on {} and {} at once",
                    signal,
                    pins[0],
                    pins[1]
                ),
                vec![id],
            );
        }
    }
    let mut by_instance: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for assign in &ctx.design.assignment {
        if let Some(inst) = function_instance(&ctx.mcu, &assign.function) {
            by_instance.entry(inst).or_default().insert(assign.pin.clone());
        }
    }
    for (inst, pins) in by_instance {
        if inst.starts_with("TIM") && pins.len() > 2 {
            push(
                project,
                out,
                "peripheral_timer_dma_conflict",
                "error",
                format!(
                    "timer instance {} is spread across {} which may collide on channels or DMA",
                    inst,
                    pins.iter().cloned().collect::<Vec<_>>().join(", ")
                ),
                vec![id],
            );
        }
    }
}

fn check_uart_sets(ctx: &FirmwareContext, project: &Project, out: &mut Vec<Finding>) {
    let id = ctx.mcu_component.id;
    for serial in &ctx.design.serial {
        let tx_ok = ctx
            .design
            .assignment
            .iter()
            .any(|a| a.pin == serial.tx && a.function == format!("{}_TX", serial.function));
        let rx_ok = ctx
            .design
            .assignment
            .iter()
            .any(|a| a.pin == serial.rx && a.function == format!("{}_RX", serial.function));
        if !tx_ok || !rx_ok {
            push(
                project,
                out,
                "peripheral_bus_unreachable",
                "error",
                format!(
                    "serial port {} needs {} on {} and {} on {} but the pin map does not match",
                    serial.port, serial.function, serial.tx, serial.function, serial.rx
                ),
                vec![id],
            );
        }
    }
}

fn check_5v_tolerance(ctx: &FirmwareContext, project: &Project, out: &mut Vec<Finding>) {
    let id = ctx.mcu_component.id;
    for assign in &ctx.design.assignment {
        let Some(pin): Option<&McuPin> = ctx.mcu.pin_by_name(&assign.pin) else {
            continue;
        };
        if pin.ft_5v {
            continue;
        }
        let Some(net_id) = pin_on_net(project, id, &assign.pin) else {
            continue;
        };
        let Some(name) = net_name(project, Some(net_id)) else {
            continue;
        };
        if is_high_rail_net(&name) {
            push(
                project,
                out,
                "peripheral_5v_tolerance",
                "error",
                format!(
                    "{} pin {} sits on net {} but the pad is not 5 V tolerant",
                    ctx.mcu_component.reference, assign.pin, name
                ),
                vec![id],
            );
        }
    }
}

fn check_boot_pins(ctx: &FirmwareContext, project: &Project, out: &mut Vec<Finding>) {
    let id = ctx.mcu_component.id;
    for pin in &ctx.mcu.pins {
        let Some(required) = &pin.boot_level else {
            continue;
        };
        let Some(net_id) = pin_on_net(project, id, pin.name.as_str()) else {
            continue;
        };
        let Some(name) = net_name(project, Some(net_id)) else {
            continue;
        };
        let high = name.to_ascii_uppercase().contains("3V3")
            || name.to_ascii_uppercase().contains("VDD")
            || name.to_ascii_uppercase().contains("+3");
        let low = name.to_ascii_uppercase().contains("GND");
        let wrong = (*required == "low" && high) || (*required == "high" && low);
        if wrong {
            push(
                project,
                out,
                "peripheral_boot_strap",
                "error",
                format!(
                    "boot pin {} must sit at {} at reset but net {} disagrees",
                    pin.name, required, name
                ),
                vec![id],
            );
        }
    }
}

fn check_debug_reuse(ctx: &FirmwareContext, project: &Project, out: &mut Vec<Finding>) {
    let id = ctx.mcu_component.id;
    for assign in &ctx.design.assignment {
        let Some(pin) = ctx.mcu.pin_by_name(&assign.pin) else {
            continue;
        };
        if pin.debug && assign.function == "GPIO" {
            push(
                project,
                out,
                "peripheral_debug_reuse",
                "warning",
                format!(
                    "{} pin {} is a debug pin but is used as GPIO",
                    ctx.mcu_component.reference, assign.pin
                ),
                vec![id],
            );
        }
    }
}

/// Rule-only peripheral checks; empty when no firmware design is attached.
pub fn peripheral(project: &Project) -> Vec<Finding> {
    let Ok(ctx) = load_firmware_context(project) else {
        return vec![];
    };
    let mut out = vec![];
    check_invalid_function(&ctx, project, &mut out);
    check_instance_conflicts(&ctx, project, &mut out);
    check_uart_sets(&ctx, project, &mut out);
    check_5v_tolerance(&ctx, project, &mut out);
    check_boot_pins(&ctx, project, &mut out);
    check_debug_reuse(&ctx, project, &mut out);
    out
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
                },
                Pin {
                    number: "PB2".into(),
                    name: "BOOT1".into(),
                    offset: pcbcraft_geom::Point { x: 0.0, y: 0.0 },
                    net: Some(11),
                    electrical_type: "input".into(),
                    direction: None,
                    unit: 1,
                },
            ],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![],
            sheet: 0,
        });
        project.nets.push(Net {
            id: 10,
            name: "+5V".into(),
            ..Net::default()
        });
        project.nets.push(Net {
            id: 11,
            name: "+3V3".into(),
            ..Net::default()
        });
        project
    }

    #[test]
    fn invalid_function_and_5v_and_boot_findings() -> Result<(), String> {
        let mut project = fixture_project();
        project.files[0].data = project.files[0]
            .data
            .replace("function = \"SPI1_SCK\"", "function = \"CAN1_TX\"");
        let findings = peripheral(&project);
        assert!(
            findings
                .iter()
                .any(|f| f.code == "peripheral_invalid_function"),
            "expected invalid function finding"
        );
        let findings = peripheral(&fixture_project());
        assert!(
            findings.iter().any(|f| f.code == "peripheral_5v_tolerance"),
            "expected 5 V tolerance finding"
        );
        assert!(
            findings.iter().any(|f| f.code == "peripheral_boot_strap"),
            "expected boot strap finding"
        );
        Ok(())
    }
}
