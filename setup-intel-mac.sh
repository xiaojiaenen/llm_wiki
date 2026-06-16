#!/bin/bash
# ============================================================
# setup-intel-mac.sh
# Intel Mac (x86_64) 构建 LLM Wiki 的准备脚本。
#
# 功能:
#   - 检查所有构建前置条件（Node.js, Rust, protoc, Xcode CLI Tools）
#   - 将 libpdfium.dylib 从 arm64 替换为 x86_64 版本
#
# 用法:
#   bash setup-intel-mac.sh          # 首次构建前运行
#   git stash && git pull && bash setup-intel-mac.sh && npm install && npx tauri build
#
# 详细文档: INTEL_MAC_BUILD.md
# ============================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PDFIUM_DIR="$SCRIPT_DIR/src-tauri/pdfium"
PDFIUM_DYLIB="$PDFIUM_DIR/libpdfium.dylib"
TMP_DIR="/tmp/pdfium-intel-mac-$$"
DOWNLOAD_URL="https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-mac-x64.tgz"

# ── 颜色输出 ──────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

ok()   { echo -e "${GREEN}✅ $*${NC}"; }
warn() { echo -e "${YELLOW}⚠️  $*${NC}"; }
err()  { echo -e "${RED}❌ $*${NC}"; }
info() { echo -e "${BLUE}ℹ️  $*${NC}"; }

# ── 架构检查 ──────────────────────────────────────────────────
echo ""
echo -e "${BLUE}🔧 LLM Wiki - Intel Mac 构建准备${NC}"
echo "=================================="

ARCH=$(uname -m)
if [ "$ARCH" != "x86_64" ]; then
    warn "当前架构是 $ARCH，不是 x86_64，此脚本不需要运行。"
    exit 0
fi
ok "架构: x86_64"

# ── 前置条件检查 ──────────────────────────────────────────────

ERRORS=0

# Node.js
if command -v node &>/dev/null; then
    NODE_VER=$(node --version | sed 's/v//' | cut -d. -f1)
    if [ "$NODE_VER" -ge 20 ] 2>/dev/null; then
        ok "Node.js $(node --version)"
    else
        err "Node.js 版本过低: $(node --version)，需要 v20+"
        info "安装: brew install node"
        ERRORS=$((ERRORS + 1))
    fi
else
    err "未找到 Node.js"
    info "安装: brew install node  或  https://nodejs.org"
    ERRORS=$((ERRORS + 1))
fi

# Rust
if command -v rustc &>/dev/null; then
    ok "Rust $(rustc --version | awk '{print $2}')"
else
    err "未找到 Rust"
    info "安装: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    ERRORS=$((ERRORS + 1))
fi

# protoc (LanceDB/prost 编译依赖)
if command -v protoc &>/dev/null; then
    ok "protoc $(protoc --version | awk '{print $2}')"
else
    warn "未找到 protoc，尝试通过 Homebrew 安装..."
    if command -v brew &>/dev/null; then
        brew install protobuf
        if command -v protoc &>/dev/null; then
            ok "protoc 已安装: $(protoc --version)"
        else
            err "protoc 安装失败"
            ERRORS=$((ERRORS + 1))
        fi
    else
        err "需要 protoc 且未安装 Homebrew"
        info "安装 Homebrew: https://brew.sh"
        info "然后运行: brew install protobuf"
        ERRORS=$((ERRORS + 1))
    fi
fi

# Xcode Command Line Tools
if xcode-select -p &>/dev/null; then
    ok "Xcode Command Line Tools"
else
    warn "未找到 Xcode Command Line Tools"
    info "安装: xcode-select --install"
    ERRORS=$((ERRORS + 1))
fi

# 如果有前置条件缺失，提前退出
if [ "$ERRORS" -gt 0 ]; then
    echo ""
    err "有 $ERRORS 个前置条件未满足，请先安装后重新运行此脚本。"
    exit 1
fi

# ── libpdfium.dylib 架构处理 ─────────────────────────────────

echo ""
echo -e "${BLUE}📦 检查 libpdfium.dylib 架构...${NC}"

if [ ! -f "$PDFIUM_DYLIB" ]; then
    err "未找到 $PDFIUM_DYLIB"
    info "请确认仓库完整性: git status"
    exit 1
fi

CURRENT_ARCH=$(file "$PDFIUM_DYLIB" 2>/dev/null | grep -o 'x86_64\|arm64' || echo "unknown")

if [ "$CURRENT_ARCH" = "x86_64" ]; then
    ok "libpdfium.dylib 已经是 x86_64，无需替换。"
    echo ""
    echo "🎉 一切就绪！可以开始构建:"
    echo "   npm install && npx tauri build"
    exit 0
fi

info "当前架构: $CURRENT_ARCH → 需要替换为 x86_64"

# 下载
mkdir -p "$TMP_DIR"
trap 'rm -rf "$TMP_DIR"' EXIT  # 退出时自动清理

echo "   正在下载 pdfium x86_64..."
if ! curl -sL "$DOWNLOAD_URL" -o "$TMP_DIR/pdfium-mac-x64.tgz"; then
    err "下载失败，请检查网络连接。"
    info "手动下载: $DOWNLOAD_URL"
    exit 1
fi

# 解压
tar xzf "$TMP_DIR/pdfium-mac-x64.tgz" -C "$TMP_DIR"
if [ ! -f "$TMP_DIR/lib/libpdfium.dylib" ]; then
    err "解压后未找到 libpdfium.dylib"
    exit 1
fi

# 验证下载的文件确实是 x86_64
DL_ARCH=$(file "$TMP_DIR/lib/libpdfium.dylib" | grep -o 'x86_64\|arm64' || echo "unknown")
if [ "$DL_ARCH" != "x86_64" ]; then
    err "下载的文件不是 x86_64 架构: $DL_ARCH"
    exit 1
fi

# 替换
cp "$TMP_DIR/lib/libpdfium.dylib" "$PDFIUM_DYLIB"
ok "libpdfium.dylib 已替换为 x86_64 版本"

echo ""
echo "🎉 准备完成！可以开始构建:"
echo "   npm install && npx tauri build"
