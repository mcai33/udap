uDAP Programmer CLI 26.34.3
================================

适用系统：Windows 7 SP1 64 位及以上版本。

使用方法：
1. 将压缩包完整解压到本地磁盘。
2. 双击 udap-cli.exe 进入交互模式，或在 cmd.exe 中运行命令。
3. 使用 --help 查看所有命令和参数。

常用命令：
  udap-cli.exe probes
  udap-cli.exe detect
  udap-cli.exe targets STM32F103
  udap-cli.exe flash firmware.hex
  udap-cli.exe flash firmware.bin --target STM32F103C8Tx --base-address 0x08000000

注意：
- CMSIS-DAP v2 调试器可能需要正确安装 WinUSB 驱动。
- 同一时间请勿用其他烧录软件占用调试器。
- 本程序不包含 GUI，也不需要 WebView2。

