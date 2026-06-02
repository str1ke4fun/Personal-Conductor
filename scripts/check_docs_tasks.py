#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
模块名: 文档任务检查
功能描述: 检查docs目录中未完成的任务并生成报告
开发阶段: 已完成
实现度: 100%
依赖模块: 无
相关文件: docs/*.md
"""

import os
import re
import logging

logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s %(name)s %(levelname)s %(message)s'
)
logger = logging.getLogger(__name__)

def check_checkbox_tasks(file_path: str) -> dict:
    """
    检查Markdown文件中的checkbox任务状态
    
    Args:
        file_path: 文件路径
        
    Returns:
        包含完成和未完成任务的字典
    """
    with open(file_path, 'r', encoding='utf-8') as f:
        content = f.read()
    
    completed = []
    incomplete = []
    
    # 匹配checkbox格式: - [x] 任务 或 - [ ] 任务
    pattern = r'-\s*\[([ xX])\]\s*(.+?)(?=\n-|$)'
    matches = re.findall(pattern, content, re.DOTALL)
    
    for status, task in matches:
        task = task.strip()
        if status.lower() == 'x':
            completed.append(task)
        else:
            incomplete.append(task)
    
    # 也匹配带数字序号的checkbox
    pattern_num = r'(\d+)\.\s*\[([ xX])\]\s*(.+?)(?=\n\d+\.|$)'
    matches_num = re.findall(pattern_num, content, re.DOTALL)
    
    for num, status, task in matches_num:
        task = task.strip()
        if status.lower() == 'x':
            completed.append(f"{num}. {task}")
        else:
            incomplete.append(f"{num}. {task}")
    
    return {
        'completed': completed,
        'incomplete': incomplete,
        'total': len(completed) + len(incomplete)
    }

def analyze_live2d_timeline():
    """
    分析Live2D个人推进清单的完成状态
    """
    file_path = r"i:\personal-agent\docs\Live2D-个人推进清单.md"
    result = check_checkbox_tasks(file_path)
    
    logger.info("=" * 60)
    logger.info("Live2D 个人推进清单 - 完成状态分析")
    logger.info("=" * 60)
    
    # 按阶段分类分析
    with open(file_path, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # 找出所有关键里程碑
    milestones = {
        "A1 - 拿到Hiyori资源": "整个目录复制到 assets/live2d/hiyori/",
        "A2 - Hiyori license检查": "在 assets/live2d/hiyori/ 下放 LICENSE-NOTE.md",
        "A3 - Editor里熟悉模型": "录一段5秒视频，能看出\"活的\"",
        "A4 - 准备4状态映射": "Round 2 派工单 T7 直接抄它实现",
        "A5 - pixi-live2d-display引Hiyori": "从控制台手动触发motion/expression切换",
        "A6 - 接进Tauri webview": "A车道在这一步切到Round 2派工单",
        "A7 - 物理摆动 + 跟踪鼠标": "鼠标移动，眼睛和头部跟随"
    }
    
    logger.info("\n【车道 A - Hiyori管道】")
    logger.info("-" * 60)
    
    # 检查当前代码中已实现的功能
    canvas_path = r"i:\personal-agent\apps\desktop\src\live2d\Live2DCanvas.tsx"
    with open(canvas_path, 'r', encoding='utf-8') as f:
        canvas_content = f.read()
    
    a_tasks = {
        "A1 - 资源引用": "已完成" if "hiyori_free_t08.model3.json" in canvas_content else "未完成",
        "A5 - 鼠标追踪": "已实现" if "model.focus" in canvas_content else "未实现",
        "A7 - PIXI交互": "已实现" if "app.stage.interactive = true" in canvas_content else "未实现",
    }
    
    for task, status in a_tasks.items():
        logger.info(f"  {'✓' if '已实现' in status else '○'} {task}")
    
    logger.info("\n【已完成功能】")
    for task in result['completed'][:5]:
        logger.info(f"  ✓ {task}")
    
    if result['incomplete']:
        logger.info("\n【待完成功能】")
        for task in result['incomplete']:
            if len(task) < 80:  # 只显示短任务
                logger.info(f"  ○ {task}")
    
    return result

def check_round2_status():
    """
    检查Round 2派工单的完成状态
    """
    file_path = r"i:\personal-agent\docs\派工-Round2-桌面壳与Live2D集成.md"
    
    logger.info("\n" + "=" * 60)
    logger.info("派工 Round 2 - 桌面壳与Live2D集成")
    logger.info("=" * 60)
    
    # 检查关键文件是否存在
    key_files = {
        "PetWindow.tsx": r"i:\personal-agent\apps\desktop\src\windows\PetWindow.tsx",
        "TaskPanel.tsx": r"i:\personal-agent\apps\desktop\src\windows\TaskPanel.tsx",
        "SettingsWindow.tsx": r"i:\personal-agent\apps\desktop\src\windows\SettingsWindow.tsx",
        "Live2DCanvas.tsx": r"i:\personal-agent\apps\desktop\src\live2d\Live2DCanvas.tsx",
        "cursor.rs": r"i:\personal-agent\apps\desktop\src-tauri\src\cursor.rs",
        "tray.rs": r"i:\personal-agent\apps\desktop\src-tauri\src\tray.rs",
    }
    
    logger.info("\n【核心文件检查】")
    for name, path in key_files.items():
        exists = os.path.exists(path)
        logger.info(f"  {'✓' if exists else '○'} {name}")
    
    # 检查功能实现
    logger.info("\n【功能实现检查】")
    
    # 检查cursor.rs
    cursor_path = r"i:\personal-agent\apps\desktop\src-tauri\src\cursor.rs"
    with open(cursor_path, 'r', encoding='utf-8') as f:
        cursor_content = f.read()
    
    features = {
        "cursor_position事件": "spawn_cursor_watcher" in cursor_content,
        "Windows坐标获取": "GetCursorPos" in cursor_content,
    }
    
    for feature, implemented in features.items():
        logger.info(f"  {'✓' if implemented else '○'} {feature}")
    
    return key_files

def generate_summary_report():
    """
    生成总结报告
    """
    logger.info("\n" + "=" * 60)
    logger.info("项目整体进度总结")
    logger.info("=" * 60)
    
    summary = {
        "鼠标追踪模块": {
            "状态": "已修复",
            "修复点": [
                "移除了focusCanvasPoint中的错误归一化转换",
                "修正了handleScreenCursorMove的坐标缩放逻辑",
                "添加了PIXI的pointermove事件监听（最可靠方式）"
            ],
            "验证方法": "运行npm run tauri dev，移动鼠标观察Hiyori的眼睛是否跟随"
        },
        "docs中待关注事项": [
            "A7里程碑验收：录制鼠标追踪效果视频",
            "检查Hiyori资源是否完整（motion/expression文件）",
            "完善stateMap.ts中的表情映射"
        ]
    }
    
    logger.info("\n【鼠标追踪修复摘要】")
    for item in summary["鼠标追踪模块"]["修复点"]:
        logger.info(f"  ✓ {item}")
    
    logger.info("\n【建议后续操作】")
    for item in summary["docs中待关注事项"]:
        logger.info(f"  • {item}")
    
    return summary

def main():
    """
    主函数
    """
    analyze_live2d_timeline()
    check_round2_status()
    generate_summary_report()

if __name__ == "__main__":
    main()
