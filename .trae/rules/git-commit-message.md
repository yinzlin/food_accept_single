---
alwaysApply: true
scene: git_message
---

使用 Conventional Commits 混合模式编写提交信息：
- type 关键字保留英文（feat/fix/docs/refactor/perf/test/chore 等）
- scope（作用域）和 description（描述）使用中文

格式：`<type>(<scope>): <中文描述>`

示例：
- `feat(商品): 添加自动计算售价并同步基础单价功能`
- `fix(订单): 修复采购订单单位换算比例错误`
- `docs(README): 更新使用说明文档`
- `refactor(数据库): 重构商品价格表结构`
