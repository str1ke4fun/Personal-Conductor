#!/bin/bash
# 文档同步监视进程 - 每 5 分钟扫描源码变更
# 用法: bash scripts/watch_changes.sh

PROJECT_ROOT="I:/personal-agent"
SYNC_DOC="$PROJECT_ROOT/docs/文档同步状态-20260527.md"
LAST_SCAN_FILE="$PROJECT_ROOT/.last_scan_timestamp"

# 初始化基线时间戳
if [ ! -f "$LAST_SCAN_FILE" ]; then
    stat -c "%Y" "$SYNC_DOC" > "$LAST_SCAN_FILE"
fi

echo "[watch] 文档同步监视进程启动 $(date)"
echo "[watch] 监控目录: crates/, apps/desktop/src-tauri/src/, apps/desktop/src/"

while true; do
    LAST_SCAN=$(cat "$LAST_SCAN_FILE" 2>/dev/null || echo "0")
    
    # 扫描 Rust 和 TypeScript 源码变更
    CHANGED_FILES=""
    while IFS= read -r f; do
        MTIME=$(stat -c "%Y" "$f" 2>/dev/null)
        if [ -n "$MTIME" ] && [ "$MTIME" -gt "$LAST_SCAN" ]; then
            CHANGED_FILES="$CHANGED_FILES$f\n"
        fi
    done < <(find "$PROJECT_ROOT/crates" "$PROJECT_ROOT/apps/desktop/src-tauri/src" "$PROJECT_ROOT/apps/desktop/src" \
        \( -name "*.rs" -o -name "*.tsx" -o -name "*.ts" \) 2>/dev/null)
    
    if [ -n "$CHANGED_FILES" ]; then
        echo ""
        echo "[watch] ===== 检测到文件变更 $(date) ====="
        echo -e "$CHANGED_FILES"
        echo "[watch] 变更文件数: $(echo -e "$CHANGED_FILES" | grep -c .)"
        echo "[watch] 请运行文档同步审查"
        echo "[watch] ======================================"
    fi
    
    # 更新扫描时间戳
    date +%s > "$LAST_SCAN_FILE"
    
    sleep 300
done
