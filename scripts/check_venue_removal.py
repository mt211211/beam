#!/usr/bin/env python3
"""Reject operational PumpSwap/Mayhem bindings; preserve reference IDLs."""
import json
import re
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "programs/beam/src"
foreign_ids = [
    json.loads((ROOT / "programs/beam/idls/pump_amm.json").read_text(encoding="utf-8"))["address"],
    json.loads((ROOT / "programs/beam/idls/mayhem.json").read_text(encoding="utf-8"))["address"],
]
forbidden = [
    r"declare_program!\s*\(\s*(pump_amm|mayhem)\s*\)",
    r"\b(pump_amm|mayhem|mayhem_interface)::(cpi|program|accounts|types|ID)\b",
    r"seeds::program\s*=\s*(MAYHEM_ID|pump_amm\.key)",
]
failures = []
for path in SOURCE.rglob("*.rs"):
    if path.name == "beam_pumpswap.rs":
        if "#[cfg(test)] pub mod beam_pumpswap;" not in (SOURCE / "lib.rs").read_text(encoding="utf-8"):
            failures.append("historical rejection-test module must remain cfg(test)")
        continue
    text = path.read_text(encoding="utf-8")
    if any(re.search(pattern, text) for pattern in forbidden) or any(value in text for value in foreign_ids):
        failures.append(path.relative_to(ROOT).as_posix())
for folder in ["sdk", "web/src"]:
    for path in (ROOT / folder).rglob("*"):
        if path.is_file() and not any(part in {"node_modules", "target"} for part in path.parts):
            if any(value in path.read_text(encoding="utf-8") for value in foreign_ids):
                failures.append(path.relative_to(ROOT).as_posix())
if failures:
    raise SystemExit("Operational foreign venue binding found: " + ", ".join(failures))
print("No operational PumpSwap/Mayhem bindings or program ids in Beam source/SDK/preview.")
print("Reference archives/IDLs remain immutable; rejection-test ids are cfg(test) only.")
