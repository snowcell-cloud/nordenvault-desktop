"""Bundle a checksum-pinned release from the official restic distribution."""
import bz2
import hashlib
import io
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import urllib.request
import zipfile

VERSION = "0.19.1"
ASSETS = {
    "aarch64-apple-darwin": ("darwin_arm64.bz2", "7be0a144ccc377880f294204aa271d76e4b79554b42a751151d425ce6ebac143"),
    "x86_64-apple-darwin": ("darwin_amd64.bz2", "c38d579622cf602f665234c5a8c315030b6cf70656028fe6dc29a786b60e5f35"),
    "x86_64-unknown-linux-gnu": ("linux_amd64.bz2", "f415415624dcc452f2a02b8c33641791a8c6d6d3b65bbb3543fcf9a25151585c"),
    "x86_64-pc-windows-msvc": ("windows_amd64.zip", "da948ad707ed690426473aaba2046cd61f8f90f6f0e7dab6be0d5796531de67d"),
}

def prepare(target):
    asset, digest = ASSETS[target]
    url = f"https://github.com/restic/restic/releases/download/v{VERSION}/restic_{VERSION}_{asset}"
    with urllib.request.urlopen(url, timeout=120) as response:
        archive = response.read()
    if hashlib.sha256(archive).hexdigest() != digest:
        raise ValueError("Restic archive checksum mismatch")
    windows = asset.endswith(".zip")
    if windows:
        with zipfile.ZipFile(io.BytesIO(archive)) as files:
            executable, = [n for n in files.namelist() if n.lower().endswith(".exe")]
            binary = files.read(executable)
    else:
        binary = bz2.decompress(archive)
    directory = Path(__file__).resolve().parents[1] / "src-tauri" / "binaries"
    directory.mkdir(exist_ok=True)
    destination = directory / ("nordenvault-restic-" + target + (".exe" if windows else ""))
    with tempfile.NamedTemporaryFile(dir=directory, delete=False) as tmp:
        tmp.write(binary)
    os.chmod(tmp.name, 0o755)
    os.replace(tmp.name, destination)
    print(destination)

if __name__ == "__main__":
    host = subprocess.check_output(["rustc", "-vV"], text=True).split("host: ")[1].splitlines()[0]
    prepare(sys.argv[1] if len(sys.argv) > 1 else host)
