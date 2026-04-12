#!/bin/bash
set -euo pipefail

# Hook script: auto-trigger pi-rust doc updates when code changes are detected.
# This script runs on AgentEnd/Stop and spawns a background Claude session
# to keep README progress and usage docs in sync.

PROJECT_DIR="/Users/ruipu/projects/Rusting/pi-rust"
cd "$PROJECT_DIR"

# Check if there are any recent code changes worth documenting.
# We look at files changed in the current HEAD vs previous, or uncommitted changes.
CHANGED_FILES=$(git diff --name-only HEAD 2>/dev/null || true)
if [ -z "$CHANGED_FILES" ]; then
  CHANGED_FILES=$(git diff --name-only HEAD~1 HEAD 2>/dev/null || true)
fi

# Only proceed if crates/ or src/ were touched
if echo "$CHANGED_FILES" | grep -qE '^(crates/|src/)'; then
  # Spawn a background Claude session with the update agent.
  # Note: this creates a short-lived, non-interactive Claude Code session.
  nohup claude -p "请以 pi-rust-update-agent agent 的身份，使用 pi-rust-doc-updater skill 检查并更新项目进度文档和 usage 文档。只修改 README.md 和 docs/ 下的文件。" "$PROJECT_DIR" > /dev/null 2>&1 &
fi
