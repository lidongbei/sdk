---
name: simple-change-record
description: 简单变更（≤2文件、无新表/模块、不改语义）不走完整superpowers流程，直接改代码 + 写 docs/superpowers/simple/ 留痕
---

# Simple Change Record

<HARD-GATE>
**在走此流程前必须先做文件计数检查：**

```bash
# 列出所有将被变更的文件（新建 + 修改 + 删除）
# 用实际变更估算，不要保守估算
```

如果改动的文件数量 **> 2 个**，**禁止使用此流程**，必须走完整 superpowers 流程（brainstorming → spec → plan → sdd）。

此 gate 不可绕过。即使改动"思路清晰""语义简单"，只要触及文件数超限就不允许走简单流程。
</HARD-GATE>

## 适用条件

满足**全部**条件时可走此流程：

- 改动 ≤ 2 个文件
- 不涉及新表、新接口、新模块
- 不改变业务流程语义（仅补字段、改 SQL 列、修命名等）
- 不涉及跨模块协调

典型例子：补填实体字段、SQL SELECT 加列、修方法名拼写、加注释。

**有一条不满足，就必须走完整 superpowers 流程（brainstorming → spec → plan → sdd）。**

## 流程

```
判断用户需求
  → 列出所有将变动的文件（新建 + 修改 + 删除）
  → 文件数 > 2 ？ → 拒绝，走完整 superpowers 流程
  → 文件数 ≤ 2 ？ → 派发 implementer 子代理执行修改
  → 编译验证
  → 派发 reviewer 子代理审查
  → review 通过后写 docs/superpowers/simple/<YYYY-MM-DD>-<功能名>.md
  → git add + commit
```

**关键规则：禁止在当前会话直接编辑文件。** 必须通过子代理完成。

跳过：brainstorming、spec、plan、subagent-driven-development。

## 留痕文档模板

```markdown
# <功能名>

**日期：** YYYY-MM-DD
**分支：** <branch>
**类型：** 简单变更

---

## 变更背景

为什么改，1-3 句

## 变更内容

改了哪些文件、每个文件改了什么

## 验证

- `mvn compile -pl cnapbp-example` — BUILD SUCCESS

## 关联

- commit：<hash>
- 前序 spec/plan：<path>
```

## 文档目录

`docs/superpowers/simple/` — 与 `specs/`（复杂设计）、`plans/`（实施计划）同级。
