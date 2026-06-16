# Intel Mac (x86_64) 构建指南

> LLM Wiki 的 CI/CD 只为 Apple Silicon (aarch64) 构建 macOS 安装包。
> Intel Mac 用户需要从源码构建。本文档记录所有已知的差异和注意事项。

---

## 目录

- [前置条件](#前置条件)
- [首次构建](#首次构建)
- [已知问题与修复](#已知问题与修复)
- [更新仓库并重新构建](#更新仓库并重新构建)
- [故障排除](#故障排除)
- [变更日志](#变更日志)

---

## 前置条件

| 工具 | 最低版本 | 安装方式 |
|------|---------|---------|
| **Node.js** | 20+ | `brew install node` 或 https://nodejs.org |
| **Rust** | 1.70+ | https://rustup.rs |
| **protoc** | 3.x | `brew install protobuf` |
| **Xcode Command Line Tools** | - | `xcode-select --install` |
| **Homebrew** | - | https://brew.sh |

验证：

```bash
node --version    # v20.x+
rustc --version   # 1.70+
protoc --version  # libprotoc 3.x
```

---

## 首次构建

```bash
# 1. 克隆仓库
git clone https://github.com/user/llm-wiki.git
cd llm-wiki

# 2. 运行 Intel Mac 准备脚本（处理所有平台差异）
bash setup-intel-mac.sh

# 3. 安装前端依赖
npm install

# 4. 构建
npx tauri build
```

构建产物位于：

```
src-tauri/target/release/bundle/dmg/LLM Wiki_<version>_x64.dmg   # DMG 安装包
src-tauri/target/release/bundle/macos/LLM Wiki.app               # App 应用
```

首次打开 App（未签名）：

```bash
# 右键 → 打开 → 弹窗中点击「打开」
# 或者命令行：
xattr -cr "src-tauri/target/release/bundle/macos/LLM Wiki.app"
open "src-tauri/target/release/bundle/macos/LLM Wiki.app"
```

---

## 已知问题与修复

### 1. libpdfium.dylib 架构不匹配 ⚠️

**问题**：仓库提交的 `src-tauri/pdfium/libpdfium.dylib` 是 **arm64** 架构，在 Intel Mac 上无法加载。

**表现**：运行时 PDF 相关功能（PDF 文本提取、图片提取）会崩溃或报错 `image not found`。

**修复**：`setup-intel-mac.sh` 会自动从 [bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries) 下载 x86_64 版本并替换。

**手动修复**：

```bash
# 下载 x86_64 版本
curl -L "https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-mac-x64.tgz" -o /tmp/pdfium-mac-x64.tgz
mkdir -p /tmp/pdfium-extract && tar xzf /tmp/pdfium-mac-x64.tgz -C /tmp/pdfium-extract

# 替换
cp /tmp/pdfium-extract/lib/libpdfium.dylib src-tauri/pdfium/libpdfium.dylib

# 验证
file src-tauri/pdfium/libpdfium.dylib
# 应输出: Mach-O 64-bit dynamically linked shared library x86_64
```

### 2. protoc 未安装 ⚠️

**问题**：`lancedb` → `lance-encoding` → `prost-build` 在编译时需要 `protoc`（Protocol Buffers 编译器）。仓库 README 的构建说明中未提及此依赖。

**表现**：`cargo build` 报错 `Could not find 'protoc'`。

**修复**：

```bash
brew install protobuf
```

### 3. 构建时间较长

**问题**：Cargo.toml 中的 release profile 启用了 `lto = true` 和 `codegen-units = 1`，编译优化级别高，首次构建 Rust 部分可能需要 10-20 分钟。

**建议**：首次构建后，增量编译会快很多。如果不需要极致优化，开发时可用 `npm run tauri dev`。

### 4. README 声明不准确

**问题**：README 声称 CI/CD 会构建 "macOS (ARM + Intel)" 的安装包，但实际上 CI 只构建 `aarch64-apple-darwin`（Apple Silicon）。没有 Intel Mac 的 CI 构建任务。

**影响**：Intel Mac 用户无法从 GitHub Releases 下载预编译包，必须从源码构建。

---

## 更新仓库并重新构建

当仓库有新版本时，按以下步骤操作：

```bash
# 1. 保存本地改动（libpdfium.dylib 等）
git stash

# 2. 拉取最新代码
git pull

# 3. 重新运行准备脚本（会自动处理 pdfium 替换和 protoc 检查）
bash setup-intel-mac.sh

# 4. 安装可能更新的依赖
npm install

# 5. 重新构建
npx tauri build
```

**一行命令版本**：

```bash
git stash && git pull && bash setup-intel-mac.sh && npm install && npx tauri build
```

> **为什么需要 stash？**
> 仓库跟踪的 `libpdfium.dylib` 是 arm64 版本，我们本地替换成了 x86_64。
> `git pull` 时如果上游更新了这个文件，会产生冲突。
> `git stash` 会临时保存我们的本地修改，pull 完成后由 `setup-intel-mac.sh` 重新处理。

---

## 故障排除

### PDF 功能不工作（文本提取、图片提取失败）

**原因**：pdfium dylib 架构不对。

```bash
# 检查 App 内的 pdfium 架构
file "src-tauri/target/release/bundle/macos/LLM Wiki.app/Contents/Frameworks/libpdfium.dylib"
# 如果显示 arm64 → 重新运行 bash setup-intel-mac.sh
```

### 编译报错 "Could not find `protoc`"

```bash
brew install protobuf
# 如果已安装但仍报错，设置环境变量：
export PROTOC=$(which protoc)
```

### 编译报错 linker 相关

确保安装了 Xcode Command Line Tools：

```bash
xcode-select --install
```

### App 打开后提示 "已损坏" 或 "无法验证开发者"

```bash
xattr -cr "src-tauri/target/release/bundle/macos/LLM Wiki.app"
```

### LanceDB / Arrow 相关编译错误

LanceDB 的 C++ 子依赖可能对编译环境敏感。确保：

```bash
# 更新 Homebrew 和编译工具
brew update && brew upgrade

# 确保使用稳定的 Rust 工具链
rustup update stable
```

### 构建中断后重新构建失败

```bash
# 清理 Rust 构建缓存（会重新编译，比较慢）
cargo clean --manifest-path src-tauri/Cargo.toml
npx tauri build
```

---

## 变更日志

记录 `setup-intel-mac.sh` 需要处理的平台差异项。当仓库引入新的平台相关依赖时，在此追加。

| 日期 | 问题 | 涉及文件 | 状态 |
|------|------|---------|------|
| 2026-06-16 | libpdfium.dylib 是 arm64，需替换为 x86_64 | `src-tauri/pdfium/libpdfium.dylib` | ✅ 脚本已处理 |
| 2026-06-16 | lancedb/prost 需要 protoc 编译器 | 系统依赖 | ✅ 脚本已检查 |

> **维护者提示**：如果仓库新增了以下类型的改动，请更新本文档和 `setup-intel-mac.sh`：
> - 新增平台相关的二进制文件（如新的 native library）
> - 新增需要系统级安装的编译依赖
> - 修改了 Tauri 的 bundle 配置（如新增 framework）
> - CI 构建矩阵变更
