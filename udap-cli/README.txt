uDAP Programmer CLI 26.34.4
================================

适用系统：Windows 7 SP1 64 位及以上版本。

使用方法：
1. 将压缩包完整解压到本地磁盘。
2. 双击 udap-cli.exe 进入连续烧录模式，或在 cmd.exe 中运行命令。
3. 使用 --help 查看所有命令和参数。

连续烧录模式：
- 启动后只需选择一次调试器、目标、固件和烧录选项。
- 每次烧录结束后更换目标板，按 Enter 即可重复使用相同配置。
- 输入 q 可退出程序。
- 不自动识别芯片，目标仅提供 PY32F002B 和 PY32F071。

常用命令：
  udap-cli.exe probes
  udap-cli.exe targets
  udap-cli.exe flash firmware.hex --target PY32F002B
  udap-cli.exe flash firmware.bin --target PY32F071 --base-address 0x08000000

注意：
- CMSIS-DAP v2 调试器可能需要正确安装 WinUSB 驱动。
- 同一时间请勿用其他烧录软件占用调试器。
- 本程序不包含 GUI，也不需要 WebView2。
