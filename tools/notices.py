#!/usr/bin/env python3
"""Bundle exact locked-crate and build-toolchain attribution; no network access."""
import argparse
import hashlib
import json
import re
import subprocess
import tarfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def safe(name):
    return re.sub(r"[^a-z0-9.]", "", name.lower())


def add(files, path, data):
    if path in files and files[path] != data:
        raise RuntimeError(f"notice path collision: {path}")
    files[path] = data
    return {"file": path, "sha256": digest(data)}


def selected(expression):
    if expression == "(MIT OR Apache-2.0) AND Unicode-3.0":
        return "Apache-2.0 AND Unicode-3.0"
    if "Apache-2.0" in expression:
        return "Apache-2.0"
    if expression in ("MIT", "Unlicense OR MIT"):
        return "MIT"
    raise RuntimeError(f"license expression requires review: {expression}")


def collect():
    files = {}
    for document in json.loads((ROOT / "licenses/upstream.json").read_text())["documents"]:
        source = ROOT / document["file"]
        if not source.is_file() or digest(source.read_bytes()) != document["sha256"]:
            raise RuntimeError(f"upstream legal document missing/modified: {source}")
    if not (ROOT / "notice").is_file():
        raise RuntimeError("project notice is missing")
    add(files, "mix/priv/license", (ROOT / "license").read_bytes())
    metadata = json.loads(command("cargo", "metadata", "--locked", "--offline", "--format-version", "1"))
    version = next(p["version"] for p in metadata["packages"] if p["name"] == "tonic" and not p["source"])
    mixnotice = f"""Tonic Mix archive {version}
Copyright 2026 Tonic contributors

The original Tonic Mix task code is licensed under the Apache License,
Version 2.0. The complete license is provided in priv/license.

This archive provides tasks that invoke an external Tonic executable and
requires an existing Mix/Elixir installation. It does not bundle the Tonic
native runtime, PCRE2, Rust libraries, or imported Elixir/Erlang sources.
Those separately distributed components retain their own applicable notices.
"""
    add(files, "mix/priv/notice", mixnotice.encode())
    lock = (ROOT / "Cargo.lock").read_bytes()
    checksums = {(p["name"], p["version"]): p.get("checksum") for p in tomllib.loads(lock.decode())["package"]}
    inventory = {"cargolocksha256": digest(lock), "packages": []}
    pcre = None
    for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
        if not package["source"]:
            continue
        directory = Path(package["manifest_path"]).parent
        expected = checksums[(package["name"], package["version"])]
        archivepath = directory.parents[2] / "cache" / directory.parent.name / f"{package['name']}-{package['version']}.crate"
        if not archivepath.is_file() or digest(archivepath.read_bytes()) != expected:
            raise RuntimeError(f"published crate archive checksum mismatch/missing: {archivepath}; run cargo fetch --locked")
        archive = tarfile.open(archivepath)
        candidates = sorted(p for p in directory.iterdir() if p.is_file() and
                            any(word in p.name.lower() for word in ("license", "licence", "copying", "copyright", "notice", "authors")))
        if not candidates:
            raise RuntimeError(f"no upstream license/attribution files for {package['name']}")
        documents = []
        for source in candidates:
            data = source.read_bytes()
            member = archive.extractfile(f"{package['name']}-{package['version']}/{source.name}")
            if member is None or member.read() != data:
                raise RuntimeError(f"published crate license checksum mismatch: {source}")
            path = f"licenses/cargo/{safe(package['name'])}/{package['version']}/{safe(source.name)}.txt"
            document = add(files, path, data)
            document["upstreamfile"] = source.name
            documents.append(document)
        inventory["packages"].append({
            "name": package["name"], "version": package["version"],
            "license": package["license"], "selected": selected(package["license"]),
            "source": package["source"], "checksum": expected,
            "origin": f"https://crates.io/crates/{package['name']}/{package['version']}",
            "repository": package["repository"], "documents": documents,
        })
        archive.close()
        if package["name"] == "pcre2-sys":
            if package["version"] != "0.2.10":
                raise RuntimeError("pcre2-sys version changed; review notice and native license texts")
            pcre = directory
            pcrearchive = archivepath
    if pcre is None:
        raise RuntimeError("bundled PCRE2 dependency missing")
    header = (pcre / "upstream/include/pcre2.h").read_text()
    major = re.search(r"#define PCRE2_MAJOR\s+(\d+)", header)[1]
    minor = re.search(r"#define PCRE2_MINOR\s+(\d+)", header)[1]
    if (major, minor) != ("10", "46"):
        raise RuntimeError("bundled PCRE2 version changed: review licenses/pcre2/license.md")
    notices = {}
    published = tarfile.open(pcrearchive)
    for source in sorted((pcre / "upstream").rglob("*")):
        if not source.is_file():
            continue
        data = source.read_bytes()
        member = published.extractfile("pcre2-sys-0.2.10/" + str(source.relative_to(pcre)))
        if member is None or member.read() != data:
            raise RuntimeError(f"bundled PCRE2 source checksum mismatch: {source}")
        text = data.decode(errors="replace")
        for match in re.finditer(r"/\*.*?\*/", text, re.S):
            notice = match[0]
            if re.search(r"copyright", notice, re.I) and "Redistribution" in notice:
                notices.setdefault(notice, []).append(str(source.relative_to(pcre)))
    published.close()
    if not notices:
        raise RuntimeError("PCRE2/SLJIT source copyright notices missing")
    data = "\n\n".join("Sources: " + ", ".join(paths) + "\n" + text for text, paths in sorted(notices.items())).encode()
    add(files, "licenses/pcre2/sourcecopyrights.txt", data)
    sljit = (pcre / "upstream/deps/sljit/sljit_src/sljitLir.h").read_text()
    sljitversion = ".".join(re.search(rf"#define SLJIT_{part}_VERSION\s+(\d+)", sljit)[1] for part in ("MAJOR", "MINOR"))
    inventory["native"] = {"pcre2": f"{major}.{minor}", "sljit": sljitversion,
                           "license": "BSD-3-Clause WITH PCRE2-exception; SLJIT BSD-2-Clause",
                           "documents": ["licenses/pcre2/license.md", "licenses/pcre2/sourcecopyrights.txt", "licenses/unicode/license.txt"]}
    add(files, "licenses/cargo/inventory.json", (json.dumps(inventory, indent=2) + "\n").encode())

    sysroot = Path(command("rustc", "--print", "sysroot"))
    docs = sysroot / "share/doc/rust"
    copyright = docs / "COPYRIGHT-library.html"
    if not copyright.is_file():
        raise RuntimeError("Rust standard-library notices missing; run rustup component add rust-docs")
    add(files, "licenses/rust/copyright.html", copyright.read_bytes())
    documents = []
    for source in sorted((docs / "licenses").glob("*.txt")):
        documents.append(add(files, f"licenses/rust/licenses/{safe(source.name)}", source.read_bytes()))
    if not documents:
        raise RuntimeError("Rust toolchain license documents missing")
    rust = {"rustc": command("rustc", "-vV"), "documents": documents,
            "copyright": {"file": "licenses/rust/copyright.html", "sha256": digest(copyright.read_bytes())},
            "scope": "Rust standard library and its in-tree/third-party attribution; no compiler executable is redistributed."}
    add(files, "licenses/rust/inventory.json", (json.dumps(rust, indent=2) + "\n").encode())
    sources = json.loads((ROOT / "licenses/sources.json").read_text())
    known = {source["file"] for source in sources["sources"]}
    imported = {str(p.relative_to(ROOT)) for p in (ROOT / "lib/ex").glob("*.ex")}
    imported |= {str(p.relative_to(ROOT)) for p in (ROOT / "lib/erl").rglob("*") if p.is_file()}
    if imported - known:
        raise RuntimeError("new imported sources require provenance/license review: " + ", ".join(sorted(imported - known)))
    for source in sources["sources"]:
        path = ROOT / source["file"]
        data = path.read_bytes()
        if not any("Modified for Tonic;" in line for line in data.decode().splitlines()[:45]):
            raise RuntimeError(f"source modification/provenance notice missing: {path}")
        source["sha256"] = digest(data)
    add(files, "licenses/sources.json", (json.dumps(sources, indent=2) + "\n").encode())
    return files, len(inventory["packages"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify bundled notices against current lockfile and toolchain")
    options = parser.parse_args()
    files, packages = collect()
    failures = []
    for name, data in sorted(files.items()):
        destination = ROOT / name
        if options.check:
            if not destination.is_file() or destination.read_bytes() != data:
                failures.append(name)
        else:
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
    if failures:
        raise SystemExit("stale or missing notices:\n" + "\n".join(failures))
    print(f"{'verified' if options.check else 'generated'} notices for {packages} locked crates, PCRE2/SLJIT, and the active Rust toolchain")


if __name__ == "__main__":
    main()
