use axum::http::{HeaderMap, StatusCode};
use axum::response::Html;
use sqlx::Row;
use std::collections::{HashMap, HashSet};

use crate::utils::layout_html;
use crate::db::pool;
use crate::models::UserCtx;

pub async fn get_user_role(headers: &HeaderMap) -> String {
    let session_token = headers.get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find(|s| s.trim().starts_with("session="))
                .map(|s| s.trim().strip_prefix("session=").unwrap_or(""))
        })
        .unwrap_or("");
    
    if session_token.is_empty() {
        return "anonymous".to_string();
    }
    
    let parts: Vec<&str> = session_token.split(':').collect();
    if parts.len() < 2 {
        return "anonymous".to_string();
    }
    
    let user_id = match parts[0].parse::<i64>() {
        Ok(id) => id,
        Err(_) => return "anonymous".to_string(),
    };
    
    let rows = sqlx::query(
        "SELECT role FROM user_account WHERE id = ? AND status = 1"
    )
    .bind(user_id)
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    if rows.is_empty() {
        return "anonymous".to_string();
    }
    
    rows[0].get::<String, _>("role")
}

pub fn has_permission(role: &str, required_role: &str) -> bool {
    let role_permissions = HashMap::from([
        ("super_admin", vec!["super_admin", "admin", "supplier", "purchaser", "query"]),
        ("admin", vec!["admin", "supplier", "purchaser", "query"]),
        ("supplier", vec!["supplier", "query"]),
        ("purchaser", vec!["purchaser", "query"]),
        ("user", vec!["query"]),
        ("anonymous", vec![]),
    ]);
    
    role_permissions.get(role)
        .map(|permissions| permissions.contains(&required_role))
        .unwrap_or(false)
}

// ===== 细粒度权限点系统（工程化 RBAC） =====
// 权限点：resource.action，如 purchase_order.update
// 角色 → 权限点映射，super_admin 拥有全部权限

/// 判断某角色是否拥有指定权限点
pub fn has_permission_point(role: &str, permission: &str) -> bool {
    // 全部业务权限点（供 super_admin 全量拥有）
    // supplier/purchaser 为基础资料管理权限点：供应商/采购方角色及以上可维护各自基础资料
    const ALL_PERMS: [&str; 22] = [
        "purchase_order.view", "purchase_order.create", "purchase_order.update", "purchase_order.approve", "purchase_order.unapprove", "purchase_order.cancel", "purchase_order.delete",
        "sales_order.view", "sales_order.create", "sales_order.update", "sales_order.approve", "sales_order.unapprove", "sales_order.adjust_price", "sales_order.cancel", "sales_order.delete",
        "query.view", "manage.admin", "manage.user", "manage.system", "manage.backup",
        "supplier", "purchaser",
    ];

    let role_perms: HashSet<&str> = match role {
        "super_admin" => HashSet::from(ALL_PERMS),
        "admin" => HashSet::from(ALL_PERMS),
        "supplier" => HashSet::from([
            "supplier",
            "purchase_order.view", "purchase_order.create", "purchase_order.update", "purchase_order.approve", "purchase_order.cancel",
            "sales_order.view", "query.view",
        ]),
        "purchaser" => HashSet::from([
            "purchaser",
            "purchase_order.view",
            "sales_order.view", "sales_order.create", "sales_order.update", "sales_order.approve", "sales_order.adjust_price", "sales_order.cancel",
            "query.view",
        ]),
        "user" => HashSet::from(["query.view"]),
        _ => HashSet::new(),
    };
    role_perms.contains(permission)
}

/// 解析用户上下文：cookie session -> (role, user_id, supplier_id, purchaser_id)
pub async fn get_user_ctx(headers: &HeaderMap) -> UserCtx {
    let session_token = headers.get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find(|s| s.trim().starts_with("session="))
                .map(|s| s.trim().strip_prefix("session=").unwrap_or(""))
        })
        .unwrap_or("");

    let empty = UserCtx { role: "anonymous".to_string(), user_id: 0, supplier_id: 0, purchaser_id: 0 };
    if session_token.is_empty() {
        return empty;
    }

    let parts: Vec<&str> = session_token.split(':').collect();
    if parts.len() < 2 {
        return empty;
    }

    let user_id = match parts[0].parse::<i64>() {
        Ok(id) => id,
        Err(_) => return empty,
    };

    let rows = sqlx::query(
        "SELECT role, COALESCE(supplier_id,0) as supplier_id, COALESCE(purchaser_id,0) as purchaser_id FROM user_account WHERE id = ? AND status = 1"
    )
    .bind(user_id)
    .fetch_all(pool())
    .await
    .unwrap_or_default();

    if rows.is_empty() {
        return empty;
    }
    let row = &rows[0];
    UserCtx {
        role: row.get::<String, _>("role"),
        user_id,
        supplier_id: row.get::<i64, _>("supplier_id"),
        purchaser_id: row.get::<i64, _>("purchaser_id"),
    }
}

/// 记录操作审计日志（关键写操作）
pub async fn log_operation(
    user: &UserCtx,
    action: &str,
    target_type: &str,
    target_id: &str,
    detail: &str,
) {
    let username: String = sqlx::query_scalar("SELECT COALESCE(username,'') FROM user_account WHERE id = ?")
        .bind(user.user_id)
        .fetch_one(pool())
        .await
        .unwrap_or_default();
    let _ = sqlx::query(
        "INSERT INTO operation_log(user_id, username, action, target_type, target_id, detail) VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(user.user_id)
    .bind(&username)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(detail)
    .execute(pool())
    .await;
}

/// 判断用户是否可操作该采购单（行级数据权限）：admin/super_admin 可见全部；supplier 仅可见自己绑定的供应商
pub fn can_access_purchase_order(user: &UserCtx, order_supplier_id: i64) -> bool {
    match user.role.as_str() {
        "super_admin" | "admin" => true,
        "supplier" => user.supplier_id == order_supplier_id,
        _ => false,
    }
}

/// 判断用户是否可操作该销售单（行级数据权限）：admin/super_admin 可见全部；purchaser 仅可见自己绑定的采购单位
pub fn can_access_sales_order(user: &UserCtx, order_purchaser_id: i64) -> bool {
    match user.role.as_str() {
        "super_admin" | "admin" => true,
        "purchaser" => user.purchaser_id == order_purchaser_id,
        _ => false,
    }
}

pub fn get_route_required_role(path: &str) -> Option<&str> {
    match path {
        "/supplier" | "/api/supplier/create" | "/api/supplier/update" | "/api/supplier/delete" => Some("supplier"),
        "/purchaser" | "/api/purchaser/create" | "/api/purchaser/update" | "/api/purchaser/delete" => Some("purchaser"),
        "/product" | "/api/product/create" | "/api/product/update" | "/api/product/delete" => Some("admin"),
        "/warehouse" | "/api/warehouse/create" | "/api/warehouse/update" | "/api/warehouse/delete" => Some("admin"),
        "/inventory" => Some("admin"),
        "/purchase" | "/api/purchase_order/create" | "/api/purchase_order/update" | "/api/purchase_order/delete" => Some("supplier"),
        "/sales" | "/api/sales_order/create" | "/api/sales_order/update" | "/api/sales_order/update_prices" | "/api/sales_order/delete" | "/api/sales_order/upload_image" | "/api/sales_order/delete_image" => Some("purchaser"),
        "/query/purchase_order" | "/query/purchase_document" | "/query/purchase_price" | "/query/purchase_summary" | "/query/supplier_balance" => Some("supplier"),
        "/query/sales_order" | "/query/sales_summary" | "/query/sales_price" | "/query/purchaser_balance" | "/query/product_rank" | "/query/reimburse_summary" | "/query/allocation_source" | "/query/order_adjust" => Some("purchaser"),
        "/query/stock_balance" | "/query/stock_flow" | "/query/stock_warning" | "/query/slow_stock" | "/query/stock_summary" | "/query/stock_summary_reimburse" => Some("admin"),
        "/query/income_expense" | "/query/profit_detail" | "/query/overview" | "/query/category_stats" | "/query/document_summary" => Some("admin"),
        "/user" | "/api/user" | "/api/user/*" => Some("super_admin"),
        "/system" | "/api/system/config" => Some("super_admin"),
        "/system/operation_log" => Some("admin"),
        "/backup" | "/api/backup" | "/api/backup/*" => Some("super_admin"),
        "/restore" | "/api/restore/*" => Some("super_admin"),
        _ => None,
    }
}

pub fn check_api_route_permission(path: &str) -> Option<&str> {
    if path.starts_with("/api/supplier/") {
        // 供应商基础资料：supplier 角色以上可访问
        Some("supplier")
    } else if path.starts_with("/api/purchaser/") {
        // 采购单位基础资料：purchaser 角色以上可访问
        Some("purchaser")
    } else if path.starts_with("/api/product/") {
        Some("manage.admin")
    } else if path.starts_with("/api/warehouse/") {
        Some("manage.admin")
    } else if path.starts_with("/api/inventory/") {
        Some("manage.admin")
    } else if path.starts_with("/api/purchase_order/") {
        // 采购单：按操作细分权限点
        if path.ends_with("/create") || path.ends_with("/import") {
            Some("purchase_order.create")
        } else if path.ends_with("/update") {
            Some("purchase_order.update")
        } else if path.ends_with("/approve") {
            Some("purchase_order.approve")
        } else if path.ends_with("/unapprove") {
            Some("purchase_order.unapprove")
        } else if path.ends_with("/delete") {
            Some("purchase_order.delete")
        } else if path.ends_with("/cancel") {
            Some("purchase_order.cancel")
        } else {
            Some("purchase_order.view")
        }
    } else if path.starts_with("/api/purchase_document/") {
        // 采购单据：上传/删除属于写操作
        if path.ends_with("/upload") || path.ends_with("/delete") {
            Some("purchase_order.update")
        } else {
            Some("purchase_order.view")
        }
    } else if path.starts_with("/api/sales_order/") {
        // 销售单：按操作细分权限点
        if path.ends_with("/create") || path.ends_with("/import") || path.contains("/generate_purchase") {
            Some("sales_order.create")
        } else if path.ends_with("/update") || path.ends_with("/correction") || path.ends_with("/upload_image") || path.ends_with("/delete_image") {
            Some("sales_order.update")
        } else if path.ends_with("/settle") {
            Some("sales_order.update")
        } else if path.ends_with("/approve") {
            Some("sales_order.approve")
        } else if path.ends_with("/unapprove") {
            Some("sales_order.unapprove")
        } else if path.ends_with("/delete") {
            Some("sales_order.delete")
        } else if path.ends_with("/update_prices") {
            Some("sales_order.adjust_price")
        } else if path.ends_with("/update_status") {
            Some("sales_order.approve")
        } else if path.ends_with("/cancel") {
            Some("sales_order.cancel")
        } else {
            Some("sales_order.view")
        }
    } else if path.starts_with("/api/query/purchase") || path.starts_with("/api/query/supplier_balance") {
        Some("query.view")
    } else if path.starts_with("/api/query/sales") || path.starts_with("/api/query/purchaser_balance") || path.starts_with("/api/query/product_rank") {
        Some("query.view")
    } else if path.starts_with("/api/query/stock") || path.starts_with("/api/query/income") || path.starts_with("/api/query/profit") || path.starts_with("/api/query/overview") || path.starts_with("/api/query/category") || path.starts_with("/api/query/document") {
        Some("manage.admin")
    } else if path.starts_with("/api/query/") {
        Some("query.view")
    } else if path.starts_with("/api/user/") {
        if path == "/api/user/list" {
            // 用户列表（用于采购单/销售单经手人选择），采购/销售/管理员均可访问
            Some("query.view")
        } else {
            Some("manage.user")
        }
    } else if path.starts_with("/api/system/") {
        Some("manage.system")
    } else if path.starts_with("/api/backup/") {
        Some("manage.backup")
    } else if path.starts_with("/api/restore/") {
        Some("manage.backup")
    } else {
        None
    }
}

/// 校验 API 权限：返回权限点对应的角色校验（权限点系统）
pub async fn check_api_permission(headers: &HeaderMap, path: &str) -> Result<String, (StatusCode, String)> {
    let ctx = get_user_ctx(headers).await;
    
    if let Some(permission) = check_api_route_permission(path) {
        if !has_permission_point(&ctx.role, permission) {
            return Err((StatusCode::FORBIDDEN, serde_json::to_string(&serde_json::json!({
                "success": false,
                "message": "您没有权限执行此操作"
            })).unwrap()));
        }
    }
    
    Ok(ctx.role)
}

pub async fn check_page_permission(headers: &HeaderMap, path: &str) -> Result<String, Html<String>> {
    let role = get_user_role(headers).await;
    
    if let Some(required_role) = get_route_required_role(path) {
        if !has_permission(&role, required_role) {
            if role == "anonymous" {
                return Err(Html(String::from(r#"
                    <!DOCTYPE html>
                    <html>
                    <head><meta http-equiv="refresh" content="0; url=/login"></head>
                    <body>请登录</body>
                    </html>
                "#)));
            }
            let content = r#"<div class="container mt-5"><div class="alert alert-danger text-center" style="font-size:1.5rem;">您没有权限访问此页面</div></div>"#;
            return Err(Html(layout_html("无权限", path, content)));
        }
    }
    
    Ok(role)
}