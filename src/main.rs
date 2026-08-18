#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use axum::{
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{delete, get, post, put},
    Router,
};
use rust_xlsxwriter::{Format, FormatAlign, FormatBorder, Workbook, Worksheet, XlsxError};
use sqlx::{AssertSqlSafe, Row};
use tao::event_loop::{ControlFlow, EventLoop};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{TrayIconBuilder, TrayIconEvent};

pub mod pages;

pub mod models;
pub mod utils;
pub mod db;
pub mod auth;
pub mod api;

use crate::pages::*;
use crate::models::*;
use crate::auth::*;
use crate::utils::*;
use crate::db::*;
use crate::api::*;


const BOOTSTRAP_CSS: &str = include_str!("../static/bootstrap.min.css");
const BOOTSTRAP_JS: &str = include_str!("../static/bootstrap.bundle.min.js");
const CHART_JS: &str = include_str!("../static/chart.umd.min.js");



// ===== 细粒度权限点系统（工程化 RBAC） =====
// 权限点：resource.action，如 purchase_order.update
// 角色 → 权限点映射，super_admin 拥有全部权限

/// 判断某角色是否拥有指定权限点

/// 当前登录用户的上下文（角色 + 用户ID + 行级数据权限关联）


/// 解析用户上下文：cookie session -> (role, user_id, supplier_id, purchaser_id)

/// 记录操作审计日志（关键写操作）

/// 判断用户是否可操作该采购单（行级数据权限）：admin/super_admin 可见全部；supplier 仅可见自己绑定的供应商

/// 判断用户是否可操作该销售单（行级数据权限）：admin/super_admin 可见全部；purchaser 仅可见自己绑定的采购单位



/// 校验 API 权限：返回权限点对应的角色校验（权限点系统）


pub(crate) async fn serve_bootstrap_css() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        BOOTSTRAP_CSS,
    )
}

pub(crate) async fn serve_bootstrap_js() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/javascript; charset=utf-8")],
        BOOTSTRAP_JS,
    )
}

pub(crate) async fn serve_chart_js() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/javascript; charset=utf-8")],
        CHART_JS,
    )
}

// 修复常见数据库损坏















































































/// 操作动作中文文案

/// 操作审计日志查询：分页 + 按操作人/动作/日期范围筛选

/// 操作审计日志导出 Excel：应用与列表一致的筛选条件，导出全量匹配记录供审计分析











































// 清理文件名前缀中的非法字符（保留中文、字母、数字、-、_）

// 将图片URL转换为服务器文件路径（兼容旧格式 /api/product/image/ 与新格式 /api/uploads/...）



// 销售订单图片上传：type = customer(客户订单) / signed(已验收签字订单)


// 采购单据列表：按供应商+日期查询




// 服务分类子目录下的图片：/api/uploads/{folder}/{filename}













// 查询商品价格变更日志（支持按商品ID过滤，可选 limit）




















// 采购入库后，按商品ID更新当前进价/历史最高进价/历史最低进价
// unit_price 为该采购明细单位单价，需换算回基础单位单价：base_unit_price = unit_price / ratio
// 这里 base_quantity/quantity 可近似换算比例，但为稳妥直接用 unit_price 与商品当前记录比较（明细已按下单单位存储）。
// 售价自动更新专用取整：保留两位小数，最末位仅允许 0/5/6/8/9
// 就近取值（不向上靠）：末位与允许集合中最近者匹配
// 映射表（百位百分位）：0→0, 1→0, 2→0, 3→5, 4→5, 5→5, 6→6, 7→8, 8→8, 9→9

#[cfg(test)]
mod price_rounding_tests {
    use super::round_to_allowed_last_digit;

    // 末位映射表：0/1/2→0, 3/4/5→5, 6→6, 7/8→8, 9→9
    fn expected(whole: i64, last: i64) -> f64 {
        let mapped = match last {
            0 | 1 | 2 => 0,
            3 | 4 | 5 => 5,
            6 => 6,
            7 | 8 => 8,
            9 => 9,
            _ => last,
        };
        (whole * 10 + mapped) as f64 / 100.0
    }

    #[test]
    fn test_last_digit_zero_to_two_rounds_to_zero() {
        for last in 0..=2 {
            let price = 8.30 + (last as f64) * 0.01;
            let r = round_to_allowed_last_digit(price);
            assert!(
                (r - expected(83, last)).abs() < 0.0001,
                "末位 {} 输入 {} 期望 {} 实际 {}",
                last, price, expected(83, last), r
            );
        }
    }

    #[test]
    fn test_last_digit_three_to_five_rounds_to_five() {
        for last in 3..=5 {
            let price = 8.30 + (last as f64) * 0.01;
            let r = round_to_allowed_last_digit(price);
            assert!(
                (r - expected(83, last)).abs() < 0.0001,
                "末位 {} 输入 {} 期望 {} 实际 {}",
                last, price, expected(83, last), r
            );
        }
    }

    #[test]
    fn test_last_digit_six_unchanged() {
        let r = round_to_allowed_last_digit(8.36);
        assert!((r - 8.36).abs() < 0.0001, "8.36 期望 8.36 实际 {}", r);
    }

    #[test]
    fn test_last_digit_seven_eight_rounds_to_eight() {
        let r7 = round_to_allowed_last_digit(8.37);
        assert!((r7 - 8.38).abs() < 0.0001, "8.37 期望 8.38 实际 {}", r7);
        let r8 = round_to_allowed_last_digit(8.38);
        assert!((r8 - 8.38).abs() < 0.0001, "8.38 期望 8.38 实际 {}", r8);
    }

    #[test]
    fn test_last_digit_nine_unchanged() {
        let r = round_to_allowed_last_digit(8.39);
        assert!((r - 8.39).abs() < 0.0001, "8.39 期望 8.39 实际 {}", r);
    }

    #[test]
    fn test_full_ten_cent_coverage() {
        // 覆盖 0.00-0.09 共 10 个尾数，期望结果末位必须属于 {0,5,6,8,9}
        let allowed = [0, 5, 6, 8, 9];
        for last in 0..=9 {
            let price = 12.30 + (last as f64) * 0.01;
            let r = round_to_allowed_last_digit(price);
            // 用 100 倍还原分
            let cents = (r * 100.0).round() as i64;
            let actual_last = cents % 10;
            assert!(
                allowed.contains(&actual_last),
                "尾数 {} 输入 {} 得到 {}，末位 {} 不在允许集合",
                last, price, r, actual_last
            );
        }
    }

    #[test]
    fn test_realistic_purchase_markup_scenarios() {
        // 模拟一批进价 × (1 + 0.5) 后的尾数
        for purchase_cents in [350i64, 437, 562, 689, 715, 832, 999, 1024, 1280, 1567, 2034] {
            let purchase = purchase_cents as f64 / 100.0;
            let raw = purchase * 1.5;
            let r = round_to_allowed_last_digit(raw);
            let cents = (r * 100.0).round() as i64;
            let last = cents % 10;
            let allowed = [0, 5, 6, 8, 9];
            assert!(
                allowed.contains(&last),
                "进价 {} → 原始售价 {} → 调整后 {} 末位 {} 不在允许集合",
                purchase, raw, r, last
            );
        }
    }

    #[test]
    fn test_zero_or_negative_returns_as_is() {
        assert_eq!(round_to_allowed_last_digit(0.0), 0.0);
        assert_eq!(round_to_allowed_last_digit(-1.5), -1.5);
    }

    #[test]
    fn test_floating_edge_cases() {
        // 浮点 8.57000000000001 应等同 8.57
        let r = round_to_allowed_last_digit(8.570_000_000_000_007);
        assert!((r - 8.58).abs() < 0.0001, "8.57(浮点) 期望 8.58 实际 {}", r);
    }
}

// 记录价格变更日志（price_type: purchase_price / base_price）
pub(crate) async fn log_price_change(
    product_id: i64,
    price_type: &str,
    old_price: f64,
    new_price: f64,
    source: &str,
    ref_id: Option<i64>,
    remark: Option<&str>,
) {
    // 仅当价格实际变化时记录
    if (old_price - new_price).abs() < 0.001 {
        return;
    }
    let _ = sqlx::query(
        "INSERT INTO product_price_log(product_id, price_type, old_price, new_price, source, ref_id, remark) VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(product_id)
    .bind(price_type)
    .bind(old_price)
    .bind(new_price)
    .bind(source)
    .bind(ref_id)
    .bind(remark)
    .execute(pool())
    .await;
}

// 根据加成率自动重算 base_price；返回是否实际更新了售价及旧/新值
// 当商品开启 auto_update_price 且 purchase_price > 0 时生效
pub(crate) async fn recalc_base_price_by_markup(
    product_id: i64,
    source: &str,
    ref_id: Option<i64>,
) {
    let row = sqlx::query(
        "SELECT purchase_price, base_price, markup_rate, auto_update_price FROM product WHERE id = ?"
    )
    .bind(product_id)
    .fetch_optional(pool())
    .await
    .unwrap_or(None);

    if let Some(r) = row {
        let purchase_price: f64 = r.get("purchase_price");
        let old_base_price: f64 = r.get("base_price");
        let markup_rate: f64 = r.get("markup_rate");
        let auto_update: i64 = r.get("auto_update_price");

        if auto_update == 0 || purchase_price <= 0.0 {
            eprintln!(
                "[售价自动更新] 商品ID={} 跳过：auto_update_price={}, purchase_price={}",
                product_id, auto_update, purchase_price
            );
            return;
        }

        let raw_price = purchase_price * (1.0 + markup_rate);
        let new_base_price = round_to_allowed_last_digit(raw_price);
        eprintln!(
            "[售价自动更新] 商品ID={} source={} 进价={:.4} 加成率={:.4} 原始售价={:.6} 取整后售价={:.4} 旧售价={:.4}",
            product_id, source, purchase_price, markup_rate, raw_price, new_base_price, old_base_price
        );
        if (old_base_price - new_base_price).abs() < 0.001 {
            eprintln!("[售价自动更新] 商品ID={} 售价未变化，跳过写库", product_id);
            return;
        }

        let _ = sqlx::query("UPDATE product SET base_price = ? WHERE id = ?")
            .bind(new_base_price)
            .bind(product_id)
            .execute(pool())
            .await;

        log_price_change(
            product_id,
            "base_price",
            old_base_price,
            new_base_price,
            source,
            ref_id,
            Some("按加成率自动重算"),
        ).await;
    }
}

// 单个商品开启/关闭自动更新售价，并立即按加成率重算 base_price

// 批量：对所有商品开启/关闭自动更新售价，并对开启的商品立即按加成率重算 base_price

// 获取某商品最近一次采购的基础单位单价（按 purchase_order_item 的 base_unit 维度的同基础单位）
// 通过该商品最近的采购单明细反推：unit_price 为下单单位价，乘以 ratio 得基础单位价

// 规则：当前进价 = 最近一次采购价；最高进价 = 历史最高；最低进价 = 历史最低（新品或价格为0时初始化）。
pub(crate) async fn update_product_purchase_prices(items: &[PurchaseOrderItemReq]) {
    for item in items {
        if item.product_id == 0 {
            continue;
        }
        // 换算为基础单位单价
        let base_unit_price = if let Some(bq) = item.base_quantity {
            if bq > 0.0 && item.quantity > 0.0 {
                // amount / base_quantity 得到基础单位单价
                if item.amount > 0.0 { item.amount / bq } else { item.unit_price }
            } else {
                item.unit_price
            }
        } else {
            item.unit_price
        };
        if base_unit_price <= 0.0 {
            continue;
        }

        // 读取商品当前的最高/最低进价
        let row = sqlx::query(
            "SELECT purchase_price, max_purchase_price, min_purchase_price FROM product WHERE id = ?"
        )
        .bind(item.product_id)
        .fetch_optional(pool())
        .await
        .unwrap_or(None);

        if let Some(r) = row {
            let old_purchase: f64 = r.get::<f64, _>("purchase_price");
            let old_max: f64 = r.get::<f64, _>("max_purchase_price");
            let old_min: f64 = r.get::<f64, _>("min_purchase_price");

            // 若历史最高/最低为0（新品或首次采购），则以本次价格初始化
            let new_max = if old_max <= 0.0 { base_unit_price } else { old_max.max(base_unit_price) };
            let new_min = if old_min <= 0.0 { base_unit_price } else { old_min.min(base_unit_price) };

            let _ = sqlx::query(
                "UPDATE product SET purchase_price = ?, max_purchase_price = ?, min_purchase_price = ? WHERE id = ?"
            )
            .bind(base_unit_price)
            .bind(new_max)
            .bind(new_min)
            .bind(item.product_id)
            .execute(pool())
            .await;

            // 记录进价变更日志
            log_price_change(
                item.product_id,
                "purchase_price",
                old_purchase,
                base_unit_price,
                "purchase_order",
                None,
                Some("采购单更新进价"),
            ).await;

            // 进价变化后，若开启自动售价更新则按加成率重算 base_price
            eprintln!(
                "[采购单进价更新] 商品ID={} 基础单位进价={:.4} 触发售价重算",
                item.product_id, base_unit_price
            );
            recalc_base_price_by_markup(item.product_id, "purchase_order", None).await;
        }
    }
}

// 采购单删除后，从剩余采购明细历史重新计算商品的最近/最高/最低进价并回写
// 规则：purchase_price = 现存明细中最近一次的采购价；max/min = 现存明细历史最高/最低
//       若该商品已无任何采购明细，则三项均置 0
pub(crate) async fn recalc_product_purchase_prices_from_history(product_id: i64) {
    // 现存所有采购明细（含已审核与待审核订单），换算为基础单位单价
    let rows = sqlx::query(
        "SELECT poi.unit_price, poi.quantity, poi.base_quantity, poi.amount, po.order_date
         FROM purchase_order_item poi
         JOIN purchase_order po ON poi.order_id = po.id
         WHERE poi.product_id = ?
         ORDER BY po.order_date DESC, po.id DESC, poi.id DESC"
    )
    .bind(product_id)
    .fetch_all(pool())
    .await
    .unwrap_or_default();

    let mut prices: Vec<f64> = Vec::new();
    let mut latest_price: f64 = 0.0;
    for r in &rows {
        let unit_price: f64 = r.get("unit_price");
        let quantity: f64 = r.get("quantity");
        let base_quantity: f64 = r.try_get("base_quantity").unwrap_or(0.0);
        let amount: f64 = r.get("amount");
        // 与 update_product_purchase_prices 保持一致的换算逻辑
        let base_unit_price = if base_quantity > 0.0 && quantity > 0.0 && amount > 0.0 {
            amount / base_quantity
        } else {
            unit_price
        };
        if base_unit_price <= 0.0 {
            continue;
        }
        if latest_price <= 0.0 {
            latest_price = base_unit_price;
        }
        prices.push(base_unit_price);
    }

    let new_purchase = latest_price;
    let new_max = prices.iter().cloned().fold(0.0, f64::max);
    let new_min = prices.iter().cloned().fold(f64::INFINITY, f64::min);
    let new_min = if new_min.is_infinite() { 0.0 } else { new_min };

    let row = sqlx::query(
        "SELECT purchase_price, max_purchase_price, min_purchase_price FROM product WHERE id = ?"
    )
    .bind(product_id)
    .fetch_optional(pool())
    .await
    .unwrap_or(None);

    if let Some(r) = row {
        let old_purchase: f64 = r.get("purchase_price");

        let _ = sqlx::query(
            "UPDATE product SET purchase_price = ?, max_purchase_price = ?, min_purchase_price = ? WHERE id = ?"
        )
        .bind(new_purchase)
        .bind(new_max)
        .bind(new_min)
        .bind(product_id)
        .execute(pool())
        .await;

        if (old_purchase - new_purchase).abs() >= 0.001 {
            log_price_change(
                product_id,
                "purchase_price",
                old_purchase,
                new_purchase,
                "purchase_order_delete",
                None,
                Some("采购单删除后从历史重算"),
            ).await;
            // 进价变化后，若开启自动售价更新则按加成率重算 base_price
            recalc_base_price_by_markup(product_id, "purchase_order_delete", None).await;
        }
        eprintln!(
            "[采购单删除回滚] 商品ID={} 最近进价={:.4} 最高={:.4} 最低={:.4}",
            product_id, new_purchase, new_max, new_min
        );
    }
}






/// 采购单审核：pending → confirmed，锁定订单禁止修改（需填写备注原因）

/// 采购单反审核：confirmed → pending，解除锁定以便修改（仅管理员，强制原因）


pub(crate) fn build_purchase_order_export_workbook(rows: Vec<sqlx::sqlite::SqliteRow>) -> axum::response::Response {
    let result: Result<Vec<u8>, XlsxError> = (|| {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();

        let header_format = Format::new()
            .set_bold()
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter);

        // 分组标题（仓库）格式：参照供应商分拣页的采购单位分组行
        let section_format = Format::new()
            .set_bold()
            .set_font_size(11)
            .set_align(FormatAlign::Left)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#E5E7EB")
            .set_font_color("#374151");

        // 分组小计/总计格式：参照图示样式（小计：包装数量 N；总计：灰底加粗）
        let summary_format = Format::new()
            .set_bold()
            .set_font_size(10)
            .set_align(FormatAlign::Left)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin);

        let summary_right_format = Format::new()
            .set_bold()
            .set_font_size(10)
            .set_align(FormatAlign::Right)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin);

        let grand_total_format = Format::new()
            .set_bold()
            .set_font_size(11)
            .set_align(FormatAlign::Left)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#E5E7EB")
            .set_font_color("#374151");

        let grand_total_right_format = Format::new()
            .set_bold()
            .set_font_size(11)
            .set_align(FormatAlign::Right)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#E5E7EB")
            .set_font_color("#374151");

        // 明细按仓库分组：主表不再显示"入库仓库"列，仓库通过分组标题行展示
        let headers = ["订单ID", "订单号", "订单日期", "供应商", "总金额", "下浮率(%)", "下浮后合计", "状态", "备注", "商品名称", "下订名称(别称1)", "配单名称(别称2)", "规格", "单位", "订购数量", "数量", "单价", "基本数量", "金额", "商品备注"];
        for (i, &header) in headers.iter().enumerate() {
            worksheet.write_with_format(0, i as u16, header, &header_format)?;
        }

        let mut row_idx = 1;
        let mut last_item_wh: Option<String> = None;
        let mut group_item_count = 0i64;
        let mut group_amount = 0.0;
        let mut grand_item_count = 0i64;
        let mut grand_amount = 0.0;

        // 每个仓库分组的末尾写一行小计（参照图示样式：仓库名 + 小计: 包装数量 N + 金额）
        let write_group_subtotal = |worksheet: &mut Worksheet, row: &mut u32, wh: &str, item_count: i64, amount: f64| -> Result<(), XlsxError> {
            let wh_label = if wh.is_empty() { "未指定".to_string() } else { wh.to_string() };
            let subtotal = format!("├── {}   小计: 包装数量 {}", wh_label, item_count);
            worksheet.merge_range(*row, 0, *row, 17, subtotal.as_str(), &summary_format)?;
            worksheet.write_with_format(*row, 18, amount, &summary_right_format)?;
            worksheet.write_with_format(*row, 19, "", &summary_format)?;
            worksheet.set_row_height(*row, 18)?;
            *row += 1;
            Ok(())
        };

        for row in rows {
            // 明细仓库作为分组键：仓库变化时插入分组标题行，并给上一分组写小计
            let item_wh: String = row.try_get::<Option<String>, _>("item_warehouse_name").unwrap_or(None).unwrap_or_default().trim().to_string();
            if last_item_wh.as_deref() != Some(item_wh.as_str()) {
                if let Some(prev_wh) = last_item_wh.take() {
                    write_group_subtotal(&mut *worksheet, &mut row_idx, &prev_wh, group_item_count, group_amount)?;
                    grand_item_count += group_item_count;
                    grand_amount += group_amount;
                }
                last_item_wh = Some(item_wh.clone());
                group_item_count = 0;
                group_amount = 0.0;
                let wh_display = if item_wh.is_empty() { "未指定".to_string() } else { item_wh.clone() };
                let section_title = format!("├── {}", wh_display);
                worksheet.merge_range(row_idx, 0, row_idx, 19, section_title.as_str(), &section_format)?;
                worksheet.set_row_height(row_idx, 20)?;
                row_idx += 1;
            }
            group_item_count += 1;
            group_amount += row.get::<Option<f64>, _>("amount").unwrap_or(0.0);

            worksheet.write(row_idx, 0, row.get::<i64, _>("id"))?;
            worksheet.write(row_idx, 1, row.get::<String, _>("order_no"))?;
            worksheet.write(row_idx, 2, row.get::<String, _>("order_date"))?;
            worksheet.write(row_idx, 3, row.get::<String, _>("supplier_name"))?;
            worksheet.write(row_idx, 4, row.get::<f64, _>("total_amount"))?;
            worksheet.write(row_idx, 5, row.get::<f64, _>("discount_rate"))?;
            worksheet.write(row_idx, 6, row.get::<f64, _>("final_amount"))?;
            worksheet.write(row_idx, 7, row.get::<String, _>("status"))?;
            worksheet.write(row_idx, 8, row.get::<Option<String>, _>("remark").unwrap_or_default())?;
            worksheet.write(row_idx, 9, row.get::<Option<String>, _>("product_name").unwrap_or_default())?;
            worksheet.write(row_idx, 10, row.get::<Option<String>, _>("alias1").unwrap_or_default())?;
            worksheet.write(row_idx, 11, row.get::<Option<String>, _>("alias2").unwrap_or_default())?;
            worksheet.write(row_idx, 12, row.get::<Option<String>, _>("spec").unwrap_or_default())?;
            worksheet.write(row_idx, 13, row.get::<Option<String>, _>("unit").unwrap_or_default())?;
            worksheet.write(row_idx, 14, row.get::<Option<f64>, _>("ordered_quantity").unwrap_or(0.0))?;
            worksheet.write(row_idx, 15, row.get::<Option<f64>, _>("quantity").unwrap_or(0.0))?;
            worksheet.write(row_idx, 16, row.get::<Option<f64>, _>("unit_price").unwrap_or(0.0))?;
            worksheet.write(row_idx, 17, row.get::<Option<f64>, _>("base_quantity").unwrap_or(0.0))?;
            worksheet.write(row_idx, 18, row.get::<Option<f64>, _>("amount").unwrap_or(0.0))?;
            worksheet.write(row_idx, 19, row.get::<Option<String>, _>("item_remark").unwrap_or_default())?;
            row_idx += 1;
        }

        // 最后一组的小计 + 全部总计
        if let Some(prev_wh) = last_item_wh.take() {
            write_group_subtotal(&mut *worksheet, &mut row_idx, &prev_wh, group_item_count, group_amount)?;
            grand_item_count += group_item_count;
            grand_amount += group_amount;
        }
        if grand_item_count > 0 {
            let grand_total = format!("总计: 包装数量 {}", grand_item_count);
            worksheet.merge_range(row_idx, 0, row_idx, 17, grand_total.as_str(), &grand_total_format)?;
            worksheet.write_with_format(row_idx, 18, grand_amount, &grand_total_right_format)?;
            worksheet.write_with_format(row_idx, 19, "", &grand_total_format)?;
            worksheet.set_row_height(row_idx, 22)?;
        }

        worksheet.set_column_width(0, 8)?;
        worksheet.set_column_width(1, 18)?;
        worksheet.set_column_width(2, 12)?;
        worksheet.set_column_width(3, 15)?;
        worksheet.set_column_width(4, 10)?;
        worksheet.set_column_width(5, 12)?;
        worksheet.set_column_width(6, 12)?;
        worksheet.set_column_width(7, 10)?;
        worksheet.set_column_width(8, 12)?;
        worksheet.set_column_width(9, 15)?;
        worksheet.set_column_width(10, 15)?;
        worksheet.set_column_width(11, 15)?;
        worksheet.set_column_width(12, 10)?;
        worksheet.set_column_width(13, 8)?;
        worksheet.set_column_width(14, 10)?;
        worksheet.set_column_width(15, 8)?;
        worksheet.set_column_width(16, 8)?;
        worksheet.set_column_width(17, 10)?;
        worksheet.set_column_width(18, 10)?;
        worksheet.set_column_width(19, 15)?;

        workbook.save_to_buffer()
    })();

    match result {
        Ok(data) => (
            StatusCode::OK,
            [
                ("Content-Type", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
                ("Content-Disposition", "attachment; filename=\"purchase_orders.xlsx\""),
            ],
            data,
        ).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("导出失败: {}", e)).into_response(),
    }
}

// 导出采购单（打印模板样式）：单张采购单按打印模板格式导出

#[derive(Debug)]
pub(crate) struct PurchaseOrderPrint {
    order_no: String,
    order_date: String,
    total_amount: f64,
    amount_reduction: f64,
    final_amount: f64,
    remark: Option<String>,
    supplier_name: Option<String>,
    supplier_phone: Option<String>,
    supplier_address: Option<String>,
    user_name: Option<String>,
    handler_phone: Option<String>,
}

#[derive(Debug)]
pub(crate) struct PurchaseOrderPrintItem {
    warehouse_name: String,
    product_name: String,
    spec: Option<String>,
    unit: Option<String>,
    quantity: f64,
    unit_price: f64,
    amount: f64,
    remark: Option<String>,
}

pub(crate) struct UserSimple { nickname: String, phone: String }

pub(crate) async fn get_purchase_order_with_items(id: i64) -> Option<(PurchaseOrderPrint, Vec<PurchaseOrderPrintItem>)> {
    let order = sqlx::query_as::<_, (
        String, String, f64, f64, f64, Option<String>,
        Option<String>, Option<String>, Option<String>, Option<String>, Option<String>,
    )>(
        "SELECT po.order_no, po.order_date, po.total_amount, po.amount_reduction, po.final_amount,
                po.remark,
                s.name, s.phone, s.address,
                u.nickname, po.handler_phone
         FROM purchase_order po
         JOIN supplier s ON po.supplier_id = s.id
         LEFT JOIN user_account u ON po.user_id = u.id
         WHERE po.id = ?"
    )
        .bind(id)
        .fetch_optional(pool())
        .await
        .ok()
        .flatten()?;

    let (order_no, order_date, total_amount, amount_reduction, final_amount, remark,
         supplier_name, supplier_phone, supplier_address, user_name, handler_phone) = order;

    let item_rows = sqlx::query_as::<_, (
        Option<String>, String, Option<String>, Option<String>, f64, f64, f64, Option<String>
    )>(
        "SELECT warehouse_name, product_name, spec, unit, quantity, unit_price, amount, remark FROM purchase_order_item WHERE order_id = ? ORDER BY warehouse_name, id"
    )
        .bind(id)
        .fetch_all(pool())
        .await
        .unwrap_or_default();

    let items: Vec<PurchaseOrderPrintItem> = item_rows
        .into_iter()
        .map(|(warehouse_name, product_name, spec, unit, quantity, unit_price, amount, remark)| {
            PurchaseOrderPrintItem {
                warehouse_name: warehouse_name.unwrap_or_default().trim().to_string(),
                product_name, spec, unit, quantity, unit_price, amount, remark
            }
        })
        .collect();

    Some((
        PurchaseOrderPrint {
            order_no, order_date, total_amount, amount_reduction, final_amount,
            remark, supplier_name, supplier_phone, supplier_address,
            user_name, handler_phone,
        },
        items,
    ))
}

pub(crate) async fn get_user_by_id(id: i64) -> Option<UserSimple> {
    sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT nickname, COALESCE(NULLIF(phone, ''), '') as phone FROM user_account WHERE id = ?"
    )
        .bind(id)
        .fetch_optional(pool())
        .await
        .ok()
        .flatten()
        .map(|(nickname, phone)| UserSimple { nickname, phone: phone.unwrap_or_default() })
}




// 一键获取订单明细的最新售价（不写入数据库），返回给前端供用户手动保存












// 商品价格同期走势：返回指定商品在时间段内的进价点（来自采购单实际成交价）和售价点（来自价格变更日志）
// 不应用尾数取整规则——历史原值



// 报销口径汇总：目标单真实明细 + 分摊增项净额，排除耗材分摊来源单本身，避免重计

// 分摊来源统计：列出所有作为分摊来源的订单及其分摊去向








// === 补全缺失的查询 API ===



pub(crate) struct StockSummaryRow {
    day: String,
    warehouse_id: i64,
    warehouse_name: String,
    is_summary: bool,
    in_amount: f64,
    in_item_count: i64,
    in_order_count: i64,
    out_amount: f64,
    out_item_count: i64,
    out_order_count: i64,
    discounted_out_amount: f64, // 下浮后出库金额 = 出库金额 × (1 - 下浮率/100)
    gross_profit: f64,          // 毛利 = 下浮后出库金额 - 入库金额
}

pub(crate) async fn compute_stock_summary(start_date: &str, end_date: &str) -> (Vec<StockSummaryRow>, f64, f64, f64) {
    // 采购入库按日+仓库汇总
    let mut purchase_where = String::from("WHERE 1=1");
    if !start_date.is_empty() {
        purchase_where.push_str(&format!(" AND po.order_date >= '{}'", start_date));
    }
    if !end_date.is_empty() {
        purchase_where.push_str(&format!(" AND po.order_date <= '{}'", end_date));
    }
    let purchase_sql = format!(
        "WITH src AS (
            SELECT po.order_date as day,
                   COALESCE(poi.warehouse_id, po.warehouse_id, 0) as warehouse_id,
                   COALESCE(NULLIF(TRIM(poi.warehouse_name), ''), po.warehouse_name, '未指定') as warehouse_name,
                   poi.amount as amount,
                   po.id as po_id
            FROM purchase_order po
            JOIN purchase_order_item poi ON poi.order_id = po.id
            {}
         )
         SELECT day,
                warehouse_id,
                warehouse_name,
                COALESCE(SUM(amount), 0) as in_amount,
                COUNT(*) as in_item_count,
                COUNT(DISTINCT po_id) as in_order_count
         FROM src
         GROUP BY day, warehouse_id, warehouse_name",
        purchase_where
    );
    let purchase_rows = sqlx::query(AssertSqlSafe(purchase_sql.as_str()))
        .fetch_all(pool())
        .await
        .unwrap_or_default();

    // 销售出库按明细行返回，含订单下浮率，在 Rust 端按明细计算下浮后金额
    let mut sales_where = String::from("WHERE 1=1");
    if !start_date.is_empty() {
        sales_where.push_str(&format!(" AND so.order_date >= '{}'", start_date));
    }
    if !end_date.is_empty() {
        sales_where.push_str(&format!(" AND so.order_date <= '{}'", end_date));
    }
    let sales_sql = format!(
        "SELECT so.order_date as day,
                COALESCE(so.warehouse_id, 0) as warehouse_id,
                COALESCE(so.warehouse_name, '未指定') as warehouse_name,
                soi.amount as out_amount,
                so.id as order_id,
                COALESCE(so.discount_rate, 0) as discount_rate
         FROM sales_order so
         JOIN sales_order_item soi ON soi.order_id = so.id
         {}",
        sales_where
    );
    let sales_rows = sqlx::query(AssertSqlSafe(sales_sql.as_str()))
        .fetch_all(pool())
        .await
        .unwrap_or_default();

    use std::collections::{BTreeMap, HashMap};

    // Key: (day, warehouse_id) -> (in_amount, in_items, in_orders, out_amount, out_items, out_orders, warehouse_name, discounted_out)
    let mut day_wh_map: HashMap<String, BTreeMap<i64, (f64, i64, i64, f64, i64, i64, String, f64)>> = HashMap::new();

    // 先收集所有仓库名称用于排序
    let mut warehouse_names: BTreeMap<i64, String> = BTreeMap::new();

    for row in &purchase_rows {
        let day: String = row.get("day");
        let wh_id: i64 = row.get::<i64, _>("warehouse_id");
        let wh_name: String = row.get::<String, _>("warehouse_name");
        let amount: f64 = row.get::<f64, _>("in_amount");
        let items: i64 = row.get::<i64, _>("in_item_count");
        let orders: i64 = row.get::<i64, _>("in_order_count");

        warehouse_names.insert(wh_id, wh_name.clone());

        let wh_map = day_wh_map.entry(day).or_insert_with(BTreeMap::new);
        let e = wh_map.entry(wh_id).or_insert((0.0, 0, 0, 0.0, 0, 0, wh_name, 0.0));
        e.0 += amount;
        e.1 += items;
        e.2 += orders;
    }

    // 按明细行汇总：out_amount 累加原值，discounted_out 累加下浮后金额
    // 下浮后金额 = amount × (1 - discount_rate/100)
    // 订单数与条数通过 Set 去重统计
    let mut seen_orders: std::collections::HashSet<(String, i64, i64)> = std::collections::HashSet::new();
    for row in &sales_rows {
        let day: String = row.get("day");
        let wh_id: i64 = row.get::<i64, _>("warehouse_id");
        let wh_name: String = row.get::<String, _>("warehouse_name");
        let amount: f64 = row.get::<f64, _>("out_amount");
        let order_id: i64 = row.get::<i64, _>("order_id");
        let discount_rate: f64 = row.get::<f64, _>("discount_rate");
        let discounted = amount * (1.0 - discount_rate / 100.0);

        warehouse_names.insert(wh_id, wh_name.clone());

        let wh_map = day_wh_map.entry(day.clone()).or_insert_with(BTreeMap::new);
        let e = wh_map.entry(wh_id).or_insert((0.0, 0, 0, 0.0, 0, 0, wh_name, 0.0));
        e.3 += amount;
        e.4 += 1; // 条数
        e.7 += discounted;
        if seen_orders.insert((day, wh_id, order_id)) {
            e.5 += 1; // 单数（去重）
        }
    }

    // 按日期倒序，每天输出仓库明细行 + 汇总行
    let mut days: Vec<String> = day_wh_map.keys().cloned().collect();
    days.sort_by(|a, b| b.cmp(a)); // 倒序

    let mut rows: Vec<StockSummaryRow> = Vec::new();
    let mut total_in = 0.0;
    let mut total_out = 0.0;
    let mut total_discounted_out = 0.0;

    for day in &days {
        if let Some(wh_map) = day_wh_map.get(day) {
            let mut day_in = 0.0;
            let mut day_out = 0.0;
            let mut day_discounted_out = 0.0;
            let mut day_in_items = 0;
            let mut day_out_items = 0;
            let mut day_in_orders = 0;
            let mut day_out_orders = 0;

            // 输出每个仓库行
            for (wh_id, v) in wh_map {
                day_in += v.0;
                day_out += v.3;
                day_discounted_out += v.7;
                day_in_items += v.1;
                day_out_items += v.4;
                day_in_orders += v.2;
                day_out_orders += v.5;

                rows.push(StockSummaryRow {
                    day: day.clone(),
                    warehouse_id: *wh_id,
                    warehouse_name: v.6.clone(),
                    is_summary: false,
                    in_amount: v.0,
                    in_item_count: v.1,
                    in_order_count: v.2,
                    out_amount: v.3,
                    out_item_count: v.4,
                    out_order_count: v.5,
                    discounted_out_amount: v.7,
                    gross_profit: v.7 - v.0,
                });
            }

            // 输出当天汇总行
            rows.push(StockSummaryRow {
                day: day.clone(),
                warehouse_id: -1,
                warehouse_name: "当日汇总".to_string(),
                is_summary: true,
                in_amount: day_in,
                in_item_count: day_in_items,
                in_order_count: day_in_orders,
                out_amount: day_out,
                out_item_count: day_out_items,
                out_order_count: day_out_orders,
                discounted_out_amount: day_discounted_out,
                gross_profit: day_discounted_out - day_in,
            });

            total_in += day_in;
            total_out += day_out;
            total_discounted_out += day_discounted_out;
        }
    }

    (rows, total_in, total_out, total_discounted_out)
}

pub(crate) async fn compute_stock_summary_reimburse(start_date: &str, end_date: &str) -> (Vec<StockSummaryRow>, f64, f64, f64) {
    // 入库：与真实账套一致
    let mut purchase_where = String::from("WHERE 1=1");
    if !start_date.is_empty() { purchase_where.push_str(&format!(" AND po.order_date >= '{}'", start_date)); }
    if !end_date.is_empty() { purchase_where.push_str(&format!(" AND po.order_date <= '{}'", end_date)); }
    let purchase_sql = format!(
        "WITH src AS (
            SELECT po.order_date as day,
                   COALESCE(poi.warehouse_id, po.warehouse_id, 0) as warehouse_id,
                   COALESCE(NULLIF(TRIM(poi.warehouse_name), ''), po.warehouse_name, '未指定') as warehouse_name,
                   poi.amount as amount,
                   po.id as po_id
            FROM purchase_order po
            JOIN purchase_order_item poi ON poi.order_id = po.id
            {}
         )
         SELECT day,
                warehouse_id,
                warehouse_name,
                COALESCE(SUM(amount), 0) as in_amount,
                COUNT(*) as in_item_count,
                COUNT(DISTINCT po_id) as in_order_count
         FROM src
         GROUP BY day, warehouse_id, warehouse_name",
        purchase_where
    );
    let purchase_rows = sqlx::query(AssertSqlSafe(purchase_sql.as_str()))
        .fetch_all(pool())
        .await
        .unwrap_or_default();

    // 真实出库（按明细行返回，含订单下浮率）
    let mut sales_where = String::from("WHERE 1=1");
    if !start_date.is_empty() { sales_where.push_str(&format!(" AND so.order_date >= '{}'", start_date)); }
    if !end_date.is_empty() { sales_where.push_str(&format!(" AND so.order_date <= '{}'", end_date)); }
    let sales_sql = format!(
        "SELECT so.order_date as day,
                COALESCE(so.warehouse_id, 0) as warehouse_id,
                COALESCE(so.warehouse_name, '未指定') as warehouse_name,
                soi.amount as out_amount,
                so.id as order_id,
                COALESCE(so.discount_rate, 0) as discount_rate
         FROM sales_order so
         JOIN sales_order_item soi ON soi.order_id = so.id
         {}",
        sales_where
    );
    let sales_rows = sqlx::query(AssertSqlSafe(sales_sql.as_str()))
        .fetch_all(pool())
        .await
        .unwrap_or_default();

    // 分摊调整1：目标单收到的增项金额（+出库），按明细行返回，含目标单下浮率
    let adj1_sql = format!(
        "SELECT so.order_date as day,
                COALESCE(so.warehouse_id, 0) as warehouse_id,
                COALESCE(so.warehouse_name, '未指定') as warehouse_name,
                osi.amount as out_amount,
                COALESCE(so.discount_rate, 0) as discount_rate
         FROM order_supplement_item osi
         JOIN sales_order so ON osi.target_order_id = so.id
         {}",
        sales_where
    );
    let adj1_rows = sqlx::query(AssertSqlSafe(adj1_sql.as_str()))
        .fetch_all(pool())
        .await
        .unwrap_or_default();

    // 分摊调整2：来源耗材单真实金额（−出库），按明细行返回，含来源单下浮率
    let adj2_sql = format!(
        "SELECT so.order_date as day,
                COALESCE(so.warehouse_id, 0) as warehouse_id,
                COALESCE(so.warehouse_name, '未指定') as warehouse_name,
                soi.amount as out_amount,
                so.id as order_id,
                COALESCE(so.discount_rate, 0) as discount_rate
         FROM sales_order so
         JOIN sales_order_item soi ON soi.order_id = so.id
         {} AND so.id IN (SELECT DISTINCT source_order_id FROM consumable_allocation)",
        sales_where
    );
    let adj2_rows = sqlx::query(AssertSqlSafe(adj2_sql.as_str()))
        .fetch_all(pool())
        .await
        .unwrap_or_default();

    use std::collections::{BTreeMap, HashMap};
    // 元组: (in_amount, in_items, in_orders, out_amount, out_items, out_orders, warehouse_name, discounted_out)
    let mut day_wh_map: HashMap<String, BTreeMap<i64, (f64, i64, i64, f64, i64, i64, String, f64)>> = HashMap::new();

    for row in &purchase_rows {
        let day: String = row.get("day");
        let wh_id: i64 = row.get::<i64, _>("warehouse_id");
        let wh_name: String = row.get::<String, _>("warehouse_name");
        let amount: f64 = row.get::<f64, _>("in_amount");
        let items: i64 = row.get::<i64, _>("in_item_count");
        let orders: i64 = row.get::<i64, _>("in_order_count");
        let wh_map = day_wh_map.entry(day).or_insert_with(BTreeMap::new);
        let e = wh_map.entry(wh_id).or_insert((0.0, 0, 0, 0.0, 0, 0, wh_name, 0.0));
        e.0 += amount; e.1 += items; e.2 += orders;
    }

    // 真实出库：累加原值与下浮后金额，订单数去重
    let mut seen_orders: std::collections::HashSet<(String, i64, i64)> = std::collections::HashSet::new();
    for row in &sales_rows {
        let day: String = row.get("day");
        let wh_id: i64 = row.get::<i64, _>("warehouse_id");
        let wh_name: String = row.get::<String, _>("warehouse_name");
        let amount: f64 = row.get::<f64, _>("out_amount");
        let order_id: i64 = row.get::<i64, _>("order_id");
        let discount_rate: f64 = row.get::<f64, _>("discount_rate");
        let discounted = amount * (1.0 - discount_rate / 100.0);
        let wh_map = day_wh_map.entry(day.clone()).or_insert_with(BTreeMap::new);
        let e = wh_map.entry(wh_id).or_insert((0.0, 0, 0, 0.0, 0, 0, wh_name, 0.0));
        e.3 += amount; e.4 += 1; e.7 += discounted;
        if seen_orders.insert((day, wh_id, order_id)) { e.5 += 1; }
    }

    // 加目标单分摊增项（含下浮）
    for row in &adj1_rows {
        let day: String = row.get("day");
        let wh_id: i64 = row.get::<i64, _>("warehouse_id");
        let wh_name: String = row.get::<String, _>("warehouse_name");
        let amount: f64 = row.get::<f64, _>("out_amount");
        let discount_rate: f64 = row.get::<f64, _>("discount_rate");
        let discounted = amount * (1.0 - discount_rate / 100.0);
        let wh_map = day_wh_map.entry(day).or_insert_with(BTreeMap::new);
        let e = wh_map.entry(wh_id).or_insert((0.0, 0, 0, 0.0, 0, 0, wh_name, 0.0));
        e.3 += amount; e.7 += discounted;
    }

    // 减来源单真实金额（含下浮）
    for row in &adj2_rows {
        let day: String = row.get("day");
        let wh_id: i64 = row.get::<i64, _>("warehouse_id");
        let wh_name: String = row.get::<String, _>("warehouse_name");
        let amount: f64 = row.get::<f64, _>("out_amount");
        let discount_rate: f64 = row.get::<f64, _>("discount_rate");
        let discounted = amount * (1.0 - discount_rate / 100.0);
        let wh_map = day_wh_map.entry(day).or_insert_with(BTreeMap::new);
        let e = wh_map.entry(wh_id).or_insert((0.0, 0, 0, 0.0, 0, 0, wh_name, 0.0));
        e.3 -= amount; e.7 -= discounted;
    }

    let mut days: Vec<String> = day_wh_map.keys().cloned().collect();
    days.sort_by(|a, b| b.cmp(a));

    let mut rows: Vec<StockSummaryRow> = Vec::new();
    let mut total_in = 0.0;
    let mut total_out = 0.0;
    let mut total_discounted_out = 0.0;

    for day in &days {
        if let Some(wh_map) = day_wh_map.get(day) {
            let mut day_in = 0.0;
            let mut day_out = 0.0;
            let mut day_discounted_out = 0.0;
            let mut day_in_items = 0;
            let mut day_out_items = 0;
            let mut day_in_orders = 0;
            let mut day_out_orders = 0;

            for (wh_id, v) in wh_map {
                day_in += v.0; day_out += v.3; day_discounted_out += v.7;
                day_in_items += v.1; day_out_items += v.4;
                day_in_orders += v.2; day_out_orders += v.5;

                rows.push(StockSummaryRow {
                    day: day.clone(),
                    warehouse_id: *wh_id,
                    warehouse_name: v.6.clone(),
                    is_summary: false,
                    in_amount: v.0,
                    in_item_count: v.1,
                    in_order_count: v.2,
                    out_amount: v.3,
                    out_item_count: v.4,
                    out_order_count: v.5,
                    discounted_out_amount: v.7,
                    gross_profit: v.7 - v.0,
                });
            }

            rows.push(StockSummaryRow {
                day: day.clone(), warehouse_id: -1, warehouse_name: "当日汇总".to_string(),
                is_summary: true,
                in_amount: day_in, in_item_count: day_in_items, in_order_count: day_in_orders,
                out_amount: day_out, out_item_count: day_out_items, out_order_count: day_out_orders,
                discounted_out_amount: day_discounted_out,
                gross_profit: day_discounted_out - day_in,
            });
            total_in += day_in; total_out += day_out; total_discounted_out += day_discounted_out;
        }
    }
    (rows, total_in, total_out, total_discounted_out)
}









// ===== 导出函数 =====





















pub(crate) fn get_category_sort_key(category_name: &str, parent_name: &str) -> i64 {
    let name = category_name.trim();
    let parent = parent_name.trim();
    if parent == "荤鲜类" || name == "荤鲜类" {
        if name == "家禽" { return 101; }
        if name == "家畜" { return 102; }
        if name == "水产" { return 103; }
        return 100;
    }
    if name == "鲜蔬类" { return 200; }
    if name == "粮油干调" { return 300; }
    if name == "豆制品" { return 400; }
    if name == "粉面制品" { return 500; }
    if name == "水果类" { return 600; }
    if name == "其它" { return 700; }
    if name == "耗材类" { return 800; }
    999
}















// 导出报销单（报销口径）：合并分摊增项后的明细

// 导出验收单（真实口径）：真实账套明细，不合并分摊增项

/// 校验用户是否有权查看/导出指定销售单（行级数据权限）
pub(crate) async fn check_sales_order_access(
    headers: &axum::http::HeaderMap,
    id: i64,
) -> Result<(), (StatusCode, String)> {
    let ctx = get_user_ctx(headers).await;
    let order_purchaser_id: i64 = sqlx::query_scalar("SELECT purchaser_id FROM sales_order WHERE id = ?")
        .bind(id)
        .fetch_one(pool())
        .await
        .unwrap_or(-1);
    if !can_access_sales_order(&ctx, order_purchaser_id) {
        return Err((StatusCode::FORBIDDEN, "您没有权限查看此订单".to_string()));
    }
    Ok(())
}

// reimburse=true 报销口径（合并分摊增项）；false 真实口径（真实账套）
pub(crate) async fn build_accept_excel(headers: &axum::http::HeaderMap, id: i64, reimburse: bool, force: bool) -> impl IntoResponse {
    // 食材供应人员① 的联系方式：取当前登录用户最近一次保存的"联系方式"（user_account.contact_phone）。
    // 用户未保存过联系方式时为空白，xlsx 中该 cell 保持空。
    let ctx = crate::auth::get_user_ctx(headers).await;
    let contact_phone: String = sqlx::query("SELECT COALESCE(contact_phone, '') as cp FROM user_account WHERE id = ?")
        .bind(ctx.user_id)
        .fetch_optional(pool())
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get("cp").ok())
        .unwrap_or_default();

    let order_row = sqlx::query(
        "SELECT so.id, so.purchaser_id, so.order_no, so.order_date, so.total_amount, so.discount_rate, so.final_amount, so.remark,
                so.supplier_company, so.truck_plate,
                p.name as purchaser_name, p.address as purchaser_address
         FROM sales_order so JOIN purchaser p ON so.purchaser_id = p.id WHERE so.id = ?"
    )
    .bind(id)
    .fetch_optional(pool())
    .await
    .unwrap_or(None);

    if order_row.is_none() {
        return (StatusCode::NOT_FOUND, "订单不存在").into_response();
    }

    let row = order_row.unwrap();
    let order_no = row.get::<String, _>("order_no");
    let order_date = row.get::<String, _>("order_date");
    let _total_amount = row.get::<f64, _>("total_amount");
    let discount_rate = row.get::<f64, _>("discount_rate");
    let _final_amount = row.get::<f64, _>("final_amount");
    let purchaser_name = row.get::<String, _>("purchaser_name");

    // 供应商名称与供货车牌号：取自主表字段（前端在新建时默认填充），
    // 主表为空时回退到与前端默认填充完全一致的文本，避免历史数据导出为空白
    // 或与前端显示不一致。
    let supplier_name_raw: Option<String> = row.try_get("supplier_company").ok().flatten();
    let car_no_raw: Option<String> = row.try_get("truck_plate").ok().flatten();
    let supplier_name = supplier_name_raw
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "湖南食全味美餐饮管理有限公司".to_string());
    let car_no = car_no_raw
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "湘A·BE9312".to_string());

    let item_rows = sqlx::query(
        "SELECT soi.id, soi.product_id, soi.product_name, soi.alias1, soi.alias2, soi.spec, p.spec as product_spec, soi.unit, p.unit as product_unit, p.base_unit as product_base_unit, soi.unit_price, soi.quantity, soi.amount, soi.remark,
                p.category_id, pc.name as category_name, pc.parent_id, pc2.name as parent_name
         FROM sales_order_item soi LEFT JOIN product p ON soi.product_id = p.id
         LEFT JOIN category pc ON p.category_id = pc.id
         LEFT JOIN category pc2 ON pc.parent_id = pc2.id
         WHERE soi.order_id = ?"
    )
    .bind(id)
    .fetch_all(pool())
    .await
    .unwrap_or_default();

    // 检查是否有金额为零的明细，存在则禁止导出
    if !force {
        let zero_amount_items: Vec<_> = item_rows.iter()
            .filter(|r| {
                let amount: f64 = r.get("amount");
                amount.abs() < 0.001
            })
            .collect();
        if !zero_amount_items.is_empty() {
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
                serde_json::to_string(&serde_json::json!({
                    "error": true,
                    "count": zero_amount_items.len(),
                    "message": format!("订单中有 {} 条明细金额为零，不允许导出，请先调整后再试", zero_amount_items.len())
                })).unwrap(),
            ).into_response();
        }
    }

    // 真实口径不合并分摊增项，仅报销口径需要
    let supplement_rows = if reimburse {
        sqlx::query(
            "SELECT soi.id, soi.target_order_id, soi.source_order_id, soi.source_remark, soi.product_id, soi.product_name, soi.alias1, soi.alias2, soi.spec, p.spec as product_spec, soi.unit, p.unit as product_unit, p.base_unit as product_base_unit, soi.unit_price, soi.quantity, soi.amount, soi.allocate_date, soi.operation_type, soi.target_order_item_id,
                    pc.name as category_name, pc2.name as parent_name
             FROM order_supplement_item soi
             LEFT JOIN product p ON soi.product_id = p.id
             LEFT JOIN category pc ON p.category_id = pc.id
             LEFT JOIN category pc2 ON pc.parent_id = pc2.id
             WHERE soi.target_order_id = ?"
        )
        .bind(id)
        .fetch_all(pool())
        .await
        .unwrap_or_default()
    } else {
        Vec::new()
    };

    use std::collections::HashMap;
    let mut item_map: HashMap<i64, (i64, String, String, f64, f64, f64, String)> = HashMap::new();
    // product_id -> 真实明细行 id，用于分摊增项 target_order_item_id 失效时回退匹配
    let mut product_to_key: HashMap<i64, i64> = HashMap::new();
    for r in &item_rows {
        let rid = r.get::<i64, _>("id");
        let pid = r.get::<i64, _>("product_id");
        product_to_key.entry(pid).or_insert(rid);
        let alias2 = r.get::<Option<String>, _>("alias2").unwrap_or_default();
        let product_name = r.get::<String, _>("product_name");
        let food_name = if alias2.is_empty() {
            product_name
        } else {
            alias2
        };
        let unit = r.get::<Option<String>, _>("unit").unwrap_or_default();
        // 打印模板的"规格"列实际为真实订单的"单位"列（件/卷等）
        // 订单明细的 unit 为空时，回退到商品表的基础单位 base_unit
        let unit_for_spec = if !unit.is_empty() {
            unit
        } else {
            r.get::<Option<String>, _>("product_base_unit").unwrap_or_default()
        };
        let mut spec = r.get::<Option<String>, _>("spec").unwrap_or_default();
        if spec.is_empty() {
            spec = r.get::<Option<String>, _>("product_spec").unwrap_or_default();
        }
        let original_remark = r.get::<Option<String>, _>("remark").unwrap_or_default();
        let remark = if spec.is_empty() { original_remark } else if original_remark.is_empty() { spec.clone() } else { format!("{}; {}", spec, original_remark) };
        let category_name = r.get::<Option<String>, _>("category_name").unwrap_or_default();
        let parent_name = r.get::<Option<String>, _>("parent_name").unwrap_or_default();
        let sort_key = get_category_sort_key(&category_name, &parent_name);
        item_map.insert(rid, (sort_key, food_name, unit_for_spec, r.get::<f64, _>("unit_price"), r.get::<f64, _>("quantity"), r.get::<f64, _>("amount"), remark));
    }

    for r in &supplement_rows {
        let op_type = r.get::<String, _>("operation_type");
        let target_item_id = r.get::<Option<i64>, _>("target_order_item_id");
        let supp_product_id = r.get::<i64, _>("product_id");
        let qty = r.get::<f64, _>("quantity");
        let amt = r.get::<f64, _>("amount");

        // 解析目标明细行 key：优先用 target_order_item_id，失效时用 product_id 回退匹配
        // （销售订单更新会 DELETE+INSERT 导致 sales_order_item.id 变化，旧 target_order_item_id 会失效）
        let resolved_key: Option<i64> = match target_item_id {
            Some(tid) if item_map.contains_key(&tid) => Some(tid),
            _ => product_to_key.get(&supp_product_id).copied(),
        };

        // 替换-冲减：不导出（原被替换商品也需从导出中扣除）
        if op_type == "replace_remove" {
            if let Some(tid) = resolved_key {
                if let Some(entry) = item_map.get_mut(&tid) {
                    let new_qty = entry.4 + qty;
                    let new_amt = entry.5 + amt;
                    // 若原明细被完全冲减（数量或金额归零），从导出中移除
                    if new_qty.abs() < 0.001 || new_amt.abs() < 0.001 {
                        item_map.remove(&tid);
                    } else {
                        *entry = (entry.0, entry.1.clone(), entry.2.clone(), entry.3, new_qty, new_amt, entry.6.clone());
                    }
                }
            }
            continue;
        }

        if op_type == "increase_quantity" {
            if let Some(tid) = resolved_key {
                if let Some(entry) = item_map.get_mut(&tid) {
                    let new_qty = entry.4 + qty;
                    let new_amt = entry.5 + amt;
                    let new_remark = format!("{}（含增项+{}）", entry.6, qty);
                    *entry = (entry.0, entry.1.clone(), entry.2.clone(), entry.3, new_qty, new_amt, new_remark);
                }
            }
        } else {
            // new_item 或 replace_add：作为新明细导出，按商品类别归类排序
            let alias2 = r.get::<Option<String>, _>("alias2").unwrap_or_default();
            let product_name = r.get::<String, _>("product_name");
            let food_name = if alias2.is_empty() { product_name } else { alias2 };
            // 打印模板的"规格"列实际为真实订单的"单位"列（件/卷等）
            // 优先取分摊增项的 unit，为空时回退到商品表的基础单位 base_unit
            let unit = {
                let u = r.get::<String, _>("unit");
                if !u.is_empty() {
                    u
                } else {
                    r.get::<Option<String>, _>("product_base_unit").unwrap_or_default()
                }
            };
            let mut spec = r.get::<Option<String>, _>("spec").unwrap_or_default();
            if spec.is_empty() {
                spec = r.get::<Option<String>, _>("product_spec").unwrap_or_default();
            }
            let category_name = r.get::<Option<String>, _>("category_name").unwrap_or_default();
            let parent_name = r.get::<Option<String>, _>("parent_name").unwrap_or_default();
            let sort_key = get_category_sort_key(&category_name, &parent_name);
            let remark = if op_type == "replace_add" {
                // 替换换入：按类别正常导出，不额外标记
                spec.clone()
            } else {
                // 普通新增增项：保留标记
                let source_remark = r.get::<Option<String>, _>("source_remark").unwrap_or_default();
                if spec.is_empty() { format!("[增项] {}", source_remark) } else { format!("{}; [增项] {}", spec, source_remark) }
            };
            // 规格列（C列）填单位（打印模板的"规格"列实际是单位列）
            item_map.insert(-r.get::<i64, _>("id"), (sort_key, food_name, unit, r.get::<f64, _>("unit_price"), qty, amt, remark));
        }
    }

    let mut items: Vec<(i64, String, String, f64, f64, f64, String)> = item_map.into_values().collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));

    let accept_total_amount: f64 = items.iter().map(|(_, _, _, _, _, amount, _)| amount).sum();
    let accept_final_amount = accept_total_amount * (1.0 - discount_rate / 100.0);

    let result: Result<Vec<u8>, XlsxError> = (|| {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();

        worksheet.set_landscape();
        worksheet.set_margins(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        worksheet.set_print_center_vertically(false);
        worksheet.set_print_center_horizontally(true);

        let title_format = Format::new()
            .set_bold()
            .set_font_size(16)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter);

        let header_format = Format::new()
            .set_bold()
            .set_font_size(10)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin)
            .set_text_wrap();

        let cell_format = Format::new()
            .set_font_size(10)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin);

        let cell_left_format = Format::new()
            .set_font_size(10)
            .set_align(FormatAlign::Left)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin)
            // .set_text_wrap();// 自动换行
            .set_shrink();// 自动缩放

        let cell_right_format = Format::new()
            .set_font_size(10)
            .set_align(FormatAlign::Right)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin);

        let label_format = Format::new()
            .set_font_size(10)
            .set_align(FormatAlign::Right)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin);

        

        

        let money_format = Format::new()
            .set_font_size(10)
            .set_align(FormatAlign::Right)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin)
            .set_num_format("¥#,##0.00");

        let percent_format = Format::new()
            .set_font_size(10)
            .set_align(FormatAlign::Right)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin)
            .set_num_format("0\"%\"");

        let info_format = Format::new()
            .set_font_size(10)
            .set_align(FormatAlign::Left)
            .set_align(FormatAlign::VerticalCenter);

        let col_widths = [4.0, 20.0, 4.0, 6.0, 8.0, 10.0, 10.0, 6.0, 10.0, 10.0, 10.0, 15.0, 10.0];
        for (i, w) in col_widths.iter().enumerate() {
            worksheet.set_column_width(i as u16, *w)?;
        }

        let headers = [
            "序号".to_string(), "品名规格".to_string(), "单位".to_string(), "数量".to_string(), "单价".to_string(), "总价".to_string(),
            "生产日期\n/批号".to_string(), "保质期".to_string(), "是否有蔬\n菜农残检\n测报告单".to_string(),
            "是否有肉\n类检疫合\n格证".to_string(), "是否异常\n(异味异色)".to_string(),
            "检验情况\n是否合格".to_string(), "备注".to_string(),
        ];

        let items_per_page = 20;
        let total_pages = ((items.len() + items_per_page - 1) / items_per_page) as i32;
        let mut current_row: u32 = 0;

        for page in 0..total_pages {
            let page_title_row = current_row;
            let info_row = current_row + 2;
            let header_row = current_row + 3;
            let first_data_row = current_row + 4;

            worksheet.merge_range(page_title_row, 0, page_title_row, 12, "颍上县公安局机关食堂食材验收单", &title_format)?;
            worksheet.set_row_height(page_title_row, 30)?;

            worksheet.write_with_format(info_row, 0, format!("供应商名称：{}", supplier_name), &info_format)?;
            worksheet.write_with_format(info_row, 6, format!("供货车牌号：{}", car_no), &info_format)?;
            worksheet.write_with_format(info_row, 10, format!("供货时间：{}", order_date), &info_format)?;

            for (i, h) in headers.iter().enumerate() {
                worksheet.write_with_format(header_row, i as u16, h, &header_format)?;
            }
            worksheet.set_row_height(header_row, 35)?;

            let start_idx = page as usize * items_per_page;
            let end_idx = std::cmp::min(start_idx + items_per_page, items.len());
            current_row = first_data_row;

            for (item_idx, (_sort_key, food_name, spec, unit_price, quantity, amount, remark)) in items[start_idx..end_idx].iter().enumerate() {
                let seq_num = (start_idx + item_idx + 1) as f64;
                worksheet.write_with_format(current_row, 0, seq_num, &cell_format)?;
                worksheet.write_with_format(current_row, 1, food_name, &cell_left_format)?;
                worksheet.write_with_format(current_row, 2, spec, &cell_format)?;
                worksheet.write_with_format(current_row, 3, *quantity, &cell_right_format)?;
                worksheet.write_with_format(current_row, 4, *unit_price, &money_format)?;
                worksheet.write_with_format(current_row, 5, *amount, &money_format)?;
                worksheet.write_with_format(current_row, 6, "", &cell_format)?;
                worksheet.write_with_format(current_row, 7, "", &cell_format)?;
                worksheet.write_with_format(current_row, 8, "□有 □无", &cell_format)?;
                worksheet.write_with_format(current_row, 9, "□有 □无", &cell_format)?;
                worksheet.write_with_format(current_row, 10, "□有 □无", &cell_format)?;
                worksheet.write_with_format(current_row, 11, "□合格 □不合格", &cell_format)?;
                worksheet.write_with_format(current_row, 12, remark, &cell_left_format)?;

                current_row += 1;
            }

            let blank_rows = (first_data_row + items_per_page as u32 - current_row) as i32;
            for _ in 0..blank_rows {
                for col in 0..13u16 {
                    worksheet.write_with_format(current_row, col, "", &cell_format)?;
                }
                current_row += 1;
            }

            worksheet.merge_range(current_row, 0, current_row, 2, "合计总价：", &label_format)?;
            worksheet.merge_range(current_row, 3, current_row, 5, "", &cell_format)?;
            let purchaser_start_row = current_row;
            let purchaser_label_format = Format::new()
                .set_font_size(10)
                .set_align(FormatAlign::Right)
                .set_align(FormatAlign::VerticalCenter)
                .set_border(FormatBorder::Thin);
            let purchaser_name_format = Format::new()
                .set_font_size(10)
                .set_bold()
                .set_align(FormatAlign::Center)
                .set_align(FormatAlign::VerticalCenter)
                .set_border(FormatBorder::Thin);

            worksheet.write_with_format(current_row, 3, accept_total_amount, &money_format)?;
            current_row += 1;

            worksheet.merge_range(current_row, 0, current_row, 2, "下浮率：", &label_format)?;
            worksheet.merge_range(current_row, 3, current_row, 5, "", &cell_format)?;
            worksheet.write_with_format(current_row, 3, discount_rate, &percent_format)?;
            current_row += 1;

            worksheet.merge_range(current_row, 0, current_row, 2, "下浮后总价：", &label_format)?;
            worksheet.merge_range(current_row, 3, current_row, 5, "", &cell_format)?;
            worksheet.write_with_format(current_row, 3, accept_final_amount, &money_format)?;
            current_row += 1;

            worksheet.merge_range(purchaser_start_row, 6, purchaser_start_row + 2, 8, "收货单位：", &purchaser_label_format)?;
            worksheet.merge_range(purchaser_start_row, 9, purchaser_start_row + 2, 12, &purchaser_name, &purchaser_name_format)?;

            let forbid_items = vec![
                "禁止采购以下食材：",
                "1、有毒、有害、腐败变质、酸败、霉变、生虫、污秽不洁、混有异物或者其他感官性状异常的食品；",
                "2、无检验检疫合格证明的肉类食品，已过保质期二分之一时间及其他不符合食品标签规定的定型包装食品；",
                "3、无卫生许可证的食品生产经营者供应的食品；",
                "4、禁止采购供应河豚、毛蚶、小海螺等高风险水产品及三文鱼、醉虾、醉蟹等生食水产品；",
                "5、禁止采购散装馅料、肉串及散热熟食制品、卤制品、腌肉、发芽土豆等食品，严禁采购加工制作的豆角（四季豆等）；",
                "6、建议时令蔬菜、瓜果和价格中低档的肉类食品，严禁采购高档食材和反季节蔬菜、瓜果。",
            ];
            for (idx, item) in forbid_items.iter().enumerate() {
                let mut format = Format::new()
                    .set_font_size(8)
                    .set_align(FormatAlign::Left)
                    .set_align(FormatAlign::VerticalCenter);
                if idx == 0 {
                    format = format.set_border_top(FormatBorder::Thin).set_border_left(FormatBorder::Thin).set_border_right(FormatBorder::Thin);
                } else if idx == forbid_items.len() - 1 {
                    format = format.set_border_left(FormatBorder::Thin).set_border_right(FormatBorder::Thin).set_border_bottom(FormatBorder::Thin);
                } else {
                    format = format.set_border_left(FormatBorder::Thin).set_border_right(FormatBorder::Thin);
                }
                worksheet.set_row_height(current_row, 10)?;
                worksheet.merge_range(current_row, 0, current_row, 12, item, &format)?;
                current_row += 1;
            }

            for sig_row in 0..3 {
                let is_first = sig_row == 0;
                let is_last = sig_row == 2;
                
                let mut label_fmt = Format::new()
                    .set_font_size(10)
                    .set_align(FormatAlign::Left)
                    .set_align(FormatAlign::VerticalCenter);
                let mut contact_fmt = Format::new()
                    .set_font_size(10)
                    .set_align(FormatAlign::Right)
                    .set_align(FormatAlign::VerticalCenter);
                let mut supplier_fmt = Format::new()
                    .set_font_size(10)
                    .set_align(FormatAlign::Right)
                    .set_align(FormatAlign::VerticalCenter);
                let mut cell_fmt = Format::new()
                    .set_font_size(10)
                    .set_align(FormatAlign::Left)
                    .set_align(FormatAlign::VerticalCenter);
                let mut last_cell_fmt = Format::new()
                    .set_font_size(10)
                    .set_align(FormatAlign::Left)
                    .set_align(FormatAlign::VerticalCenter)
                    .set_border_right(FormatBorder::Thin);

                if is_first {
                    label_fmt = label_fmt.set_border_top(FormatBorder::Thin).set_border_left(FormatBorder::Thin);
                    contact_fmt = contact_fmt.set_border_top(FormatBorder::Thin);
                    supplier_fmt = supplier_fmt.set_border_top(FormatBorder::Thin);
                    cell_fmt = cell_fmt.set_border_top(FormatBorder::Thin);
                    last_cell_fmt = last_cell_fmt.set_border_top(FormatBorder::Thin);
                } else {
                    label_fmt = label_fmt.set_border_left(FormatBorder::Thin);
                }
                if is_last {
                    label_fmt = label_fmt.set_border_bottom(FormatBorder::Thin);
                    contact_fmt = contact_fmt.set_border_bottom(FormatBorder::Thin);
                    supplier_fmt = supplier_fmt.set_border_bottom(FormatBorder::Thin);
                    cell_fmt = cell_fmt.set_border_bottom(FormatBorder::Thin);
                    last_cell_fmt = last_cell_fmt.set_border_bottom(FormatBorder::Thin);
                }

                let row = current_row;
                worksheet.set_row_height(row, 25)?;
                if sig_row == 0 {
                    worksheet.merge_range(row, 0, row, 1, "食材供应人员①：", &label_fmt)?;
                    worksheet.merge_range(row, 2, row, 3, "联系方式：", &contact_fmt)?;
                    // 食材供应人员①的联系方式：填入当前登录用户最近一次保存的联系方式
                    worksheet.merge_range(row, 4, row, 5, &contact_phone, &cell_fmt)?;
                    worksheet.merge_range(row, 6, row, 7, "公安验收人员①：", &supplier_fmt)?;
                    worksheet.merge_range(row, 8, row, 9, "联系方式：", &contact_fmt)?;
                    worksheet.merge_range(row, 10, row, 12, "", &last_cell_fmt)?;
                } else if sig_row == 1 {
                    worksheet.merge_range(row, 0, row, 1, "食材供应人员②：", &label_fmt)?;
                    worksheet.merge_range(row, 2, row, 3, "联系方式：", &contact_fmt)?;
                    worksheet.merge_range(row, 4, row, 5, "", &cell_fmt)?;
                    worksheet.merge_range(row, 6, row, 7, "公安验收人员②：", &supplier_fmt)?;
                    worksheet.merge_range(row, 8, row, 9, "联系方式：", &contact_fmt)?;
                    worksheet.merge_range(row, 10, row, 12, "", &last_cell_fmt)?;
                } else {
                    worksheet.merge_range(row, 0, row, 1, "食材供应人员③：", &label_fmt)?;
                    worksheet.merge_range(row, 2, row, 3, "联系方式：", &contact_fmt)?;
                    worksheet.merge_range(row, 4, row, 5, "", &cell_fmt)?;
                    worksheet.merge_range(row, 6, row, 7, "厨师①：", &supplier_fmt)?;
                    worksheet.merge_range(row, 8, row, 9, "联系方式：", &contact_fmt)?;
                    worksheet.merge_range(row, 10, row, 12, "", &last_cell_fmt)?;
                }
                current_row += 1;
            }

            let page_info = format!("第{}页，共{}页", page + 1, total_pages);
            let footer_format = Format::new()
                .set_font_size(8)
                .set_align(FormatAlign::Center)
                .set_align(FormatAlign::Bottom);
            worksheet.merge_range(current_row, 0, current_row, 12, &page_info, &footer_format)?;
            current_row += 1;

            if page < total_pages - 1 {
                let _ = worksheet.set_page_breaks(&[current_row]);
            }
        }

        let buf = workbook.save_to_buffer()?;
        Ok(buf)
    })();

    match result {
        Ok(buf) => {
            let filename = if reimburse {
                format!("报销单_{}.xlsx", order_no)
            } else {
                format!("验收单_{}.xlsx", order_no)
            };
            xlsx_response(buf, &filename)
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, format!("生成Excel失败: {}", e)).into_response()
        }
    }
}












/// 销售单审核：pending → confirmed，锁定订单禁止修改（需填写备注原因）

/// 销售单反审核：confirmed → pending，解除锁定以便修改（仅管理员，强制原因）




fn build_router() -> Router {
    Router::new()
        .route("/static/bootstrap.min.css", get(serve_bootstrap_css))
        .route("/static/bootstrap.bundle.min.js", get(serve_bootstrap_js))
        .route("/static/chart.umd.min.js", get(serve_chart_js))
        .route("/", get(page_index))
        .route("/supplier", get(page_supplier))
        .route("/purchaser", get(page_purchaser))
        .route("/product", get(page_product))
        .route("/warehouse", get(page_warehouse))
        .route("/inventory", get(page_inventory))
        .route("/purchase", get(page_purchase))
        .route("/sales", get(page_sales))
        .route("/supplement", get(page_supplement))
        .route("/query/purchase_order", get(page_query_purchase_order))
        .route("/query/purchase_document", get(page_query_purchase_document))
        .route("/query/purchase_price", get(page_query_purchase_price))
        .route("/query/purchase_summary", get(page_query_purchase_summary))
        .route("/query/supplier_balance", get(page_query_supplier_balance))
        .route("/query/sales_order", get(page_query_sales_order))
        .route("/query/sales_summary", get(page_query_sales_summary))
        .route("/query/sales_price", get(page_query_sales_price))
        .route("/query/purchaser_balance", get(page_query_purchaser_balance))
        .route("/query/product_rank", get(page_query_product_rank))
        .route("/query/reimburse_summary", get(page_query_reimburse_summary))
        .route("/query/allocation_source", get(page_query_allocation_source))
        .route("/query/order_adjust", get(page_order_adjust))
        .route("/query/stock_balance", get(page_query_stock_balance))
        .route("/query/stock_flow", get(page_query_stock_flow))
        .route("/query/stock_summary", get(page_query_stock_summary))
        .route("/query/stock_summary_reimburse", get(page_query_stock_summary_reimburse))
        .route("/query/stock_warning", get(page_query_stock_warning))
        .route("/query/slow_stock", get(page_query_slow_stock))
        .route("/query/income_expense", get(page_query_income_expense))
        .route("/query/profit_detail", get(page_query_profit_detail))
        .route("/query/finance_settlement", get(page_query_finance_settlement))
        .route("/query/overview", get(page_query_overview))
        .route("/query/category_stats", get(page_query_category_stats))
        .route("/query/document_summary", get(page_query_document_summary))
        .route("/user", get(page_user))
        .route("/system", get(page_system))
        .route("/system/operation_log", get(page_operation_log))
        .route("/backup", get(page_backup))
        .route("/restore", get(page_restore))
        .route("/api/system/config", post(api_system_config))
        .route("/api/system/operation_log", get(api_operation_log_list))
        .route("/api/system/operation_log/export", get(api_operation_log_export))
        .route("/api/user/list", get(api_user_list))
        .route("/api/user/contact_phone", get(api_user_get_contact_phone))
        .route("/api/user/contact_phone", post(api_user_set_contact_phone))
        .route("/api/user/{id}", get(api_user_get))
        .route("/api/user", post(api_user_create))
        .route("/api/user/{id}", put(api_user_update))
        .route("/api/user/{id}", delete(api_user_delete))
        .route("/api/user/{id}/status", put(api_user_status))
        .route("/api/backup", post(api_backup))
        .route("/api/backup/download/{id}", get(api_backup_download))
        .route("/api/backup/delete/{id}", delete(api_backup_delete))
        .route("/api/restore/{id}", post(api_restore))
        .route("/api/restore/file", post(api_restore_file))
        .route("/api/clean_invalid_orders", post(api_clean_invalid_orders))
        .route("/api/inspect_corrupted_items", get(api_inspect_corrupted_items))
        .route("/api/clean_corrupted_items", post(api_clean_corrupted_items))
        .route("/api/supplier/list", get(api_supplier_list))
        .route("/api/supplier/create", post(api_supplier_create))
        .route("/api/supplier/update", post(api_supplier_update))
        .route("/api/supplier/delete", post(api_supplier_delete))
        .route("/api/supplier/approve", post(api_supplier_approve))
        .route("/api/supplier/unapprove", post(api_supplier_unapprove))
        .route("/api/supplier/export", get(api_supplier_export))
        .route("/api/supplier/import", post(api_supplier_import))
        .route("/api/purchaser/list", get(api_purchaser_list))
        .route("/api/purchaser/create", post(api_purchaser_create))
        .route("/api/purchaser/update", post(api_purchaser_update))
        .route("/api/purchaser/delete", post(api_purchaser_delete))
        .route("/api/purchaser/approve", post(api_purchaser_approve))
        .route("/api/purchaser/unapprove", post(api_purchaser_unapprove))
        .route("/api/purchaser/export", get(api_purchaser_export))
        .route("/api/purchaser/import", post(api_purchaser_import))
        .route("/api/product/list", get(api_product_list))
        .route("/api/product/check_name", get(api_product_check_name))
        .route("/api/product/search", get(api_product_search))
        .route("/api/product/by_id", get(api_product_by_id))
        .route("/api/product/create", post(api_product_create))
        .route("/api/product/update", post(api_product_update))
        .route("/api/product/delete", post(api_product_delete))
        .route("/api/product/approve", post(api_product_approve))
        .route("/api/product/unapprove", post(api_product_unapprove))
        .route("/api/product/toggle_status/{id}", post(api_product_toggle_status))
        .route("/api/product/export", get(api_product_export))
        .route("/api/product/import", post(api_product_import))
        .route("/api/product/upload_image", post(api_product_upload_image))
        .route("/api/product/delete_image", get(api_product_delete_image))
        .route("/api/product/image/{filename}", get(api_product_get_image))
        .route("/api/uploads/{folder}/{filename}", get(api_get_uploaded_image))
        .route("/api/product/unit/create", post(api_product_unit_create))
        .route("/api/product/unit/update", post(api_product_unit_update))
        .route("/api/product/unit/delete", post(api_product_unit_delete))
        .route("/api/product/unit/delete_by_product", post(api_product_unit_delete_by_product))
        .route("/api/product/unit/list", get(api_product_unit_list))
        .route("/api/product/price/upsert", post(api_product_price_upsert))
        .route("/api/product/price/list", get(api_product_price_list))
        .route("/api/product/price/delete", post(api_product_price_delete))
        .route("/api/product/price/delete_by_product", post(api_product_price_delete_by_product))
        .route("/api/product/sync_base_price", post(api_product_sync_base_price))
        .route("/api/product/price_log/list", get(api_product_price_log_list))
        .route("/api/product/last_purchase_price", get(api_product_last_purchase_price))
        .route("/api/product/set_auto_update_price", post(api_product_set_auto_update_price))
        .route("/api/product/today_price_items", get(api_product_today_price_items))
        .route("/api/product/today_price_save", post(api_product_today_price_save))
        .route("/api/product/today_price_excel", get(api_product_today_price_excel))
        .route("/api/product/today_price_a4", get(api_product_today_price_a4))
        .route("/api/product/today_price_excel_by_category", get(api_product_today_price_excel_by_category))
        .route("/api/product/batch_set_auto_update_price", post(api_product_batch_set_auto_update_price))
        .route("/api/category/list", get(api_category_list))
        .route("/api/category/tree", get(api_category_tree))
        .route("/api/category/create", post(api_category_create))
        .route("/api/category/delete", post(api_category_delete))
        .route("/api/category/rename", post(api_category_rename))
        .route("/api/inventory/list", get(api_inventory_list))
        .route("/api/warehouse/list", get(api_warehouse_list))
        .route("/api/warehouse/create", post(api_warehouse_create))
        .route("/api/warehouse/update", post(api_warehouse_update))
        .route("/api/warehouse/delete", post(api_warehouse_delete))
        .route("/api/warehouse/approve", post(api_warehouse_approve))
        .route("/api/warehouse/unapprove", post(api_warehouse_unapprove))
        .route("/api/purchase_order/create", post(api_purchase_order_create))
        .route("/api/purchase_order/list", get(api_purchase_order_list))
        .route("/api/purchase_order/detail/{id}", get(api_purchase_order_detail))
        .route("/api/purchase_order/update", post(api_purchase_order_update))
        .route("/api/purchase_order/approve/{id}", post(api_purchase_order_approve))
        .route("/api/purchase_order/settle/{id}", post(api_purchase_order_settle))
        .route("/api/purchase_order/unapprove/{id}", post(api_purchase_order_unapprove))
        .route("/api/purchase_order/delete/{id}", delete(api_purchase_order_delete))
        .route("/api/purchase_order/export", get(api_purchase_order_export))
        .route("/api/purchase_order/export_print/{id}", get(api_purchase_order_print_excel))
        .route("/api/purchase_order/import", post(api_purchase_order_import))
        .route("/api/sales_order/create", post(api_sales_order_create))
        .route("/api/sales_order/list", get(api_sales_order_list))
        .route("/api/sales_order/by_purchaser/{purchaser_id}", get(api_sales_order_by_purchaser))
        .route("/api/sales_order/detail/{id}", get(api_sales_order_detail))
        .route("/api/sales_order/update", post(api_sales_order_update))
        .route("/api/sales_order/approve/{id}", post(api_sales_order_approve))
        .route("/api/sales_order/settle/{id}", post(api_sales_order_settle))
        .route("/api/sales_order/unapprove/{id}", post(api_sales_order_unapprove))
        .route("/api/sales_order/update_prices/{id}", post(api_sales_order_update_prices))
        .route("/api/sales_order/upload_image", post(api_sales_order_upload_image))
        .route("/api/sales_order/delete_image", post(api_sales_order_delete_image))
        .route("/api/sales_order/delete/{id}", delete(api_sales_order_delete))
        .route("/api/sales_order/export", get(api_sales_order_export))
        .route("/api/sales_order/import", post(api_sales_order_import))
        .route("/api/sales_order/accept/{id}", get(api_sales_order_accept))
        .route("/api/sales_order/accept_excel/{id}", get(api_sales_order_accept_excel))
        .route("/api/sales_order/real_excel/{id}", get(api_sales_order_real_excel))
        .route("/api/supplement/create", post(api_supplement_create))
        .route("/api/supplement/list_by_target/{order_id}", get(api_supplement_list_by_target))
        .route("/api/supplement/adjusted_orders", get(api_adjusted_orders))
        .route("/api/supplement/list_by_source/{order_id}", get(api_supplement_list_by_source))
        .route("/api/supplement/delete/{id}", delete(api_supplement_delete))
        .route("/api/supplement/compare/{order_id}", get(api_supplement_compare))
        .route("/api/allocation/create", post(api_allocation_create))
        .route("/api/allocation/summary/{source_order_id}", get(api_allocation_summary))
        .route("/api/allocation/allocated_orders", get(api_allocation_allocated_orders))
        .route("/api/allocation/terminate", post(api_allocation_terminate))
        .route("/api/allocation/cancel", post(api_allocation_cancel))
        .route("/api/allocation/complete", post(api_allocation_complete))
        .route("/api/sales_order/sort_items", get(api_sales_order_sort_items))
        .route("/api/sales_order/sort_items_excel", get(api_sales_order_sort_items_excel))
        .route("/api/sales_order/sort_items_by_purchaser", get(api_sales_order_sort_items_by_purchaser))
        .route("/api/sales_order/sort_items_by_purchaser_excel", get(api_sales_order_sort_items_by_purchaser_excel))
        .route("/api/sales_order/sort_items_by_category", get(api_sales_order_sort_items_by_category))
        .route("/api/sales_order/sort_items_by_category_excel", get(api_sales_order_sort_items_by_category_excel))
        .route("/api/sales_order/sort_items_by_supplier", get(api_sales_order_sort_items_by_supplier))
        .route("/api/sales_order/sort_items_by_supplier_excel", get(api_sales_order_sort_items_by_supplier_excel))
        .route("/api/sales_order/update_status", post(api_sales_order_update_status))
        .route("/api/sales_order/correction", post(api_sales_order_correction))
        .route("/api/sales_order/generate_purchase/{id}", post(api_sales_order_generate_purchase))
        .route("/mobile/sort", get(page_mobile_sort))
        .route("/mobile/sort_by_purchaser", get(page_mobile_sort_by_purchaser))
        .route("/mobile/sort_by_category", get(page_mobile_sort_by_category))
        .route("/mobile/sort_by_supplier", get(page_mobile_sort_by_supplier))
        .route("/mobile/sort_comprehensive", get(page_mobile_sort_comprehensive))
        .route("/mobile/today_price", get(page_mobile_today_price))
        .route("/api/sales_order/sort_comprehensive", get(api_sales_order_sort_comprehensive))
        .route("/api/sales_order/sort_comprehensive_excel", get(api_sales_order_sort_comprehensive_excel))
        .route("/api/query/purchase_order", get(api_query_purchase_order))
        .route("/api/purchase_document/list", get(api_purchase_document_list))
        .route("/api/purchase_document/list/export", get(api_purchase_document_list_export))
        .route("/api/purchase_document/upload", post(api_purchase_document_upload))
        .route("/api/purchase_document/delete/{id}", delete(api_purchase_document_delete))
        .route("/api/query/purchase_order/export", get(api_query_purchase_order_export))
        .route("/api/query/purchase_price", get(api_query_purchase_price))
        .route("/api/query/purchase_price/export", get(api_query_purchase_price_export))
        .route("/api/query/purchase_summary", get(api_query_purchase_summary))
        .route("/api/query/purchase_summary/export", get(api_query_purchase_summary_export))
        .route("/api/query/supplier_balance", get(api_query_supplier_balance))
        .route("/api/query/supplier_balance/export", get(api_query_supplier_balance_export))
        .route("/api/query/sales_order", get(api_query_sales_order))
        .route("/api/query/sales_order/export", get(api_query_sales_order_export))
        .route("/api/query/sales_price", get(api_query_sales_price))
        .route("/api/query/sales_price/export", get(api_query_sales_price_export))
        .route("/api/query/product_price_trend", get(api_query_product_price_trend))
        .route("/api/query/sales_summary", get(api_query_sales_summary))
        .route("/api/query/sales_summary/export", get(api_query_sales_summary_export))
        .route("/api/query/purchaser_balance", get(api_query_purchaser_balance))
        .route("/api/query/purchaser_balance/export", get(api_query_purchaser_balance_export))
        .route("/api/query/product_rank", get(api_query_product_rank))
        .route("/api/query/product_rank/export", get(api_query_product_rank_export))
        .route("/api/query/reimburse_summary", get(api_query_reimburse_summary))
        .route("/api/query/reimburse_summary/export", get(api_query_reimburse_summary_export))
        .route("/api/query/allocation_source", get(api_query_allocation_source))
        .route("/api/query/allocation_source/export", get(api_query_allocation_source_export))
        .route("/api/query/stock_balance", get(api_query_stock_balance))
        .route("/api/query/stock_balance/export", get(api_query_stock_balance_export))
        .route("/api/query/stock_flow", get(api_query_stock_flow))
        .route("/api/query/stock_flow/export", get(api_query_stock_flow_export))
        .route("/api/query/stock_summary", get(api_query_stock_summary))
        .route("/api/query/stock_summary/export", get(api_query_stock_summary_export))
        .route("/api/query/stock_summary_reimburse", get(api_query_stock_summary_reimburse))
        .route("/api/query/stock_summary_reimburse/export", get(api_query_stock_summary_reimburse_export))
        .route("/api/query/stock_warning", get(api_query_stock_warning))
        .route("/api/query/stock_warning/export", get(api_query_stock_warning_export))
        .route("/api/query/slow_stock", get(api_query_slow_stock))
        .route("/api/query/slow_stock/export", get(api_query_slow_stock_export))
        .route("/api/query/income_expense", get(api_query_income_expense))
        .route("/api/query/income_expense/export", get(api_query_income_expense_export))
        .route("/api/query/profit_detail", get(api_query_profit_detail))
        .route("/api/query/profit_detail/export", get(api_query_profit_detail_export))
        .route("/api/query/finance_settlement", get(api_query_finance_settlement))
        .route("/api/query/overview", get(api_query_overview))
        .route("/api/query/overview/export", get(api_query_overview_export))
        .route("/api/query/category_stats", get(api_query_category_stats))
        .route("/api/query/category_stats/export", get(api_query_category_stats_export))
        .route("/api/query/document_summary", get(api_query_document_summary))
        .route("/api/query/document_summary/export", get(api_query_document_summary_export))
        .route("/api/order/generate_no", get(api_order_generate_no))
        .route("/api/accept/create", post(api_accept_create))
        .route("/api/accept/list", get(api_accept_list))
        .route("/login", get(page_login))
        .route("/api/login", post(api_login))
        .route("/api/login/check", get(api_login_check))
        .route("/api/logout", get(api_logout))
}



fn main() {
    std::thread::spawn(|| {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("创建 runtime 失败");
        rt.block_on(async {
            init_pool().await;
            let app = build_router();
            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 3000));
            let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
            axum::serve(listener, app).await.unwrap();
        });
    });

    let event_loop = EventLoop::new();

    let menu = Menu::new();
    let open_item = MenuItem::with_id("open", "打开页面", true, None);
    let quit_item = MenuItem::with_id("quit", "退出", true, None);
    let _ = menu.append(&open_item);
    let _ = menu.append(&quit_item);

    let icon = make_app_icon();
    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("食材收发系统")
        .with_icon(icon)
        .build()
        .expect("创建托盘图标失败");

    let menu_channel = MenuEvent::receiver();
    let tray_channel = TrayIconEvent::receiver();

    event_loop.run(move |_event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        if let Ok(menu_event) = menu_channel.try_recv() {
            match menu_event.id().as_ref() {
                "open" => open_browser(),
                "quit" => *control_flow = ControlFlow::Exit,
                _ => {}
            }
        }

        if let Ok(tray_event) = tray_channel.try_recv() {
            if let TrayIconEvent::Click { button_state, .. } = tray_event {
                if button_state == tray_icon::MouseButtonState::Up {
                    open_browser();
                }
            }
        }
    });
}
