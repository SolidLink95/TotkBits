import subprocess
from pathlib import Path
import shutil
import os, sys, stat
from time import time
import requests
# from tauri_build import build_dotnet

try:
    from tqdm import tqdm  # type: ignore
except ImportError:
    print("Install tqdm first using command: pip install tqdm")
    sys.exit(1)
CWD = Path(__file__).parent.resolve()


def download_files():
    files = {
        "https://github.com/SolidLink95/TotkBits/releases/download/v0.0.9/xlink_tool.dll": "src-tauri/bin/dlls/xlink_tool.dll",
        # "https://github.com/SolidLink95/xlink2_bindings_rs/releases/download/0.1/xlink_tool.exp": "src-tauri/bin/dlls/xlink_tool.exp",
        # "https://github.com/SolidLink95/xlink2_bindings_rs/releases/download/0.1/xlink_tool.lib": "src-tauri/bin/dlls/xlink_tool.lib",
        "https://github.com/SolidLink95/oead/releases/download/v1.0/oead_byml_pipe.exe": "src-tauri/bin/cpp/oead_byml_pipe.exe",
        "https://github.com/SolidLink95/MeshCodec/releases/download/v1.0/meshcodec.dll": "src-tauri/bin/dlls/meshcodec.dll",
        "https://raw.githubusercontent.com/SolidLink95/roead/master/data/botw_hashed_names.txt": "src-tauri/bin/botw_hashed_names.txt",
        # "https://github.com/SolidLink95/MeshCodec/releases/download/v1.0/meshcodec.exp": "src-tauri/bin/dlls/meshcodec.exp",
        # "https://github.com/SolidLink95/MeshCodec/releases/download/v1.0/MeshCodec.lib": "src-tauri/bin/dlls/MeshCodec.lib",
    }
    if not files:
        return
    print("[+] Downloading files")
    for url, local_path in files.items():
        local_path = Path(local_path)
        local_path.parent.mkdir(parents=True, exist_ok=True)
        download_file(url, local_path)
    print(f"[+] Downloaded {len(files.keys())} files")


def remove_file(file):
    x = Path(file)
    if not x.is_file(): return
    file_str = str(x)
    try:
        subprocess.run(["cmd", "/c", "del", file_str], check=True)
        print(f"[+] Removed: {file_str}")
    except subprocess.CalledProcessError:
        print(f"[-] Failed to remove: {file_str}")


def rename_directory(source, new_name):
    source_path = Path(source)
    new_path = source_path.parent / new_name
    if not new_path.exists():
        source_path.rename(new_path)
        print(f"Directory renamed from {source_path} to {new_path}")
    return new_path


def download_file(url, local_path):
    # Create the directory if it doesn't exist
    Path(local_path).parent.mkdir(parents=True, exist_ok=True)

    # Send a GET request to the URL
    response = requests.get(url, stream=True)
    # Check if the request was successful
    response.raise_for_status()

    # Get the total file size from the response headers
    total_size = int(response.headers.get("content-length", 0))
    local_path = str(local_path)
    # Open a local file for writing in binary mode
    with open(local_path, "wb") as f, tqdm(
        desc=local_path,
        total=total_size,
        unit="iB",
        unit_scale=True,
        unit_divisor=1024,
    ) as bar:
        # Write the response content to the local file in chunks
        for chunk in response.iter_content(chunk_size=8192):
            f.write(chunk)
            bar.update(len(chunk))
    return local_path


def copy_files(bin_path):
    files_to_copy = {}
    for file1, file2 in files_to_copy.items():
        shutil.copyfile(file1, file2)
        print(f"[+] Copied {file1} -> {file2}")

def safe_copy(file1, file2, verbose=True):
    file1, file2 = Path(file1), Path(file2)
    if not file1.is_file(): return
    if file2.is_file(): return
    if verbose: print(f"Copying: {file1.name}")
    shutil.copyfile(file1, file2)
    
def safe_copy_dir(dir1, dir2, verbose=True):
    dir1, dir2 = Path(dir1), Path(dir2)
    if not dir1.is_dir(): return
    if dir2.is_dir() and next(dir2.glob("*"), None) is not None: return
    if verbose: print(f"Copying folder: {dir1.name}")
    shutil.copytree(dir1, dir2)

def npm_install():
    p = subprocess.run(["cmd", "/c", "npm", "install"], check=True)
    return p

def repo_init():
    cwd_path = Path(__file__).parent
    cwd = str(cwd_path)
    misc_dir = cwd_path / "src-tauri/misc"
    bin_path = cwd_path / "src-tauri/bin"
    dll_dir = bin_path / "dlls"
    if bin_path.is_dir():
        shutil.rmtree(bin_path)
    bin_path.mkdir(parents=True, exist_ok=True)
    (cwd_path / "tmp").mkdir(parents=True, exist_ok=True)

    print(f"[+] Installing npm dependencies")
    npm_install()
    print(f"[+] npm dependencies installed")
    print(f"[+] Copying compressed json files")

    # Copy zlib compressed json files
    dll_dir.mkdir(parents=True, exist_ok=True)
    for file in misc_dir.glob("*.dll"):
        safe_copy(file, dll_dir / file.name)
        
    files = list(misc_dir.glob("*.bin"))
    files += list(misc_dir.glob("*.json"))
    files += list(misc_dir.glob("*.txt"))
    for file in files:
        safe_copy(file, cwd_path / "src-tauri/bin" / file.name)

    # Copy directories
    dirs_to_copy = {}
    for src_dir, dest_dir in dirs_to_copy.items():
        safe_copy_dir(src_dir, dest_dir)

    # Download dlls
    download_files()

    print(
        "\n[+] Totkbits initialized successfully. In order to build the project remember to install all other dependencies listed in README file"
    )


if __name__ == "__main__":
    repo_init()
