"""Cross-compile, package, and verify the fmn release archives."""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tarfile
import tempfile


ROOT = Path(__file__).resolve().parent.parent
TARGETS = {
    "x86_64-apple-darwin": ("darwin", "amd64"),
    "aarch64-apple-darwin": ("darwin", "arm64"),
    "x86_64-unknown-linux-musl": ("linux", "amd64"),
    "aarch64-unknown-linux-musl": ("linux", "arm64"),
    "x86_64-unknown-freebsd": ("freebsd", "amd64"),
    "aarch64-unknown-freebsd": ("freebsd", "arm64"),
}
THEMES = ROOT / "docs/themes"
VERSION = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?")
DOCUMENTS = ("README.md", "LICENSE", "CHANGELOG.md")


def archive_name(target):
    if target not in TARGETS:
        raise ValueError(f"Unsupported release target: {target}")
    return f"fmn-{target}.tar.xz"


def checksum(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def required_files(target):
    prefix = f"fmn-{target}"
    return {
        f"{prefix}/fmn",
        *(f"{prefix}/{name}" for name in DOCUMENTS),
        f"{prefix}/THEMES.md",
        *(f"{prefix}/licenses/{path.name}" for path in (THEMES / "licenses").glob("*.txt")),
    }


def validate_archive(directory, target):
    filename = archive_name(target)
    archive = directory / filename
    expected = f"{checksum(archive)}  {filename}\n"
    if Path(f"{archive}.sha256").read_text() != expected:
        raise ValueError(f"Missing or mismatched checksum for {archive.name}")
    prefix = f"fmn-{target}"
    with tarfile.open(archive) as source:
        files = set()
        names = set()
        for entry in source:
            path = PurePosixPath(entry.name)
            if (
                path.is_absolute()
                or ".." in path.parts
                or not path.parts
                or path.parts[0] != prefix
                or not (entry.isfile() or entry.isdir())
                or entry.name in names
            ):
                raise ValueError(f"Unsafe archive member: {entry.name}")
            names.add(entry.name)
            if entry.isfile():
                files.add(entry.name)
        missing = required_files(target) - files
        if missing:
            raise ValueError(f"Missing archive contents: {sorted(missing)}")
        if not source.getmember(f"{prefix}/fmn").mode & 0o111:
            raise ValueError("Packaged fmn is not executable")
    return archive


def package(binary, target, directory):
    directory.mkdir(parents=True, exist_ok=True)
    archive = directory / archive_name(target)
    prefix = f"fmn-{target}"
    with tarfile.open(archive, "w:xz") as output:
        output.add(binary, arcname=f"{prefix}/fmn")
        for name in DOCUMENTS:
            output.add(ROOT / name, arcname=f"{prefix}/{name}")
        output.add(THEMES / "THEMES.md", arcname=f"{prefix}/THEMES.md")
        output.add(THEMES / "licenses", arcname=f"{prefix}/licenses")
    Path(f"{archive}.sha256").write_text(f"{checksum(archive)}  {archive.name}\n")
    validate_archive(directory, target)



def build(version, directory):
    if version != "go-dev" and not VERSION.fullmatch(version):
        raise ValueError(f"Invalid release version: {version}")
    with tempfile.TemporaryDirectory(prefix="fmn-build-") as temporary:
        stage = Path(temporary)
        binary = stage / "fmn"
        archives = stage / "archives"
        for target, (system, arch) in TARGETS.items():
            print(f"Building {target}", flush=True)
            subprocess.run(
                ["go", "build", "-mod=readonly", "-trimpath",
                 "-ldflags", f"-s -w -X main.version={version}", "-o", binary, "./cmd/fmn"],
                cwd=ROOT, check=True,
                env={**os.environ, "CGO_ENABLED": "0", "GOOS": system, "GOARCH": arch},
            )
            package(binary, target, archives)
        (archives / "sha256.sum").write_text("".join(
            (archives / f"{archive_name(target)}.sha256").read_text() for target in TARGETS
        ))
        shutil.copyfile(ROOT / "bin/install.sh", archives / "fmn-installer.sh")
        directory.mkdir(parents=True, exist_ok=True)
        for file in archives.iterdir():
            shutil.copyfile(file, directory / file.name)


def verify(directory, target, version):
    archive = validate_archive(directory, target)
    with tempfile.TemporaryDirectory(prefix="fmn-release-") as temporary:
        root = Path(temporary)
        with tarfile.open(archive) as source:
            source.extractall(root, filter="data")
        binary = root / f"fmn-{target}" / "fmn"
        details = subprocess.check_output(["file", "-b", binary], text=True)
        print(details, end="")
        if TARGETS[target][0] != "darwin" and "statically linked" not in details:
            raise ValueError("Expected a statically linked release binary")
        environment = {
            **os.environ,
            "FMN_DATA_DIR": str(root / "history"),
            "FMN_CONFIG": str(root / "config.toml"),
        }
        environment.pop("FMN_DISABLE", None)

        def run(*arguments, **kwargs):
            return subprocess.check_output(
                [binary, *arguments], env=environment, text=True, **kwargs
            )

        if run("--version").strip() != f"fmn {version}":
            raise ValueError("Packaged fmn version does not match the requested version")
        run("doctor")
        run("_record-start", "--shell", "bash", "--session", "release-smoke",
            "--cwd", str(root), "--started-at", "1700000000000", input="echo release-smoke")
        run("_record-finish", "--session", "release-smoke", "--exit-code", "7",
            "--finished-at", "1700000000100")
        entries = json.loads(run("history", "list", "--json"))
        if len(entries) != 1 or entries[0]["command"] != "echo release-smoke" or entries[0]["exit_code"] != 7:
            raise ValueError("Packaged fmn failed the history round trip")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    compile_all = commands.add_parser("build")
    compile_all.add_argument("--version", default="go-dev")
    compile_all.add_argument("--directory", type=Path, default=ROOT / "bin/dist")
    check = commands.add_parser("verify")
    check.add_argument("--target", choices=TARGETS, required=True)
    check.add_argument("--directory", type=Path, required=True)
    check.add_argument("--version", required=True)
    args = parser.parse_args()
    if args.command == "build":
        build(args.version, args.directory)
    else:
        verify(args.directory, args.target, args.version)


if __name__ == "__main__":
    main()
