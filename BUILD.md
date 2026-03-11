# pyrunner 跨平台构建说明

本文档描述如何将 `pyrunner` 构建为几个主流平台可运行的二进制。

## 支持目标

当前构建方案覆盖：

- macOS Apple Silicon：`aarch64-apple-darwin`
- macOS Intel：`x86_64-apple-darwin`
- Linux x86_64 GNU：`x86_64-unknown-linux-gnu`
- Linux ARM64 GNU：`aarch64-unknown-linux-gnu`
- Windows x86_64 MSVC：`x86_64-pc-windows-msvc`

## 推荐方式

### 本地构建

使用仓库脚本：

```bash
./scripts/build-release.sh <target>
```

示例：

```bash
./scripts/build-release.sh x86_64-unknown-linux-gnu
./scripts/build-release.sh aarch64-apple-darwin
./scripts/build-release.sh x86_64-pc-windows-msvc
```

### 一次构建全部主流平台

```bash
./scripts/build-all.sh
```

## 依赖工具

### 通用

- `rustup`
- 对应 target：

```bash
rustup target add x86_64-unknown-linux-gnu
rustup target add aarch64-unknown-linux-gnu
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin
rustup target add x86_64-pc-windows-msvc
```

### Linux / macOS 交叉构建

推荐安装：

- `cargo-zigbuild`
- `zig`

```bash
cargo install cargo-zigbuild
brew install zig
```

### Windows 交叉构建

推荐安装：

- `cargo-xwin`

```bash
cargo install cargo-xwin
```

## 产物位置

本地脚本会把产物复制到：

```text
dist/
```

命名规则：

```text
pyrunner-<target>
pyrunner-<target>.exe
```

打包压缩后：

```text
pyrunner-<target>.tar.gz
pyrunner-<target>.zip
```

## GitHub Actions

仓库包含一套多平台构建 workflow：

```text
.github/workflows/release.yml
```

它会：

- 在 macOS / Linux / Windows runner 上构建
- 上传各平台 artifacts
- 在打 tag 时自动创建 release 并附带产物

建议发布流程：

```bash
git tag v0.1.0
git push origin v0.1.0
```

## 备注

- `rusqlite` 使用了 `bundled`，有利于跨平台发布
- 当前方案优先保证主流桌面 / 服务端平台分发
- 更复杂的 MUSL、静态 OpenSSL、通用 Linux 兼容性优化可以后续追加
