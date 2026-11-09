#!/usr/bin/env python3
"""Generate bundled STM32F405RG and STM32G431CB MCU tables (schema 1)."""
from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent

PROVENANCE_F405 = {
    "source_repository": "https://www.st.com/resource/en/reference_manual/rm0090-stm32f405407-stm32f415417-advanced-armbased-32bit-mcus-stmicroelectronics.pdf",
    "revision": "RM0090 Rev 19",
    "licence": "ST document licence (pin names and mux facts transcribed for PCBCraft)",
    "fetched": "2026-10-30",
}

PROVENANCE_G431 = {
    "source_repository": "https://www.st.com/resource/en/reference_manual/rm0440-stm32g4-series-advanced-armbased-32bit-mcus-stmicroelectronics.pdf",
    "revision": "RM0440 Rev 7",
    "licence": "ST document licence (pin names and mux facts transcribed for PCBCraft)",
    "fetched": "2026-10-30",
}

F405_LQFP64 = [
    "VBAT",
    "PC13",
    "PC14",
    "PC15",
    "PF0",
    "PF1",
    "NRST",
    "PC0",
    "PC1",
    "PC2",
    "PC3",
    "VDD",
    "VSS",
    "PF2",
    "PF3",
    "PF4",
    "PF5",
    "VSS",
    "VDD",
    "PA0",
    "PA1",
    "PA2",
    "PA3",
    "VSS",
    "VDD",
    "PA4",
    "PA5",
    "PA6",
    "PA7",
    "PC4",
    "PC5",
    "PB0",
    "PB1",
    "PB2",
    "PB10",
    "PB11",
    "VSS",
    "VDD",
    "PB12",
    "PB13",
    "PB14",
    "PB15",
    "PD8",
    "PD9",
    "PD10",
    "PD11",
    "PD12",
    "PD13",
    "PD14",
    "PD15",
    "PC6",
    "PC7",
    "PC8",
    "PC9",
    "PA8",
    "PA9",
    "PA10",
    "PA11",
    "PA12",
    "PA13",
    "VSS",
    "VDD",
    "PA14",
    "PA15",
]

G431_LQFP48 = [
    "VBAT",
    "PC13",
    "PC14",
    "PC15",
    "PF0",
    "PF1",
    "NRST",
    "PC0",
    "PC1",
    "PC2",
    "PC3",
    "VDD",
    "VSS",
    "PF2",
    "PA0",
    "PA1",
    "PA2",
    "PA3",
    "VSS",
    "VDD",
    "PA4",
    "PA5",
    "PA6",
    "PA7",
    "PC4",
    "PC5",
    "PB0",
    "PB1",
    "PB2",
    "PB10",
    "PB11",
    "VSS",
    "VDD",
    "PB12",
    "PB13",
    "PB14",
    "PB15",
    "PA8",
    "PA9",
    "PA10",
    "PA11",
    "PA12",
    "PA13",
    "VSS",
    "VDD",
    "PA14",
    "PA15",
    "VDD",
]

F405_MUX = {
    "PA0": [(1, "TIM2.CH1"), (2, "USART2.CTS"), (7, "USART1.CTS"), (8, "USART6.TX")],
    "PA1": [(1, "TIM2.CH2"), (2, "USART2.RTS"), (7, "USART1.RTS"), (8, "USART6.RX")],
    "PA2": [(1, "TIM2.CH3"), (2, "USART2.TX"), (7, "USART1.TX"), (8, "USART6.TX")],
    "PA3": [(1, "TIM2.CH4"), (2, "USART2.RX"), (7, "USART1.RX"), (8, "USART6.RX")],
    "PA4": [(5, "SPI1.NSS"), (6, "SPI3.NSS"), (7, "USART2.CK")],
    "PA5": [(1, "TIM2.CH1"), (5, "SPI1.SCK"), (6, "SPI3.SCK")],
    "PA6": [(1, "TIM3.CH1"), (5, "SPI1.MISO"), (6, "SPI3.MISO"), (13, "TIM13.CH1")],
    "PA7": [(1, "TIM3.CH2"), (5, "SPI1.MOSI"), (6, "SPI3.MOSI"), (14, "TIM14.CH1")],
    "PA8": [(1, "TIM1.CH1"), (7, "USART1.CK")],
    "PA9": [(1, "TIM1.CH2"), (7, "USART1.TX"), (10, "USB_OTG_FS.VBUS")],
    "PA10": [(1, "TIM1.CH3"), (7, "USART1.RX")],
    "PA11": [(1, "TIM1.CH4"), (10, "USB_OTG_FS.DM")],
    "PA12": [(1, "TIM1.ETR"), (10, "USB_OTG_FS.DP")],
    "PA13": [(7, "USART6.RX")],
    "PA14": [(7, "USART6.TX")],
    "PA15": [(5, "SPI1.NSS"), (6, "SPI3.NSS")],
    "PB0": [(1, "TIM3.CH3"), (2, "TIM8.CH2"), (5, "SPI5.SCK")],
    "PB1": [(1, "TIM3.CH4"), (2, "TIM8.CH3")],
    "PB2": [(5, "SPI3.MOSI")],
    "PB10": [(1, "TIM2.CH3"), (4, "I2C2.SCL"), (7, "USART3.TX")],
    "PB11": [(1, "TIM2.CH4"), (4, "I2C2.SDA"), (7, "USART3.RX")],
    "PB12": [(4, "I2C2.SMBALERT"), (5, "SPI2.NSS"), (7, "USART3.CK"), (9, "CAN2.RX")],
    "PB13": [(1, "TIM1.CH1N"), (4, "I2C2.SCL"), (5, "SPI2.SCK"), (9, "CAN2.TX")],
    "PB14": [(1, "TIM1.CH2N"), (4, "I2C2.SDA"), (5, "SPI2.MISO"), (9, "CAN2.RX")],
    "PB15": [(1, "TIM1.CH3N"), (5, "SPI2.MOSI"), (9, "CAN2.TX")],
    "PC6": [(1, "TIM3.CH1"), (2, "TIM8.CH1"), (7, "USART6.TX")],
    "PC7": [(1, "TIM3.CH2"), (2, "TIM8.CH2"), (7, "USART6.RX")],
    "PC8": [(1, "TIM3.CH3"), (2, "TIM8.CH3"), (7, "USART6.CK")],
    "PC9": [(1, "TIM3.CH4"), (2, "TIM8.CH4")],
    "PC10": [(5, "SPI3.SCK"), (7, "USART3.TX")],
    "PC11": [(5, "SPI3.MISO"), (7, "USART3.RX")],
    "PC12": [(5, "SPI3.MOSI"), (7, "USART3.CK")],
    "PD8": [(7, "USART3.TX")],
    "PD9": [(7, "USART3.RX")],
    "PD10": [(7, "USART3.CK")],
    "PD11": [(7, "USART3.CTS")],
    "PD12": [(7, "USART3.RTS")],
    "PF0": [(4, "I2C2.SDA")],
    "PF1": [(4, "I2C2.SCL")],
}

G431_MUX = {
    "PA0": [(1, "TIM2.CH1"), (2, "USART2.CTS"), (7, "USART1.CTS")],
    "PA1": [(1, "TIM2.CH2"), (2, "USART2.RTS"), (7, "USART1.RTS")],
    "PA2": [(1, "TIM2.CH3"), (2, "USART2.TX"), (7, "USART1.TX")],
    "PA3": [(1, "TIM2.CH4"), (2, "USART2.RX"), (7, "USART1.RX")],
    "PA4": [(5, "SPI1.NSS"), (7, "USART2.CK")],
    "PA5": [(1, "TIM2.CH1"), (5, "SPI1.SCK")],
    "PA6": [(1, "TIM3.CH1"), (5, "SPI1.MISO"), (13, "TIM13.CH1")],
    "PA7": [(1, "TIM3.CH2"), (5, "SPI1.MOSI"), (14, "TIM14.CH1")],
    "PA8": [(1, "TIM1.CH1"), (7, "USART1.CK")],
    "PA9": [(1, "TIM1.CH2"), (7, "USART1.TX")],
    "PA10": [(1, "TIM1.CH3"), (7, "USART1.RX")],
    "PA11": [(1, "TIM1.CH4"), (10, "USB.FS.DM")],
    "PA12": [(1, "TIM1.ETR"), (10, "USB.FS.DP")],
    "PA15": [(5, "SPI1.NSS")],
    "PB0": [(1, "TIM3.CH3"), (6, "TIM8.CH2")],
    "PB1": [(1, "TIM3.CH4"), (6, "TIM8.CH3")],
    "PB2": [(5, "SPI3.MOSI")],
    "PB10": [(1, "TIM2.CH3"), (4, "I2C2.SCL"), (7, "USART3.TX")],
    "PB11": [(1, "TIM2.CH4"), (4, "I2C2.SDA"), (7, "USART3.RX")],
    "PB12": [(4, "I2C2.SMBALERT"), (5, "SPI2.NSS"), (7, "USART3.CK")],
    "PB13": [(1, "TIM1.CH1N"), (4, "I2C2.SCL"), (5, "SPI2.SCK")],
    "PB14": [(1, "TIM1.CH2N"), (4, "I2C2.SDA"), (5, "SPI2.MISO")],
    "PB15": [(1, "TIM1.CH3N"), (5, "SPI2.MOSI")],
    "PC6": [(1, "TIM3.CH1"), (6, "TIM8.CH1")],
    "PC7": [(1, "TIM3.CH2"), (6, "TIM8.CH2")],
    "PC8": [(1, "TIM3.CH3"), (6, "TIM8.CH3")],
    "PC9": [(1, "TIM3.CH4"), (6, "TIM8.CH4")],
    "PF0": [(4, "I2C2.SDA")],
    "PF1": [(4, "I2C2.SCL")],
}

PERIPHERALS_F405 = [
    ("uart", "USART1", ["TX", "RX", "CK", "CTS", "RTS"]),
    ("uart", "USART2", ["TX", "RX", "CK", "CTS", "RTS"]),
    ("uart", "USART3", ["TX", "RX", "CK", "CTS", "RTS"]),
    ("uart", "USART6", ["TX", "RX", "CK", "CTS", "RTS"]),
    ("spi", "SPI1", ["NSS", "SCK", "MISO", "MOSI"]),
    ("spi", "SPI2", ["NSS", "SCK", "MISO", "MOSI"]),
    ("spi", "SPI3", ["NSS", "SCK", "MISO", "MOSI"]),
    ("spi", "SPI5", ["NSS", "SCK", "MISO", "MOSI"]),
    ("i2c", "I2C2", ["SCL", "SDA", "SMBALERT"]),
    ("can", "CAN2", ["RX", "TX"]),
    ("usb", "USB_OTG_FS", ["DM", "DP", "VBUS"]),
    ("timer", "TIM1", [], ["CH1", "CH2", "CH3", "CH4", "CH1N", "CH2N", "CH3N", "ETR"]),
    ("timer", "TIM2", [], ["CH1", "CH2", "CH3", "CH4"]),
    ("timer", "TIM3", [], ["CH1", "CH2", "CH3", "CH4"]),
    ("timer", "TIM8", [], ["CH1", "CH2", "CH3", "CH4"]),
    ("timer", "TIM13", [], ["CH1"]),
    ("timer", "TIM14", [], ["CH1"]),
]

PERIPHERALS_G431 = [
    ("uart", "USART1", ["TX", "RX", "CK", "CTS", "RTS"]),
    ("uart", "USART2", ["TX", "RX", "CK", "CTS", "RTS"]),
    ("uart", "USART3", ["TX", "RX", "CK", "CTS", "RTS"]),
    ("spi", "SPI1", ["NSS", "SCK", "MISO", "MOSI"]),
    ("spi", "SPI2", ["NSS", "SCK", "MISO", "MOSI"]),
    ("spi", "SPI3", ["NSS", "SCK", "MISO", "MOSI"]),
    ("i2c", "I2C2", ["SCL", "SDA", "SMBALERT"]),
    ("usb", "USB.FS", ["DM", "DP"]),
    ("timer", "TIM1", [], ["CH1", "CH2", "CH3", "CH4", "CH1N", "CH2N", "CH3N", "ETR"]),
    ("timer", "TIM2", [], ["CH1", "CH2", "CH3", "CH4"]),
    ("timer", "TIM3", [], ["CH1", "CH2", "CH3", "CH4"]),
    ("timer", "TIM8", [], ["CH1", "CH2", "CH3", "CH4"]),
    ("timer", "TIM13", [], ["CH1"]),
    ("timer", "TIM14", [], ["CH1"]),
]


def gpio_signal(name: str) -> str:
    if name.startswith(("VDD", "VSS", "VBAT", "NRST")):
        return ""
    if len(name) >= 3 and name[0] == "P" and name[1].isalpha() and name[2:].isdigit():
        return f"GPIO{name[1]}.{name[2:]}"
    return ""


def build_pins(package_pins: list[str], mux: dict[str, list[tuple[int, str]]]) -> list[dict]:
    rows = []
    for name in package_pins:
        row: dict = {"name": name, "alternate": []}
        if name in ("VDD", "VSS", "VBAT"):
            row["power_domain"] = name
        if name == "NRST":
            row["boot_strap"] = {"required_level": "high"}
        alts: list[tuple[int, str]] = []
        g = gpio_signal(name)
        if g:
            alts.append((0, g))
        for af, signal in mux.get(name, []):
            if not any(existing == af for existing, _ in alts):
                alts.append((af, signal))
        for af, signal in sorted(alts, key=lambda item: item[0]):
            row["alternate"].append({"af": af, "signal": signal})
        rows.append(row)
    return rows


def build_peripherals(spec: list[tuple]) -> list[dict]:
    out = []
    for kind, instance, signals, *rest in spec:
        channels = rest[0] if rest else []
        out.append(
            {
                "kind": kind,
                "instance": instance,
                "signals": signals,
                "channels": channels,
            }
        )
    return out


def emit_toml(doc: dict) -> str:
    lines = ["schema = 1", ""]
    lines.append("[provenance]")
    for k, v in doc["provenance"].items():
        lines.append(f'{k} = "{v}"')
    lines.append("")
    lines.append("[mcu]")
    for k, v in doc["mcu"].items():
        if isinstance(v, str):
            lines.append(f'{k} = "{v}"')
        else:
            lines.append(f"{k} = {v}")
    lines.append("")
    for pkg in doc["package"]:
        lines.append("[[package]]")
        lines.append(f'id = "{pkg["id"]}"')
        lines.append(f'name = "{pkg["name"]}"')
        lines.append('pins = [')
        for pin in pkg["pins"]:
            lines.append(f'  "{pin}",')
        lines.append("]")
        lines.append("")
    for per in doc["peripheral"]:
        lines.append("[[peripheral]]")
        lines.append(f'kind = "{per["kind"]}"')
        lines.append(f'instance = "{per["instance"]}"')
        if per["signals"]:
            lines.append("signals = [")
            for s in per["signals"]:
                lines.append(f'  "{s}",')
            lines.append("]")
        if per.get("channels"):
            lines.append("channels = [")
            for c in per["channels"]:
                lines.append(f'  "{c}",')
            lines.append("]")
        lines.append("")
    for pin in doc["pin"]:
        lines.append("[[pin]]")
        lines.append(f'name = "{pin["name"]}"')
        if pin.get("power_domain"):
            lines.append(f'power_domain = "{pin["power_domain"]}"')
        if pin.get("boot_strap"):
            bs = pin["boot_strap"]["required_level"]
            lines.append("[pin.boot_strap]")
            lines.append(f'required_level = "{bs}"')
        if pin.get("five_volt_tolerance"):
            lines.append("five_volt_tolerance = true")
        for alt in pin["alternate"]:
            lines.append("[[pin.alternate]]")
            lines.append(f'af = {alt["af"]}')
            lines.append(f'signal = "{alt["signal"]}"')
        lines.append("")
    return "\n".join(lines)


def build_doc(mcu_id: str, name: str, family: str, provenance: dict, package_id: str, package_name: str, pins: list[str], mux: dict, peripherals: list) -> dict:
    return {
        "provenance": provenance,
        "mcu": {
            "id": mcu_id,
            "name": name,
            "vendor": "STMicroelectronics",
            "family": family,
        },
        "package": [{"id": package_id, "name": package_name, "pins": pins}],
        "peripheral": build_peripherals(peripherals),
        "pin": build_pins(pins, mux),
        "dma": [],
    }


def main() -> None:
    f405 = build_doc(
        "stm32f405rg",
        "STM32F405RG",
        "STM32F4",
        PROVENANCE_F405,
        "lqfp64",
        "LQFP-64",
        F405_LQFP64,
        F405_MUX,
        PERIPHERALS_F405,
    )
    g431 = build_doc(
        "stm32g431cb",
        "STM32G431CB",
        "STM32G4",
        PROVENANCE_G431,
        "lqfp48",
        "LQFP-48",
        G431_LQFP48,
        G431_MUX,
        PERIPHERALS_G431,
    )
    (ROOT / "stm32f405rg.toml").write_text(emit_toml(f405), encoding="utf-8")
    (ROOT / "stm32g431cb.toml").write_text(emit_toml(g431), encoding="utf-8")
    bundle_dir = ROOT / "bundles"
    bundle_dir.mkdir(exist_ok=True)
    for chip_id, doc in [("stm32f405rg", f405), ("stm32g431cb", g431)]:
        (bundle_dir / f"{chip_id}.json").write_text(
            json.dumps(doc, indent=2) + "\n", encoding="utf-8"
        )
    print("wrote stm32f405rg.toml, stm32g431cb.toml, and JSON bundles")


if __name__ == "__main__":
    main()
