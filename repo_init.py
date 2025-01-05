import subprocess
from pathlib import Path
import shutil
import os, sys
import requests
try:
    from tqdm import tqdm # type: ignore
except ImportError:
    print("Install tqdm first using command: pip install tqdm")
    sys.exit(1)

def remove_file(file):
    x = Path(file)
    if x.exists() and x.is_file():
        subprocess.run(["cmd", "/c", "del", x.as_posix()], check=True)
        print(f"[+] Removed: {x.name}")

def rename_directory(source, new_name):
    source_path = Path(source)
    new_path = source_path.parent / new_name
    if not new_path.exists():
        source_path.rename(new_path)
        print(f"Directory renamed from {source_path} to {new_path}")
    return new_path

def create_directory(path):
    os.makedirs(path, exist_ok=True)

def download_file(url, local_path):
    # Create the directory if it doesn't exist
    create_directory(os.path.dirname(local_path))
    
    # Send a GET request to the URL
    response = requests.get(url, stream=True)
    # Check if the request was successful
    response.raise_for_status()
    
    # Get the total file size from the response headers
    total_size = int(response.headers.get('content-length', 0))
    
    # Open a local file for writing in binary mode
    with open(local_path, 'wb') as f, tqdm(
        desc=local_path,
        total=total_size,
        unit='iB',
        unit_scale=True,
        unit_divisor=1024,
    ) as bar:
        # Write the response content to the local file in chunks
        for chunk in response.iter_content(chunk_size=8192):
            f.write(chunk)
            bar.update(len(chunk))
    return local_path
    
    
def repo_init():
    cwd = os.getcwd()
    cwd_path = Path(cwd)
    bin_path = "src-tauri/bin"
    if not Path(f"{bin_path}/asb/asb.py").exists() or not Path(f"{bin_path}/ainb/ainb/ainb.py").exists():
        p = subprocess.run(["git", "submodule", "init"]);
        # if p.returncode != 0:
        #     raise Exception("[-] Failed to init git submodule")
        p = subprocess.run(["git", "submodule", "update", "--init", "--recursive"]);
        if p.returncode != 0:
            raise Exception("[-] Failed to update git submodule")
    file1 = "src-tauri/misc/asb.py"
    file2 = f"{bin_path}/asb/asb.py"
    shutil.copyfile(file1, file2)
    print(f"[+] Copied {file1} -> {file2}")
    
    tmp_path = Path("tmp")
    winpython_installer_exe = tmp_path / "winpython.exe"
    if not winpython_installer_exe.exists():
        print("[+] Downloading winpython")
        url = "https://github.com/winpython/winpython/releases/download/7.1.20240203final/Winpython64-3.11.8.0dot.exe"
        if tmp_path.exists():
            shutil.rmtree(str(tmp_path))
        tmp_path.mkdir(parents=True, exist_ok=True)
        winpython_installer_exe = str(winpython_installer_exe)
        download_file(url, winpython_installer_exe)
        
    #winpython_dir = Path(os.path.join(cwd, bin_path, "winpython"))
    winpython_dir = cwd_path / bin_path
    python_exe = next((e for e in winpython_dir.rglob("*.exe") if e.name=="python.exe"), None) if winpython_dir.exists() else None
    if python_exe is None:
        print("[+] Installing winpython")
        #p = subprocess.run([winpython_installer_exe, "/silent", f"/dir={winpython_dir}"])
        p = subprocess.run([winpython_installer_exe, f"-o{str(winpython_dir)}", f"-y"])
        if p.returncode != 0:
            raise Exception("[-] Failed to install winpython")
    
    winpython_dir = rename_directory(winpython_dir / "WPy64-31180", str(winpython_dir / "winpython"))
    
    python_exe = next((e for e in winpython_dir.rglob("*.exe") if e.name=="python.exe"), None)
    if python_exe is None:
        sys.exit("[-] Unable to find python.exe in winpython directory")
    python_exe_str = python_exe.as_posix()
    requirements_txt = "src-tauri/pip.txt"
    if not Path(requirements_txt).exists():
        sys.exit(f"[-] Unable to find pip.txt: {requirements_txt}")
    if not python_exe.exists():
        sys.exit(f"[-] Unable to find python.exe: {python_exe}")
    
    remove_file(winpython_installer_exe)
    print("[+] Intalling winpython dependencies") # python -m pip install --upgrade pip

    p = subprocess.run([python_exe_str, "-m", "pip", "install", "--upgrade", "pip"])
    p = subprocess.run([python_exe_str, "-m", "pip", "install", "-r", requirements_txt])
    if p.returncode != 0:
        raise Exception("[-] Failed to install winpython dependencies")
    print(f"[+] Copying compressed json files")
    
    for file in (cwd_path / "src-tauri/misc").glob("*.bin"):
        destfile = cwd_path / "src-tauri/bin" / file.name
        if not destfile.exists():
            print(f"Copying: {file.name}")
            shutil.copyfile(file, destfile)
    print(f"[+] Removing unused exe files")
    files_to_remove = [
            # "bin/winpython/python-3.11.8.amd64/Lib/site-packages/pip/_vendor/distlib/t64-arm.exe",
            # "bin/winpython/python-3.11.8.amd64/Lib/site-packages/pip/_vendor/distlib/w64-arm.exe",
            # "bin/winpython/python-3.11.8.amd64/Lib/site-packages/setuptools/cli-arm64.exe",
            # "bin/winpython/python-3.11.8.amd64/Lib/site-packages/setuptools/gui-arm64.exe",
            "src-tauri/bin/winpython/Jupyter Lab.exe",
            "src-tauri/bin/winpython/Jupyter Notebook.exe",
            "src-tauri/bin/winpython/Pyzo.exe",
            "src-tauri/bin/winpython/Qt Assistant.exe",
            "src-tauri/bin/winpython/Qt Linguist.exe",
            "src-tauri/bin/winpython/Qt Designer.exe",
            "src-tauri/bin/winpython/Spyder reset.exe",
            "src-tauri/bin/winpython/Spyder.exe",
            "src-tauri/bin/winpython/WinPython Terminal.exe",
            "src-tauri/bin/winpython/WinPython Powershell Prompt.exe",
            "src-tauri/bin/winpython/WinPython Interpreter.exe",
            "src-tauri/bin/winpython/WinPython Control Panel.exe",
            "src-tauri/bin/winpython/VS Code.exe",
            "src-tauri/bin/winpython/WinPython Command Prompt.exe",
            ]
    files_to_remove = [Path(f) for f in files_to_remove]
    files_to_remove += list(Path("src-tauri/bin/winpython/python-3.11.8.amd64/Lib/site-packages/pip/_vendor/distlib").glob("*.exe"))
    files_to_remove += list(Path("src-tauri/bin/winpython/python-3.11.8.amd64/Lib/site-packages/setuptools").glob("*.exe"))
    
    for file in files_to_remove:
        remove_file(file.resolve())
    
    if tmp_path.exists():
        shutil.rmtree(str(tmp_path))
    
    
    print("/nTotkbits initialized successfully. In order to build the project remember to install all other dependencies listed in README file")
        


if __name__=="__main__":
    repo_init()