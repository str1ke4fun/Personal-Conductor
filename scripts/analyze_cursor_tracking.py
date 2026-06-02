#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
模块名: 鼠标追踪分析
功能描述: 分析Live2D鼠标追踪模块的坐标转换问题
开发阶段: 分析与修复
实现度: 100%
依赖模块: 无
相关文件: apps/desktop/src/live2d/Live2DCanvas.tsx
"""

import os
import re
import logging

logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s %(name)s %(levelname)s %(message)s'
)
logger = logging.getLogger(__name__)

def analyze_live2d_canvas():
    """
    分析Live2DCanvas.tsx中的鼠标追踪问题
    """
    canvas_path = r"i:\personal-agent\apps\desktop\src\live2d\Live2DCanvas.tsx"
    
    with open(canvas_path, 'r', encoding='utf-8') as f:
        content = f.read()
    
    logger.info("=" * 60)
    logger.info("Live2D鼠标追踪问题分析")
    logger.info("=" * 60)
    
    # 问题1: focusCanvasPoint中的错误归一化
    logger.info("\n【问题1】focusCanvasPoint中的坐标归一化错误")
    logger.info("-" * 60)
    match = re.search(r'function focusCanvasPoint\(x: number, y: number\)\s*\{([^}]+)\}', content, re.DOTALL)
    if match:
        logger.info("当前代码:")
        lines = match.group(1).strip().split('\n')
        for line in lines:
            logger.info(f"  {line.strip()}")
        
        logger.info("\n问题分析:")
        logger.info("  model.focus() 期望的是 CANVAS 坐标系下的像素坐标")
        logger.info("  但代码中做了错误的归一化: (x / rect.width) * CANVAS_WIDTH")
        logger.info("  当 x 已经是 canvas 内的像素坐标时，这个转换是错误的！")
    
    # 问题2: handleScreenCursorMove的坐标计算
    logger.info("\n【问题2】handleScreenCursorMove的坐标计算过于复杂")
    logger.info("-" * 60)
    match2 = re.search(r'function handleScreenCursorMove\(position: CursorPosition\)\s*\{([^}]+)\}', content, re.DOTALL)
    if match2:
        logger.info("当前代码:")
        lines = match2.group(1).strip().split('\n')
        for line in lines:
            logger.info(f"  {line.strip()}")
        
        logger.info("\n问题分析:")
        logger.info("  1. outerPosition() 返回窗口外边界（含标题栏）")
        logger.info("  2. getBoundingClientRect() 返回canvas相对于webview的位置")
        logger.info("  3. 两者相减可能产生坐标偏差")
    
    # 问题3: 缺少调试日志
    logger.info("\n【问题3】缺少调试日志，无法验证坐标是否正确")
    logger.info("-" * 60)
    logger.info("建议: 添加console.log输出关键坐标值")
    
    # 解决方案
    logger.info("\n" + "=" * 60)
    logger.info("修复方案")
    logger.info("=" * 60)
    logger.info("\n方案1: 直接使用PIXI的pointermove事件（最可靠）")
    logger.info("  app.stage.interactive = true;")
    logger.info("  app.stage.on('pointermove', (e) => model.focus(e.global.x, e.global.y));")
    
    logger.info("\n方案2: 简化坐标转换逻辑，移除错误的归一化")
    logger.info("  focusCanvasPoint应该直接使用传入的像素坐标")
    logger.info("  不需要除以rect.width再乘以CANVAS_WIDTH")
    
    logger.info("\n方案3: 添加调试日志")
    logger.info("  console.log('Cursor tracking:', { screenX, screenY, canvasX, canvasY });")

def generate_fixed_code():
    """
    生成修复后的代码片段
    """
    fixed_code = '''
    function focusCanvasPoint(x: number, y: number) {
      const model = modelRef.current;
      const canvas = canvasRef.current;
      if (!model?.focus || !canvas) return;

      if (animationFrame) cancelAnimationFrame(animationFrame);
      animationFrame = requestAnimationFrame(() => {
        // 直接使用canvas坐标，不需要归一化转换
        // model.focus() 期望的是canvas内的像素坐标
        model.focus?.(x, y);
      });
    }

    function handleScreenCursorMove(position: CursorPosition) {
      const canvas = canvasRef.current;
      if (!canvas) return;
      
      // 屏幕坐标 -> 窗口内坐标 -> canvas坐标
      const rect = canvas.getBoundingClientRect();
      const windowX = position.x - windowPositionRef.current.x;
      const windowY = position.y - windowPositionRef.current.y;
      
      // 考虑标题栏偏移（Windows窗口标题栏大约32px）
      const titleBarHeight = 32;
      const canvasX = windowX - rect.left;
      const canvasY = windowY - rect.top - titleBarHeight;
      
      focusCanvasPoint(canvasX, canvasY);
    }

    // 模型加载后启用PIXI交互
    app.stage.interactive = true;
    app.stage.hitArea = new PIXI.Rectangle(0, 0, CANVAS_WIDTH, CANVAS_HEIGHT);
    app.stage.on('pointermove', (event) => {
      const globalPos = event.global;
      model.focus?.(globalPos.x, globalPos.y);
    });
'''
    return fixed_code

def main():
    """
    主函数
    """
    analyze_live2d_canvas()
    
    logger.info("\n" + "=" * 60)
    logger.info("建议的修复代码片段")
    logger.info("=" * 60)
    print(generate_fixed_code())

if __name__ == "__main__":
    main()
