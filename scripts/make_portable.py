#!/usr/bin/env python3
"""Pack the release exe into a portable zip (no installer needed)."""
import os
import shutil
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXE = os.path.join(ROOT, "src-tauri", "target", "release", "fuck_ensp.exe")
VERSION = "0.1.0"
OUT_DIR = os.path.join(ROOT, "dist-portable")
OUT = os.path.join(OUT_DIR, f"fuck_ensp-v{VERSION}-windows-x64-portable.zip")

README = """fuck_ensp v{ver} - 便携版 (Windows x64)
=====================================

用法: 解压后右键 fuck_ensp.exe -> 以管理员身份运行
(程序内置 requireAdministrator, 双击通常也会直接提权)

- 单文件便携, 不写注册表, 卸载直接删 exe 即可
- 首次运行会把 hook 组件释放到 %LOCALAPPDATA%\\fuck_ensp\\bin
  日志写入 %LOCALAPPDATA%\\fuck_ensp\\hooks
- eNSP 相关: 打开"实时日志"页启动 eNSP 即可自动注入抓取协议日志
""".format(ver=VERSION)


def main():
    if not os.path.isfile(EXE):
        raise SystemExit(f"missing {EXE}, run `pnpm tauri build` first")
    os.makedirs(OUT_DIR, exist_ok=True)
    if os.path.isfile(OUT):
        os.remove(OUT)
    with zipfile.ZipFile(OUT, "w", zipfile.ZIP_DEFLATED) as z:
        z.write(EXE, "fuck_ensp.exe")
        z.writestr("README.txt", README)
    size = os.path.getsize(OUT)
    print(f"{OUT}  ({size / 1024 / 1024:.1f} MB)")
    with zipfile.ZipFile(OUT) as z:
        print("\n".join(f"  {i.filename}  {i.file_size / 1024 / 1024:.1f} MB" for i in z.infolist()))


if __name__ == "__main__":
    main()
