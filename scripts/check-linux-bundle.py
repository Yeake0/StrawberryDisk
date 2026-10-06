#!/usr/bin/env python3
"""Check the final Linux bundles against the Ubuntu 22.04 baseline."""

from pathlib import Path
import platform
import re
import stat
import struct
import subprocess
import tempfile


MAX_GLIBC = (2, 35)
BUNDLE_DIR = Path("target/release/bundle")


def only_bundle(pattern: str) -> Path:
    matches = list(BUNDLE_DIR.glob(pattern))
    if len(matches) != 1:
        raise RuntimeError(f"expected one {pattern} bundle, found {len(matches)}")
    return matches[0].resolve()


def check_elf_versions(
    root: Path, expected_machine: int
) -> tuple[int, tuple[int, int], Path | None]:
    count = 0
    highest = (0, 0)
    highest_path = None
    for path in root.rglob("*"):
        if path.is_symlink() or not path.is_file():
            continue
        with path.open("rb") as source:
            header = source.read(20)
            if header[:4] != b"\x7fELF":
                continue
        if (
            header[:6] != b"\x7fELF\x02\x01"
            or struct.unpack("<H", header[18:20])[0] != expected_machine
        ):
            raise RuntimeError(f"{path.relative_to(root)} has the wrong ELF architecture")
        count += 1
        result = subprocess.run(
            ["objdump", "-T", str(path)], capture_output=True, text=True, check=True
        )
        versions = [
            tuple(map(int, match))
            for match in re.findall(r"GLIBC_(\d+)\.(\d+)", result.stdout)
        ]
        if versions and max(versions) > highest:
            highest = max(versions)
            highest_path = path.relative_to(root)
        if versions and max(versions) > MAX_GLIBC:
            required = max(versions)
            raise RuntimeError(
                f"{path.relative_to(root)} requires GLIBC_{required[0]}.{required[1]}"
            )
    return count, highest, highest_path


def main() -> None:
    deb = only_bundle("deb/*.deb")
    appimage = only_bundle("appimage/*.AppImage")
    expected_arch = {
        "x86_64": (62, "amd64"),
        "aarch64": (183, "arm64"),
    }.get(platform.machine())
    if expected_arch is None:
        raise RuntimeError(f"unsupported Linux architecture: {platform.machine()}")
    elf_machine, deb_arch = expected_arch
    actual_deb_arch = subprocess.run(
        ["dpkg-deb", "--field", str(deb), "Architecture"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    if actual_deb_arch != deb_arch:
        raise RuntimeError(f"DEB architecture is {actual_deb_arch}, expected {deb_arch}")
    with appimage.open("rb") as source:
        elf_header = source.read(20)
    if elf_header[:6] != b"\x7fELF\x02\x01" or struct.unpack("<H", elf_header[18:20])[0] != elf_machine:
        raise RuntimeError(f"AppImage is not a {platform.machine()} ELF: {appimage}")
    if not (appimage.stat().st_mode & stat.S_IXOTH):
        raise RuntimeError(f"AppImage is not executable: {appimage}")

    with tempfile.TemporaryDirectory(prefix="strawberrydisk-linux-bundle-") as tmp:
        root = Path(tmp)
        deb_root = root / "deb"
        deb_root.mkdir()
        subprocess.run(["dpkg-deb", "-x", str(deb), str(deb_root)], check=True)
        subprocess.run(
            [str(appimage), "--appimage-extract"],
            cwd=root,
            stdout=subprocess.DEVNULL,
            check=True,
        )
        image_root = root / "squashfs-root"
        for name in ("AppRun", "AppRun.tauri", "AppRun.wrapped"):
            mode = stat.S_IMODE((image_root / name).stat().st_mode)
            if mode != 0o755:
                raise RuntimeError(f"{name} mode is {mode:04o}, expected 0755")
        if (image_root / "AppRun").read_bytes() != Path(
            "scripts/appimage-apprun.sh"
        ).read_bytes():
            raise RuntimeError("AppRun does not use the verified GIO module launcher")
        gio_modules = image_root / "usr/lib" / f"{platform.machine()}-linux-gnu/gio/modules"
        if not gio_modules.is_dir():
            raise RuntimeError(f"bundled GIO modules are missing: {gio_modules}")
        for label, bundle_root in (("deb", deb_root), ("AppImage", image_root)):
            count, version, path = check_elf_versions(bundle_root, elf_machine)
            print(f"{label}: checked {count} ELF files, highest GLIBC_{version[0]}.{version[1]} ({path})")
    print("Linux bundle permissions and Ubuntu 22.04 glibc compatibility verified")


if __name__ == "__main__":
    main()
