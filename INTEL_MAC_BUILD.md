# Intel Mac (x86_64) 构建指南

> LLM Wiki 的 CI/CD 只为 Apple Silicon (aarch64) 构建 macOS 安装包。
> Intel Mac 用户需要从源码构建。本文档记录所有已知的差异和注意事项。

---

## 目录

- [分支结构](#分支结构)
- [前置条件](#前置条件)
- [首次构建](#首次构建)
- [已知问题与修复](#已知问题与修复)
- [更新仓库并重新构建](#更新仓库并重新构建)
- [故障排除](#故障排除)
- [变更日志](#变更日志)

---

## 分支结构

本仓库采用双分支策略，保持 fork 与上游同步：

| 分支 | 用途 | 来源 |
|------|------|------|
| `main` | 与上游仓库 `nashsu/llm_wiki` 保持一致 | 上游同步 |
| `intel-mac` | Intel Mac 构建所需的改动（pdfium 替换、本文档、准备脚本） | 本地维护 |

> `main` 分支**不包含**任何本地改动，方便直接从上游拉取更新。
> 所有 Intel Mac 相关的修改都在 `intel-mac` 分支上。

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
# 1. 克隆你的 fork
git clone https://github.com/xiaojiaenen/llm_wiki.git
cd llm_wiki

# 2. 添加上游仓库
git remote add upstream https://github.com/nashsu/llm_wiki.git

# 3. 切换到 Intel Mac 分支
git checkout intel-mac

# 4. 运行准备脚本（检查前置条件 + 替换 pdfium）
bash setup-intel-mac.sh

# 5. 安装依赖
npm install

# 6. 构建
npx tauri build
```

构建产物位于：

```
src-tauri/target/release/bundle/dmg/LLM Wiki_<version>_x64.dmg   # DMG 安装包
src-tauri/target/release/bundle/macos/LLM Wiki.app               # App 应用
```

首次打开 App（未签名）：

```bash
xattr -cr "src-tauri/target/release/bundle/macos/LLM Wiki.app"
open "src-tauri/target/release/bundle/macos/LLM Wiki.app"
```

---

## 已知问题与修复

### 1. libpdfium.dylib 架构不匹配 ⚠️

**问题**：上游仓库提交的 `src-tauri/pdfium/libpdfium.dylib` 是 **arm64** 架构，在 Intel Mac 上无法加载。

**表现**：运行时 PDF 相关功能（PDF 文本提取、图片提取）会崩溃或报错 `image not found`。

**修复**：`setup-intel-mac.sh` 会自动从 [bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries) 下载 x86_64 版本并替换。

**手动修复**：

```bash
curl -L "https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-mac-x64.tgz" -o /tmp/pdfium-mac-x64.tgz
mkdir -p /tmp/pdfium-extract && tar xzf /tmp/pdfium-mac-x64.tgz -C /tmp/pdfium-extract
cp /tmp/pdfium-extract/lib/libpdfium.dylib src-tauri/pdfium/libpdfium.dylib
file src-tauri/pdfium/libpdfium.dylib
# 应输出: Mach-O 64-bit dynamically linked shared library x86_64
```

### 2. protoc 未安装 ⚠️

**问题**：`lancedb` → `lance-encoding` → `prost-build` 在编译时需要 `protoc`（Protocol Buffers 编译器）。上游 README 的构建说明中未提及此依赖。

**表现**：`cargo build` 报错 `Could not find 'protoc'`。

**修复**：`brew install protobuf`

### 3. 构建时间较长

**问题**：Cargo.toml 中的 release profile 启用了 `lto = true` 和 `codegen-units = 1`，首次构建 Rust 部分可能需要 10-20 分钟。

**建议**：首次构建后增量编译会快很多。开发时可用 `npm run tauri dev`。

### 4. README 声明不准确

**问题**：上游 README 声称 CI/CD 会构建 "macOS (ARM + Intel)" 的安装包，但实际 CI 只构建 `aarch64-apple-darwin`。

**影响**：Intel Mac 用户无法从 GitHub Releases 下载预编译包，必须从源码构建。

---

## 更新仓库并重新构建

当上游仓库有新版本时，按以下步骤操作：

```bash
# 1. 同步上游到 main
git checkout main
git fetch upstream
git merge upstream/main

# 2. 将更新合并到 intel-mac 分支
git checkout intel-mac
git merge main

# 3. 重新运行准备脚本（pdfium 可能被上游更新覆盖）
bash setup-intel-mac.sh

# 4. 安装可能更新的依赖
npm install

# 5. 重新构建
npx tauri build
```

**一行命令版本**：

```bash
git checkout main && git fetch upstream && git merge upstream/main && git checkout intel-mac && git merge main && bash setup-intel-mac.sh && npm install && npx tauri build
```

> **为什么用分支而不是 stash？**
> - `main` 分支始终与上游一致，合并无冲突
> - `intel-mac` 分支的改动（文档、脚本）与上游代码不重叠，合并通常干净
> - `libpdfium.dylib` 的替换由脚本每次自动处理，不需要 git 追踪

---

## 故障排除

### PDF 功能不工作（文本提取、图片提取失败）

```bash
file "src-tauri/target/release/bundle/macos/LLM Wiki.app/Contents/Frameworks/libpdfium.dylib"
# 如果显示 arm64 → 重新运行 bash setup-intel-mac.sh
```

### 编译报错 "Could not find `protoc`"

```bash
brew install protobuf
# 如果已安装但仍报错：
export PROTOC=$(which protoc)
```

### 编译报错 linker 相关

```bash
xcode-select --install
```

### App 打开后提示 "已损坏" 或 "无法验证开发者"

```bash
xattr -cr "src-tauri/target/release/bundle/macos/LLM Wiki.app"
```

### LanceDB / Arrow 相关编译错误

```bash
brew update && brew upgrade
rustup update stable
```

### 构建中断后重新构建失败

```bash
cargo clean --manifest-path src-tauri/Cargo.toml
npx tauri build
```

### merge main 时与 intel-mac 分支冲突

通常不会冲突（改动的文件不重叠）。如果发生：

```bash
# 查看冲突文件
git status

# 解决冲突后
git add .
git commit
```

---

## 变更日志

记录 `setup-intel-mac.sh` 需要处理的平台差异项。当上游仓库引入新的平台相关依赖时，在此追加。

| 日期 | 问题 | 涉及文件 | 状态 |
|------|------|---------|------|
| 2026-06-16 | libpdfium.dylib 是 arm64，需替换为 x86_64 | `src-tauri/pdfium/libpdfium.dylib` | ✅ 脚本已处理 |
| 2026-06-16 | lancedb/prost 需要 protoc 编译器 | 系统依赖 | ✅ 脚本已检查 |

> **维护提示**：如果上游仓库新增了以下类型的改动，请更新本文档和 `setup-intel-mac.sh`：
> - 新增平台相关的二进制文件（如新的 native library）
> - 新增需要系统级安装的编译依赖
> - 修改了 Tauri 的 bundle 配置（如新增 framework）
> - CI 构建矩阵变更
