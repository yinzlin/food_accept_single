# 食材采购验收系统 · Code Wiki

> 仓库级代码知识库，面向维护者与二次开发者。涵盖整体架构、模块职责、关键类与函数、数据库设计、依赖关系与运行方式。
>
> 源代码主体为单文件：[src/main.rs](file:///workspace/src/main.rs)（约 2.6 万行）。

---

## 目录

1. [项目概述](#1-项目概述)
2. [技术栈与依赖](#2-技术栈与依赖)
3. [目录结构](#3-目录结构)
4. [整体架构](#4-整体架构)
5. [数据库设计](#5-数据库设计)
6. [核心数据结构](#6-核心数据结构)
7. [模块职责划分](#7-模块职责划分)
8. [关键函数说明](#8-关键函数说明)
9. [认证与权限体系](#9-认证与权限体系)
10. [订单状态机](#10-订单状态机)
11. [耗材分摊方案](#11-耗材分摊方案)
12. [依赖关系](#12-依赖关系)
13. [项目运行方式](#13-项目运行方式)

---

## 1. 项目概述

**项目名称**：`food_accept_single`（食材采购验收系统）

**定位**：面向食材配送业务的**采购 / 验收 / 对账一体化管理系统**。以 Rust + Axum + SQLite 构建的**单文件服务端程序**，内置系统托盘图标，双击即可在本地启动，**完全离线可用**（前端资源全部本地化，无任何 CDN 依赖）。

**核心能力**：
- 基础资料管理（供应商、采购方、商品含多单位/多价格、仓库）
- 采购订单 / 销售订单全生命周期管理（含 Excel 导入导出、图片上传、打印）
- 配单分拣（按采购方/品类/供应商/综合多种视图，PC + 移动端）
- 验收单 / 报销单导出（真实口径与报销口径双账套）
- 查询分析（采购/销售汇总、价格趋势折线图、往来对账、库存进销存、毛利、品类统计）
- 耗材分摊（一库双账：真实账套 + 分摊账套）
- RBAC 权限 + 行级数据权限 + 操作审计日志
- 系统管理（用户、参数、数据库备份/恢复/完整性检查、异常数据清理）

**默认账号**：系统初始化自动创建超级管理员 `super_admin`，登录后需在「用户管理」修改密码。

---

## 2. 技术栈与依赖

### 2.1 Cargo 依赖清单

依赖定义见 [Cargo.toml](file:///workspace/Cargo.toml)。

| crate | 版本 | 用途 |
|-------|------|------|
| `axum` | 0.8（含 `multipart`） | Web 框架，路由 + handler + 文件上传 |
| `tokio` | 1（`macros`, `rt-multi-thread`） | 异步运行时 |
| `tower-http` | 0.6（`trace`） | HTTP 中间件 |
| `sqlx` | 0.9（`sqlite`, `runtime-tokio`, `macros`） | SQLite 异步驱动 + 连接池 |
| `serde` / `serde_json` | 1 | JSON 序列化/反序列化 |
| `chrono` | 0.4 | 日期时间处理 |
| `anyhow` | 1 | 错误处理（动态） |
| `thiserror` | 2 | 错误处理（静态） |
| `rand` | 0.8 | 随机数（session token 生成） |
| `bytes` | 1 | 二进制字节流（Excel 导入读取） |
| `rust_xlsxwriter` | 0.81 | Excel 导出 |
| `calamine` | 0.36 | Excel 导入解析 |
| `tray-icon` | 0.24.1 | 系统托盘图标 |
| `tao` | 0.35.3 | 跨平台事件循环（托盘事件分发） |
| `multer` | 2 | multipart 解析 |
| `mime` | 0.3 | MIME 类型判断 |
| `bcrypt` | 0.15 | 密码 bcrypt 加密 |

### 2.2 前端资源

全部本地内嵌（`include_str!` 编译期嵌入二进制），见 [src/main.rs#L62-L64](file:///workspace/src/main.rs#L62-L64)：

| 资源 | 文件 | 用途 |
|------|------|------|
| Bootstrap 5 CSS | [static/bootstrap.min.css](file:///workspace/static/bootstrap.min.css) | UI 样式 |
| Bootstrap 5 JS | [static/bootstrap.bundle.min.js](file:///workspace/static/bootstrap.bundle.min.js) | UI 交互 |
| Chart.js 4 | [static/chart.umd.min.js](file:///workspace/static/chart.umd.min.js) | 价格趋势折线图 |

### 2.3 Rust 版本与平台

- `edition = "2021"`
- Windows 发布版静态链接 CRT：[.cargo/config.toml](file:///workspace/.cargo/config.toml) 设置 `target-feature=+crt-static`
- `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`：Release 模式隐藏控制台窗口（[src/main.rs#L1](file:///workspace/src/main.rs#L1)）

---

## 3. 目录结构

```
food_accept_single/
├── src/
│   └── main.rs              # 单文件后端 + 前端页面（约 2.6 万行）
├── static/                  # 本地前端资源（离线可用，编译期 include_str! 嵌入）
│   ├── bootstrap.min.css
│   ├── bootstrap.bundle.min.js
│   └── chart.umd.min.js
├── .cargo/
│   └── config.toml          # Windows MSVC 静态链接 CRT
├── .trae/rules/
│   └── git-commit-message.md  # Conventional Commits 提交规范
├── food_accept_v3.db        # SQLite 主数据库（结构随版本入库）
├── uploads/                 # 上传图片（运行时生成，不入库）
├── backups/                 # 数据库备份目录
├── Cargo.toml               # 包定义与依赖
├── Cargo.lock
├── README.md                # 项目说明
├── 导出验收单.md            # 验收单/报销单导出口径说明
├── 耗材分摊方案.md          # 耗材分摊增项处理方案
├── food_accept_single.exe   # 发布产物（可选）
└── CODE_WIKI.md             # 本文档
```

---

## 4. 整体架构

### 4.1 架构总览

系统采用**单体单文件 SSR（服务端渲染）架构**，所有后端逻辑、HTTP 路由、HTML 页面模板、权限校验、数据库访问都集中在 [src/main.rs](file:///workspace/src/main.rs) 一个文件中。

```
┌─────────────────────────────────────────────────────────────┐
│                    进程入口 main()                            │
│  ┌──────────────────────────┐  ┌──────────────────────────┐  │
│  │ Tokio 工作线程（子线程）   │  │ tao 事件循环（主线程）     │  │
│  │  init_pool() → 连接池     │  │  系统托盘 TrayIcon        │  │
│  │  build_router() → 路由    │  │  「打开页面」「退出」菜单   │  │
│  │  axum::serve(0.0.0.0:3000)│ │  open_browser()           │  │
│  └──────────────────────────┘  └──────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
            │ HTTP 请求
            ▼
┌─────────────────────────────────────────────────────────────┐
│                  Axum Router（约 130 条路由）                 │
│  /static/*        静态资源（CSS/JS）                          │
│  /                页面路由 page_*（SSR 返回 Html<String>）     │
│  /mobile/*        移动端分拣页面                              │
│  /api/*           JSON API（业务 CRUD + 查询 + 导入导出）      │
│  /login /api/login /api/logout   认证                        │
└─────────────────────────────────────────────────────────────┘
            │ check_api_permission / get_user_ctx（Cookie session）
            ▼
┌─────────────────────────────────────────────────────────────┐
│                  业务逻辑层（handler 函数）                   │
│  基础资料 │ 采购单 │ 销售单 │ 耗材分摊 │ 查询分析 │ 系统管理  │
└─────────────────────────────────────────────────────────────┘
            │ sqlx::query / query_as
            ▼
┌─────────────────────────────────────────────────────────────┐
│      SQLite 连接池 DB_POOL（OnceLock 全局单例）               │
│      food_accept_v3.db（21 张表 + 索引 + 视图）               │
└─────────────────────────────────────────────────────────────┘
```

### 4.2 设计要点

1. **单文件集成**：约 2.6 万行 Rust，按职责用注释分章节（`// ===== xxx =====`），无独立 crate/模块拆分，便于单文件分发。
2. **静态资源编译期嵌入**：CSS/JS 通过 `include_str!` 打包进二进制，运行时零外部文件依赖。
3. **双线程模型**：主线程跑 `tao` 事件循环（托盘），子线程跑 `tokio` 异步 Web 服务（[src/main.rs#L26692-L26705](file:///workspace/src/main.rs#L26692-L26705)）。
4. **全局连接池**：`DB_POOL: OnceLock<SqlitePool>` 在 `init_pool()` 中初始化后全局共享（[src/main.rs#L60](file:///workspace/src/main.rs#L60), [src/main.rs#L577](file:///workspace/src/main.rs#L577)）。
5. **SSR + 原生 JS**：页面由 `layout_html()` + 各 `page_*` 函数拼装返回，前端用原生 fetch + Bootstrap + Chart.js 交互，无前端构建链。
6. **权限双重校验**：页面级（`check_page_permission`）+ API 级（`check_api_permission`）双重拦截。

---

## 5. 数据库设计

数据库为 SQLite，主文件 `food_accept_v3.db`，结构随版本入库。表结构由 `init_tables()` 创建（[src/main.rs#L581](file:///workspace/src/main.rs#L581)），通过 `CREATE TABLE IF NOT EXISTS` + 大量 `ALTER TABLE ADD COLUMN` 实现渐进式迁移（兼容历史数据）。

### 5.1 表清单

| 表名 | 行号 | 职责 |
|------|------|------|
| `category` | [L584](file:///workspace/src/main.rs#L584) | 分类（自引用父子树，支持 supplier/purchaser/product 共用 `entity_type`） |
| `supplier` | [L600](file:///workspace/src/main.rs#L600) | 供应商 |
| `purchaser` | [L616](file:///workspace/src/main.rs#L616) | 采购方 |
| `product` | [L632](file:///workspace/src/main.rs#L632) | 商品（含别名、规格、基础单位、进价、售价、加成率、自动调价开关、最高/最低进价） |
| `product_price_log` | [L697](file:///workspace/src/main.rs#L697) | 价格变更日志（审计/对账） |
| `product_unit` | [L740](file:///workspace/src/main.rs#L740) | 商品多单位（单位名、换算比例、单位售价/进价） |
| `product_price` | [L770](file:///workspace/src/main.rs#L770) | 商品多价格类型 |
| `warehouse` | [L787](file:///workspace/src/main.rs#L787) | 仓库 |
| `inventory` | [L806](file:///workspace/src/main.rs#L806) | 库存余额（商品 × 仓库，含最低/最高库存预警） |
| `purchase_order` | [L836](file:///workspace/src/main.rs#L836) | 采购订单主表（含乐观锁 `version`、来源销售单 `source_sales_order_id`） |
| `purchase_order_item` | [L854](file:///workspace/src/main.rs#L854) | 采购订单明细 |
| `sales_order` | [L890](file:///workspace/src/main.rs#L890) | 销售订单主表（含客户订单图片、签字验收单图片、乐观锁 `version`） |
| `sales_order_item` | [L910](file:///workspace/src/main.rs#L910) | 销售订单明细（含供应商归属、预售数量） |
| `consumable_allocation` | [L934](file:///workspace/src/main.rs#L934) | 耗材分摊方案主表（总金额、已分摊、未分摊余额、状态） |
| `order_supplement_item` | [L954](file:///workspace/src/main.rs#L954) | 增项流水表（`new_item` 新增行 / `increase_quantity` 追加数量） |
| `purchase_document` | [L983](file:///workspace/src/main.rs#L983) | 采购单据图片（按供应商+日期采集多张） |
| `operation_log` | [L1081](file:///workspace/src/main.rs#L1081) | 操作审计日志 |
| `food_accept` | [L1153](file:///workspace/src/main.rs#L1153) | 食材验收单（历史表，含车牌、供应时间、折扣） |
| `food_item` | [L1174](file:///workspace/src/main.rs#L1174) | 食材验收明细（含检疫/检测标志） |
| `system_config` | [L1198](file:///workspace/src/main.rs#L1198) | 系统参数（KV） |
| `backup_record` | [L1211](file:///workspace/src/main.rs#L1211) | 备份记录 |
| `user_account` | [L1225](file:///workspace/src/main.rs#L1225) | 用户账号（含角色、状态、绑定的 supplier/purchaser 外键） |

### 5.2 关键表字段说明

**product（商品）** —— 价格策略核心：
```
id, name, spec, unit, base_unit(基础单位), base_price(售价),
purchase_price(当前进价), max_purchase_price, min_purchase_price,
markup_rate(加成率，base_price = purchase_price * (1 + markup_rate)),
auto_update_price(是否按加成率自动算售价), alias1, alias2, image_url,
status(启用/停用), category_id
```

**purchase_order / sales_order（订单）**：
```
id, supplier_id/purchaser_id, order_no(唯一), order_date, total_amount,
status(状态机), remark, version(乐观锁), source_sales_order_id(采购单的来源销售单)
sales_order 另含: customer_order_image(客户订单图片), signed_order_image(签字验收单图片)
```

**order_supplement_item（分摊增项流水）**：
```
id, target_order_id(目标食材订单), source_order_id(来源耗材订单),
product_id, product_name, unit, unit_price, quantity, amount,
allocate_date, operation_type('new_item' | 'increase_quantity'),
target_order_item_id(追加模式时关联的真实明细行)
```

### 5.3 数据库健壮性

`init_pool()`（[src/main.rs#L496](file:///workspace/src/main.rs#L496)）启动时执行：
1. 连接池配置（16 最大 / 4 最小连接，300s 空闲超时）+ PRAGMA 调优（cache_size、synchronous=NORMAL、temp_store=MEMORY、auto_vacuum=INCREMENTAL）。
2. `PRAGMA integrity_check` 损坏检测，失败则调用 `repair_db_corruption()`（[src/main.rs#L422](file:///workspace/src/main.rs#L422)）尝试修复。
3. `init_tables()` 建表。
4. 一次性修复耗材分摊金额重算（修复历史 bug）。
5. **孤儿数据清理**：删除无主键关联的明细、无明细的订单、无商品关联的库存等十余条 DELETE 语句，最后 `VACUUM` 压缩。

---

## 6. 核心数据结构

本工程未定义独立 enum（错误处理直接用 `anyhow`/字符串）。核心结构体均为请求/响应模型，集中在 [src/main.rs#L1405](file:///workspace/src/main.rs#L1405) 起。

### 6.1 用户上下文

```rust
// src/main.rs#L157
struct UserCtx {
    role: String,        // super_admin/admin/supplier/purchaser/user/anonymous
    user_id: i64,
    supplier_id: i64,    // 行级权限：绑定的供应商，0 表示未绑定
    purchaser_id: i64,   // 行级权限：绑定的采购方
}
```

### 6.2 请求体结构（`#[derive(Deserialize)]`）

| 结构体 | 行号 | 用途 |
|--------|------|------|
| `SupplierReq` | [L1406](file:///workspace/src/main.rs#L1406) | 供应商增改 |
| `PurchaserReq` | [L1418](file:///workspace/src/main.rs#L1418) | 采购方增改 |
| `DeleteReq` | [L1430](file:///workspace/src/main.rs#L1430) | 通用删除（带 id） |
| `LoginReq` | [L1435](file:///workspace/src/main.rs#L1435) | 登录（username/password） |
| `ProductReq` | [L1441](file:///workspace/src/main.rs#L1441) | 商品创建 |
| `ProductUnitReq` | [L1456](file:///workspace/src/main.rs#L1456) | 商品单位增改 |
| `ProductPriceReq` | [L1466](file:///workspace/src/main.rs#L1466) | 商品价格 upsert |
| `CategoryReq` | [L1475](file:///workspace/src/main.rs#L1475) | 分类创建 |
| `PurchaseOrderReq` | [L1483](file:///workspace/src/main.rs#L1483) | 采购单创建/更新（含明细列表） |
| `PurchaseOrderItemReq` | [L1503](file:///workspace/src/main.rs#L1503) | 采购单明细 |
| `SalesOrderReq` | [L1522](file:///workspace/src/main.rs#L1522) | 销售单创建/更新 |
| `SalesOrderItemReq` | [L1540](file:///workspace/src/main.rs#L1540) | 销售单明细 |
| `OrderSupplementItemReq` | [L1559](file:///workspace/src/main.rs#L1559) | 分摊增项创建 |
| `AcceptReq` | [L1578](file:///workspace/src/main.rs#L1578) | 食材验收单 |
| `FoodItemReq` | [L1590](file:///workspace/src/main.rs#L1590) | 验收明细 |
| `ProductUpdateReq` | [L14090](file:///workspace/src/main.rs#L14090) | 商品更新（含别名/图片） |
| `CategoryRenameReq` | [L15402](file:///workspace/src/main.rs#L15402) | 分类重命名 |
| `WarehouseCreateReq` | [L15509](file:///workspace/src/main.rs#L15509) | 仓库创建 |
| `WarehouseUpdateReq` | [L15545](file:///workspace/src/main.rs#L15545) | 仓库更新 |

### 6.3 内部模型

| 结构体 | 行号 | 用途 |
|--------|------|------|
| `PurchaseOrderPrint` | [L16933](file:///workspace/src/main.rs#L16933) | 采购单打印数据组装 |
| `PurchaseOrderPrintItem` | [L16949](file:///workspace/src/main.rs#L16949) | 打印明细行 |
| `UserSimple` | [L16959](file:///workspace/src/main.rs#L16959) | 简化用户信息（昵称/电话） |
| `StockSummaryRow` | [L20101](file:///workspace/src/main.rs#L20101) | 进销存汇总行（期初/入库/出库/期末） |

---

## 7. 模块职责划分

代码按注释章节组织（无 `mod` 拆分）。各逻辑模块及其行号范围：

| 模块 | 行号范围 | 职责 |
|------|----------|------|
| **导入与常量** | [L1-L64](file:///workspace/src/main.rs#L1-L64) | `use` 声明、静态资源 `include_str!`、`DB_POOL` 全局 |
| **导出辅助函数** | [L17-L49](file:///workspace/src/main.rs#L17-L49) | `xlsx_header_format`、`xlsx_response`、`urlencode_filename` |
| **认证与权限** | [L66-L395](file:///workspace/src/main.rs#L66-L395) | `get_user_role`、`has_permission`、`has_permission_point`、`UserCtx`、`get_user_ctx`、`log_operation`、行级权限、路由权限映射、`check_api/page_permission` |
| **静态资源服务** | [L397-L420](file:///workspace/src/main.rs#L397-L420) | `serve_bootstrap_css/js`、`serve_chart_js` |
| **数据库初始化** | [L422-L579](file:///workspace/src/main.rs#L422-L579) | `repair_db_corruption`、`init_pool`、`pool` |
| **建表与迁移** | [L581-L1603](file:///workspace/src/main.rs#L581-L1603) | `init_tables`（21 张表 + 索引 + 默认仓库 + 超管账号） |
| **页面布局** | [L1605-L2708](file:///workspace/src/main.rs#L1605-L2708) | `sidebar_html`、`layout_html`（SSR 外壳 + 侧边栏） |
| **PC 页面 handler** | [L2710-L10243](file:///workspace/src/main.rs#L2710-L10243) | `page_index`…`page_restore`（首页、基础资料、订单、查询、系统） |
| **移动端分拣页面** | [L10245-L12052](file:///workspace/src/main.rs#L10245-L12052) | `page_mobile_sort*`（5 种分拣视图） |
| **系统与审计 API** | [L12053-L12853](file:///workspace/src/main.rs#L12053-L12853) | `api_system_config`、`api_operation_log_*`、`api_user_*`、`api_backup/restore/*`、`api_clean_*` |
| **Excel 导入导出工具** | [L12855-L13076](file:///workspace/src/main.rs#L12855-L13076) | `parse_keyword_pattern`、`parse_csv`、`api_supplier_export/import` |
| **认证 API** | [L13078-L13327](file:///workspace/src/main.rs#L13078-L13327) | `page_login`、`api_login`、`api_logout`、`api_login_check` |
| **基础资料 API** | [L13329-L15511](file:///workspace/src/main.rs#L13329-L15511) | 供应商、采购方、商品（含图片/单位/价格/调价）、分类、库存、仓库 |
| **订单生成与编号** | [L15612-L16078](file:///workspace/src/main.rs#L15612-L16078) | `generate_order_no`、`round_to_allowed_last_digit`、`log_price_change`、`recalc_base_price_by_markup`、`update_product_purchase_prices` |
| **采购单 API** | [L16079-L17021](file:///workspace/src/main.rs#L16079-L17021) | 创建/列表/详情/更新/审核/删除/导出/打印 Excel/导入 |
| **销售单 API** | [L17023-L17620](file:///workspace/src/main.rs#L17023-L17620) | 详情/更新/调价/删除/导出/导入 |
| **查询分析 API** | [L17892-L19967](file:///workspace/src/main.rs#L17892-L19967) | 采购/销售订单查询、价格、汇总、对账、排行、报销、分摊来源 |
| **库存与毛利 API** | [L19968-L21198](file:///workspace/src/main.rs#L19968-L21198) | 库存余额/流水/进销存汇总/预警/呆滞、收支、毛利、品类统计 |
| **导出 API** | [L20945-L21228](file:///workspace/src/main.rs#L20945-L21228) | 各查询的 Excel 导出 endpoint |
| **销售单生成采购单** | [L19142](file:///workspace/src/main.rs#L19142) | `api_sales_order_generate_purchase`（一键生成供应商采购单） |
| **销售单创建与验收** | [L21229-L21616](file:///workspace/src/main.rs#L21229-L21616) | `api_sales_order_create/list/accept` |
| **耗材分摊页面与 API** | [L21618-L24464](file:///workspace/src/main.rs#L21618-L24464) | `page_supplement`、分摊方案 CRUD、增项 CRUD、对比、验收单 Excel |
| **分拣 API** | [L24465-L26122](file:///workspace/src/main.rs#L24465-L26122) | 综合/采购方/品类/供应商多视图分拣 + Excel |
| **订单状态机 API** | [L26124-L26400](file:///workspace/src/main.rs#L26124-L26400) | `update_status`、`approve/unapprove`、`correction`、`accept_create/list` |
| **路由装配** | [L26434-L26656](file:///workspace/src/main.rs#L26434-L26656) | `build_router()`（约 130 条路由） |
| **托盘与入口** | [L26658-L26745](file:///workspace/src/main.rs#L26658-L26745) | `make_app_icon`、`open_browser`、`main` |

---

## 8. 关键函数说明

### 8.1 启动与基础设施

| 函数 | 行号 | 说明 |
|------|------|------|
| `main()` | [L26692](file:///workspace/src/main.rs#L26692) | 入口。子线程跑 Tokio Web 服务，主线程跑 tao 托盘事件循环 |
| `build_router()` | [L26434](file:///workspace/src/main.rs#L26434) | 装配全部 ~130 条路由（页面 + API + 静态资源 + 移动端） |
| `init_pool()` | [L496](file:///workspace/src/main.rs#L496) | 建连接池、PRAGMA 调优、完整性检查、修复、清理孤儿数据 |
| `init_tables()` | [L581](file:///workspace/src/main.rs#L581) | 建表 + ALTER 渐进迁移 + 默认仓库 + 超管账号 |
| `pool()` | [L577](file:///workspace/src/main.rs#L577) | 获取全局连接池单例 |
| `repair_db_corruption()` | [L422](file:///workspace/src/main.rs#L422) | 数据库损坏自动修复 |
| `make_app_icon()` | [L26658](file:///workspace/src/main.rs#L26658) | 生成 64×64 RGBA 托盘图标 |
| `open_browser()` | [L26686](file:///workspace/src/main.rs#L26686) | 调用系统命令打开浏览器到 127.0.0.1:3000 |

### 8.2 认证与权限

| 函数 | 行号 | 说明 |
|------|------|------|
| `get_user_role(headers)` | [L66](file:///workspace/src/main.rs#L66) | 从 Cookie `session=user_id:token` 解析用户角色 |
| `get_user_ctx(headers)` | [L165](file:///workspace/src/main.rs#L165) | 解析 `UserCtx`（角色 + user_id + 行级 supplier/purchaser_id） |
| `has_permission(role, required)` | [L104](file:///workspace/src/main.rs#L104) | 粗粒度角色层级判断 |
| `has_permission_point(role, permission)` | [L124](file:///workspace/src/main.rs#L124) | 细粒度 RBAC 权限点判断（`resource.action`） |
| `log_operation(user, action, ...)` | [L210](file:///workspace/src/main.rs#L210) | 写 `operation_log` 审计日志 |
| `can_access_purchase_order(user, supplier_id)` | [L236](file:///workspace/src/main.rs#L236) | 行级权限：供应商只能访问自己绑定的采购单 |
| `can_access_sales_order(user, purchaser_id)` | [L245](file:///workspace/src/main.rs#L245) | 行级权限：采购方同理 |
| `get_route_required_role(path)` | [L253](file:///workspace/src/main.rs#L253) | 页面路由 → 所需角色映射 |
| `check_api_route_permission(path)` | [L275](file:///workspace/src/main.rs#L275) | API 路由 → 所需权限点映射 |
| `check_api_permission(headers, path)` | [L360](file:///workspace/src/main.rs#L360) | API 权限校验入口，返回角色或 403 错误 |
| `check_page_permission(headers, path)` | [L375](file:///workspace/src/main.rs#L375) | 页面权限校验，返回角色或重定向登录页 |
| `api_login(Json)` | [L13189](file:///workspace/src/main.rs#L13189) | 登录：bcrypt 校验 + 下发 `session` Cookie |

### 8.3 价格策略

| 函数 | 行号 | 说明 |
|------|------|------|
| `round_to_allowed_last_digit(price)` | [L15656](file:///workspace/src/main.rs#L15656) | 售价尾数规则：仅保留 0/5/6/8/9 尾数（行业定价习惯） |
| `log_price_change(...)` | [L15792](file:///workspace/src/main.rs#L15792) | 价格变更写入 `product_price_log` |
| `recalc_base_price_by_markup(...)` | [L15821](file:///workspace/src/main.rs#L15821) | 按加成率重算售价 `base_price = purchase_price * (1 + markup_rate)` |
| `update_product_purchase_prices(items)` | [L16010](file:///workspace/src/main.rs#L16010) | 采购单保存时自动更新商品进价 + 售价 |
| `generate_order_no(order_type, date)` | [L15612](file:///workspace/src/main.rs#L15612) | 生成唯一订单号 |

### 8.4 订单状态机

| 函数 | 行号 | 说明 |
|------|------|------|
| `api_sales_order_update_status(headers, Json)` | [L26124](file:///workspace/src/main.rs#L26124) | 销售单状态流转，含状态机校验 + 乐观锁（`version+1`） |
| `api_sales_order_approve/unapprove` | [L26204/L26250](file:///workspace/src/main.rs#L26204) | 审核/反审核（pending ↔ confirmed） |
| `api_sales_order_correction(...)` | [L26296](file:///workspace/src/main.rs#L26296) | 订单纠错 |

### 8.5 Excel 导入导出

| 函数 | 行号 | 说明 |
|------|------|------|
| `xlsx_response(buf, filename)` | [L26](file:///workspace/src/main.rs#L26) | 通用 Excel 响应（含 UTF-8 文件名编码） |
| `parse_csv(content)` | [L12862](file:///workspace/src/main.rs#L12862) | CSV 解析为二维数组 |
| `api_supplier_export/import` | [L12913/L12974](file:///workspace/src/main.rs#L12913) | 供应商导入导出 |
| `build_purchase_order_export_workbook(rows)` | [L16679](file:///workspace/src/main.rs#L16679) | 采购单 Excel 工作簿构造 |
| `build_accept_excel(id, reimburse, force)` | [L23974](file:///workspace/src/main.rs#L23974) | 验收单/报销单 Excel（真实/报销双口径，零金额拦截） |

### 8.6 耗材分摊

| 函数 | 行号 | 说明 |
|------|------|------|
| `api_allocation_create/terminate/cancel/complete` | [L23192](file:///workspace/src/main.rs#L23192) | 分摊方案生命周期 |
| `api_supplement_create/list_by_target/list_by_source/delete/compare` | [L23468](file:///workspace/src/main.rs#L23468) | 增项 CRUD + 真实/分摊账套对比 |
| `api_allocation_summary/allocated_orders` | [L23254/L23287](file:///workspace/src/main.rs#L23254) | 分摊汇总 |

---

## 9. 认证与权限体系

### 9.1 会话机制

- **Cookie session**：登录成功后下发 `session={user_id}:{random_token}; HttpOnly; Path=/`（[src/main.rs#L13227](file:///workspace/src/main.rs#L13227)）。
- **密码**：bcrypt 加密存储于 `user_account.password`（[src/main.rs#L13216](file:///workspace/src/main.rs#L13216)）。
- **解析**：`get_user_role` / `get_user_ctx` 从请求头 `cookie` 提取 `session=`，拆分 `:` 取 `user_id`，查 `user_account` 获取角色与绑定关系。

### 9.2 角色层级（粗粒度）

`has_permission()` 实现角色包含关系（[src/main.rs#L104](file:///workspace/src/main.rs#L104)）：

```
super_admin ⊇ admin ⊇ {supplier, purchaser, query}
supplier ⊇ {supplier, query}
purchaser ⊇ {purchaser, query}
user ⊇ {query}
anonymous ⊇ {}
```

### 9.3 RBAC 权限点（细粒度）

`has_permission_point()` 按 `resource.action` 粒度控制（[src/main.rs#L124](file:///workspace/src/main.rs#L124)），共 20 个权限点：

```
purchase_order.{view,create,update,approve,unapprove,cancel,delete}
sales_order.{view,create,update,approve,unapprove,adjust_price,cancel,delete}
query.view
manage.{admin,user,system,backup}
```

各角色权限点映射：
| 角色 | 权限点集合 |
|------|-----------|
| super_admin | 全部 20 个 |
| admin | 除 `manage.user/system/backup` 外全部业务权限 + `manage.admin` |
| supplier | 采购单 view/create/update/approve/cancel + 销售单 view + query |
| purchaser | 采购单 view + 销售单 view/create/update/approve/adjust_price/cancel + query |
| user | 仅 query.view |

### 9.4 双重校验

- **页面**：`get_route_required_role` 映射路由→所需角色，`check_page_permission` 校验。
- **API**：`check_api_route_permission` 映射路由→权限点（按 `path.ends_with("/create")` 等细分），`check_api_permission` 校验。

### 9.5 行级数据权限

通过 `user_account` 绑定 `supplier_id` / `purchaser_id` 实现：
- `can_access_purchase_order`：supplier 账号仅能访问自己绑定的供应商订单（[src/main.rs#L236](file:///workspace/src/main.rs#L236)）。
- `can_access_sales_order`：purchaser 账号同理（[src/main.rs#L245](file:///workspace/src/main.rs#L245)）。

### 9.6 订单状态机约束

仅 `pending`（待配单）状态的订单允许修改/删除，防止已流转单据被篡改（见各 `*_update` / `*_delete` handler）。

### 9.7 操作审计

所有关键写操作（创建/修改/删除/调价/审核/状态变更/分摊操作等）调用 `log_operation` 写入 `operation_log`，记录操作人、时间、动作、目标、详情，可按时间回溯追责（[src/main.rs#L209](file:///workspace/src/main.rs#L209)）。

---

## 10. 订单状态机

销售单状态机定义于 `api_sales_order_update_status`（[src/main.rs#L26164](file:///workspace/src/main.rs#L26164)）：

```
pending（待配单）
   ↓
confirmed（已审核，锁定态）
   ↓
sorting（分拣中）
   ↓
sorted（已分拣）
   ↓
delivering（配送中）
   ↓
delivered（已送达）
   ↓
accepted（已验收）
   ↓
settled（已结算）
```

**允许的迁移**（双向含回退路径）：
```
pending      → {confirmed, sorting}
confirmed    → {pending, sorting}      // confirmed 为锁定态，可反审核回 pending
sorting      → {pending, sorted}
sorted       → {sorting, delivering}
delivering   → {sorted, delivered}
delivered    → {delivering, accepted}
accepted     → {delivered, settled}
settled      → {accepted}
```

**并发防护**：状态更新使用乐观锁 —— `UPDATE sales_order SET status=?, version=version+1 WHERE id=? AND status=?`，若 `rows_affected()=0` 返回 409 提示「订单状态已变化，请刷新后重试」（[src/main.rs#L26183](file:///workspace/src/main.rs#L26183)）。`purchase_order` / `sales_order` 均有 `version` 字段。

---

## 11. 耗材分摊方案

详见 [耗材分摊方案.md](file:///workspace/耗材分摊方案.md) 与 [导出验收单.md](file:///workspace/导出验收单.md)。采用**一库双账**设计：

### 11.1 双账套

| 账套 | 表 | 特性 |
|------|----|------|
| 真实账套 | `sales_order` / `sales_order_item` | 原始真实单据，永不修改 |
| 分摊账套 | `consumable_allocation` + `order_supplement_item` | 独立表，记录增项 |

### 11.2 增项操作类型

`order_supplement_item.operation_type`：
- `new_item`：新增商品行（有独立 `supplement_id`）
- `increase_quantity`：追加已有商品数量（通过 `target_order_item_id` 关联真实明细行，多条记录可累计合并）

**展示合并**：同一真实商品的多次「追加数量」在 `order_supplement_item` 中是多条流水，但展示分摊账套时合并为一条，保证「只保持一条记录」的视觉效果。

### 11.3 金额平衡

`consumable_allocation` 维护：
```
total_amount（耗材总金额）= allocated_amount（已分摊）+ remaining_balance（未分摊）
```
增项金额受「未分摊余额」限制，不能超出。`init_pool` 启动时自动重算 `allocated_amount` 与 `remaining_balance` 修复历史偏差（[src/main.rs#L552](file:///workspace/src/main.rs#L552)）。

### 11.4 状态

```
0 未初始化 → 1 未分摊 → 2 分摊中 → 3 已完成 / 4 已终止
```

### 11.5 对比 API

`GET /api/supplement/compare/{order_id}`（[src/main.rs#L23765](file:///workspace/src/main.rs#L23765)）返回真实账套与分摊账套对比数据，前端左右并排展示差异。

---

## 12. 依赖关系

### 12.1 代码内调用依赖

```
main()
 ├─ init_pool() ── repair_db_corruption() / init_tables() / pool()
 ├─ build_router() ── 装配所有 page_* 与 api_* handler
 └─ tao EventLoop ── 托盘菜单事件 → open_browser()

每个 api_* handler:
 ├─ check_api_permission(headers, path) ── has_permission_point() / get_user_ctx()
 ├─ get_user_ctx(headers) ── 解析行级权限
 ├─ can_access_purchase/sales_order() ── 行级数据过滤
 ├─ sqlx::query(pool()) ── 数据库读写
 ├─ log_operation() ── 审计日志
 └─ xlsx_response() / build_*_excel() ── Excel 导出

每个 page_* handler:
 ├─ check_page_permission(headers, path) ── 权限校验
 └─ layout_html(title, page, content) ── sidebar_html() + SSR 拼装
```

### 12.2 业务流程依赖

```
销售单(SO) ──generate_purchase──> 采购单(PO) ──入库──> 库存(inventory)
   │                                     │
   │                                     └── 更新商品进价/售价(product)
   │
   ├── accept ──> 验收单/报销单 Excel（真实/报销双口径）
   └── supplement ──> 耗材分摊(order_supplement_item) ← consumable_allocation
```

**关键业务链**：销售订单（客户需求）→ 一键生成各供应商采购订单（带防重复生成保护，[src/main.rs#L19142](file:///workspace/src/main.rs#L19142)）→ 采购单保存自动回写商品进价并按加成率重算售价 → 配单分拣 → 验收 → 状态流转 → 对账结算。

---

## 13. 项目运行方式

### 13.1 环境要求

- Rust 工具链（stable channel）
- Windows 发布版额外要求 MSVC 工具链（静态链接 CRT，见 [.cargo/config.toml](file:///workspace/.cargo/config.toml)）

### 13.2 开发运行

```bash
cargo run
```

启动后自动打开浏览器访问 <http://127.0.0.1:3000>（监听 `0.0.0.0:3000`）。

### 13.3 发布构建

```bash
cargo build --release
```

产物位于 `target/release/food_accept_single.exe`，可独立分发（连同 `static/` 资源已在二进制内嵌入，仅需携带 `food_accept_v3.db`；首次运行会自动建库）。

### 13.4 运行时目录

| 路径 | 说明 |
|------|------|
| `food_accept_v3.db` | SQLite 主数据库（自动创建） |
| `uploads/` | 上传图片目录（运行时生成） |
| `backups/` | 数据库备份目录 |

### 13.5 测试

价格尾数规则有单元测试：[src/main.rs#L15678](file:///workspace/src/main.rs#L15678)（`mod price_rounding_tests`）。

```bash
cargo test
```

### 13.6 Git 提交规范

遵循 Conventional Commits 混合模式（见 [.trae/rules/git-commit-message.md](file:///workspace/.trae/rules/git-commit-message.md)）：
- type 保留英文（feat/fix/docs/refactor/perf/test/chore 等）
- scope 与 description 使用中文
- 格式：`<type>(<scope>): <中文描述>`

---

## 附录：相关文档

- [README.md](file:///workspace/README.md) — 项目总说明
- [导出验收单.md](file:///workspace/导出验收单.md) — 验收单/报销单导出口径
- [耗材分摊方案.md](file:///workspace/耗材分摊方案.md) — 耗材分摊业务方案
