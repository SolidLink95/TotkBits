import os
import sys
import shutil
from pathlib import Path
import subprocess
import time


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

def tauri_build():
    t1 = time.time()
    os.system("cls")
    cwd_str = os.getcwd()
    cwd  = Path(cwd_str)
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
    