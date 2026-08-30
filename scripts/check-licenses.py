#!/usr/bin/env python3
"""Fail when a resolved package lacks declared licensing metadata."""
import json
import subprocess

metadata = json.loads(subprocess.check_output([
    "cargo", "metadata", "--format-version", "1", "--locked"
]))
missing = []
forbidden = []
licenses = set()
for package in metadata["packages"]:
    license_expression = package.get("license")
    if not license_expression and not package.get("license_file"):
        missing.append(f"{package['name']} {package['version']}")
        continue
    expression = license_expression or "LICENSE-FILE"
    licenses.add(expression)
    upper = expression.upper()
    has_copyleft = any(token in upper for token in ("GPL", "AGPL", "SSPL"))
    has_permissive_choice = any(
        token in upper for token in ("MIT", "APACHE", "BSD", "ISC", "ZLIB", "UNICODE")
    )
    if has_copyleft and not has_permissive_choice:
        forbidden.append(f"{package['name']} {package['version']}: {expression}")
if missing:
    raise SystemExit("packages without license metadata:\n" + "\n".join(missing))
if forbidden:
    raise SystemExit("forbidden strong-copyleft license metadata:\n" + "\n".join(forbidden))
print(f"license metadata present for {len(metadata['packages'])} resolved packages")
print("expressions: " + ", ".join(sorted(licenses)))
