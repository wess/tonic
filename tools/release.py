#!/usr/bin/env python3
"""Build a native compiler distribution for the current host."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parent.parent


def run(arguments, directory, environment):
    subprocess.run(arguments, cwd=directory, env=environment, check=True, timeout=900)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def build(output, archive_input=None, offline=False):
    version = tomllib.loads((ROOT / "compiler/Cargo.toml").read_text())["package"]["version"]
    for component in ("runtime", "shared"):
        assert tomllib.loads((ROOT / component / "Cargo.toml").read_text())["package"]["version"] == version
    system = {"Linux": "linux", "Darwin": "macos"}.get(platform.system())
    architecture = {"x86_64": "x64", "arm64": "arm64", "aarch64": "arm64"}.get(platform.machine())
    if not system or not architecture:
        raise SystemExit("release bundles currently support Linux and macOS on x64/arm64")
    environment = dict(os.environ, MIX_ENV="prod", PCRE2_SYS_STATIC="1")
    if system == "macos":
        environment["MACOSX_DEPLOYMENT_TARGET"] = "14.0"
    cargo_options = ["--offline"] if offline else []
    run(["cargo", "fetch", "--locked", *cargo_options], ROOT, environment)
    run(["cargo", "build", "--release", "--workspace", "--locked", *cargo_options], ROOT, environment)
    run(["python3", "tools/notices.py"], ROOT, environment)
    target = Path(environment.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not target.is_absolute():
        target = ROOT / target
    compiler = target / "release/tonic"
    actual = subprocess.check_output([compiler, "--version"], text=True).strip()
    assert actual == f"tonic {version}", actual
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="tonicbundle") as temporary:
        bundle = Path(temporary) / f"tonic{version}"
        for directory in ("bin", "lib", "share/tonic"):
            (bundle / directory).mkdir(parents=True)
        shutil.copy2(compiler, bundle / "bin/tonic")
        shutil.copy2(target / "release/libtonic_rt.a", bundle / "lib/libtonic_rt.a")
        archive = bundle / "share/tonic/tonic.ez"
        if archive_input is None:
            run(["mix", "archive.build", "-o", str(archive)], ROOT / "mix", environment)
        else:
            shutil.copy2(archive_input, archive)
        with zipfile.ZipFile(archive) as contents:
            application, = (name for name in contents.namelist() if name.endswith("/ebin/tonic.app"))
            assert f'{{vsn,"{version}"}}' in contents.read(application).decode(), "Mix archive version mismatch"
            base = application.removesuffix("ebin/tonic.app")
            for name in ("license", "notice"):
                assert contents.read(base + "priv/" + name) == (ROOT / "mix/priv" / name).read_bytes(), "Mix archive legal documents mismatch"
        for name in ("license", "notice", "README.md"):
            shutil.copy2(ROOT / name, bundle / name)
        shutil.copytree(ROOT / "licenses", bundle / "licenses")
        shutil.copytree(ROOT / "docs", bundle / "docs")
        shutil.copy2(ROOT / "tools/install.sh", bundle / "install.sh")
        shutil.copy2(ROOT / "examples/hello.exs", bundle / "hello.exs")
        metadata = {"version": version, "platform": system + architecture,
                    "files": {str(path.relative_to(bundle)): digest(path)
                              for path in sorted(bundle.rglob("*")) if path.is_file()}}
        (bundle / "manifest.json").write_text(json.dumps(metadata, indent=2) + "\n")
        artifact = output / f"tonic{version}.{system}{architecture}.tar.gz"
        epoch = int(environment.get("SOURCE_DATE_EPOCH", "0"))

        def normalize(info):
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            info.mtime = epoch
            return info

        with artifact.open("wb") as raw:
            with gzip.GzipFile(filename="", fileobj=raw, mode="wb", mtime=epoch) as compressed:
                with tarfile.open(fileobj=compressed, mode="w") as contents:
                    contents.add(bundle, arcname=bundle.name, filter=normalize)
        artifact.with_suffix(".sha256").write_text(f"{digest(artifact)}  {artifact.name}\n")
        print(artifact)
        return artifact


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "dist")
    parser.add_argument("--archive", type=Path, help="use an existing same-version Mix archive instead of rebuilding it")
    parser.add_argument("--offline", action="store_true", help="require all locked Cargo packages to be cached")
    options = parser.parse_args()
    build(options.output.resolve(), options.archive.resolve() if options.archive else None, options.offline)
