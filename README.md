# uDAP Programmer

跨平台 CMSIS-DAP 在线烧录工具，使用 Tauri 2、Svelte 5 和 probe-rs 构建。

## 开发

```bash
npm install
npm run tauri dev
```

Linux 开发环境需要 Tauri 的 WebKitGTK 依赖以及 `libudev-dev`。普通用户访问调试器时还需要安装合适的 udev rules。

## 当前功能

- 枚举 probe-rs 支持的调试器
- 自动探测目标 MCU，失败后从 probe-rs 内置目标库手动选择
- 烧录 ELF、HEX、BIN 和 UF2 固件
- 扇区/全片擦除、烧录后校验、完成后复位
- Windows、macOS 和 Linux GitHub Actions 构建

macOS 开发构建使用 ad-hoc 签名，首次在其他 Mac 打开时仍可能需要在“系统设置 → 隐私与安全性”中明确允许。正式公开分发需要配置 Developer ID Application 证书和 Apple 公证。
