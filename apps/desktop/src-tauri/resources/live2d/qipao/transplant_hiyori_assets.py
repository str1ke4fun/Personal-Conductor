"""
Hiyori → QipaoGirl 动作/表情嫁接脚本（占位版）

【用途】
当 QipaoGirl.cmo3 在 Cubism Editor 中完成参数绑定并 Export for SDK 后，
本脚本把 Hiyori 自带的 motion / expression 直接嫁接到 QipaoGirl runtime，
让她立刻拥有 idle 呼吸、tap、surprised 等动作而不必从零建模动作。

【前置条件】
1. QipaoGirl.cmo3 已在 Cubism Editor 里完成至少这 6 个标准参数的绑定：
   - ParamAngleX / ParamAngleY / ParamAngleZ
   - ParamEyeBallX / ParamEyeBallY
   - ParamBreath
   （多绑越多越好，至少这 6 个才能让 idle / focus 看起来活）
2. 在 Editor 中执行 File → Export for SDK → moc3 + model3.json
   把 runtime 文件输出到: <RUNTIME_OUT_DIR>
3. 已确认目录里有: <MODEL_NAME>.moc3 / <MODEL_NAME>.model3.json / textures/

【运行方式】
   python transplant_hiyori_assets.py

【输出】
在 RUNTIME_OUT_DIR 下补齐:
  motions/        ← 从 Hiyori 复制
  expressions/    ← 从 Hiyori 复制
  <MODEL_NAME>.model3.json  ← 重写，加上 Motions / Expressions 段
"""

from __future__ import annotations

import json
import shutil
from pathlib import Path

# ============================================================================
# TODO: 等模型导出后填写以下常量
# ============================================================================

# QipaoGirl 的 model 名（即 .moc3 / .model3.json 的 stem）
# 例如导出文件叫 "QipaoGirl.moc3"，这里就填 "QipaoGirl"
MODEL_NAME: str = ""  # TODO: 填写

# Cubism Editor 导出 runtime 的目录
# 例如: r"I:\personal-agent\apps\desktop\src-tauri\resources\live2d\qipao\runtime"
RUNTIME_OUT_DIR: Path = Path("")  # TODO: 填写

# Hiyori 资源根目录（用 hiyori_free_en 也行，hiyori_pro_en 动作更丰富）
HIYORI_RUNTIME_DIR: Path = Path(
    r"I:\personal-agent\apps\desktop\src-tauri\resources\live2d\hiyori\hiyori_pro_en\runtime"
)
HIYORI_MODEL_NAME: str = "hiyori_pro_t11"  # hiyori_free_t08 / hiyori_pro_t11

# 嫁接哪些 motion group（key 是 group 名，value 是要复制的 .motion3.json 列表）
# 留空 = 复制 Hiyori 全部 motion group
MOTION_GROUPS_TO_COPY: dict[str, list[str]] = {
    # "Idle": ["hiyori_m01.motion3.json", "hiyori_m05.motion3.json"],
    # "Tap":  ["hiyori_m07.motion3.json"],
    # TODO: 视情况筛选，留空则全复制
}

# 是否复制 expressions
COPY_EXPRESSIONS: bool = True

# ============================================================================
# 以下是脚本逻辑，参数填完后无需改动
# ============================================================================


def _validate_inputs() -> None:
    if not MODEL_NAME:
        raise SystemExit(
            "❌ 请先填写 MODEL_NAME 常量（QipaoGirl.cmo3 export 出的 .moc3 文件名 stem）"
        )
    if not RUNTIME_OUT_DIR or not RUNTIME_OUT_DIR.exists():
        raise SystemExit(
            f"❌ RUNTIME_OUT_DIR 不存在: {RUNTIME_OUT_DIR}\n"
            "请确认 Cubism Editor 已经 Export for SDK 到该目录"
        )

    moc3 = RUNTIME_OUT_DIR / f"{MODEL_NAME}.moc3"
    model3 = RUNTIME_OUT_DIR / f"{MODEL_NAME}.model3.json"
    if not moc3.exists():
        raise SystemExit(f"❌ 找不到 {moc3}，请先在 Editor 完成 Export for SDK")
    if not model3.exists():
        raise SystemExit(f"❌ 找不到 {model3}")

    if not HIYORI_RUNTIME_DIR.exists():
        raise SystemExit(f"❌ Hiyori runtime 目录不存在: {HIYORI_RUNTIME_DIR}")


def _copy_motions() -> dict[str, list[dict[str, str]]]:
    """把 Hiyori 的 motion 文件复制过去，返回 model3.json 用的 Motions 字段。"""
    src_motion_dir = HIYORI_RUNTIME_DIR / "motion"
    if not src_motion_dir.exists():
        # hiyori_free 用 motions 目录
        src_motion_dir = HIYORI_RUNTIME_DIR / "motions"
    if not src_motion_dir.exists():
        print("⚠️  Hiyori 没有 motions 目录，跳过 motion 嫁接")
        return {}

    dst_motion_dir = RUNTIME_OUT_DIR / "motions"
    dst_motion_dir.mkdir(exist_ok=True)

    # 复制 Hiyori 自带 model3.json 里的 Motions 结构
    hiyori_model3_path = HIYORI_RUNTIME_DIR / f"{HIYORI_MODEL_NAME}.model3.json"
    with hiyori_model3_path.open(encoding="utf-8") as fh:
        hiyori_model3 = json.load(fh)

    src_motions = hiyori_model3.get("FileReferences", {}).get("Motions", {})
    new_motions: dict[str, list[dict[str, str]]] = {}

    for group, entries in src_motions.items():
        # 按用户配置过滤
        if MOTION_GROUPS_TO_COPY and group not in MOTION_GROUPS_TO_COPY:
            continue
        filter_list = MOTION_GROUPS_TO_COPY.get(group) if MOTION_GROUPS_TO_COPY else None

        new_motions[group] = []
        for entry in entries:
            file_rel = entry["File"]  # e.g. "motion/hiyori_m01.motion3.json"
            filename = Path(file_rel).name
            if filter_list and filename not in filter_list:
                continue
            src = HIYORI_RUNTIME_DIR / file_rel
            if not src.exists():
                print(f"⚠️  跳过不存在的 motion: {src}")
                continue
            dst = dst_motion_dir / filename
            shutil.copy2(src, dst)
            new_motions[group].append({"File": f"motions/{filename}"})
            print(f"  motion ← {filename}")

    return new_motions


def _copy_expressions() -> list[dict[str, str]]:
    """复制 Hiyori 表情（如果有），返回 model3.json 用的 Expressions 字段。"""
    if not COPY_EXPRESSIONS:
        return []
    src_dir = HIYORI_RUNTIME_DIR / "expressions"
    if not src_dir.exists():
        return []  # Hiyori free/pro 通常不带 expressions
    dst_dir = RUNTIME_OUT_DIR / "expressions"
    dst_dir.mkdir(exist_ok=True)
    expressions = []
    for exp_file in src_dir.glob("*.exp3.json"):
        shutil.copy2(exp_file, dst_dir / exp_file.name)
        expressions.append({"Name": exp_file.stem.split(".")[0], "File": f"expressions/{exp_file.name}"})
        print(f"  exp ← {exp_file.name}")
    return expressions


def _rewrite_model3(motions: dict, expressions: list) -> None:
    """把 motion/expression 段写回 QipaoGirl 的 model3.json。"""
    model3_path = RUNTIME_OUT_DIR / f"{MODEL_NAME}.model3.json"
    with model3_path.open(encoding="utf-8") as fh:
        model3 = json.load(fh)

    file_refs = model3.setdefault("FileReferences", {})
    if motions:
        file_refs["Motions"] = motions
    if expressions:
        file_refs["Expressions"] = expressions

    with model3_path.open("w", encoding="utf-8") as fh:
        json.dump(model3, fh, indent="\t", ensure_ascii=False)
    print(f"✅ 重写 {model3_path}")


def main() -> None:
    _validate_inputs()
    print(f"嫁接 Hiyori 资源 → {RUNTIME_OUT_DIR}\n")

    print("[1/3] 复制 motions...")
    motions = _copy_motions()
    print(f"  共 {sum(len(v) for v in motions.values())} 个 motion 文件")

    print("\n[2/3] 复制 expressions...")
    expressions = _copy_expressions()
    print(f"  共 {len(expressions)} 个 expression 文件")

    print("\n[3/3] 重写 model3.json...")
    _rewrite_model3(motions, expressions)

    print("\n🎉 嫁接完成！下一步：")
    print(f"  - 修改 apps/desktop/src/live2d/Live2DCanvas.tsx 的 MODEL_URL")
    print(f"    指向: /live2d/qipao/runtime/{MODEL_NAME}.model3.json")
    print(f"  - 运行: cd apps/desktop && pnpm tauri dev")


if __name__ == "__main__":
    main()
