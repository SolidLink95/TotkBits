import os
import sys
import shutil
from pathlib import Path
import subprocess
import time
os.system('cls')

def remove_file_if_exists(file):
    x = Path(file)
    if x.exists() and x.is_file():
        run(["cmd", "/c", "del", "/f", "/q", str(x)])

def run(cmd):
    # subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
    subprocess.run(cmd,  check=True)
    
def delete_file(file):
    x = Path(file)
    if x.exists() and x.is_file():
        run(["cmd", "/c", "del", "/f", "/q", str(x)])

def prepare_main_rs(cwd):
    main_rs = cwd / "src-tauri/src/main.rs"
    assert(main_rs.exists())
    data = main_rs.read_text()
    data = data.replace("""// #![windows_subsystem = "windows"]""", """#![windows_subsystem = "windows"]""")
    main_rs.write_text(data)

def restore_main_rs(cwd):
    main_rs = cwd / "src-tauri/src/main.rs"
    assert(main_rs.exists())
    data = main_rs.read_text()
    data = data.replace("""#![windows_subsystem = "windows"]""", """// #![windows_subsystem = "windows"]""")
    main_rs.write_text(data)

def get_dotnet_exe_path() -> str:
    p = subprocess.run(["cmd","/c","where", "dotnet"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True, text=True)
    lines = p.stdout.splitlines()
    lines = [e for e in lines if e and Path(e).exists()]
    exe_x64 = next((e for e in lines if "x86" not in e), None)
    exe_x86 = next((e for e in lines if "x86" in e), None)
    if exe_x64 is not None:
        return exe_x64
    if exe_x86 is not None:
        return exe_x86
    return lines[0]

def merge_with_ilmerge(cwd:Path, publish_path:Path, exe_name:str):
    ilmerge_exe = Path(os.path.expandvars(r"%USERPROFILE%/.nuget\packages\ilmerge\3.0.41\tools\net452\ILMerge.exe"))
    if not ilmerge_exe.exists():
        print(f"[-] ILMerge not found at {str(ilmerge_exe)}")
        return
    exe_path = publish_path / f"{exe_name}.exe"
    if not exe_path.exists():
        print(f"[-] {exe_path} not found")
        return
    dlls = [str(e) for e in publish_path.glob("*.dll")]
    if not dlls:
        print(f"[-] No dlls found in {publish_path}")
        return
    print(f"[+] Merging assemblies with ILMerge")
    tmp_exe_path = Path(str(exe_path)[:-4] + "_tmp.exe").resolve()
    os.rename(str(exe_path), str(tmp_exe_path))
    assert(not exe_path.exists())
    assert(tmp_exe_path.exists())
    c = [
        str(ilmerge_exe),
        "/targetplatform:v4,C:\\Windows\\Microsoft.NET\\Framework\\v4.0.30319",
        "/target:exe",
        f"/out:{str(publish_path / exe_path.name)}",
        str(tmp_exe_path)
    ]
    c+=dlls
    p = subprocess.run(c, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if p.returncode !=0:
        print(f"[-] ILMerge failed with code {p.returncode}")
        print(p.stderr.decode())
        print(p.stdout.decode())
        sys.exit()
    for file in publish_path.glob("*"):
        if not file.is_file() or file.name == exe_name:
            continue
        remove_file_if_exists(file)
    print(f"[+] Merging completed")

def build_dotnet(cwd:Path):
    name = "DotNetWrapper"
    dotnet_exe = get_dotnet_exe_path()
    print(f"[+] Dotnet exe path: {dotnet_exe}")
    project_dir = cwd / f"ext_projects/{name}"
    publish_path = (project_dir / "publish" ).resolve()
    bin_path = (cwd / "src-tauri" / f"bin/cs").resolve()
    if bin_path.exists():
        shutil.rmtree(bin_path)
    bin_path.mkdir(parents=True, exist_ok=True)
    cs_source_path = Path(r"src-tauri\misc\DotNetWrapper.cs").resolve()
    cs_dest_path = (project_dir / "Program.cs").resolve()
    packages = ["Costura.Fody","Fody", "Newtonsoft.Json", "BfevLibrary", "YamlDotNet"]
    print(f"[+] Building dotnet wrapper")
    if project_dir.exists():
        print(f"[+] Deleting old project")
        shutil.rmtree(project_dir)
    os.chdir(str(project_dir.parent))
    run([dotnet_exe,"new",  "console", "-n" , name])
    os.chdir(str(project_dir))
    for package in packages:
        subprocess.run(["cmd", "/c", dotnet_exe, "add", "package", package], stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
    shutil.copyfile(cs_source_path, cs_dest_path)
    (project_dir / "FodyWeavers.xml").write_text("""<Weavers>
  <Costura />
</Weavers>""")
    print(f"[+] Copied {cs_source_path} to {cs_dest_path}")
    result = subprocess.run([
        dotnet_exe, "publish", project_dir,
        "-c", "Release",
        "-r", "win-x64",
        "-o", str(publish_path),
        "/p:PublishSingleFile=true",
        "/p:IncludeAllContentForSelfExtract=true",
        "/p:DebugType=None",
        "/p:CosturaIncludeDebugSymbols=false"
    ], capture_output=True, text=True)
    if result.returncode != 0:
        print("[-] Build failed:")
        print(result.stderr)
        print(result.stdout)
        sys.exit()
    assert(publish_path / f"{name}.exe").exists()
    # run([dotnet_exe, 'publish', '-c', 'Release', '-r', 'win-x64', '--self-contained', 'true', '-o', 'publish'])
    if bin_path.exists():
        shutil.rmtree(bin_path)
    # merge_with_ilmerge(cwd, publish_path, name)
    shutil.copytree(publish_path, bin_path)
    
    os.chdir(str(cwd))
    if project_dir.exists():
        print(f"[+] Deleting project")
        shutil.rmtree(project_dir)
    print("[+] Dotnet wrapper built")

def tauri_build():
    t1 = time.time()
    os.system("cls")
    cwd_str = os.getcwd()
    cwd  = Path(cwd_str)
    #Dotnet wrapper
    build_dotnet(cwd)
    # Updater
    print(f"[+] Building updater")
    os.chdir(str(cwd / "ext_projects/updater"))
    run(["cargo", "clean"])
    run(["cargo", "build", "--release"])
    os.chdir(cwd_str)
    dest_file = cwd / "src-tauri/updater.exe"
    delete_file(dest_file)
    # if dest_file.exists():
    #     run(["cmd", "/c", "del", "/f", "/q", str(dest_file)])
    shutil.copyfile(cwd / "ext_projects/updater/target/release/updater.exe", dest_file)
    assert(dest_file.exists())
    print(f"[+] Updater built")
    os.chdir(str(cwd / "src-tauri"))
    print(f"[+] Cleaning tauri project")
    run(["cargo", "clean"])
    print(f"[+] Preparing main.rs")
    prepare_main_rs(cwd)
    os.chdir(cwd_str)
    print(f"[+] Building tauri project")
    # run(["cargo", "tauri", "build",  "-- --release"])
    run(["cargo", "tauri", "build"])
    print(f"[+] Restoring main.rs")
    restore_main_rs(cwd)
    subprocess.run(["cmd", "/c", "start", "explorer", str(cwd / "src-tauri\\target\\release\\bundle\\nsis")])
    t2 = time.time()
    mins = int((t2-t1) // 60)
    secs = int((t2-t1) % 60)
    print(f"[+] Done in {mins} mins {secs} secs")
    
if __name__ == "__main__":
    tauri_build()
    