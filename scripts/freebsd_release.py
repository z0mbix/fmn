"""Package native FreeBSD builds and qualify the exact archives being released."""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import tarfile
import tempfile


ROOT = Path(__file__).resolve().parent.parent
TARGETS = ("x86_64-unknown-freebsd", "aarch64-unknown-freebsd")
DOCUMENTS = ("README.md", "LICENSE", "CHANGELOG.md")


def archive_name(target):
    if target not in TARGETS:
        raise ValueError(f"Unsupported FreeBSD target: {target}")
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
        *(f"{prefix}/licenses/{path.name}" for path in (ROOT / "src/tui/licenses").glob("*.txt")),
    }


def validate_archive(directory, target, staged=False):
    filename = archive_name(target)
    archive = directory / (f".input-{filename}" if staged else filename)
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
        output.add(ROOT / "src/tui/THEMES.md", arcname=f"{prefix}/THEMES.md")
        output.add(ROOT / "src/tui/licenses", arcname=f"{prefix}/licenses")
    Path(f"{archive}.sha256").write_text(f"{checksum(archive)}  {archive.name}\n")
    validate_archive(directory, target)
    # dist clears planned global output filenames before building extra assets.
    # Keep separate inputs in the workflow artifact. The generated publishing
    # glob (artifacts/*) excludes these dot-prefixed copies from the release.
    for name in (archive.name, f"{archive.name}.sha256"):
        shutil.copyfile(directory / name, directory / f".input-{name}")


def collect(source, destination, staged=False):
    # Validate both architectures before copying anything into dist's input.
    archives = [validate_archive(source, target, staged=staged) for target in TARGETS]
    destination.mkdir(parents=True, exist_ok=True)
    for target, archive in zip(TARGETS, archives):
        for suffix in ("", ".sha256"):
            shutil.copyfile(Path(f"{archive}{suffix}"), destination / f"{archive_name(target)}{suffix}")


def verify(directory, target):
    # The Ubuntu 22.04 global artifact collector uses Python 3.10. Only native
    # verification (with FreeBSD's packaged Python 3.11+) needs TOML parsing.
    import tomllib

    archive = validate_archive(directory, target)
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    with tempfile.TemporaryDirectory(prefix="fmn-release-") as temporary:
        root = Path(temporary)
        with tarfile.open(archive) as source:
            source.extractall(root, filter="data")
        binary = root / f"fmn-{target}" / "fmn"
        subprocess.run(["file", binary], check=True)
        linkage = subprocess.check_output(["ldd", binary], text=True)
        print(linkage, flush=True)
        # A bundled SQLite build must run without libraries installed by pkg.
        libraries = [line.partition("=>")[2].split()[0]
                     for line in linkage.splitlines() if "=>" in line]
        if "not found" in linkage or any(
            not library.startswith(("/lib/", "/usr/lib/", "/libexec/"))
            for library in libraries
        ):
            raise ValueError("FreeBSD release depends on a non-base-system library")
        environment = {
            **os.environ,
            "FMN_DATA_DIR": str(root / "history"),
            "FMN_CONFIG": str(root / "config.toml"),
            "TERM": "xterm-256color",
        }

        def run(*arguments, **kwargs):
            return subprocess.check_output(
                [binary, *arguments], env=environment, text=True, **kwargs
            )

        if run("--version").strip() != f"fmn {version}":
            raise ValueError("Packaged fmn version does not match Cargo.toml")
        run("doctor")
        run("_record-start", "--shell", "bash", "--session", "release-smoke",
            "--cwd", str(root), "--started-at", "1700000000000", input="echo release-smoke")
        run("_record-finish", "--session", "release-smoke", "--exit-code", "7",
            "--finished-at", "1700000000100")
        entries = json.loads(run("history", "list", "--json"))
        if len(entries) != 1 or entries[0]["command"] != "echo release-smoke" or entries[0]["exit_code"] != 7:
            raise ValueError("Packaged fmn failed the history round trip")
        # Use the packaged binary for shell insertion, opt-in execution, and
        # cancellation. Each shell gets its own database and config.
        for shell in ("bash", "zsh", "fish"):
            subprocess.run(
                ["expect", ROOT / "tests/support/freebsd_release.exp", binary, shell, root / shell],
                env=environment, check=True, timeout=90,
            )
        (root / "config.toml").write_text("[ui]\nexecute_on_select = true\n")
        for scenario in ("sigterm", "repeated-sigterm", "sigint", "sighup"):
            for stream in ("stdout", "stderr"):
                arguments = [binary, "search", "--cwd", str(root)]
                if stream == "stdout":
                    arguments.append("--shell-integration")
                subprocess.run(
                    [sys.executable, ROOT / "tests/support/terminal_recovery.py", scenario, stream, *arguments],
                    env=environment, check=True, timeout=30,
                )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    pack = commands.add_parser("package")
    pack.add_argument("--binary", type=Path, required=True)
    pack.add_argument("--target", choices=TARGETS, required=True)
    pack.add_argument("--directory", type=Path, default=ROOT / "target/freebsd")
    gather = commands.add_parser("collect")
    gather.add_argument("--source", type=Path, default=ROOT / "target/distrib")
    gather.add_argument("--directory", type=Path, default=ROOT / "target/freebsd")
    gather.add_argument("--staged-inputs", action="store_true")
    check = commands.add_parser("verify")
    check.add_argument("--target", choices=TARGETS, required=True)
    check.add_argument("--directory", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "package":
        package(args.binary, args.target, args.directory)
    elif args.command == "collect":
        collect(args.source, args.directory, staged=args.staged_inputs)
    else:
        verify(args.directory, args.target)


if __name__ == "__main__":
    main()
