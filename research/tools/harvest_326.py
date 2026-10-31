#!/usr/bin/env python3
"""Fetch manufacturer datasheets and write manifest + part TOMLs for harvest #326."""

from __future__ import annotations

import sys
import urllib.error
import urllib.request
from datetime import date
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib  # type: ignore

ROOT = Path(__file__).resolve().parents[1]
LIB = ROOT / "sources" / "library"
DATASHEETS = LIB / "datasheets"
PARTS = LIB / "parts"
MANIFEST = LIB / "manifest.tsv"
GAPS = LIB / "gaps.toml"
CATALOG = Path(__file__).with_name("harvest_catalog.toml")

LICENSE_NOTE = "Manufacturer copyright, internal reference only"
USED_FOR = "#218 (harvest #326)"
USER_AGENT = "PCBCraft-KB-Harvest/326 (+https://github.com/wdunn001/pcbcraft)"

# Optional extra pin rows appended after the template (confirmed false until reviewed).
PIN_SNIPPETS: dict[str, list[str]] = {
    "rp2350": [
        '[[pin]]',
        'number = 1',
        'name = "IOVDD"',
        'function = "IO supply"',
        'kind = "power"',
        "page = 18",
        "confirmed = false",
        "",
        '[[pin]]',
        'number = 23',
        'name = "DVDD"',
        'function = "Core supply"',
        'kind = "power"',
        "page = 18",
        "confirmed = false",
        "",
    ],
    "esp32-s3": [
        '[[pin]]',
        'number = 3',
        'name = "VDDA"',
        'function = "Analog power"',
        'kind = "power"',
        "page = 28",
        "confirmed = false",
        "",
        '[[pin]]',
        'number = 4',
        'name = "GND"',
        'function = "Ground"',
        'kind = "ground"',
        "page = 28",
        "confirmed = false",
        "",
    ],
    "esp32-c3": [
        '[[pin]]',
        'number = 2',
        'name = "3V3"',
        'function = "Power supply"',
        'kind = "power"',
        "page = 20",
        "confirmed = false",
        "",
    ],
    "rp2040": [
        '[[pin]]',
        'name = "IOVDD"',
        'function = "IO supply"',
        'kind = "power"',
        "page = 17",
        "confirmed = false",
        "",
        '[[pin]]',
        'name = "DVDD"',
        'function = "Core supply"',
        'kind = "power"',
        "page = 17",
        "confirmed = false",
        "",
    ],
    "rp2040-hardware-design": [
        "[[requirement]]",
        'type = "decoupling"',
        'text = "Place 100nF close to each RP2040 supply pin; use bulk capacitance per power tree."',
        "page = 15",
        "confirmed = false",
        "",
        "[[requirement]]",
        'type = "crystal"',
        'text = "Follow crystal load, ground guard, and placement rules in the design guide."',
        "page = 18",
        "confirmed = false",
        "",
    ],
}


def load_catalog() -> tuple[list[dict], list[dict]]:
    data = tomllib.loads(CATALOG.read_text(encoding="utf-8"))
    return list(data.get("part", [])), list(data.get("gap", []))


def fetch_pdf(url: str, dest: Path, timeout: float = 90.0) -> None:
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        body = resp.read()
        ctype = resp.headers.get("Content-Type", "")
    if not body.startswith(b"%PDF"):
        raise ValueError(f"not a PDF (content-type={ctype!r}, len={len(body)})")
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_bytes(body)


def pdf_ok(path: Path) -> bool:
    if not path.is_file():
        return False
    head = path.read_bytes()[:4]
    return head == b"%PDF"


def rel_path(path: Path) -> str:
    repo_root = ROOT.parent
    return str(path.relative_to(repo_root)).replace("\\", "/")


def json_quote(s: str) -> str:
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def write_part_toml(entry: dict, pdf_rel: str, url: str) -> None:
    part_id = entry["id"]
    lines = [
        "[part]",
        f'manufacturer = {json_quote(entry["manufacturer"])}',
        f'mpn = {json_quote(entry["mpn"])}',
        f'datasheet_path = {json_quote(pdf_rel)}',
        f'datasheet_url = {json_quote(url)}',
        "",
        f"package = {json_quote(entry['package'])}",
        f"pin_count = {entry['pin_count']}",
        f"land_pattern_ref = {json_quote(entry['land_pattern_ref'])}",
        "",
        "[limits]",
        'abs_max_vdd = { value = "see datasheet", page = 1, confirmed = false }',
        'tj_max = { value = "see datasheet", page = 1, confirmed = false }',
        "",
    ]
    if part_id not in PIN_SNIPPETS:
        lines.extend(
            [
                "[[requirement]]",
                'type = "decoupling"',
                'text = "Follow manufacturer decoupling and layout guidance in the datasheet."',
                "page = 1",
                "confirmed = false",
                "",
            ]
        )
    else:
        lines.extend(PIN_SNIPPETS[part_id])
    PARTS.mkdir(parents=True, exist_ok=True)
    (PARTS / f"{part_id}.toml").write_text("\n".join(lines), encoding="utf-8")


def write_manifest(rows: list[list[str]]) -> None:
    header = [
        "topic",
        "kind",
        "path",
        "url",
        "title",
        "retrieved",
        "license",
        "used_for",
        "stored",
    ]
    with MANIFEST.open("w", encoding="utf-8") as f:
        f.write("\t".join(header) + "\n")
        for row in rows:
            f.write("\t".join(row) + "\n")


def write_gaps(gaps: list[dict]) -> None:
    blocks: list[str] = []
    for g in gaps:
        blocks.append(
            "\n".join(
                [
                    "[[gap]]",
                    f'id = {json_quote(g["id"])}',
                    f'manufacturer = {json_quote(g["manufacturer"])}',
                    f'mpn = {json_quote(g["mpn"])}',
                    f'reason = {json_quote(g["reason"])}',
                    "",
                ]
            )
        )
    GAPS.write_text("".join(blocks), encoding="utf-8")


def main() -> int:
    parts, gaps = load_catalog()
    retrieved = date.today().isoformat()
    manifest_rows: list[list[str]] = []
    failed: list[dict] = []

    for entry in parts:
        part_id = entry["id"]
        dest = DATASHEETS / entry["mfr_slug"] / f"{part_id}.pdf"
        url = entry["url"]
        try:
            if not pdf_ok(dest):
                fetch_pdf(url, dest)
            pdf_rel = rel_path(dest)
            write_part_toml(entry, pdf_rel, url)
            manifest_rows.append(
                [
                    "library",
                    "datasheet",
                    pdf_rel,
                    url,
                    entry["title"],
                    retrieved,
                    LICENSE_NOTE,
                    USED_FOR,
                    "true",
                ]
            )
            print(f"OK {part_id}")
        except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError, ValueError) as e:
            print(f"FAIL {part_id}: {e}", file=sys.stderr)
            failed.append(
                {
                    "id": part_id,
                    "manufacturer": entry["manufacturer"],
                    "mpn": entry["mpn"],
                    "reason": str(e),
                }
            )

    write_manifest(manifest_rows)
    write_gaps(gaps + failed)
    print(f"Stored {len(manifest_rows)} PDFs; gaps {len(gaps) + len(failed)}")
    return 0 if manifest_rows else 1


if __name__ == "__main__":
    raise SystemExit(main())
