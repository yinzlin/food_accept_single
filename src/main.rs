#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use axum::{
    extract::{Json, Path},
    http::{header, StatusCode},
    response::{Html, IntoResponse},
    routing::{delete, get, post, put},
    Router,
};
use bytes::Bytes;
use calamine::{open_workbook_auto_from_rs, Data, Reader};
use chrono::Local;
use axum::extract::Multipart;
use rust_xlsxwriter::{Format, FormatAlign, FormatBorder, Workbook, XlsxError};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{AssertSqlSafe, Row};
use sqlx::SqlitePool;
use std::sync::OnceLock;
use tao::event_loop::{ControlFlow, EventLoop};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIconBuilder, TrayIconEvent};

static DB_POOL: OnceLock<SqlitePool> = OnceLock::new();

const BOOTSTRAP_CSS: &str = include_str!("../static/bootstrap.min.css");
const BOOTSTRAP_JS: &str = include_str!("../static/bootstrap.bundle.min.js");

async fn get_user_role(headers: &axum::http::HeaderMap) -> String {
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

fn has_permission(role: &str, required_role: &str) -> bool {
    let role_permissions = std::collections::HashMap::from([
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

fn get_route_required_role(path: &str) -> Option<&str> {
    match path {
        "/supplier" | "/api/supplier/create" | "/api/supplier/update" | "/api/supplier/delete" => Some("supplier"),
        "/purchaser" | "/api/purchaser/create" | "/api/purchaser/update" | "/api/purchaser/delete" => Some("purchaser"),
        "/product" | "/api/product/create" | "/api/product/update" | "/api/product/delete" => Some("admin"),
        "/warehouse" | "/api/warehouse/create" | "/api/warehouse/update" | "/api/warehouse/delete" => Some("admin"),
        "/inventory" => Some("admin"),
        "/purchase" | "/api/purchase_order/create" | "/api/purchase_order/update" | "/api/purchase_order/delete" => Some("supplier"),
        "/sales" | "/api/sales_order/create" | "/api/sales_order/update" | "/api/sales_order/delete" => Some("purchaser"),
        "/query/purchase_order" | "/query/purchase_price" | "/query/purchase_summary" | "/query/supplier_balance" => Some("supplier"),
        "/query/sales_order" | "/query/sales_summary" | "/query/sales_price" | "/query/purchaser_balance" | "/query/product_rank" => Some("purchaser"),
        "/query/stock_balance" | "/query/stock_flow" | "/query/stock_warning" | "/query/slow_stock" => Some("admin"),
        "/query/income_expense" | "/query/profit_detail" | "/query/overview" | "/query/category_stats" | "/query/document_summary" => Some("admin"),
        "/user" | "/api/user" | "/api/user/*" => Some("super_admin"),
        "/system" | "/api/system/config" => Some("super_admin"),
        "/backup" | "/api/backup" | "/api/backup/*" => Some("super_admin"),
        "/restore" | "/api/restore/*" => Some("super_admin"),
        _ => None,
    }
}

fn check_api_route_permission(path: &str) -> Option<&str> {
    if path.starts_with("/api/supplier/") {
        if path.starts_with("/api/supplier/list") || path.starts_with("/api/supplier/export") {
            Some("supplier")
        } else {
            Some("supplier")
        }
    } else if path.starts_with("/api/purchaser/") {
        Some("purchaser")
    } else if path.starts_with("/api/product/") {
        Some("admin")
    } else if path.starts_with("/api/warehouse/") {
        Some("admin")
    } else if path.starts_with("/api/purchase_order/") {
        Some("supplier")
    } else if path.starts_with("/api/sales_order/") {
        Some("purchaser")
    } else if path.starts_with("/api/query/purchase") {
        Some("supplier")
    } else if path.starts_with("/api/query/sales") || path.starts_with("/api/query/purchaser_balance") || path.starts_with("/api/query/product_rank") {
        Some("purchaser")
    } else if path.starts_with("/api/query/stock") || path.starts_with("/api/query/income") || path.starts_with("/api/query/profit") || path.starts_with("/api/query/overview") || path.starts_with("/api/query/category") || path.starts_with("/api/query/document") {
        Some("admin")
    } else if path.starts_with("/api/user/") {
        Some("super_admin")
    } else if path.starts_with("/api/system/") {
        Some("super_admin")
    } else if path.starts_with("/api/backup/") {
        Some("super_admin")
    } else if path.starts_with("/api/restore/") {
        Some("super_admin")
    } else {
        None
    }
}

async fn check_page_permission(headers: &axum::http::HeaderMap, path: &str) -> Result<String, Html<String>> {
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

async fn check_api_permission(headers: &axum::http::HeaderMap, path: &str) -> Result<String, (StatusCode, String)> {
    let role = get_user_role(headers).await;
    
    if let Some(required_role) = check_api_route_permission(path) {
        if !has_permission(&role, required_role) {
            return Err((StatusCode::FORBIDDEN, serde_json::to_string(&serde_json::json!({
                "success": false,
                "message": "您没有权限执行此操作"
            })).unwrap()));
        }
    }
    
    Ok(role)
}

async fn serve_bootstrap_css() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        BOOTSTRAP_CSS,
    )
}

async fn serve_bootstrap_js() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/javascript; charset=utf-8")],
        BOOTSTRAP_JS,
    )
}

async fn init_pool() {
    let pool = SqlitePoolOptions::new()
        .max_connections(32)
        .connect_with(
            SqliteConnectOptions::new()
                .filename("food_accept_v3.db")
                .create_if_missing(true)
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Delete),
        )
        .await
        .expect("数据库连接失败");
    init_tables(&pool).await.expect("初始化数据表失败");
    
    let _ = sqlx::query(
        "DELETE FROM sales_order_item WHERE unit_price IS NULL OR quantity IS NULL OR quantity = 0 OR amount = 0"
    )
    .execute(&pool)
    .await;
    
    let _ = sqlx::query(
        "DELETE FROM sales_order WHERE id NOT IN (SELECT DISTINCT order_id FROM sales_order_item)"
    )
    .execute(&pool)
    .await;
    
    let _ = sqlx::query("VACUUM").execute(&pool).await;
    
    DB_POOL.set(pool).expect("数据库连接池已初始化");
}

fn pool() -> &'static SqlitePool {
    DB_POOL.get().expect("数据库连接池未初始化")
}

async fn init_tables(pool: &SqlitePool) -> Result<(), anyhow::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS category (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            parent_id INTEGER,
            entity_type TEXT NOT NULL,
            sort_order INTEGER DEFAULT 0,
            create_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(parent_id) REFERENCES category(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS supplier (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            contact TEXT,
            phone TEXT,
            address TEXT,
            category_id INTEGER REFERENCES category(id),
            create_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS purchaser (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            contact TEXT,
            phone TEXT,
            address TEXT,
            category_id INTEGER REFERENCES category(id),
            create_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS product (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            spec TEXT,
            unit TEXT DEFAULT '个',
            base_unit TEXT DEFAULT '个',
            base_price REAL DEFAULT 0,
            purchase_price REAL DEFAULT 0,
            category_id INTEGER REFERENCES category(id),
            create_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(name, spec)
        )
        "#,
    )
    .execute(pool)
    .await?;

    let _ = sqlx::query("ALTER TABLE product ADD COLUMN base_unit TEXT DEFAULT '个'")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE product ADD COLUMN base_price REAL DEFAULT 0")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE product ADD COLUMN purchase_price REAL DEFAULT 0")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE product ADD COLUMN alias1 TEXT")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE product ADD COLUMN alias2 TEXT")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE product ADD COLUMN image_url TEXT")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE product ADD COLUMN status INTEGER DEFAULT 1")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE supplier ADD COLUMN business_scope TEXT")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE supplier ADD COLUMN remark TEXT")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE purchaser ADD COLUMN business_scope TEXT")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE purchaser ADD COLUMN remark TEXT")
        .execute(pool)
        .await;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS product_unit (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            product_id INTEGER NOT NULL,
            unit_name TEXT NOT NULL,
            ratio REAL NOT NULL DEFAULT 1,
            unit_price REAL DEFAULT 0,
            purchase_price REAL DEFAULT 0,
            sort_order INTEGER DEFAULT 0,
            FOREIGN KEY(product_id) REFERENCES product(id),
            UNIQUE(product_id, unit_name)
        )
        "#,
    )
    .execute(pool)
    .await?;

    let _ = sqlx::query("ALTER TABLE product_unit ADD COLUMN unit_price REAL DEFAULT 0")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE product_unit ADD COLUMN purchase_price REAL DEFAULT 0")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE product_unit ADD COLUMN sort_order INTEGER DEFAULT 0")
        .execute(pool)
        .await;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS product_price (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            product_id INTEGER NOT NULL,
            price_type TEXT NOT NULL,
            price REAL NOT NULL DEFAULT 0,
            collected_at DATETIME,
            source TEXT,
            FOREIGN KEY(product_id) REFERENCES product(id),
            UNIQUE(product_id, price_type)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS warehouse (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            code TEXT UNIQUE,
            address TEXT,
            contact TEXT,
            phone TEXT,
            status INTEGER DEFAULT 1,
            sort_order INTEGER DEFAULT 0,
            create_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            update_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS inventory (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            product_id INTEGER NOT NULL,
            warehouse_id INTEGER NOT NULL DEFAULT 1,
            quantity REAL NOT NULL DEFAULT 0,
            min_stock REAL DEFAULT 0,
            max_stock REAL DEFAULT 1000,
            last_update DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(product_id) REFERENCES product(id),
            FOREIGN KEY(warehouse_id) REFERENCES warehouse(id),
            UNIQUE(product_id, warehouse_id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("ALTER TABLE inventory ADD COLUMN IF NOT EXISTS warehouse_id INTEGER DEFAULT 1")
        .execute(pool)
        .await
        .ok();

    sqlx::query(
        "INSERT OR IGNORE INTO warehouse (id, name, code, status) VALUES (1, '默认仓库', 'WH001', 1)"
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS purchase_order (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            supplier_id INTEGER NOT NULL,
            order_no TEXT NOT NULL UNIQUE,
            order_date TEXT NOT NULL,
            total_amount REAL NOT NULL DEFAULT 0,
            status TEXT DEFAULT 'pending',
            remark TEXT,
            create_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(supplier_id) REFERENCES supplier(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS purchase_order_item (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            order_id INTEGER NOT NULL,
            product_id INTEGER NOT NULL,
            product_name TEXT NOT NULL,
            alias1 TEXT,
            alias2 TEXT,
            spec TEXT,
            unit TEXT NOT NULL,
            unit_price REAL NOT NULL,
            quantity REAL NOT NULL,
            base_quantity REAL NOT NULL DEFAULT 0,
            amount REAL NOT NULL DEFAULT 0,
            remark TEXT,
            FOREIGN KEY(order_id) REFERENCES purchase_order(id),
            FOREIGN KEY(product_id) REFERENCES product(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    let _ = sqlx::query("ALTER TABLE purchase_order_item ADD COLUMN alias1 TEXT")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE purchase_order_item ADD COLUMN alias2 TEXT")
        .execute(pool)
        .await;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sales_order (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            purchaser_id INTEGER NOT NULL,
            order_no TEXT NOT NULL UNIQUE,
            order_date TEXT NOT NULL,
            total_amount REAL NOT NULL DEFAULT 0,
            status TEXT DEFAULT 'pending',
            remark TEXT,
            create_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(purchaser_id) REFERENCES purchaser(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sales_order_item (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            order_id INTEGER NOT NULL,
            product_id INTEGER NOT NULL,
            product_name TEXT NOT NULL,
            alias1 TEXT,
            alias2 TEXT,
            spec TEXT,
            unit TEXT NOT NULL,
            unit_price REAL NOT NULL,
            quantity REAL NOT NULL,
            base_quantity REAL NOT NULL DEFAULT 0,
            amount REAL NOT NULL DEFAULT 0,
            remark TEXT,
            FOREIGN KEY(order_id) REFERENCES sales_order(id),
            FOREIGN KEY(product_id) REFERENCES product(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    let _ = sqlx::query("ALTER TABLE purchase_order ADD COLUMN discount_rate REAL DEFAULT 0")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE purchase_order ADD COLUMN final_amount REAL DEFAULT 0")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE purchase_order ADD COLUMN amount_reduction REAL DEFAULT 0")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE sales_order ADD COLUMN discount_rate REAL DEFAULT 0")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE sales_order ADD COLUMN final_amount REAL DEFAULT 0")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE sales_order ADD COLUMN amount_reduction REAL DEFAULT 0")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE sales_order ADD COLUMN warehouse_id INTEGER DEFAULT 0")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE sales_order ADD COLUMN warehouse_name TEXT")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE purchase_order ADD COLUMN warehouse_id INTEGER DEFAULT 0")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE purchase_order ADD COLUMN warehouse_name TEXT")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE purchase_order_item ADD COLUMN remark TEXT")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE sales_order_item ADD COLUMN alias1 TEXT")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE sales_order_item ADD COLUMN alias2 TEXT")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE sales_order_item ADD COLUMN remark TEXT")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE sales_order_item ADD COLUMN supplier_id INTEGER DEFAULT 0")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE sales_order_item ADD COLUMN supplier_name TEXT")
        .execute(pool)
        .await;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS food_accept (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            supplier_id INTEGER NOT NULL,
            purchaser_id INTEGER NOT NULL,
            car_no TEXT,
            supply_time TEXT NOT NULL,
            total_price REAL NOT NULL DEFAULT 0,
            discount_rate REAL NOT NULL DEFAULT 0,
            final_price REAL NOT NULL DEFAULT 0,
            status TEXT DEFAULT 'pending',
            create_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(supplier_id) REFERENCES supplier(id),
            FOREIGN KEY(purchaser_id) REFERENCES purchaser(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS food_item (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            accept_id INTEGER NOT NULL,
            food_name TEXT NOT NULL,
            spec TEXT,
            unit_price REAL NOT NULL,
            quantity REAL NOT NULL,
            sub_total REAL NOT NULL DEFAULT 0,
            produce_batch TEXT,
            shelf_life TEXT,
            has_veg_report INTEGER DEFAULT 0,
            has_meat_quarantine INTEGER DEFAULT 0,
            has_abnormal INTEGER DEFAULT 0,
            pass_check INTEGER DEFAULT 1,
            remark TEXT,
            FOREIGN KEY(accept_id) REFERENCES food_accept(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS system_config (
            key TEXT PRIMARY KEY,
            value TEXT,
            create_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            update_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS backup_record (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            backup_time TEXT NOT NULL,
            file_name TEXT NOT NULL,
            size INTEGER NOT NULL,
            create_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS user_account (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT NOT NULL UNIQUE,
            password TEXT NOT NULL,
            nickname TEXT DEFAULT '',
            role TEXT DEFAULT 'user',
            status INTEGER DEFAULT 1,
            last_login_time DATETIME,
            create_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            update_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;

    let super_admin_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM user_account WHERE username = 'super_admin')")
        .fetch_one(pool)
        .await?;
    
    if super_admin_exists {
        sqlx::query("UPDATE user_account SET nickname = '超级管理员', role = COALESCE(NULLIF(role, ''), 'super_admin'), status = 1 WHERE username = 'super_admin'")
            .execute(pool)
            .await?;
    } else {
        let super_admin_pwd = bcrypt::hash("admin123", bcrypt::DEFAULT_COST).unwrap();
        sqlx::query("INSERT INTO user_account (username, password, nickname, role) VALUES (?, ?, ?, ?)")
            .bind("super_admin")
            .bind(&super_admin_pwd)
            .bind("超级管理员")
            .bind("super_admin")
            .execute(pool)
            .await?;
    }
    
    let admin_pwd = bcrypt::hash("admin123", bcrypt::DEFAULT_COST).unwrap();
    let admin_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM user_account WHERE username = 'admin')")
        .fetch_one(pool)
        .await?;
    if admin_exists {
        sqlx::query("UPDATE user_account SET password = ?, nickname = '管理员', role = 'admin', status = 1 WHERE username = 'admin'")
            .bind(&admin_pwd)
            .execute(pool)
            .await?;
    } else {
        sqlx::query("INSERT INTO user_account (username, password, nickname, role) VALUES (?, ?, ?, ?)")
            .bind("admin")
            .bind(&admin_pwd)
            .bind("管理员")
            .bind("admin")
            .execute(pool)
            .await?;
    }
    
    let supplier_pwd = bcrypt::hash("supplier123", bcrypt::DEFAULT_COST).unwrap();
    let supplier_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM user_account WHERE username = 'supplier')")
        .fetch_one(pool)
        .await?;
    if supplier_exists {
        sqlx::query("UPDATE user_account SET password = ?, nickname = '供应商', role = 'supplier', status = 1 WHERE username = 'supplier'")
            .bind(&supplier_pwd)
            .execute(pool)
            .await?;
    } else {
        sqlx::query("INSERT INTO user_account (username, password, nickname, role) VALUES (?, ?, ?, ?)")
            .bind("supplier")
            .bind(&supplier_pwd)
            .bind("供应商")
            .bind("supplier")
            .execute(pool)
            .await?;
    }
    
    let purchaser_pwd = bcrypt::hash("purchaser123", bcrypt::DEFAULT_COST).unwrap();
    let purchaser_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM user_account WHERE username = 'purchaser')")
        .fetch_one(pool)
        .await?;
    if purchaser_exists {
        sqlx::query("UPDATE user_account SET password = ?, nickname = '采购方', role = 'purchaser', status = 1 WHERE username = 'purchaser'")
            .bind(&purchaser_pwd)
            .execute(pool)
            .await?;
    } else {
        sqlx::query("INSERT INTO user_account (username, password, nickname, role) VALUES (?, ?, ?, ?)")
            .bind("purchaser")
            .bind(&purchaser_pwd)
            .bind("采购方")
            .bind("purchaser")
            .execute(pool)
            .await?;
    }

    // 预置分类数据 - 供应商分类
    sqlx::query("INSERT OR IGNORE INTO category(id, name, parent_id, entity_type) VALUES (1, '食材供应商', NULL, 'supplier')")
        .execute(pool).await?;
    sqlx::query("INSERT OR IGNORE INTO category(id, name, parent_id, entity_type) VALUES (2, '蔬菜供应商', 1, 'supplier')")
        .execute(pool).await?;
    sqlx::query("INSERT OR IGNORE INTO category(id, name, parent_id, entity_type) VALUES (3, '肉类供应商', 1, 'supplier')")
        .execute(pool).await?;
    // 预置分类数据 - 采购方分类
    sqlx::query("INSERT OR IGNORE INTO category(id, name, parent_id, entity_type) VALUES (4, '政府单位', NULL, 'purchaser')")
        .execute(pool).await?;
    sqlx::query("INSERT OR IGNORE INTO category(id, name, parent_id, entity_type) VALUES (5, '学校', NULL, 'purchaser')")
        .execute(pool).await?;
    // 预置分类数据 - 商品分类
    sqlx::query("INSERT OR IGNORE INTO category(id, name, parent_id, entity_type) VALUES (6, '荤鲜类', NULL, 'product')")
        .execute(pool).await?;
    sqlx::query("INSERT OR IGNORE INTO category(id, name, parent_id, entity_type) VALUES (10, '家禽', 6, 'product')")
        .execute(pool).await?;
    sqlx::query("INSERT OR IGNORE INTO category(id, name, parent_id, entity_type) VALUES (11, '家畜', 6, 'product')")
        .execute(pool).await?;
    sqlx::query("INSERT OR IGNORE INTO category(id, name, parent_id, entity_type) VALUES (12, '水产', 6, 'product')")
        .execute(pool).await?;
    sqlx::query("INSERT OR IGNORE INTO category(id, name, parent_id, entity_type) VALUES (7, '鲜蔬类', NULL, 'product')")
        .execute(pool).await?;
    sqlx::query("INSERT OR IGNORE INTO category(id, name, parent_id, entity_type) VALUES (8, '粮油干调', NULL, 'product')")
        .execute(pool).await?;
    sqlx::query("INSERT OR IGNORE INTO category(id, name, parent_id, entity_type) VALUES (9, '豆制品', NULL, 'product')")
        .execute(pool).await?;
    sqlx::query("INSERT OR IGNORE INTO category(id, name, parent_id, entity_type) VALUES (13, '粉面制品', NULL, 'product')")
        .execute(pool).await?;
    sqlx::query("INSERT OR IGNORE INTO category(id, name, parent_id, entity_type) VALUES (14, '水果类', NULL, 'product')")
        .execute(pool).await?;
    sqlx::query("INSERT OR IGNORE INTO category(id, name, parent_id, entity_type) VALUES (15, '其它', NULL, 'product')")
        .execute(pool).await?;
    sqlx::query("INSERT OR IGNORE INTO category(id, name, parent_id, entity_type) VALUES (16, '耗材类', NULL, 'product')")
        .execute(pool).await?;

    sqlx::query(
        "INSERT OR IGNORE INTO supplier(name, contact, phone, address, category_id) VALUES ('湖南食全味美餐饮管理有限公司', '张经理', '13800138000', '湖南省长沙市', 1)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT OR IGNORE INTO purchaser(name, contact, phone, address) VALUES ('胜利派出所', '李所长', '13900139000', '巡警城区')",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT OR IGNORE INTO purchaser(name, contact, phone, address) VALUES ('城东派出所', '王所长', '13700137000', '城东新区')",
    )
    .execute(pool)
    .await?;

    Ok(())
}

#[derive(Deserialize, Serialize)]
struct SupplierReq {
    id: Option<i64>,
    name: String,
    contact: Option<String>,
    phone: Option<String>,
    address: Option<String>,
    business_scope: Option<String>,
    remark: Option<String>,
    category_id: Option<i64>,
}

#[derive(Deserialize, Serialize)]
struct PurchaserReq {
    id: Option<i64>,
    name: String,
    contact: Option<String>,
    phone: Option<String>,
    address: Option<String>,
    business_scope: Option<String>,
    remark: Option<String>,
    category_id: Option<i64>,
}

#[derive(Deserialize, Serialize)]
struct DeleteReq {
    id: i64,
}

#[derive(Deserialize)]
struct LoginReq {
    username: String,
    password: String,
}

#[derive(Deserialize, Serialize)]
struct ProductReq {
    id: Option<i64>,
    name: String,
    spec: Option<String>,
    alias1: Option<String>,
    alias2: Option<String>,
    unit: Option<String>,
    base_unit: Option<String>,
    base_price: Option<f64>,
    purchase_price: Option<f64>,
    image_url: Option<String>,
    category_id: Option<i64>,
}

#[derive(Deserialize, Serialize)]
struct ProductUnitReq {
    product_id: i64,
    unit_name: String,
    ratio: f64,
    unit_price: Option<f64>,
    purchase_price: Option<f64>,
    sort_order: Option<i32>,
}

#[derive(Deserialize, Serialize)]
struct ProductPriceReq {
    product_id: i64,
    price_type: String,
    price: Option<f64>,
    collected_at: Option<String>,
    source: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct CategoryReq {
    name: String,
    parent_id: Option<i64>,
    entity_type: String,
    sort_order: Option<i32>,
}

#[derive(Deserialize, Serialize)]
struct PurchaseOrderReq {
    id: Option<i64>,
    supplier_id: i64,
    order_no: String,
    order_date: String,
    total_amount: f64,
    discount_rate: f64,
    amount_reduction: f64,
    final_amount: f64,
    warehouse_id: i64,
    warehouse_name: String,
    items: Vec<PurchaseOrderItemReq>,
    remark: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct PurchaseOrderItemReq {
    product_id: i64,
    product_name: String,
    alias1: Option<String>,
    alias2: Option<String>,
    spec: Option<String>,
    unit: Option<String>,
    unit_price: f64,
    quantity: f64,
    base_quantity: Option<f64>,
    amount: f64,
    remark: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct SalesOrderReq {
    id: Option<i64>,
    purchaser_id: i64,
    order_no: String,
    order_date: String,
    total_amount: f64,
    discount_rate: f64,
    amount_reduction: f64,
    final_amount: f64,
    warehouse_id: i64,
    warehouse_name: String,
    items: Vec<SalesOrderItemReq>,
    remark: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct SalesOrderItemReq {
    product_id: i64,
    product_name: String,
    alias1: Option<String>,
    alias2: Option<String>,
    spec: Option<String>,
    unit: Option<String>,
    unit_price: f64,
    quantity: f64,
    base_quantity: Option<f64>,
    amount: f64,
    supplier_id: i64,
    supplier_name: String,
    category_id: Option<i64>,
    remark: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct AcceptReq {
    supplier_id: i64,
    purchaser_id: i64,
    car_no: Option<String>,
    supply_time: String,
    total_price: f64,
    discount_rate: f64,
    final_price: f64,
    items: Vec<FoodItemReq>,
}

#[derive(Deserialize, Serialize)]
struct FoodItemReq {
    food_name: String,
    spec: Option<String>,
    unit_price: f64,
    quantity: f64,
    sub_total: f64,
    produce_batch: Option<String>,
    shelf_life: Option<String>,
    has_veg_report: bool,
    has_meat_quarantine: bool,
    has_abnormal: bool,
    pass_check: bool,
    remark: Option<String>,
}

fn sidebar_html() -> String {
    String::from(r#"
        <div class="sidebar">
            <div class="sidebar-header">
                <div class="logo"><span class="logo-icon">🍽️</span></div>
                <div class="logo-text">进销存管理系统</div>
            </div>
            <div class="sidebar-search">
                <input type="text" id="treeSearch" placeholder="🔍 搜索菜单..." oninput="filterTree()">
            </div>
            <ul class="tree-menu" id="treeMenu">
                <li class="tree-node leaf" data-path="/">
                    <a href="/"><span class="node-icon">🏠</span><span class="node-label">首页</span></a>
                </li>
                <li class="tree-node folder" data-path="base" data-role="admin">
                    <div class="node-header" onclick="toggleNode(this)">
                        <span class="toggle-icon">▶</span>
                        <span class="node-icon">📁</span>
                        <span class="node-label">基础数据</span>
                    </div>
                    <ul class="tree-children">
                        <li class="tree-node folder" data-path="/supplier" id="supplierCatFolder" data-role="supplier">
                            <div class="node-header" onclick="toggleNode(this)" oncontextmenu="showSupplierRootContextMenu(event)">
                                <span class="toggle-icon">▶</span>
                                <span class="node-icon">🏭</span>
                                <span class="node-label">供应商管理</span>
                            </div>
                            <ul class="tree-children" id="supplierCatTree">
                                <li class="tree-node leaf" data-path="/supplier">
                                    <a href="/supplier" onclick="event.preventDefault(); filterSuppliersByCategory(null, '全部供应商'); return false;"><span class="node-icon">📋</span><span class="node-label">全部供应商</span></a>
                                </li>
                            </ul>
                        </li>
                        <li class="tree-node folder" data-path="/purchaser" id="purchaserCatFolder" data-role="purchaser">
                            <div class="node-header" onclick="toggleNode(this)" oncontextmenu="showPurchaserRootContextMenu(event)">
                                <span class="toggle-icon">▶</span>
                                <span class="node-icon">🏢</span>
                                <span class="node-label">采购方管理</span>
                            </div>
                            <ul class="tree-children" id="purchaserCatTree">
                                <li class="tree-node leaf" data-path="/purchaser">
                                    <a href="/purchaser" onclick="event.preventDefault(); filterPurchasersByCategory(null, '全部采购方'); return false;"><span class="node-icon">📋</span><span class="node-label">全部采购方</span></a>
                                </li>
                            </ul>
                        </li>
                        <li class="tree-node folder" data-path="/product" id="productCatFolder" data-role="admin">
                            <div class="node-header" onclick="toggleNode(this)" oncontextmenu="showProductRootContextMenu(event)">
                                <span class="toggle-icon">▶</span>
                                <span class="node-icon">📦</span>
                                <span class="node-label">商品管理</span>
                            </div>
                            <ul class="tree-children" id="productCatTree">
                                <li class="tree-node leaf" data-path="/product">
                                    <a href="/product" onclick="event.preventDefault(); filterProductsByCategory(null, '全部商品'); return false;"><span class="node-icon">📋</span><span class="node-label">全部商品</span></a>
                                </li>
                            </ul>
                        </li>
                        <li class="tree-node leaf" data-path="/warehouse" data-role="admin">
                            <a href="/warehouse"><span class="node-icon">🏠</span><span class="node-label">仓库管理</span></a>
                        </li>
                    </ul>
                </li>
                <li class="tree-node folder" data-path="order" data-role="admin">
                    <div class="node-header" onclick="toggleNode(this)">
                        <span class="toggle-icon">▶</span>
                        <span class="node-icon">📁</span>
                        <span class="node-label">订单管理</span>
                    </div>
                    <ul class="tree-children">
                        <li class="tree-node leaf" data-path="/purchase" data-role="supplier">
                            <a href="/purchase"><span class="node-icon">🛒</span><span class="node-label">采购订单</span></a>
                        </li>
                        <li class="tree-node leaf" data-path="/sales" data-role="purchaser">
                            <a href="/sales"><span class="node-icon">💰</span><span class="node-label">销售订单</span></a>
                        </li>
                    </ul>
                </li>
                <li class="tree-node folder" data-path="query" data-role="query">
                    <div class="node-header" onclick="toggleNode(this)">
                        <span class="toggle-icon">▶</span>
                        <span class="node-icon">🔍</span>
                        <span class="node-label">数据查询</span>
                    </div>
                    <ul class="tree-children">
                        <li class="tree-node folder" data-path="query-purchase" data-role="supplier">
                            <div class="node-header" onclick="toggleNode(this)">
                                <span class="toggle-icon">▶</span>
                                <span class="node-icon">📦</span>
                                <span class="node-label">采购查询</span>
                            </div>
                            <ul class="tree-children">
                                <li class="tree-node leaf" data-path="/query/purchase_order">
                                    <a href="/query/purchase_order"><span class="node-icon">📋</span><span class="node-label">采购订单查询</span></a>
                                </li>
                                <li class="tree-node leaf" data-path="/query/purchase_price">
                                    <a href="/query/purchase_price"><span class="node-icon">💰</span><span class="node-label">采购价格查询</span></a>
                                </li>
                                <li class="tree-node leaf" data-path="/query/purchase_summary">
                                    <a href="/query/purchase_summary"><span class="node-icon">📊</span><span class="node-label">采购汇总统计</span></a>
                                </li>
                                <li class="tree-node leaf" data-path="/query/supplier_balance">
                                    <a href="/query/supplier_balance"><span class="node-icon">📈</span><span class="node-label">供应商往来对账</span></a>
                                </li>
                            </ul>
                        </li>
                        <li class="tree-node folder" data-path="query-sales" data-role="purchaser">
                            <div class="node-header" onclick="toggleNode(this)">
                                <span class="toggle-icon">▶</span>
                                <span class="node-icon">💵</span>
                                <span class="node-label">销售查询</span>
                            </div>
                            <ul class="tree-children">
                                <li class="tree-node leaf" data-path="/query/sales_order">
                                    <a href="/query/sales_order"><span class="node-icon">📋</span><span class="node-label">销售订单查询</span></a>
                                </li>
                                <li class="tree-node leaf" data-path="/query/sales_summary">
                                    <a href="/query/sales_summary"><span class="node-icon">📊</span><span class="node-label">销售汇总报表</span></a>
                                </li>
                                <li class="tree-node leaf" data-path="/query/sales_price">
                                    <a href="/query/sales_price"><span class="node-icon">💰</span><span class="node-label">销售价格查询</span></a>
                                </li>
                                <li class="tree-node leaf" data-path="/query/purchaser_balance">
                                    <a href="/query/purchaser_balance"><span class="node-icon">📈</span><span class="node-label">采购方应收对账</span></a>
                                </li>
                                <li class="tree-node leaf" data-path="/query/product_rank">
                                    <a href="/query/product_rank"><span class="node-icon">🏆</span><span class="node-label">畅销滞销商品</span></a>
                                </li>
                            </ul>
                        </li>
                        <li class="tree-node folder" data-path="query-stock" data-role="admin">
                            <div class="node-header" onclick="toggleNode(this)">
                                <span class="toggle-icon">▶</span>
                                <span class="node-icon">📦</span>
                                <span class="node-label">库存查询</span>
                            </div>
                            <ul class="tree-children">
                                <li class="tree-node leaf" data-path="/query/stock_balance">
                                    <a href="/query/stock_balance"><span class="node-icon">📊</span><span class="node-label">实时库存余额</span></a>
                                </li>
                                <li class="tree-node leaf" data-path="/query/stock_flow">
                                    <a href="/query/stock_flow"><span class="node-icon">📋</span><span class="node-label">库存明细台账</span></a>
                                </li>
                                <li class="tree-node leaf" data-path="/query/stock_warning">
                                    <a href="/query/stock_warning"><span class="node-icon">⚠️</span><span class="node-label">库存上下限预警</span></a>
                                </li>
                                <li class="tree-node leaf" data-path="/query/slow_stock">
                                    <a href="/query/slow_stock"><span class="node-icon">⏳</span><span class="node-label">呆滞库存查询</span></a>
                                </li>
                            </ul>
                        </li>
                        <li class="tree-node folder" data-path="query-finance" data-role="admin">
                            <div class="node-header" onclick="toggleNode(this)">
                                <span class="toggle-icon">▶</span>
                                <span class="node-icon">💰</span>
                                <span class="node-label">财务查询</span>
                            </div>
                            <ul class="tree-children">
                                <li class="tree-node leaf" data-path="/query/income_expense">
                                    <a href="/query/income_expense"><span class="node-icon">📈</span><span class="node-label">收支流水查询</span></a>
                                </li>
                                <li class="tree-node leaf" data-path="/query/profit_detail">
                                    <a href="/query/profit_detail"><span class="node-icon">📊</span><span class="node-label">毛利明细查询</span></a>
                                </li>
                            </ul>
                        </li>
                        <li class="tree-node folder" data-path="query-report" data-role="admin">
                            <div class="node-header" onclick="toggleNode(this)">
                                <span class="toggle-icon">▶</span>
                                <span class="node-icon">📈</span><span class="node-label">统计报表</span>
                            </div>
                            <ul class="tree-children">
                                <li class="tree-node leaf" data-path="/query/overview">
                                    <a href="/query/overview"><span class="node-icon">📋</span><span class="node-label">进销存汇总报表</span></a>
                                </li>
                                <li class="tree-node leaf" data-path="/query/category_stats">
                                    <a href="/query/category_stats"><span class="node-icon">📊</span><span class="node-label">品类进销存统计</span></a>
                                </li>
                                <li class="tree-node leaf" data-path="/query/document_summary">
                                    <a href="/query/document_summary"><span class="node-icon">📄</span><span class="node-label">单据汇总查询</span></a>
                                </li>
                            </ul>
                        </li>
                    </ul>
                </li>
                <li class="tree-node folder" data-path="system" data-role="super_admin">
                    <div class="node-header" onclick="toggleNode(this)">
                        <span class="toggle-icon">▶</span>
                        <span class="node-icon">⚙️</span>
                        <span class="node-label">系统设置</span>
                    </div>
                    <ul class="tree-children">
                        <li class="tree-node leaf" data-path="/user">
                            <a href="/user"><span class="node-icon">👥</span><span class="node-label">用户管理</span></a>
                        </li>
                        <li class="tree-node leaf" data-path="/system">
                            <a href="/system"><span class="node-icon">📋</span><span class="node-label">系统参数</span></a>
                        </li>
                        <li class="tree-node leaf" data-path="/backup">
                            <a href="/backup"><span class="node-icon">💾</span><span class="node-label">数据备份</span></a>
                        </li>
                        <li class="tree-node leaf" data-path="/restore">
                            <a href="/restore"><span class="node-icon">🔄</span><span class="node-label">数据恢复</span></a>
                        </li>
                    </ul>
                </li>
            </ul>
        </div>
    "#)
}

fn layout_html(title: &str, page: &str, content: &str) -> String {
    let sidebar = sidebar_html();
    let sidebar_with_active = sidebar
        .replace(&format!("data-path=\"{}\"", page), &format!("data-path=\"{}\" data-active=\"1\"", page));
    
    format!(r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>{}</title>
    <link rel="stylesheet" href="/static/bootstrap.min.css">
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; }}
        .app-container {{ display: flex; height: 100vh; overflow: hidden; }}
        .sidebar {{ width: 230px; background: linear-gradient(180deg, #1e3a8a 0%, #3b82f6 100%); color: white; display: flex; flex-direction: column; position: fixed; left: 0; top: 0; height: 100vh; z-index: 100; overflow-y: auto; }}
        .sidebar-header {{ padding: 18px 15px; text-align: center; border-bottom: 1px solid rgba(255,255,255,0.1); }}
        .logo {{ font-size: 32px; margin-bottom: 6px; }}
        .logo-icon {{ font-size: 32px; }}
        .logo-text {{ font-size: 15px; font-weight: bold; }}
        .sidebar-search {{ padding: 10px 12px; border-bottom: 1px solid rgba(255,255,255,0.1); }}
        .sidebar-search input {{ width: 100%; padding: 7px 10px; border-radius: 6px; border: 1px solid rgba(255,255,255,0.2); background: rgba(255,255,255,0.1); color: white; font-size: 13px; }}
        .sidebar-search input::placeholder {{ color: rgba(255,255,255,0.6); }}
        .sidebar-search input:focus {{ outline: none; background: rgba(255,255,255,0.2); }}
        .tree-menu {{ list-style: none; padding: 8px 0; flex: 1; overflow-y: auto; }}
        .tree-menu::-webkit-scrollbar {{ width: 6px; }}
        .tree-menu::-webkit-scrollbar-thumb {{ background: rgba(255,255,255,0.2); border-radius: 3px; }}
        .tree-node {{ position: relative; }}
        .tree-node.leaf a, .tree-node.folder .node-header {{ display: flex; align-items: center; padding: 9px 12px; color: rgba(255,255,255,0.9); text-decoration: none; border-radius: 6px; margin: 1px 8px; transition: all 0.2s; cursor: pointer; font-size: 14px; user-select: none; }}
        .tree-node.leaf a:hover, .tree-node.folder .node-header:hover {{ background: rgba(255,255,255,0.1); color: white; }}
        .tree-node.leaf[data-active="1"] > a {{ background: rgba(255,255,255,0.2); color: white; border-left: 3px solid #fbbf24; padding-left: 9px; font-weight: 600; }}
        .tree-node.folder.expanded > .node-header {{ background: rgba(0,0,0,0.15); color: white; }}
        .tree-node.folder.expanded > .node-header .toggle-icon {{ transform: rotate(90deg); }}
        .toggle-icon {{ display: inline-block; width: 14px; font-size: 10px; margin-right: 4px; transition: transform 0.2s; color: rgba(255,255,255,0.7); }}
        .node-icon {{ margin-right: 8px; font-size: 15px; }}
        .tree-children {{ list-style: none; max-height: 0; overflow: hidden; transition: max-height 0.25s ease-in-out; padding-left: 18px; }}
        .tree-node.folder.expanded > .tree-children {{ max-height: 1000px; }}
        .tree-children .tree-node.leaf a {{ font-size: 13px; padding: 7px 12px; }}
        .tree-children .tree-children {{ padding-left: 18px; }}
        .tree-node.category > .node-header {{ font-size: 13px; padding: 6px 12px; }}
        .tree-node.category > .node-header .node-icon {{ font-size: 13px; }}
        .context-menu {{ position: fixed; z-index: 999999; background: white; border: 1px solid #ddd; border-radius: 6px; box-shadow: 0 4px 12px rgba(0,0,0,0.15); min-width: 160px; padding: 4px 0; display: none; }}
        .context-menu .menu-item {{ padding: 8px 16px; cursor: pointer; font-size: 13px; color: #333; }}
        .context-menu .menu-item:hover {{ background: #f0f5ff; color: #1e40af; }}
        .context-menu .menu-separator {{ height: 1px; background: #eee; margin: 4px 0; }}
        .context-menu .menu-header {{ padding: 6px 16px; font-size: 12px; color: #888; border-bottom: 1px solid #eee; margin-bottom: 4px; }}
        .main-content {{ flex: 1; margin-left: 230px; display: flex; flex-direction: column; background: #f5f7fa; }}
        .top-header {{ background: white; padding: 15px 25px; box-shadow: 0 2px 4px rgba(0,0,0,0.05); display: flex; justify-content: space-between; align-items: center; }}
        .top-header h2 {{ margin: 0; font-size: 20px; color: #333; }}
        .top-header .header-right {{ display: flex; align-items: center; gap: 15px; }}
        .page-content {{ padding: 25px; overflow-y: auto; flex: 1; }}
        @media print {{
            .sidebar {{ display: none !important; }}
            .main-content {{ margin-left: 0 !important; }}
            .top-header {{ display: none !important; }}
        }}
        .search-dropdown {{
            position: absolute;
            top: 100%;
            left: 0;
            right: 0;
            z-index: 1000;
            background: white;
            border: 1px solid #ddd;
            border-radius: 4px;
            box-shadow: 0 4px 12px rgba(0,0,0,0.15);
            display: none;
            max-height: 300px;
            overflow-y: auto;
        }}
        .search-results {{
            list-style: none;
            padding: 0;
            margin: 0;
        }}
        .search-results li {{
            padding: 10px 12px;
            cursor: pointer;
            border-bottom: 1px solid #f0f0f0;
            font-size: 13px;
        }}
        .search-results li:hover {{
            background: #f0f5ff;
        }}
        .search-results li:last-child {{
            border-bottom: none;
        }}
        .search-results li small {{
            color: #888;
            font-size: 12px;
        }}
        .search-results li .text-muted {{
            color: #999;
        }}
    </style>
</head>
<body>
    <div class="app-container">
        {}
        <div class="main-content">
            <div class="top-header">
                <h2>{}</h2>
                <div class="header-right">
                    <span>{}</span>
                    <div class="user-info" id="userInfo" style="display:none;">
                        <span id="userNickname"></span>
                        <button class="btn btn-sm btn-outline-danger" onclick="logout()">退出</button>
                    </div>
                </div>
            </div>
            <div class="page-content">
                {}
            </div>
        </div>
    </div>
    <div class="context-menu" id="contextMenu"></div>
    <script>
        let currentUser = null;
        
        async function checkLogin() {{
            try {{
                const res = await fetch('/api/login/check', {{ method: 'GET' }});
                const data = await res.json();
                if (data.logged_in) {{
                    currentUser = data.user;
                    document.getElementById('userNickname').textContent = currentUser.nickname || currentUser.username;
                    document.getElementById('userInfo').style.display = 'flex';
                    filterMenuByRole(currentUser.role);
                }} else {{
                    window.location.href = '/login';
                }}
            }} catch (e) {{
                window.location.href = '/login';
            }}
        }}
        
        function filterMenuByRole(role) {{
            const rolePermissions = {{
                super_admin: ['admin', 'supplier', 'purchaser', 'query', 'super_admin'],
                admin: ['admin', 'supplier', 'purchaser', 'query'],
                supplier: ['supplier', 'query'],
                purchaser: ['purchaser', 'query'],
                anonymous: []
            }};
            
            const allowedRoles = rolePermissions[role] || [];
            const nodes = document.querySelectorAll('.tree-node[data-role]');
            
            nodes.forEach(node => {{
                const nodeRole = node.getAttribute('data-role');
                if (!allowedRoles.includes(nodeRole)) {{
                    node.style.display = 'none';
                }}
            }});
            
            document.querySelectorAll('.tree-node').forEach(node => {{
                const nodeRole = node.getAttribute('data-role');
                if (nodeRole && allowedRoles.includes(nodeRole)) {{
                    return;
                }}
                const children = node.querySelectorAll(':scope > ul.tree-children > .tree-node');
                const visibleChildren = Array.from(children).filter(c => c.style.display !== 'none');
                if (visibleChildren.length === 0 && children.length > 0) {{
                    node.style.display = 'none';
                }}
            }});
        }}
        
        async function logout() {{
            try {{
                await fetch('/api/logout', {{ method: 'GET' }});
                window.location.href = '/login';
            }} catch (e) {{
                window.location.href = '/login';
            }}
        }}
        
        checkLogin();
        
        function toggleNode(header) {{
            const node = header.parentElement;
            node.classList.toggle('expanded');
        }}
        function expandPathToActive() {{
            const active = document.querySelector('.tree-node.leaf[data-active="1"]');
            if (!active) return;
            let node = active.parentElement;
            while (node && node.id !== 'treeMenu') {{
                if (node.classList && node.classList.contains('folder')) {{
                    node.classList.add('expanded');
                }}
                node = node.parentElement;
            }}
        }}
        function filterTree() {{
            const q = document.getElementById('treeSearch').value.trim().toLowerCase();
            const allNodes = document.querySelectorAll('.tree-node');
            if (!q) {{
                allNodes.forEach(n => {{ n.style.display = ''; }});
                return;
            }}
            allNodes.forEach(n => {{
                const label = n.querySelector('.node-label');
                if (!label) {{ n.style.display = 'none'; return; }}
                const match = label.textContent.toLowerCase().includes(q);
                if (n.classList.contains('leaf')) {{
                    n.style.display = match ? '' : 'none';
                }}
            }});
            document.querySelectorAll('.tree-node.folder').forEach(folder => {{
                let hasVisible = false;
                folder.querySelectorAll('.tree-children .tree-node.leaf').forEach(leaf => {{
                    if (leaf.style.display !== 'none') hasVisible = true;
                }});
                folder.style.display = hasVisible ? '' : 'none';
                if (hasVisible) folder.classList.add('expanded');
            }});
        }}
        let currentCtxTarget = null;
        
        function hideContextMenu() {{
            const menu = document.getElementById('contextMenu');
            if (menu) menu.style.display = 'none';
            currentCtxTarget = null;
        }}
        
        document.addEventListener('click', hideContextMenu);
        
        function renderCategoryTree(children, parentUl) {{
            if (!children || children.length === 0) return;
            children.forEach(function(cat) {{
                const hasChildren = cat.children && cat.children.length > 0;
                const li = document.createElement('li');
                li.className = 'tree-node category folder';
                li.setAttribute('data-cat-id', cat.id);
                li.setAttribute('data-cat-name', cat.name);
                li.setAttribute('data-path', '/product/cat/' + cat.id);
                
                const header = document.createElement('div');
                header.className = 'node-header';
                header.onclick = function(e) {{ e.stopPropagation(); toggleNode(this); filterProductsByCategory(cat.id, cat.name); }};
                header.oncontextmenu = function(e) {{ e.preventDefault(); e.stopPropagation(); showCategoryContextMenu(e, cat); }};
                
                const toggle = document.createElement('span');
                toggle.className = 'toggle-icon';
                toggle.textContent = hasChildren ? '▶' : '•';
                header.appendChild(toggle);
                
                const icon = document.createElement('span');
                icon.className = 'node-icon';
                icon.textContent = '📂';
                header.appendChild(icon);
                
                const label = document.createElement('span');
                label.className = 'node-label';
                label.textContent = cat.name;
                header.appendChild(label);
                
                li.appendChild(header);
                
                if (hasChildren) {{
                    const ul = document.createElement('ul');
                    ul.className = 'tree-children';
                    renderCategoryTree(cat.children, ul);
                    li.appendChild(ul);
                }}
                
                parentUl.appendChild(li);
            }});
        }}
        
        async function loadProductCategories() {{
            try {{
                const res = await fetch('/api/category/tree?entity_type=product');
                const data = await res.json();
                const container = document.getElementById('productCatTree');
                if (!container) return;
                const existing = container.querySelectorAll('.tree-node.category');
                existing.forEach(function(el) {{ el.remove(); }});
                renderCategoryTree(data, container);
            }} catch(e) {{
                console.error('加载分类失败:', e);
            }}
        }}
        
        function showProductRootContextMenu(e) {{
            e.preventDefault();
            e.stopPropagation();
            currentCtxTarget = {{ type: 'root', entityType: 'product' }};
            const menu = document.getElementById('contextMenu');
            menu.innerHTML = `
                <div class="menu-header">📦 商品分类管理</div>
                <div class="menu-item" onclick="ctxAddRootCategory()">➕ 新增顶级分类</div>
                <div class="menu-separator"></div>
                <div class="menu-item" onclick="ctxRefreshCategoryTree()">🔄 刷新分类树</div>
            `;
            menu.style.display = 'block';
            menu.style.left = Math.min(e.clientX, window.innerWidth - 180) + 'px';
            menu.style.top = Math.min(e.clientY, window.innerHeight - 120) + 'px';
        }}
        
        function showCategoryContextMenu(e, cat) {{
            e.preventDefault();
            e.stopPropagation();
            currentCtxTarget = {{ type: 'category', id: cat.id, name: cat.name, parentId: cat.parent_id, entityType: 'product' }};
            const menu = document.getElementById('contextMenu');
            menu.innerHTML = `
                <div class="menu-header">📂 ${{escapeHtml(cat.name)}}</div>
                <div class="menu-item" onclick="ctxAddSubCategory()">➕ 新增子分类</div>
                <div class="menu-item" onclick="ctxAddSiblingCategory()">➕ 新增同级分类</div>
                <div class="menu-separator"></div>
                <div class="menu-item" onclick="ctxRenameCategory()">✏️ 重命名</div>
                <div class="menu-item" onclick="ctxDeleteCategory()">🗑️ 删除</div>
            `;
            menu.style.display = 'block';
            menu.style.left = Math.min(e.clientX, window.innerWidth - 180) + 'px';
            menu.style.top = Math.min(e.clientY, window.innerHeight - 160) + 'px';
        }}
        
        function escapeHtml(text) {{
            const div = document.createElement('div');
            div.textContent = text;
            return div.innerHTML;
        }}
        
        async function ctxAddRootCategory() {{
            if (!currentCtxTarget) return;
            const name = prompt('请输入新的顶级分类名称：');
            if (!name) return;
            try {{
                const res = await fetch('/api/category/create', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ name: name, parent_id: null, entity_type: 'product', sort_order: 0 }})
                }});
                if (res.ok) {{
                    hideContextMenu();
                    loadProductCategories();
                }} else {{
                    alert('创建失败');
                }}
            }} catch(e) {{ alert('创建失败: ' + e.message); }}
        }}
        
        async function ctxAddSubCategory() {{
            if (!currentCtxTarget || currentCtxTarget.type !== 'category') return;
            const name = prompt('请输入新的子分类名称：');
            if (!name) return;
            try {{
                const res = await fetch('/api/category/create', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ name: name, parent_id: currentCtxTarget.id, entity_type: 'product', sort_order: 0 }})
                }});
                if (res.ok) {{
                    hideContextMenu();
                    loadProductCategories();
                }} else {{
                    alert('创建失败');
                }}
            }} catch(e) {{ alert('创建失败: ' + e.message); }}
        }}
        
        async function ctxAddSiblingCategory() {{
            if (!currentCtxTarget || currentCtxTarget.type !== 'category') return;
            const name = prompt('请输入新的同级分类名称：');
            if (!name) return;
            try {{
                const res = await fetch('/api/category/create', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ name: name, parent_id: currentCtxTarget.parentId, entity_type: 'product', sort_order: 0 }})
                }});
                if (res.ok) {{
                    hideContextMenu();
                    loadProductCategories();
                }} else {{
                    alert('创建失败');
                }}
            }} catch(e) {{ alert('创建失败: ' + e.message); }}
        }}
        
        async function ctxRenameCategory() {{
            if (!currentCtxTarget || currentCtxTarget.type !== 'category') return;
            const newName = prompt('请输入新的分类名称：', currentCtxTarget.name);
            if (!newName || newName === currentCtxTarget.name) return;
            try {{
                const res = await fetch('/api/category/rename', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ id: currentCtxTarget.id, name: newName }})
                }});
                if (res.ok) {{
                    hideContextMenu();
                    loadProductCategories();
                }} else {{
                    alert('重命名失败');
                }}
            }} catch(e) {{ alert('重命名失败: ' + e.message); }}
        }}
        
        async function ctxDeleteCategory() {{
            if (!currentCtxTarget || currentCtxTarget.type !== 'category') return;
            if (!confirm('确定要删除分类"' + currentCtxTarget.name + '"吗？\\n注意：有子分类或已被引用的分类无法删除。')) return;
            try {{
                const res = await fetch('/api/category/delete', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ id: currentCtxTarget.id }})
                }});
                const text = await res.text();
                if (res.ok) {{
                    hideContextMenu();
                    loadProductCategories();
                }} else {{
                    alert(text || '删除失败');
                }}
            }} catch(e) {{ alert('删除失败: ' + e.message); }}
        }}
        
        function ctxRefreshCategoryTree() {{
            hideContextMenu();
            loadProductCategories();
        }}
        
        function filterProductsByCategory(catId, catName) {{
            if (typeof loadProductsByCategory === 'function') {{
                if (typeof setCurrentCategory === 'function') {{
                    setCurrentCategory(catId, catName);
                }}
                loadProductsByCategory(catId);
            }} else {{
                let url = '/product';
                if (catId) {{
                    url += '?category_id=' + catId;
                }}
                window.location.href = url;
            }}
        }}
        
        function renderSupplierCategoryTree(children, parentUl) {{
            if (!children || children.length === 0) return;
            children.forEach(function(cat) {{
                const hasChildren = cat.children && cat.children.length > 0;
                const li = document.createElement('li');
                li.className = 'tree-node category folder';
                li.setAttribute('data-cat-id', cat.id);
                li.setAttribute('data-cat-name', cat.name);
                li.setAttribute('data-path', '/supplier/cat/' + cat.id);
                
                const header = document.createElement('div');
                header.className = 'node-header';
                header.onclick = function(e) {{ e.stopPropagation(); toggleNode(this); filterSuppliersByCategory(cat.id, cat.name); }};
                header.oncontextmenu = function(e) {{ e.preventDefault(); e.stopPropagation(); showSupplierCategoryContextMenu(e, cat); }};
                
                const toggle = document.createElement('span');
                toggle.className = 'toggle-icon';
                toggle.textContent = hasChildren ? '▶' : '•';
                header.appendChild(toggle);
                
                const icon = document.createElement('span');
                icon.className = 'node-icon';
                icon.textContent = '📂';
                header.appendChild(icon);
                
                const label = document.createElement('span');
                label.className = 'node-label';
                label.textContent = cat.name;
                header.appendChild(label);
                
                li.appendChild(header);
                
                if (hasChildren) {{
                    const ul = document.createElement('ul');
                    ul.className = 'tree-children';
                    renderSupplierCategoryTree(cat.children, ul);
                    li.appendChild(ul);
                }}
                
                parentUl.appendChild(li);
            }});
        }}
        
        async function loadSupplierCategories() {{
            try {{
                const res = await fetch('/api/category/tree?entity_type=supplier');
                const data = await res.json();
                const container = document.getElementById('supplierCatTree');
                if (!container) return;
                const existing = container.querySelectorAll('.tree-node.category');
                existing.forEach(function(el) {{ el.remove(); }});
                renderSupplierCategoryTree(data, container);
            }} catch(e) {{
                console.error('加载供应商分类失败:', e);
            }}
        }}
        
        function showSupplierRootContextMenu(e) {{
            e.preventDefault();
            e.stopPropagation();
            currentCtxTarget = {{ type: 'root', entityType: 'supplier' }};
            const menu = document.getElementById('contextMenu');
            menu.innerHTML = `
                <div class="menu-header">🏭 供应商分类管理</div>
                <div class="menu-item" onclick="ctxAddSupplierRootCategory()">➕ 新增顶级分类</div>
                <div class="menu-separator"></div>
                <div class="menu-item" onclick="ctxRefreshSupplierCategoryTree()">🔄 刷新分类树</div>
            `;
            menu.style.display = 'block';
            menu.style.left = Math.min(e.clientX, window.innerWidth - 180) + 'px';
            menu.style.top = Math.min(e.clientY, window.innerHeight - 120) + 'px';
        }}
        
        function showSupplierCategoryContextMenu(e, cat) {{
            e.preventDefault();
            e.stopPropagation();
            currentCtxTarget = {{ type: 'category', id: cat.id, name: cat.name, parentId: cat.parent_id, entityType: 'supplier' }};
            const menu = document.getElementById('contextMenu');
            menu.innerHTML = `
                <div class="menu-header">📂 ${{escapeHtml(cat.name)}}</div>
                <div class="menu-item" onclick="ctxAddSupplierSubCategory()">➕ 新增子分类</div>
                <div class="menu-item" onclick="ctxAddSupplierSiblingCategory()">➕ 新增同级分类</div>
                <div class="menu-separator"></div>
                <div class="menu-item" onclick="ctxRenameSupplierCategory()">✏️ 重命名</div>
                <div class="menu-item" onclick="ctxDeleteSupplierCategory()">🗑️ 删除</div>
            `;
            menu.style.display = 'block';
            menu.style.left = Math.min(e.clientX, window.innerWidth - 180) + 'px';
            menu.style.top = Math.min(e.clientY, window.innerHeight - 160) + 'px';
        }}
        
        async function ctxAddSupplierRootCategory() {{
            if (!currentCtxTarget) return;
            const name = prompt('请输入新的顶级分类名称：');
            if (!name) return;
            try {{
                const res = await fetch('/api/category/create', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ name: name, parent_id: null, entity_type: 'supplier', sort_order: 0 }})
                }});
                if (res.ok) {{
                    hideContextMenu();
                    loadSupplierCategories();
                }} else {{
                    alert('创建失败');
                }}
            }} catch(e) {{ alert('创建失败: ' + e.message); }}
        }}
        
        async function ctxAddSupplierSubCategory() {{
            if (!currentCtxTarget || currentCtxTarget.type !== 'category') return;
            const name = prompt('请输入新的子分类名称：');
            if (!name) return;
            try {{
                const res = await fetch('/api/category/create', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ name: name, parent_id: currentCtxTarget.id, entity_type: 'supplier', sort_order: 0 }})
                }});
                if (res.ok) {{
                    hideContextMenu();
                    loadSupplierCategories();
                }} else {{
                    alert('创建失败');
                }}
            }} catch(e) {{ alert('创建失败: ' + e.message); }}
        }}
        
        async function ctxAddSupplierSiblingCategory() {{
            if (!currentCtxTarget || currentCtxTarget.type !== 'category') return;
            const name = prompt('请输入新的同级分类名称：');
            if (!name) return;
            try {{
                const res = await fetch('/api/category/create', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ name: name, parent_id: currentCtxTarget.parentId, entity_type: 'supplier', sort_order: 0 }})
                }});
                if (res.ok) {{
                    hideContextMenu();
                    loadSupplierCategories();
                }} else {{
                    alert('创建失败');
                }}
            }} catch(e) {{ alert('创建失败: ' + e.message); }}
        }}
        
        async function ctxRenameSupplierCategory() {{
            if (!currentCtxTarget || currentCtxTarget.type !== 'category') return;
            const newName = prompt('请输入新的分类名称：', currentCtxTarget.name);
            if (!newName || newName === currentCtxTarget.name) return;
            try {{
                const res = await fetch('/api/category/rename', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ id: currentCtxTarget.id, name: newName }})
                }});
                if (res.ok) {{
                    hideContextMenu();
                    loadSupplierCategories();
                }} else {{
                    alert('重命名失败');
                }}
            }} catch(e) {{ alert('重命名失败: ' + e.message); }}
        }}
        
        async function ctxDeleteSupplierCategory() {{
            if (!currentCtxTarget || currentCtxTarget.type !== 'category') return;
            if (!confirm('确定要删除分类"' + currentCtxTarget.name + '"吗？\\n注意：有子分类或已被引用的分类无法删除。')) return;
            try {{
                const res = await fetch('/api/category/delete', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ id: currentCtxTarget.id }})
                }});
                const text = await res.text();
                if (res.ok) {{
                    hideContextMenu();
                    loadSupplierCategories();
                }} else {{
                    alert(text || '删除失败');
                }}
            }} catch(e) {{ alert('删除失败: ' + e.message); }}
        }}
        
        function ctxRefreshSupplierCategoryTree() {{
            hideContextMenu();
            loadSupplierCategories();
        }}
        
        function filterSuppliersByCategory(catId, catName) {{
            if (typeof loadSuppliersByCategory === 'function') {{
                if (typeof setCurrentCategory === 'function') {{
                    setCurrentCategory(catId, catName);
                }}
                loadSuppliersByCategory(catId);
            }} else {{
                let url = '/supplier';
                if (catId) {{
                    url += '?category_id=' + catId;
                }}
                window.location.href = url;
            }}
        }}
        
        function renderPurchaserCategoryTree(children, parentUl) {{
            if (!children || children.length === 0) return;
            children.forEach(function(cat) {{
                const hasChildren = cat.children && cat.children.length > 0;
                const li = document.createElement('li');
                li.className = 'tree-node category folder';
                li.setAttribute('data-cat-id', cat.id);
                li.setAttribute('data-cat-name', cat.name);
                li.setAttribute('data-path', '/purchaser/cat/' + cat.id);
                
                const header = document.createElement('div');
                header.className = 'node-header';
                header.onclick = function(e) {{ e.stopPropagation(); toggleNode(this); filterPurchasersByCategory(cat.id, cat.name); }};
                header.oncontextmenu = function(e) {{ e.preventDefault(); e.stopPropagation(); showPurchaserCategoryContextMenu(e, cat); }};
                
                const toggle = document.createElement('span');
                toggle.className = 'toggle-icon';
                toggle.textContent = hasChildren ? '▶' : '•';
                header.appendChild(toggle);
                
                const icon = document.createElement('span');
                icon.className = 'node-icon';
                icon.textContent = '📂';
                header.appendChild(icon);
                
                const label = document.createElement('span');
                label.className = 'node-label';
                label.textContent = cat.name;
                header.appendChild(label);
                
                li.appendChild(header);
                
                if (hasChildren) {{
                    const ul = document.createElement('ul');
                    ul.className = 'tree-children';
                    renderPurchaserCategoryTree(cat.children, ul);
                    li.appendChild(ul);
                }}
                
                parentUl.appendChild(li);
            }});
        }}
        
        async function loadPurchaserCategories() {{
            try {{
                const res = await fetch('/api/category/tree?entity_type=purchaser');
                const data = await res.json();
                const container = document.getElementById('purchaserCatTree');
                if (!container) return;
                const existing = container.querySelectorAll('.tree-node.category');
                existing.forEach(function(el) {{ el.remove(); }});
                renderPurchaserCategoryTree(data, container);
            }} catch(e) {{
                console.error('加载采购方分类失败:', e);
            }}
        }}
        
        function showPurchaserRootContextMenu(e) {{
            e.preventDefault();
            e.stopPropagation();
            currentCtxTarget = {{ type: 'root', entityType: 'purchaser' }};
            const menu = document.getElementById('contextMenu');
            menu.innerHTML = `
                <div class="menu-header">🏢 采购方分类管理</div>
                <div class="menu-item" onclick="ctxAddPurchaserRootCategory()">➕ 新增顶级分类</div>
                <div class="menu-separator"></div>
                <div class="menu-item" onclick="ctxRefreshPurchaserCategoryTree()">🔄 刷新分类树</div>
            `;
            menu.style.display = 'block';
            menu.style.left = Math.min(e.clientX, window.innerWidth - 180) + 'px';
            menu.style.top = Math.min(e.clientY, window.innerHeight - 120) + 'px';
        }}
        
        function showPurchaserCategoryContextMenu(e, cat) {{
            e.preventDefault();
            e.stopPropagation();
            currentCtxTarget = {{ type: 'category', id: cat.id, name: cat.name, parentId: cat.parent_id, entityType: 'purchaser' }};
            const menu = document.getElementById('contextMenu');
            menu.innerHTML = `
                <div class="menu-header">📂 ${{escapeHtml(cat.name)}}</div>
                <div class="menu-item" onclick="ctxAddPurchaserSubCategory()">➕ 新增子分类</div>
                <div class="menu-item" onclick="ctxAddPurchaserSiblingCategory()">➕ 新增同级分类</div>
                <div class="menu-separator"></div>
                <div class="menu-item" onclick="ctxRenamePurchaserCategory()">✏️ 重命名</div>
                <div class="menu-item" onclick="ctxDeletePurchaserCategory()">🗑️ 删除</div>
            `;
            menu.style.display = 'block';
            menu.style.left = Math.min(e.clientX, window.innerWidth - 180) + 'px';
            menu.style.top = Math.min(e.clientY, window.innerHeight - 160) + 'px';
        }}
        
        async function ctxAddPurchaserRootCategory() {{
            if (!currentCtxTarget) return;
            const name = prompt('请输入新的顶级分类名称：');
            if (!name) return;
            try {{
                const res = await fetch('/api/category/create', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ name: name, parent_id: null, entity_type: 'purchaser', sort_order: 0 }})
                }});
                if (res.ok) {{
                    hideContextMenu();
                    loadPurchaserCategories();
                }} else {{
                    alert('创建失败');
                }}
            }} catch(e) {{ alert('创建失败: ' + e.message); }}
        }}
        
        async function ctxAddPurchaserSubCategory() {{
            if (!currentCtxTarget || currentCtxTarget.type !== 'category') return;
            const name = prompt('请输入新的子分类名称：');
            if (!name) return;
            try {{
                const res = await fetch('/api/category/create', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ name: name, parent_id: currentCtxTarget.id, entity_type: 'purchaser', sort_order: 0 }})
                }});
                if (res.ok) {{
                    hideContextMenu();
                    loadPurchaserCategories();
                }} else {{
                    alert('创建失败');
                }}
            }} catch(e) {{ alert('创建失败: ' + e.message); }}
        }}
        
        async function ctxAddPurchaserSiblingCategory() {{
            if (!currentCtxTarget || currentCtxTarget.type !== 'category') return;
            const name = prompt('请输入新的同级分类名称：');
            if (!name) return;
            try {{
                const res = await fetch('/api/category/create', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ name: name, parent_id: currentCtxTarget.parentId, entity_type: 'purchaser', sort_order: 0 }})
                }});
                if (res.ok) {{
                    hideContextMenu();
                    loadPurchaserCategories();
                }} else {{
                    alert('创建失败');
                }}
            }} catch(e) {{ alert('创建失败: ' + e.message); }}
        }}
        
        async function ctxRenamePurchaserCategory() {{
            if (!currentCtxTarget || currentCtxTarget.type !== 'category') return;
            const newName = prompt('请输入新的分类名称：', currentCtxTarget.name);
            if (!newName || newName === currentCtxTarget.name) return;
            try {{
                const res = await fetch('/api/category/rename', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ id: currentCtxTarget.id, name: newName }})
                }});
                if (res.ok) {{
                    hideContextMenu();
                    loadPurchaserCategories();
                }} else {{
                    alert('重命名失败');
                }}
            }} catch(e) {{ alert('重命名失败: ' + e.message); }}
        }}
        
        async function ctxDeletePurchaserCategory() {{
            if (!currentCtxTarget || currentCtxTarget.type !== 'category') return;
            if (!confirm('确定要删除分类"' + currentCtxTarget.name + '"吗？\\n注意：有子分类或已被引用的分类无法删除。')) return;
            try {{
                const res = await fetch('/api/category/delete', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ id: currentCtxTarget.id }})
                }});
                const text = await res.text();
                if (res.ok) {{
                    hideContextMenu();
                    loadPurchaserCategories();
                }} else {{
                    alert(text || '删除失败');
                }}
            }} catch(e) {{ alert('删除失败: ' + e.message); }}
        }}
        
        function ctxRefreshPurchaserCategoryTree() {{
            hideContextMenu();
            loadPurchaserCategories();
        }}
        
        function filterPurchasersByCategory(catId, catName) {{
            if (typeof loadPurchasersByCategory === 'function') {{
                if (typeof setCurrentCategory === 'function') {{
                    setCurrentCategory(catId, catName);
                }}
                loadPurchasersByCategory(catId);
            }} else {{
                let url = '/purchaser';
                if (catId) {{
                    url += '?category_id=' + catId;
                }}
                window.location.href = url;
            }}
        }}
        
        loadProductCategories();
        loadSupplierCategories();
        loadPurchaserCategories();
        expandPathToActive();
    </script>
    <script src="/static/bootstrap.bundle.min.js"></script>
</body>
</html>
    "#, title, sidebar_with_active, title, Local::now().format("%Y-%m-%d %H:%M"), content)
}

async fn page_index(headers: axum::http::HeaderMap) -> Html<String> {
    match check_page_permission(&headers, "/").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    
    let content = format!(r#"
        <div class="row row-cols-1 row-cols-md-2 row-cols-lg-3 g-4">
            <div class="col">
                <div class="card bg-primary text-white">
                    <div class="card-body">
                        <h5 class="card-title">供应商管理</h5>
                        <p class="card-text">管理供应商信息</p>
                        <a href="/supplier" class="btn btn-light">进入</a>
                    </div>
                </div>
            </div>
            <div class="col">
                <div class="card bg-success text-white">
                    <div class="card-body">
                        <h5 class="card-title">采购方管理</h5>
                        <p class="card-text">管理采购单位信息</p>
                        <a href="/purchaser" class="btn btn-light">进入</a>
                    </div>
                </div>
            </div>
            <div class="col">
                <div class="card bg-info text-white">
                    <div class="card-body">
                        <h5 class="card-title">商品管理</h5>
                        <p class="card-text">管理食材商品信息</p>
                        <a href="/product" class="btn btn-light">进入</a>
                    </div>
                </div>
            </div>
            <div class="col">
                <div class="card bg-warning text-white">
                    <div class="card-body">
                        <h5 class="card-title">库存管理</h5>
                        <p class="card-text">查看和管理库存</p>
                        <a href="/inventory" class="btn btn-light">进入</a>
                    </div>
                </div>
            </div>
            <div class="col">
                <div class="card bg-danger text-white">
                    <div class="card-body">
                        <h5 class="card-title">采购订单</h5>
                        <p class="card-text">创建和管理采购订单</p>
                        <a href="/purchase" class="btn btn-light">进入</a>
                    </div>
                </div>
            </div>
            <div class="col">
                <div class="card bg-secondary text-white">
                    <div class="card-body">
                        <h5 class="card-title">销售订单</h5>
                        <p class="card-text">创建和管理销售订单</p>
                        <a href="/sales" class="btn btn-light">进入</a>
                    </div>
                </div>
            </div>
            <div class="col">
                <div class="card" style="background-color: #059669; color: white;">
                    <div class="card-body">
                        <h5 class="card-title">采购分拣</h5>
                        <p class="card-text">统筹汇总所有采购需求</p>
                        <a href="/mobile/sort" class="btn btn-light">进入</a>
                    </div>
                </div>
            </div>
            <div class="col">
                <div class="card" style="background-color: #f59e0b; color: white;">
                    <div class="card-body">
                        <h5 class="card-title">按单位分拣</h5>
                        <p class="card-text">按采购单位分组采购</p>
                        <a href="/mobile/sort_by_purchaser" class="btn btn-light">进入</a>
                    </div>
                </div>
            </div>
            <div class="col">
                <div class="card" style="background-color: #7c3aed; color: white;">
                    <div class="card-body">
                        <h5 class="card-title">按分类分拣</h5>
                        <p class="card-text">按商品分类汇总采购</p>
                        <a href="/mobile/sort_by_category" class="btn btn-light">进入</a>
                    </div>
                </div>
            </div>
            <div class="col">
                <div class="card" style="background-color: #10b981; color: white;">
                    <div class="card-body">
                        <h5 class="card-title">按供应商分拣</h5>
                        <p class="card-text">按供应商分组汇总采购</p>
                        <a href="/mobile/sort_by_supplier" class="btn btn-light">进入</a>
                    </div>
                </div>
            </div>
            <div class="col">
                <div class="card" style="background-color: #06b6d4; color: white;">
                    <div class="card-body">
                        <h5 class="card-title">综合分拣</h5>
                        <p class="card-text">按采购单位+分类汇总</p>
                        <a href="/mobile/sort_comprehensive" class="btn btn-light">进入</a>
                    </div>
                </div>
            </div>
        </div>
    "#);
    
    Html(layout_html("进销存管理系统", "/", &content))
}

async fn page_supplier(headers: axum::http::HeaderMap) -> Html<String> {
    match check_page_permission(&headers, "/supplier").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    
    let cat_rows = sqlx::query("SELECT id, name FROM category WHERE entity_type='supplier' ORDER BY sort_order, id")
        .fetch_all(pool())
        .await
        .unwrap_or_default();

    let mut category_options = String::from("<option value=\"\">无分类</option>");
    for row in &cat_rows {
        category_options.push_str(&format!(
            "<option value=\"{0}\">{1}</option>",
            row.get::<i64, _>("id"),
            row.get::<String, _>("name"),
        ));
    }

    let content = format!(r#"
        <div class="card mb-4">
            <div class="card-body">
                <h4>新增供应商</h4>
                <form onsubmit="createSupplier(event)">
                    <div class="row g-2">
                        <div class="col-md-3">
                            <input type="text" name="name" placeholder="供应商名称" class="form-control" required>
                        </div>
                        <div class="col-md-2">
                            <input type="text" name="contact" placeholder="联系人" class="form-control">
                        </div>
                        <div class="col-md-2">
                            <input type="text" name="phone" placeholder="电话" class="form-control">
                        </div>
                        <div class="col-md-3">
                            <input type="text" name="address" placeholder="地址" class="form-control">
                        </div>
                        <div class="col-md-2">
                            <select name="category_id" class="form-control">{0}</select>
                        </div>
                        <div class="col-md-4">
                            <input type="text" name="business_scope" placeholder="经营范围" class="form-control">
                        </div>
                        <div class="col-md-4">
                            <input type="text" name="remark" placeholder="备注" class="form-control">
                        </div>
                        <div class="col-md-2">
                            <button type="submit" class="btn btn-primary">新增</button>
                        </div>
                    </div>
                </form>
            </div>
        </div>

        <div class="d-flex justify-content-between align-items-center mb-3">
            <h5 id="supplierListTitle">全部供应商</h5>
            <div class="d-flex gap-2 align-items-center">
                <input type="text" id="searchKeyword" placeholder="搜索供应商名称" class="form-control form-control-sm" style="width:200px" onkeydown="if(event.key==='Enter')searchSuppliers()">
                <button class="btn btn-sm btn-outline-primary" onclick="searchSuppliers()">搜索</button>
                <button class="btn btn-sm btn-outline-secondary" onclick="resetSearch()">显示全部</button>
                <a href="/api/supplier/export" class="btn btn-sm btn-success">导出</a>
                <button class="btn btn-sm btn-warning" onclick="importSuppliers()">导入</button>
                <input type="file" id="supplierFileInput" style="display:none" accept=".xlsx,.csv" onchange="handleSupplierFile(this)">
            </div>
        </div>

        <table class="table table-bordered table-sm">
            <thead><tr><th>ID</th><th>名称</th><th>联系人</th><th>电话</th><th>地址</th><th>经营范围</th><th>备注</th><th>分类</th><th style="width:140px">操作</th></tr></thead>
            <tbody id="supplierTableBody">
                <tr><td colspan="10" class="text-center text-muted">加载中...</td></tr>
            </tbody>
        </table>

        <div class="modal fade" id="editSupplierModal" tabindex="-1">
            <div class="modal-dialog">
                <div class="modal-content">
                    <div class="modal-header">
                        <h5 class="modal-title">编辑供应商</h5>
                        <button type="button" class="btn-close" data-bs-dismiss="modal"></button>
                    </div>
                    <div class="modal-body">
                        <form id="editForm">
                            <input type="hidden" name="id">
                            <div class="mb-3"><label class="form-label">供应商名称</label><input type="text" name="name" class="form-control" required></div>
                            <div class="mb-3"><label class="form-label">联系人</label><input type="text" name="contact" class="form-control"></div>
                            <div class="mb-3"><label class="form-label">电话</label><input type="text" name="phone" class="form-control"></div>
                            <div class="mb-3"><label class="form-label">地址</label><input type="text" name="address" class="form-control"></div>
                            <div class="mb-3"><label class="form-label">经营范围</label><textarea name="business_scope" class="form-control" rows="2"></textarea></div>
                            <div class="mb-3"><label class="form-label">备注</label><textarea name="remark" class="form-control" rows="2"></textarea></div>
                            <div class="mb-3"><label class="form-label">分类</label><select name="category_id" class="form-control">{0}</select></div>
                        </form>
                    </div>
                    <div class="modal-footer">
                        <button type="button" class="btn btn-secondary" data-bs-dismiss="modal">取消</button>
                        <button type="button" class="btn btn-primary" onclick="submitEdit()">保存</button>
                    </div>
                </div>
            </div>
        </div>
        <script>
            let currentCategoryId=null,currentCategoryName='全部供应商',currentKeyword='',allSuppliers=[];
            async function loadSuppliersByCategory(categoryId){{
                currentCategoryId=categoryId;
                let params=[];
                if(categoryId){{params.push('category_id='+categoryId);}}
                if(currentKeyword){{params.push('keyword='+encodeURIComponent(currentKeyword));}}
                let url='/api/supplier/list';
                if(params.length>0){{url+='?'+params.join('&');}}
                try{{
                    const res=await fetch(url);
                    const suppliers=await res.json();
                    renderSupplierTable(suppliers);
                    updateCategoryTitle(categoryId);
                    setFormCategory(categoryId);
                }}catch(e){{console.error('加载供应商失败:',e);}}
            }}
            function renderSupplierTable(suppliers){{
                allSuppliers=suppliers||[];
                const tbody=document.getElementById('supplierTableBody');
                if(!suppliers||suppliers.length===0){{
                    tbody.innerHTML='<tr><td colspan="9" class="text-center text-muted">暂无供应商数据</td></tr>';
                    return;
                }}
                let html='';
                suppliers.forEach(function(p){{
                    html+='<tr><td>'+p.id+'</td><td>'+escapeHtml(p.name)+'</td><td>'+escapeHtml(p.contact||'')+'</td><td>'+escapeHtml(p.phone||'')+'</td><td>'+escapeHtml(p.address||'')+'</td><td title="'+escapeHtml(p.business_scope||'')+'">'+escapeHtml(truncateText(p.business_scope||'',20))+'</td><td title="'+escapeHtml(p.remark||'')+'">'+escapeHtml(truncateText(p.remark||'',20))+'</td><td>'+escapeHtml(p.category_name||'无分类')+'</td>';
                    html+='<td><button class="btn btn-sm btn-outline-primary me-1" onclick="editSupplier('+p.id+')">编辑</button><button class="btn btn-sm btn-outline-danger" onclick="deleteSupplier('+p.id+')">删除</button></td></tr>';
                }});
                tbody.innerHTML=html;
            }}
            function truncateText(text,maxLen){{
                if(!text)return '';
                return text.length>maxLen?text.substring(0,maxLen)+'...':text;
            }}
            function searchSuppliers(){{
                currentKeyword=document.getElementById('searchKeyword').value.trim();
                loadSuppliersByCategory(currentCategoryId);
            }}
            function resetSearch(){{
                document.getElementById('searchKeyword').value='';
                currentKeyword='';
                currentCategoryId=null;
                loadSuppliersByCategory(null);
            }}
            function editSupplier(id){{
                const p=allSuppliers.find(x=>x.id===id);
                if(!p)return;
                const form=document.getElementById('editForm');
                form.id.value=p.id;
                form.name.value=p.name||'';
                form.contact.value=p.contact||'';
                form.phone.value=p.phone||'';
                form.address.value=p.address||'';
                form.business_scope.value=p.business_scope||'';
                form.remark.value=p.remark||'';
                form.category_id.value=p.category_id||'';
                const modal=new bootstrap.Modal(document.getElementById('editSupplierModal'));
                modal.show();
            }}
            async function submitEdit(){{
                const form=document.getElementById('editForm');
                const data={{
                    id:parseInt(form.id.value),
                    name:form.name.value,
                    contact:form.contact.value||null,
                    phone:form.phone.value||null,
                    address:form.address.value||null,
                    business_scope:form.business_scope.value||null,
                    remark:form.remark.value||null,
                    category_id:form.category_id.value?parseInt(form.category_id.value):null
                }};
                const res=await fetch('/api/supplier/update',{{method:'POST',headers:{{'Content-Type':'application/json'}},body:JSON.stringify(data)}});
                if(res.ok){{bootstrap.Modal.getInstance(document.getElementById('editSupplierModal')).hide();loadSuppliersByCategory(currentCategoryId);}}
            }}
            async function deleteSupplier(id){{
                const p=allSuppliers.find(x=>x.id===id);
                const name=p?p.name:'';
                if(!confirm('确定要删除供应商「'+name+'」吗？'))return;
                const res=await fetch('/api/supplier/delete',{{method:'POST',headers:{{'Content-Type':'application/json'}},body:JSON.stringify({{id:id}})}});
                if(res.ok){{loadSuppliersByCategory(currentCategoryId);}}
            }}
            function updateCategoryTitle(categoryId){{
                const title=document.getElementById('supplierListTitle');
                if(categoryId){{title.textContent='分类供应商 - '+currentCategoryName;}}else{{title.textContent='全部供应商';currentCategoryName='全部供应商';}}
            }}
            function setCurrentCategory(catId,catName){{currentCategoryId=catId;currentCategoryName=catName||'全部供应商';}}
            function setFormCategory(categoryId){{
                const select=document.querySelector('form[onsubmit="createSupplier(event)"] select[name="category_id"]');
                if(select){{select.value=categoryId?categoryId:'';}}
            }}
            async function createSupplier(e){{
                e.preventDefault();
                const form=e.target;
                const data={{
                    name:form.name.value,
                    contact:form.contact.value||null,
                    phone:form.phone.value||null,
                    address:form.address.value||null,
                    business_scope:form.business_scope.value||null,
                    remark:form.remark.value||null,
                    category_id:form.category_id.value?parseInt(form.category_id.value):null
                }};
                const res=await fetch('/api/supplier/create',{{method:'POST',headers:{{'Content-Type':'application/json'}},body:JSON.stringify(data)}});
                if(res.ok){{form.reset();loadSuppliersByCategory(currentCategoryId);}}
            }}
            function escapeHtml(text){{const div=document.createElement('div');div.textContent=text;return div.innerHTML;}}
            function importSuppliers(){{
                document.getElementById('supplierFileInput').click();
            }}
            async function handleSupplierFile(input){{
                const file=input.files[0];
                if(!file)return;
                const res=await fetch('/api/supplier/import',{{method:'POST',body:file}});
                const result=await res.text();
                alert(result);
                if(res.ok){{loadSuppliersByCategory(currentCategoryId);}}
                input.value='';
            }}
            function getUrlParam(name){{const urlParams=new URLSearchParams(window.location.search);return urlParams.get(name);}}
            const initialCategoryId=getUrlParam('category_id');
            if(initialCategoryId){{currentCategoryId=parseInt(initialCategoryId);currentCategoryName='分类供应商';loadSuppliersByCategory(currentCategoryId);}}else{{loadSuppliersByCategory(null);}}
        </script>
    "#, category_options);
    
    Html(layout_html("供应商管理", "/supplier", &content))
}

async fn page_purchaser(headers: axum::http::HeaderMap) -> Html<String> {
    match check_page_permission(&headers, "/purchaser").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let cat_rows = sqlx::query("SELECT id, name FROM category WHERE entity_type='purchaser' ORDER BY sort_order, id")
        .fetch_all(pool())
        .await
        .unwrap_or_default();

    let mut category_options = String::from("<option value=\"\">无分类</option>");
    for row in &cat_rows {
        category_options.push_str(&format!(
            "<option value=\"{0}\">{1}</option>",
            row.get::<i64, _>("id"),
            row.get::<String, _>("name"),
        ));
    }

    let content = format!(r#"
        <div class="card mb-4">
            <div class="card-body">
                <h4>新增采购单位</h4>
                <form onsubmit="createPurchaser(event)">
                    <div class="row g-2">
                        <div class="col-md-3">
                            <input type="text" name="name" placeholder="单位名称" class="form-control" required>
                        </div>
                        <div class="col-md-2">
                            <input type="text" name="contact" placeholder="联系人" class="form-control">
                        </div>
                        <div class="col-md-2">
                            <input type="text" name="phone" placeholder="电话" class="form-control">
                        </div>
                        <div class="col-md-3">
                            <input type="text" name="address" placeholder="地址" class="form-control">
                        </div>
                        <div class="col-md-2">
                            <select name="category_id" class="form-control">{0}</select>
                        </div>
                        <div class="col-md-4">
                            <input type="text" name="business_scope" placeholder="经营范围" class="form-control">
                        </div>
                        <div class="col-md-4">
                            <input type="text" name="remark" placeholder="备注" class="form-control">
                        </div>
                        <div class="col-md-2">
                            <button type="submit" class="btn btn-primary">新增</button>
                        </div>
                    </div>
                </form>
            </div>
        </div>

        <div class="d-flex justify-content-between align-items-center mb-3">
            <h5 id="purchaserListTitle">全部采购方</h5>
            <div class="d-flex gap-2 align-items-center">
                <input type="text" id="searchKeyword" placeholder="搜索采购方名称" class="form-control form-control-sm" style="width:200px" onkeydown="if(event.key==='Enter')searchPurchasers()">
                <button class="btn btn-sm btn-outline-primary" onclick="searchPurchasers()">搜索</button>
                <button class="btn btn-sm btn-outline-secondary" onclick="resetSearch()">显示全部</button>
                <a href="/api/purchaser/export" class="btn btn-sm btn-success">导出</a>
                <button class="btn btn-sm btn-warning" onclick="importPurchasers()">导入</button>
                <input type="file" id="purchaserFileInput" style="display:none" accept=".xlsx,.csv" onchange="handlePurchaserFile(this)">
            </div>
        </div>

        <table class="table table-bordered table-sm">
            <thead><tr><th>ID</th><th>名称</th><th>联系人</th><th>电话</th><th>地址</th><th>经营范围</th><th>备注</th><th>分类</th><th style="width:140px">操作</th></tr></thead>
            <tbody id="purchaserTableBody">
                <tr><td colspan="10" class="text-center text-muted">加载中...</td></tr>
            </tbody>
        </table>

        <div class="modal fade" id="editPurchaserModal" tabindex="-1">
            <div class="modal-dialog">
                <div class="modal-content">
                    <div class="modal-header">
                        <h5 class="modal-title">编辑采购方</h5>
                        <button type="button" class="btn-close" data-bs-dismiss="modal"></button>
                    </div>
                    <div class="modal-body">
                        <form id="editForm">
                            <input type="hidden" name="id">
                            <div class="mb-3"><label class="form-label">单位名称</label><input type="text" name="name" class="form-control" required></div>
                            <div class="mb-3"><label class="form-label">联系人</label><input type="text" name="contact" class="form-control"></div>
                            <div class="mb-3"><label class="form-label">电话</label><input type="text" name="phone" class="form-control"></div>
                            <div class="mb-3"><label class="form-label">地址</label><input type="text" name="address" class="form-control"></div>
                            <div class="mb-3"><label class="form-label">经营范围</label><textarea name="business_scope" class="form-control" rows="2"></textarea></div>
                            <div class="mb-3"><label class="form-label">备注</label><textarea name="remark" class="form-control" rows="2"></textarea></div>
                            <div class="mb-3"><label class="form-label">分类</label><select name="category_id" class="form-control">{0}</select></div>
                        </form>
                    </div>
                    <div class="modal-footer">
                        <button type="button" class="btn btn-secondary" data-bs-dismiss="modal">取消</button>
                        <button type="button" class="btn btn-primary" onclick="submitEdit()">保存</button>
                    </div>
                </div>
            </div>
        </div>
        <script>
            let currentCategoryId=null,currentCategoryName='全部采购方',currentKeyword='',allPurchasers=[];
            async function loadPurchasersByCategory(categoryId){{
                currentCategoryId=categoryId;
                let params=[];
                if(categoryId){{params.push('category_id='+categoryId);}}
                if(currentKeyword){{params.push('keyword='+encodeURIComponent(currentKeyword));}}
                let url='/api/purchaser/list';
                if(params.length>0){{url+='?'+params.join('&');}}
                try{{
                    const res=await fetch(url);
                    const purchasers=await res.json();
                    renderPurchaserTable(purchasers);
                    updateCategoryTitle(categoryId);
                    setFormCategory(categoryId);
                }}catch(e){{console.error('加载采购方失败:',e);}}
            }}
            function renderPurchaserTable(purchasers){{
                allPurchasers=purchasers||[];
                const tbody=document.getElementById('purchaserTableBody');
                if(!purchasers||purchasers.length===0){{
                    tbody.innerHTML='<tr><td colspan="9" class="text-center text-muted">暂无采购方数据</td></tr>';
                    return;
                }}
                let html='';
                purchasers.forEach(function(p){{
                    html+='<tr><td>'+p.id+'</td><td>'+escapeHtml(p.name)+'</td><td>'+escapeHtml(p.contact||'')+'</td><td>'+escapeHtml(p.phone||'')+'</td><td>'+escapeHtml(p.address||'')+'</td><td title="'+escapeHtml(p.business_scope||'')+'">'+escapeHtml(truncateText(p.business_scope||'',20))+'</td><td title="'+escapeHtml(p.remark||'')+'">'+escapeHtml(truncateText(p.remark||'',20))+'</td><td>'+escapeHtml(p.category_name||'无分类')+'</td>';
                    html+='<td><button class="btn btn-sm btn-outline-primary me-1" onclick="editPurchaser('+p.id+')">编辑</button><button class="btn btn-sm btn-outline-danger" onclick="deletePurchaser('+p.id+')">删除</button></td></tr>';
                }});
                tbody.innerHTML=html;
            }}
            function truncateText(text,maxLen){{
                if(!text)return '';
                return text.length>maxLen?text.substring(0,maxLen)+'...':text;
            }}
            function searchPurchasers(){{
                currentKeyword=document.getElementById('searchKeyword').value.trim();
                loadPurchasersByCategory(currentCategoryId);
            }}
            function resetSearch(){{
                document.getElementById('searchKeyword').value='';
                currentKeyword='';
                currentCategoryId=null;
                loadPurchasersByCategory(null);
            }}
            function editPurchaser(id){{
                const p=allPurchasers.find(x=>x.id===id);
                if(!p)return;
                const form=document.getElementById('editForm');
                form.id.value=p.id;
                form.name.value=p.name||'';
                form.contact.value=p.contact||'';
                form.phone.value=p.phone||'';
                form.address.value=p.address||'';
                form.business_scope.value=p.business_scope||'';
                form.remark.value=p.remark||'';
                form.category_id.value=p.category_id||'';
                const modal=new bootstrap.Modal(document.getElementById('editPurchaserModal'));
                modal.show();
            }}
            async function submitEdit(){{
                const form=document.getElementById('editForm');
                const data={{
                    id:parseInt(form.id.value),
                    name:form.name.value,
                    contact:form.contact.value||null,
                    phone:form.phone.value||null,
                    address:form.address.value||null,
                    business_scope:form.business_scope.value||null,
                    remark:form.remark.value||null,
                    category_id:form.category_id.value?parseInt(form.category_id.value):null
                }};
                const res=await fetch('/api/purchaser/update',{{method:'POST',headers:{{'Content-Type':'application/json'}},body:JSON.stringify(data)}});
                if(res.ok){{bootstrap.Modal.getInstance(document.getElementById('editPurchaserModal')).hide();loadPurchasersByCategory(currentCategoryId);}}
            }}
            async function deletePurchaser(id){{
                const p=allPurchasers.find(x=>x.id===id);
                const name=p?p.name:'';
                if(!confirm('确定要删除采购方「'+name+'」吗？'))return;
                const res=await fetch('/api/purchaser/delete',{{method:'POST',headers:{{'Content-Type':'application/json'}},body:JSON.stringify({{id:id}})}});
                if(res.ok){{loadPurchasersByCategory(currentCategoryId);}}
            }}
            function updateCategoryTitle(categoryId){{
                const title=document.getElementById('purchaserListTitle');
                if(categoryId){{title.textContent='分类采购方 - '+currentCategoryName;}}else{{title.textContent='全部采购方';currentCategoryName='全部采购方';}}
            }}
            function setCurrentCategory(catId,catName){{currentCategoryId=catId;currentCategoryName=catName||'全部采购方';}}
            function setFormCategory(categoryId){{
                const select=document.querySelector('form[onsubmit="createPurchaser(event)"] select[name="category_id"]');
                if(select){{select.value=categoryId?categoryId:'';}}
            }}
            async function createPurchaser(e){{
                e.preventDefault();
                const form=e.target;
                const data={{
                    name:form.name.value,
                    contact:form.contact.value||null,
                    phone:form.phone.value||null,
                    address:form.address.value||null,
                    business_scope:form.business_scope.value||null,
                    remark:form.remark.value||null,
                    category_id:form.category_id.value?parseInt(form.category_id.value):null
                }};
                const res=await fetch('/api/purchaser/create',{{method:'POST',headers:{{'Content-Type':'application/json'}},body:JSON.stringify(data)}});
                if(res.ok){{form.reset();loadPurchasersByCategory(currentCategoryId);}}
            }}
            function escapeHtml(text){{const div=document.createElement('div');div.textContent=text;return div.innerHTML;}}
            function importPurchasers(){{
                document.getElementById('purchaserFileInput').click();
            }}
            async function handlePurchaserFile(input){{
                const file=input.files[0];
                if(!file)return;
                const res=await fetch('/api/purchaser/import',{{method:'POST',body:file}});
                const result=await res.text();
                alert(result);
                if(res.ok){{loadPurchasersByCategory(currentCategoryId);}}
                input.value='';
            }}
            function getUrlParam(name){{const urlParams=new URLSearchParams(window.location.search);return urlParams.get(name);}}
            const initialCategoryId=getUrlParam('category_id');
            if(initialCategoryId){{currentCategoryId=parseInt(initialCategoryId);currentCategoryName='分类采购方';loadPurchasersByCategory(currentCategoryId);}}else{{loadPurchasersByCategory(null);}}
        </script>
    "#, category_options);
    
    Html(layout_html("采购方管理", "/purchaser", &content))
}

async fn page_product(headers: axum::http::HeaderMap) -> Html<String> {
    match check_page_permission(&headers, "/product").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let cat_rows = sqlx::query("SELECT id, name FROM category WHERE entity_type='product' ORDER BY sort_order, id")
        .fetch_all(pool())
        .await
        .unwrap_or_default();

    let mut category_options = String::from("<option value=\"\">无分类</option>");
    for row in &cat_rows {
        category_options.push_str(&format!(
            "<option value=\"{0}\">{1}</option>",
            row.get::<i64, _>("id"),
            row.get::<String, _>("name"),
        ));
    }

    let content = format!(r#"
        <div class="card mb-4">
            <div class="card-body">
                <h4>新增商品</h4>
                <form method="post" onsubmit="createProduct(event)">
                    <div class="row">
                        <div class="col-md-2">
                            <input type="text" name="name" placeholder="商品名称" class="form-control" required>
                        </div>
                        <div class="col-md-2">
                            <input type="text" name="spec" placeholder="规格" class="form-control">
                        </div>
                        <div class="col-md-1">
                            <input type="text" name="unit" placeholder="显示单位" class="form-control">
                        </div>
                        <div class="col-md-1">
                            <input type="text" name="base_unit" placeholder="基础单位" class="form-control">
                        </div>
                        <div class="col-md-2">
                            <input type="number" step="0.01" name="base_price" placeholder="基础单价(售价)" class="form-control">
                        </div>
                        <div class="col-md-2">
                            <input type="number" step="0.01" name="purchase_price" placeholder="进价" class="form-control">
                        </div>
                        <div class="col-md-2">
                            <select name="category_id" class="form-control">{0}</select>
                        </div>
                        <div class="col-md-2">
                            <button type="submit" class="btn btn-primary">新增</button>
                        </div>
                    </div>
                </form>
            </div>
        </div>

        <div class="d-flex justify-content-between align-items-center mb-3">
            <h5 id="productListTitle">全部商品</h5>
            <div class="d-flex gap-2 align-items-center">
                <input type="text" id="searchKeyword" placeholder="搜索商品名称" class="form-control form-control-sm" style="width:200px" onkeydown="if(event.key==='Enter')searchProducts()">
                <button class="btn btn-sm btn-outline-primary" onclick="searchProducts()">搜索</button>
                <button class="btn btn-sm btn-outline-secondary" onclick="resetSearch()">显示全部</button>
                <a href="/api/product/export" class="btn btn-sm btn-success">导出</a>
                <button class="btn btn-sm btn-warning" onclick="importProducts()">导入</button>
                <input type="file" id="productFileInput" style="display:none" accept=".xlsx,.csv" onchange="handleProductFile(this)">
            </div>
        </div>

        <table class="table table-bordered">
            <thead><tr><th>ID</th><th>图片</th><th>名称</th><th>规格</th><th>显示单位</th><th>基础单位</th><th>售价</th><th>进价</th><th>多单位</th><th>分类</th><th>状态</th><th style="width:140px">操作</th></tr></thead>
            <tbody id="productTableBody">
                <tr><td colspan="12" class="text-center text-muted">加载中...</td></tr>
            </tbody>
        </table>

        <!-- 编辑模态框 -->
        <div class="modal fade" id="editProductModal" tabindex="-1">
            <div class="modal-dialog modal-lg">
                <div class="modal-content">
                    <div class="modal-header">
                        <h5 class="modal-title">编辑商品</h5>
                        <button type="button" class="btn-close" data-bs-dismiss="modal"></button>
                    </div>
                    <div class="modal-body">
                        <form id="editForm">
                            <input type="hidden" name="id">
                            <div class="row">
                                <div class="col-md-4">
                                    <label class="form-label">商品名称（通用名）</label>
                                    <input type="text" name="name" class="form-control" required>
                                </div>
                                <div class="col-md-3">
                                    <label class="form-label">别称1（如地域称呼）</label>
                                    <input type="text" name="alias1" class="form-control" placeholder="如：甜蕉">
                                </div>
                                <div class="col-md-3">
                                    <label class="form-label">别称2（如地域称呼）</label>
                                    <input type="text" name="alias2" class="form-control" placeholder="如：甘蕉">
                                </div>
                                <div class="col-md-2">
                                    <label class="form-label">显示单位</label>
                                    <input type="text" name="unit" class="form-control">
                                </div>
                            </div>
                            <div class="row mt-3">
                                <div class="col-md-2">
                                    <label class="form-label">规格</label>
                                    <input type="text" name="spec" class="form-control">
                                </div>
                                <div class="col-md-2">
                                    <label class="form-label">基础单位</label>
                                    <input type="text" name="base_unit" class="form-control">
                                </div>
                                <div class="col-md-4">
                                    <label class="form-label">基础单价（每基础单位，售价）</label>
                                    <input type="number" step="0.01" name="base_price" class="form-control">
                                </div>
                                <div class="col-md-4">
                                    <label class="form-label">进价（每基础单位）</label>
                                    <input type="number" step="0.01" name="purchase_price" class="form-control">
                                </div>
                                <div class="col-md-4">
                                    <label class="form-label">分类</label>
                                    <select name="category_id" class="form-control">{0}</select>
                                </div>
                            </div>

                            <div class="mt-4">
                                <div class="d-flex justify-content-between align-items-center">
                                    <h6>价格管理</h6>
                                </div>
                                <div class="alert alert-info py-2 mt-2 mb-2 small">
                                    售价计算规则：若有政采平台价则以政采平台价为售价；否则取三个商超的最高价；若无任何价格则使用基础单价。
                                </div>
                                <div class="row mt-2">
                                    <div class="col-md-3">
                                        <label class="form-label">政采平台价</label>
                                        <input type="number" step="0.01" name="gov_price" class="form-control" oninput="calcSellingPrice()">
                                    </div>
                                    <div class="col-md-3">
                                        <label class="form-label">商超1零售价</label>
                                        <input type="number" step="0.01" name="supermarket_1" class="form-control" oninput="calcSellingPrice()">
                                    </div>
                                    <div class="col-md-3">
                                        <label class="form-label">商超2零售价</label>
                                        <input type="number" step="0.01" name="supermarket_2" class="form-control" oninput="calcSellingPrice()">
                                    </div>
                                    <div class="col-md-3">
                                        <label class="form-label">商超3零售价</label>
                                        <input type="number" step="0.01" name="supermarket_3" class="form-control" oninput="calcSellingPrice()">
                                    </div>
                                </div>
                                <div class="row mt-3">
                                    <div class="col-md-4">
                                        <label class="form-label">AI实时采集价（预留）</label>
                                        <input type="number" step="0.01" name="ai_realtime" class="form-control">
                                    </div>
                                    <div class="col-md-4">
                                        <label class="form-label">计算售价（只读）</label>
                                        <input type="number" step="0.01" name="selling_price" class="form-control" readonly>
                                    </div>
                                </div>
                            </div>
                        </form>
                        
                        <div class="mt-4">
                            <div class="d-flex justify-content-between align-items-center">
                                <h6>商品图片</h6>
                            </div>
                            <div class="mt-2">
                                <div id="productImagePreview" class="mb-3" style="display:flex;align-items:center;gap:15px;">
                                    <div id="imagePlaceholder" style="width:120px;height:120px;background:#f5f5f5;border-radius:8px;display:flex;align-items:center;justify-content:center;color:#999;border:2px dashed #ddd;">
                                        <span>暂无图片</span>
                                    </div>
                                    <div id="imageActions" style="display:none;">
                                        <button class="btn btn-sm btn-danger" onclick="deleteProductImage()">🗑️ 删除图片</button>
                                    </div>
                                </div>
                                <div>
                                    <input type="file" id="productImageInput" accept="image/*" style="display:none" onchange="uploadProductImage()">
                                    <button class="btn btn-sm btn-outline-primary" onclick="document.getElementById('productImageInput').click()">📷 上传图片</button>
                                    <span class="text-muted small ml-2">支持 JPG、PNG、GIF、WebP 格式，最大5MB</span>
                                </div>
                            </div>
                        </div>

                        <div class="mt-4">
                            <div class="d-flex justify-content-between align-items-center">
                                <h6>多单位设置</h6>
                                <button class="btn btn-sm btn-primary" onclick="addUnitRow()">+ 添加单位</button>
                            </div>
                            <div class="alert alert-info py-2 mt-2 mb-2 small">
                                示例：基础单位为「斤」，新增「件」单位，1件=20斤，则换算比例填 <b>20</b>；
                                若整件批发价55元（比按比例算的60元便宜），则在单位单价填 <b>55</b>，留0则自动按比例计算。
                                单位采购价用于整采整卖场景，留0则使用进价按比例计算。
                            </div>
                            <table class="table table-sm table-bordered mt-2" id="unitTable">
                                <thead><tr><th>单位名称</th><th>换算比例（1本单位=?基础单位）</th><th>单位售价（留0则按比例自动算）</th><th>单位采购价（留0则按进价比例算）</th><th>排序</th><th>操作</th></tr></thead>
                                <tbody id="unitTableBody"></tbody>
                            </table>
                        </div>
                    </div>
                    <div class="modal-footer">
                        <button type="button" class="btn btn-secondary" data-bs-dismiss="modal">取消</button>
                        <button type="button" class="btn btn-primary" onclick="submitEdit()">保存</button>
                    </div>
                </div>
            </div>
        </div>

        <div class="modal fade" id="duplicateProductModal" tabindex="-1">
            <div class="modal-dialog">
                <div class="modal-content">
                    <div class="modal-header">
                        <h5 class="modal-title">提示：存在同名商品</h5>
                        <button type="button" class="btn-close" data-bs-dismiss="modal"></button>
                    </div>
                    <div class="modal-body">
                        <p>发现以下同名商品，是否需要先查看？</p>
                        <table class="table table-sm table-bordered mt-2" id="duplicateProductTable">
                            <thead><tr><th>ID</th><th>名称</th><th>规格</th><th>单位</th><th>单价</th><th>分类</th><th>操作</th></tr></thead>
                            <tbody id="duplicateProductTableBody"></tbody>
                        </table>
                    </div>
                    <div class="modal-footer">
                        <button type="button" class="btn btn-secondary" data-bs-dismiss="modal">取消</button>
                        <button type="button" class="btn btn-primary" onclick="proceedCreateProduct()">继续新增</button>
                    </div>
                </div>
            </div>
        </div>
        <script>
            let currentCategoryId = null;
            let currentCategoryName = '全部商品';
            let currentKeyword = '';
            let allProducts = [];
            let editingProductId = null;
            let pendingProductData = null;

            async function loadProductsByCategory(categoryId) {{
                currentCategoryId = categoryId;
                let params = [];
                if (categoryId) {{ params.push('category_id=' + categoryId); }}
                if (currentKeyword) {{ params.push('keyword=' + encodeURIComponent(currentKeyword)); }}
                let url = '/api/product/list';
                if (params.length > 0) {{ url += '?' + params.join('&'); }}
                try {{
                    const res = await fetch(url);
                    const products = await res.json();
                    renderProductTable(products);
                    updateCategoryTitle(categoryId);
                    setFormCategory(categoryId);
                }} catch(e) {{
                    console.error('加载商品失败:', e);
                }}
            }}

            function renderProductTable(products) {{
                allProducts = products || [];
                const tbody = document.getElementById('productTableBody');
                if (!products || products.length === 0) {{
                    tbody.innerHTML = '<tr><td colspan="11" class="text-center text-muted">暂无商品数据</td></tr>';
                    return;
                }}
                let html = '';
                products.forEach(function(p) {{
                    let unitsText = '';
                    if (p.units && p.units.length > 0) {{
                        unitsText = p.units.map(u => u.unit_name + '(' + u.ratio + ')').join(', ');
                    }}
                    let imageHtml = '';
                    if (p.image_url) {{
                        imageHtml = '<img src="' + p.image_url + '" style="width:50px;height:50px;object-fit:cover;border-radius:4px;" alt="商品图片">';
                    }} else {{
                        imageHtml = '<div style="width:50px;height:50px;background:#f5f5f5;border-radius:4px;display:flex;align-items:center;justify-content:center;color:#ccc;">无图</div>';
                    }}
                    let statusBadge = p.status === 1 ? '<span class="badge bg-success">启用</span>' : '<span class="badge bg-secondary">停用</span>';
                    let toggleBtnClass = p.status === 1 ? 'btn-outline-warning' : 'btn-outline-success';
                    let toggleBtnText = p.status === 1 ? '停用' : '启用';
                    html += '<tr><td>' + p.id + '</td><td>' + imageHtml + '</td><td>' + escapeHtml(p.name) + '</td><td>' + escapeHtml(p.spec || '') + '</td><td>' + escapeHtml(p.unit || '') + '</td><td>' + escapeHtml(p.base_unit || '') + '</td><td>' + p.base_price + '</td><td>' + (p.purchase_price || 0) + '</td><td>' + escapeHtml(unitsText) + '</td><td>' + escapeHtml(p.category_name || '无分类') + '</td><td>' + statusBadge + '</td>';
                    html += '<td><button class="btn btn-sm btn-outline-primary me-1" onclick="editProduct(' + p.id + ')">编辑</button><button class="btn btn-sm ' + toggleBtnClass + ' me-1" onclick="toggleProductStatus(' + p.id + ')">' + toggleBtnText + '</button><button class="btn btn-sm btn-outline-danger" onclick="deleteProduct(' + p.id + ')">删除</button></td></tr>';
                }});
                tbody.innerHTML = html;
            }}

            function searchProducts() {{
                currentKeyword = document.getElementById('searchKeyword').value.trim();
                loadProductsByCategory(currentCategoryId);
            }}

            function resetSearch() {{
                document.getElementById('searchKeyword').value = '';
                currentKeyword = '';
                currentCategoryId = null;
                loadProductsByCategory(null);
            }}

            function calcSellingPrice() {{
                const form = document.getElementById('editForm');
                const govPrice = parseFloat(form.gov_price.value) || 0;
                const sm1 = parseFloat(form.supermarket_1.value) || 0;
                const sm2 = parseFloat(form.supermarket_2.value) || 0;
                const sm3 = parseFloat(form.supermarket_3.value) || 0;
                let sellingPrice = 0;
                if (govPrice > 0) {{
                    sellingPrice = govPrice;
                }} else {{
                    const maxSm = Math.max(sm1, sm2, sm3);
                    if (maxSm > 0) {{
                        sellingPrice = maxSm;
                    }} else {{
                        sellingPrice = parseFloat(form.base_price.value) || 0;
                    }}
                }}
                form.selling_price.value = sellingPrice.toFixed(2);
            }}

            function addUnitRow(unitData) {{
                const tbody = document.getElementById('unitTableBody');
                const tr = document.createElement('tr');
                tr.innerHTML = `
                    <td><input type="text" class="form-control form-control-sm" name="unit_name" value="${{unitData ? escapeHtml(unitData.unit_name) : ''}}"></td>
                    <td><input type="number" step="0.01" class="form-control form-control-sm" name="ratio" value="${{unitData ? unitData.ratio : 1}}"></td>
                    <td><input type="number" step="0.01" class="form-control form-control-sm" name="unit_price" value="${{unitData ? unitData.unit_price : 0}}"></td>
                    <td><input type="number" step="0.01" class="form-control form-control-sm" name="purchase_price" value="${{unitData ? (unitData.purchase_price || 0) : 0}}"></td>
                    <td><input type="number" class="form-control form-control-sm" name="sort_order" value="${{unitData ? unitData.sort_order : 0}}"></td>
                    <td><button class="btn btn-sm btn-danger" onclick="this.parentElement.parentElement.remove()">删除</button></td>
                `;
                tbody.appendChild(tr);
            }}

            async function editProduct(id) {{
                editingProductId = id;
                const p = allProducts.find(x => x.id === id);
                if (!p) return;
                const form = document.getElementById('editForm');
                form.id.value = p.id;
                form.name.value = p.name || '';
                form.alias1.value = p.alias1 || '';
                form.alias2.value = p.alias2 || '';
                form.spec.value = p.spec || '';
                form.unit.value = p.unit || '';
                form.base_unit.value = p.base_unit || '';
                form.base_price.value = p.base_price || 0;
                form.purchase_price.value = p.purchase_price || 0;
                form.category_id.value = p.category_id || '';
                
                form.gov_price.value = '';
                form.supermarket_1.value = '';
                form.supermarket_2.value = '';
                form.supermarket_3.value = '';
                form.ai_realtime.value = '';
                form.selling_price.value = '';
                
                if (p.prices) {{
                    for (const price of p.prices) {{
                        if (price.price_type === 'gov_procurement') form.gov_price.value = price.price;
                        else if (price.price_type === 'supermarket_1') form.supermarket_1.value = price.price;
                        else if (price.price_type === 'supermarket_2') form.supermarket_2.value = price.price;
                        else if (price.price_type === 'supermarket_3') form.supermarket_3.value = price.price;
                        else if (price.price_type === 'ai_realtime') form.ai_realtime.value = price.price;
                    }}
                }}
                form.selling_price.value = p.selling_price || '';
                calcSellingPrice();

                const tbody = document.getElementById('unitTableBody');
                tbody.innerHTML = '';
                if (p.units) {{
                    p.units.forEach(function(u) {{
                        addUnitRow(u);
                    }});
                }}

                const imagePlaceholder = document.getElementById('imagePlaceholder');
                const imageActions = document.getElementById('imageActions');
                if (p.image_url) {{
                    imagePlaceholder.innerHTML = '<img src="' + p.image_url + '" style="width:120px;height:120px;object-fit:cover;border-radius:8px;">';
                    imagePlaceholder.style.border = 'none';
                    imageActions.style.display = 'block';
                }} else {{
                    imagePlaceholder.innerHTML = '<span>暂无图片</span>';
                    imagePlaceholder.style.border = '2px dashed #ddd';
                    imageActions.style.display = 'none';
                }}

                const modal = new bootstrap.Modal(document.getElementById('editProductModal'));
                modal.show();
            }}

            async function uploadProductImage() {{
                const input = document.getElementById('productImageInput');
                const file = input.files[0];
                if (!file) return;

                const formData = new FormData();
                formData.append('file', file);

                try {{
                    const res = await fetch('/api/product/upload_image?product_id=' + editingProductId, {{
                        method: 'POST',
                        body: formData
                    }});
                    const result = await res.json();
                    if (res.ok && result.url) {{
                        const imagePlaceholder = document.getElementById('imagePlaceholder');
                        const imageActions = document.getElementById('imageActions');
                        imagePlaceholder.innerHTML = '<img src="' + result.url + '" style="width:120px;height:120px;object-fit:cover;border-radius:8px;">';
                        imagePlaceholder.style.border = 'none';
                        imageActions.style.display = 'block';
                        
                        const p = allProducts.find(x => x.id === editingProductId);
                        if (p) {{
                            p.image_url = result.url;
                        }}
                    }} else {{
                        alert('上传失败');
                    }}
                }} catch(e) {{
                    alert('上传失败: ' + e.message);
                }}
                input.value = '';
            }}

            async function deleteProductImage() {{
                if (!confirm('确定要删除这张图片吗？')) return;
                
                try {{
                    const res = await fetch('/api/product/delete_image?product_id=' + editingProductId);
                    if (res.ok) {{
                        const imagePlaceholder = document.getElementById('imagePlaceholder');
                        const imageActions = document.getElementById('imageActions');
                        imagePlaceholder.innerHTML = '<span>暂无图片</span>';
                        imagePlaceholder.style.border = '2px dashed #ddd';
                        imageActions.style.display = 'none';
                        
                        const p = allProducts.find(x => x.id === editingProductId);
                        if (p) {{
                            p.image_url = null;
                        }}
                    }} else {{
                        alert('删除失败');
                    }}
                }} catch(e) {{
                    alert('删除失败: ' + e.message);
                }}
            }}

            async function submitEdit() {{
                const form = document.getElementById('editForm');
                const p = allProducts.find(x => x.id === editingProductId);
                const data = {{
                    id: parseInt(form.id.value),
                    name: form.name.value,
                    spec: form.spec.value || null,
                    alias1: form.alias1.value || null,
                    alias2: form.alias2.value || null,
                    unit: form.unit.value || null,
                    base_unit: form.base_unit.value || null,
                    base_price: parseFloat(form.base_price.value) || null,
                    purchase_price: parseFloat(form.purchase_price.value) || null,
                    image_url: p ? p.image_url : null,
                    category_id: form.category_id.value ? parseInt(form.category_id.value) : null
                }};
                const res = await fetch('/api/product/update', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify(data)
                }});
                if (res.ok) {{
                    await saveUnits();
                    await savePrices();
                    bootstrap.Modal.getInstance(document.getElementById('editProductModal')).hide();
                    loadProductsByCategory(currentCategoryId);
                }}
            }}

            async function savePrices() {{
                const form = document.getElementById('editForm');
                const priceTypes = [
                    {{ name: 'gov_price', type: 'gov_procurement' }},
                    {{ name: 'supermarket_1', type: 'supermarket_1' }},
                    {{ name: 'supermarket_2', type: 'supermarket_2' }},
                    {{ name: 'supermarket_3', type: 'supermarket_3' }},
                    {{ name: 'ai_realtime', type: 'ai_realtime' }}
                ];
                
                await fetch('/api/product/price/delete_by_product', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ product_id: editingProductId }})
                }});
                
                for (const pt of priceTypes) {{
                    const price = parseFloat(form[pt.name].value) || 0;
                    if (price > 0) {{
                        await fetch('/api/product/price/upsert', {{
                            method: 'POST',
                            headers: {{ 'Content-Type': 'application/json' }},
                            body: JSON.stringify({{
                                product_id: editingProductId,
                                price_type: pt.type,
                                price: price
                            }})
                        }});
                    }}
                }}

                await fetch('/api/product/sync_base_price', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ product_id: editingProductId }})
                }});
            }}

            async function saveUnits() {{
                await fetch('/api/product/unit/delete_by_product', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ product_id: editingProductId }})
                }});
                
                const rows = document.querySelectorAll('#unitTableBody tr');
                for (let i = 0; i < rows.length; i++) {{
                    const row = rows[i];
                    const inputs = row.querySelectorAll('input');
                    const unitName = inputs[0].value;
                    const ratio = parseFloat(inputs[1].value) || 1;
                    const unitPrice = parseFloat(inputs[2].value) || 0;
                    const purchasePrice = parseFloat(inputs[3].value) || 0;
                    const sortOrder = parseInt(inputs[4].value) || 0;
                    
                    if (!unitName) continue;
                    
                    await fetch('/api/product/unit/create', {{
                        method: 'POST',
                        headers: {{ 'Content-Type': 'application/json' }},
                        body: JSON.stringify({{
                            product_id: editingProductId,
                            unit_name: unitName,
                            ratio: ratio,
                            unit_price: unitPrice,
                            purchase_price: purchasePrice,
                            sort_order: sortOrder
                        }})
                    }});
                }}
            }}

            async function toggleProductStatus(id) {{
                const p = allProducts.find(x => x.id === id);
                const name = p ? p.name : '';
                const action = p && p.status === 1 ? '停用' : '启用';
                if (!confirm('确定要' + action + '商品「' + name + '」吗？')) return;
                const res = await fetch('/api/product/toggle_status/' + id, {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }}
                }});
                if (res.ok) {{
                    loadProductsByCategory(currentCategoryId);
                }}
            }}

            async function deleteProduct(id) {{
                const p = allProducts.find(x => x.id === id);
                const name = p ? p.name : '';
                if (!confirm('确定要删除商品「' + name + '」吗？')) return;
                const res = await fetch('/api/product/delete', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ id: id }})
                }});
                if (res.ok) {{
                    loadProductsByCategory(currentCategoryId);
                }}
            }}

            function updateCategoryTitle(categoryId) {{
                const title = document.getElementById('productListTitle');
                if (categoryId) {{
                    title.textContent = '分类商品 - ' + currentCategoryName;
                }} else {{
                    title.textContent = '全部商品';
                    currentCategoryName = '全部商品';
                }}
            }}

            function setCurrentCategory(catId, catName) {{
                currentCategoryId = catId;
                currentCategoryName = catName || '全部商品';
            }}

            function setFormCategory(categoryId) {{
                const select = document.querySelector('form[onsubmit="createProduct(event)"] select[name="category_id"]');
                if (select) {{
                    select.value = categoryId ? categoryId : '';
                }}
            }}

            async function createProduct(e) {{
                e.preventDefault();
                const form = e.target;
                const data = {{
                    name: form.name.value,
                    spec: form.spec.value || null,
                    unit: form.unit.value || null,
                    base_unit: form.base_unit.value || null,
                    base_price: parseFloat(form.base_price.value) || null,
                    purchase_price: parseFloat(form.purchase_price.value) || null,
                    category_id: form.category_id.value ? parseInt(form.category_id.value) : null
                }};
                
                const checkRes = await fetch('/api/product/check_name?name=' + encodeURIComponent(data.name));
                const duplicates = await checkRes.json();
                
                if (duplicates && duplicates.length > 0) {{
                    pendingProductData = {{ form: form, data: data }};
                    showDuplicateModal(duplicates);
                    return;
                }}
                
                await doCreateProduct(form, data);
            }}

            function showDuplicateModal(products) {{
                const tbody = document.getElementById('duplicateProductTableBody');
                let html = '';
                products.forEach(function(p) {{
                    html += '<tr><td>' + p.id + '</td><td>' + escapeHtml(p.name) + '</td><td>' + escapeHtml(p.spec || '') + '</td><td>' + escapeHtml(p.unit) + '</td><td>' + (p.base_price || 0).toFixed(2) + '</td><td>' + escapeHtml(p.category_name || '无分类') + '</td>';
                    html += '<td><button class="btn btn-sm btn-outline-primary" onclick="openDuplicateProduct(' + p.id + ')">查看</button></td></tr>';
                }});
                tbody.innerHTML = html;
                const modal = new bootstrap.Modal(document.getElementById('duplicateProductModal'));
                modal.show();
            }}

            function openDuplicateProduct(id) {{
                bootstrap.Modal.getInstance(document.getElementById('duplicateProductModal')).hide();
                editProduct(id);
            }}

            async function proceedCreateProduct() {{
                bootstrap.Modal.getInstance(document.getElementById('duplicateProductModal')).hide();
                if (pendingProductData) {{
                    await doCreateProduct(pendingProductData.form, pendingProductData.data);
                    pendingProductData = null;
                }}
            }}

            async function doCreateProduct(form, data) {{
                const res = await fetch('/api/product/create', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify(data)
                }});
                if (res.ok) {{
                    form.reset();
                    loadProductsByCategory(currentCategoryId);
                }}
            }}

            function escapeHtml(text) {{
                const div = document.createElement('div');
                div.textContent = text;
                return div.innerHTML;
            }}

            function importProducts() {{
                document.getElementById('productFileInput').click();
            }}
            async function handleProductFile(input) {{
                const file = input.files[0];
                if (!file) return;
                const res = await fetch('/api/product/import', {{ method: 'POST', body: file }});
                const result = await res.text();
                alert(result);
                if (res.ok) {{ loadProductsByCategory(currentCategoryId); }}
                input.value = '';
            }}

            function getUrlParam(name) {{
                const urlParams = new URLSearchParams(window.location.search);
                return urlParams.get(name);
            }}

            const initialCategoryId = getUrlParam('category_id');
            if (initialCategoryId) {{
                currentCategoryId = parseInt(initialCategoryId);
                currentCategoryName = '分类商品';
                loadProductsByCategory(currentCategoryId);
            }} else {{
                loadProductsByCategory(null);
            }}
        </script>
    "#, category_options);
    
    Html(layout_html("商品管理", "/product", &content))
}

async fn page_warehouse(headers: axum::http::HeaderMap) -> Html<String> {
    match check_page_permission(&headers, "/warehouse").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let content = r#"
        <div class="card p-4">
            <div class="d-flex justify-content-between align-items-center mb-4">
                <h3>仓库管理</h3>
                <button class="btn btn-primary" onclick="openWarehouseModal()">新建仓库</button>
            </div>
            <div class="mb-3">
                <input type="text" id="searchKeyword" class="form-control" placeholder="搜索仓库名称或编号..." oninput="searchWarehouses()">
            </div>
            <table class="table table-bordered table-sm">
                <thead><tr><th>ID</th><th>编号</th><th>名称</th><th>联系人</th><th>电话</th><th>地址</th><th>状态</th><th style="width:120px">操作</th></tr></thead>
                <tbody id="warehouseTableBody">
                    <tr><td colspan="8" class="text-center text-muted">加载中...</td></tr>
                </tbody>
            </table>
        </div>

        <div class="modal fade" id="warehouseModal" tabindex="-1">
            <div class="modal-dialog">
                <div class="modal-content">
                    <div class="modal-header">
                        <h5 class="modal-title" id="warehouseModalTitle">新建仓库</h5>
                        <button type="button" class="btn-close" data-bs-dismiss="modal"></button>
                    </div>
                    <div class="modal-body">
                        <form id="warehouseForm">
                            <input type="hidden" name="id">
                            <div class="mb-3"><label class="form-label">仓库名称</label><input type="text" name="name" class="form-control" required></div>
                            <div class="mb-3"><label class="form-label">仓库编号</label><input type="text" name="code" class="form-control" placeholder="如 WH002"></div>
                            <div class="mb-3"><label class="form-label">联系人</label><input type="text" name="contact" class="form-control"></div>
                            <div class="mb-3"><label class="form-label">电话</label><input type="text" name="phone" class="form-control"></div>
                            <div class="mb-3"><label class="form-label">地址</label><textarea name="address" class="form-control" rows="2"></textarea></div>
                            <div class="mb-3">
                                <label class="form-label">状态</label>
                                <select name="status" class="form-control">
                                    <option value="1">启用</option>
                                    <option value="0">停用</option>
                                </select>
                            </div>
                        </form>
                    </div>
                    <div class="modal-footer">
                        <button type="button" class="btn btn-secondary" data-bs-dismiss="modal">取消</button>
                        <button type="button" class="btn btn-primary" onclick="submitWarehouse()">保存</button>
                    </div>
                </div>
            </div>
        </div>
        <script>
            let allWarehouses = [];
            async function loadWarehouses() {
                try {
                    const res = await fetch('/api/warehouse/list');
                    const warehouses = await res.json();
                    allWarehouses = warehouses;
                    renderWarehouseTable(warehouses);
                } catch(e) {
                    console.error('加载仓库失败:', e);
                }
            }
            function renderWarehouseTable(warehouses) {
                const tbody = document.getElementById('warehouseTableBody');
                if (!warehouses || warehouses.length === 0) {
                    tbody.innerHTML = '<tr><td colspan="8" class="text-center text-muted">暂无仓库数据</td></tr>';
                    return;
                }
                let html = '';
                warehouses.forEach(function(w) {
                    const statusBadge = w.status === 1 
                        ? '<span class="badge bg-success">启用</span>' 
                        : '<span class="badge bg-secondary">停用</span>';
                    html += '<tr><td>' + w.id + '</td><td>' + escapeHtml(w.code || '') + '</td><td>' + escapeHtml(w.name) + '</td><td>' + escapeHtml(w.contact || '') + '</td><td>' + escapeHtml(w.phone || '') + '</td><td title="' + escapeHtml(w.address || '') + '">' + escapeHtml(truncateText(w.address || '', 20)) + '</td><td>' + statusBadge + '</td>';
                    if (w.id === 1) {
                        html += '<td><button class="btn btn-sm btn-outline-primary" onclick="editWarehouse(' + w.id + ')">编辑</button></td></tr>';
                    } else {
                        html += '<td><button class="btn btn-sm btn-outline-primary me-1" onclick="editWarehouse(' + w.id + ')">编辑</button><button class="btn btn-sm btn-outline-danger" onclick="deleteWarehouse(' + w.id + ')">删除</button></td></tr>';
                    }
                });
                tbody.innerHTML = html;
            }
            function searchWarehouses() {
                const keyword = document.getElementById('searchKeyword').value.toLowerCase().trim();
                if (!keyword) {
                    renderWarehouseTable(allWarehouses);
                    return;
                }
                const filtered = allWarehouses.filter(w => 
                    w.name.toLowerCase().includes(keyword) || 
                    (w.code && w.code.toLowerCase().includes(keyword))
                );
                renderWarehouseTable(filtered);
            }
            function openWarehouseModal() {
                document.getElementById('warehouseModalTitle').textContent = '新建仓库';
                document.getElementById('warehouseForm').reset();
                document.querySelector('input[name="id"]').value = '';
                const modal = new bootstrap.Modal(document.getElementById('warehouseModal'));
                modal.show();
            }
            function editWarehouse(id) {
                const warehouse = allWarehouses.find(w => w.id === id);
                if (!warehouse) return;
                document.getElementById('warehouseModalTitle').textContent = '编辑仓库';
                const form = document.getElementById('warehouseForm');
                form.querySelector('input[name="id"]').value = warehouse.id;
                form.querySelector('input[name="name"]').value = warehouse.name;
                form.querySelector('input[name="code"]').value = warehouse.code || '';
                form.querySelector('input[name="contact"]').value = warehouse.contact || '';
                form.querySelector('input[name="phone"]').value = warehouse.phone || '';
                form.querySelector('textarea[name="address"]').value = warehouse.address || '';
                form.querySelector('select[name="status"]').value = warehouse.status;
                const modal = new bootstrap.Modal(document.getElementById('warehouseModal'));
                modal.show();
            }
            async function submitWarehouse() {
                const form = document.getElementById('warehouseForm');
                const id = form.querySelector('input[name="id"]').value;
                const data = {
                    name: form.querySelector('input[name="name"]').value,
                    code: form.querySelector('input[name="code"]').value || null,
                    contact: form.querySelector('input[name="contact"]').value || null,
                    phone: form.querySelector('input[name="phone"]').value || null,
                    address: form.querySelector('textarea[name="address"]').value || null,
                    status: parseInt(form.querySelector('select[name="status"]').value),
                    sort_order: 0
                };
                let url = '/api/warehouse/create';
                let method = 'POST';
                if (id) {
                    url = '/api/warehouse/update';
                    data.id = parseInt(id);
                }
                try {
                    const res = await fetch(url, {
                        method: method,
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify(data)
                    });
                    const text = await res.text();
                    if (res.ok) {
                        bootstrap.Modal.getInstance(document.getElementById('warehouseModal')).hide();
                        loadWarehouses();
                    }
                    alert(text);
                } catch(e) {
                    alert('操作失败: ' + e.message);
                }
            }
            async function deleteWarehouse(id) {
                if (!confirm('确定要删除该仓库吗？删除后无法恢复！')) return;
                try {
                    const res = await fetch('/api/warehouse/delete', {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ id: id })
                    });
                    const text = await res.text();
                    if (res.ok) {
                        loadWarehouses();
                    }
                    alert(text);
                } catch(e) {
                    alert('删除失败: ' + e.message);
                }
            }
            function escapeHtml(text) {
                const div = document.createElement('div');
                div.textContent = text;
                return div.innerHTML;
            }
            function truncateText(text, maxLen) {
                if (!text) return '';
                return text.length > maxLen ? text.substring(0, maxLen) + '...' : text;
            }
            loadWarehouses();
        </script>
    "#;
    Html(layout_html("仓库管理", "/warehouse", &content))
}

async fn page_inventory(headers: axum::http::HeaderMap) -> Html<String> {
    match check_page_permission(&headers, "/inventory").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let rows = sqlx::query(
        "SELECT i.id, i.product_id, p.name, p.spec, i.quantity, i.min_stock, i.max_stock 
         FROM inventory i JOIN product p ON i.product_id = p.id ORDER BY i.id DESC"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();

    let mut table_html = String::new();
    for row in rows {
        table_html.push_str(&format!(
            r#"<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>"#,
            row.get::<i64, _>("id"),
            row.get::<i64, _>("product_id"),
            row.get::<String, _>("name"),
            row.get::<Option<String>, _>("spec").unwrap_or_default(),
            row.get::<f64, _>("quantity"),
            row.get::<f64, _>("min_stock"),
            row.get::<f64, _>("max_stock"),
        ));
    }

    let content = format!(r#"
        <table class="table table-bordered">
            <thead><tr><th>ID</th><th>商品ID</th><th>商品名称</th><th>规格</th><th>库存数量</th><th>最低库存</th><th>最高库存</th></tr></thead>
            <tbody>{}</tbody>
        </table>
    "#, table_html);
    
    Html(layout_html("库存管理", "/inventory", &content))
}

async fn page_purchase(headers: axum::http::HeaderMap) -> Html<String> {
    match check_page_permission(&headers, "/purchase").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let supplier_rows = sqlx::query("SELECT id, name FROM supplier")
        .fetch_all(pool())
        .await
        .unwrap_or_default();

    let mut supplier_js_array = String::from("[");
    for (i, row) in supplier_rows.iter().enumerate() {
        if i > 0 { supplier_js_array.push_str(","); }
        supplier_js_array.push_str(&format!(
            "{{id:{},name:'{}'}}",
            row.get::<i64, _>("id"),
            row.get::<String, _>("name").replace("'", "\\'"),
        ));
    }
    supplier_js_array.push_str("]");

    let now = Local::now().format("%Y-%m-%d").to_string();

    let content = format!(r#"
        <div class="card mb-4">
            <div class="card-body">
                <h4>新建采购订单</h4>
                <div class="row mb-3">
                    <div class="col-md-3">
                        <label>供应商：</label>
                        <div class="position-relative">
                            <input type="text" id="supplierInput" class="form-control" placeholder="单击选择 / 双击搜索" readonly>
                            <input type="hidden" id="supplierId" value="">
                            <div id="supplierDropdown" class="search-dropdown"></div>
                        </div>
                    </div>
                    <div class="col-md-3">
                        <label>入库仓库：</label>
                        <div class="position-relative">
                            <input type="text" id="warehouseInput" class="form-control" placeholder="单击选择 / 双击搜索" readonly>
                            <input type="hidden" id="warehouseId" value="">
                            <div id="warehouseDropdown" class="search-dropdown"></div>
                        </div>
                    </div>
                    <div class="col-md-3">
                        <label>订单号：</label>
                        <input type="text" id="orderNoInput" class="form-control" readonly>
                    </div>
                    <div class="col-md-3">
                        <label>订单日期：</label>
                        <input type="date" id="orderDateInput" class="form-control" value="{}" onchange="generateOrderNo('purchase')">
                    </div>
                    <div class="col-md-3">
                        <label>备注：</label>
                        <input type="text" id="remarkInput" class="form-control">
                    </div>
                </div>

                <table class="table table-bordered">
                    <thead>
                        <tr><th>商品名称</th><th>规格</th><th>单位</th><th>数量</th><th>单价</th><th>金额</th><th>备注</th><th>操作</th></tr>
                    </thead>
                    <tbody id="itemsTable"></tbody>
                </table>

                <div class="d-flex justify-content-between mt-3">
                    <button onclick="addItem()" class="btn btn-primary">新增商品行</button>
                    <div class="font-weight-bold">合计：¥<span id="totalAmount">0.00</span></div>
                </div>

                <div class="d-flex justify-content-end mt-3">
                    <div class="mr-4">
                        <label>下浮率：</label>
                        <input type="number" step="0.1" id="discountRateInput" value="0" oninput="updateFinalAmount()" class="form-control-sm" style="width: 80px;">%
                    </div>
                    <div class="mr-4">
                        <label>下浮后：</label>
                        <span class="font-weight-bold">¥<span id="discountAmount">0.00</span></span>
                    </div>
                    <div class="mr-4">
                        <label>金额折减：</label>
                        <input type="number" step="0.01" id="amountReductionInput" value="0" oninput="updateFinalAmount()" class="form-control-sm" style="width: 80px;">
                    </div>
                    <div>
                        <label>最终合计：</label>
                        <span class="font-weight-bold text-danger">¥<span id="finalAmount">0.00</span></span>
                    </div>
                </div>

                <button onclick="saveOrder()" class="btn btn-success mt-3">保存采购订单</button>
                <button onclick="resetForm()" class="btn btn-secondary mt-3 ml-2">新建订单</button>
            </div>
        </div>

        <h4>采购订单列表</h4>
        <div class="mb-3">
            <input type="text" id="searchInput" class="form-control" placeholder="搜索订单号、供应商、日期..." oninput="searchOrders()" style="width: 300px; display: inline-block;">
            <button onclick="searchOrders()" class="btn btn-primary ml-2">搜索</button>
            <button onclick="resetSearch()" class="btn btn-secondary ml-2">重置</button>
            <button onclick="cancelOrder()" class="btn btn-warning ml-2">取消</button>
            <a href="/api/purchase_order/export" class="btn btn-success ml-2">导出</a>
            <button onclick="importPurchaseOrders()" class="btn btn-warning ml-2">导入</button>
            <input type="file" id="purchaseOrderFileInput" style="display:none" accept=".csv" onchange="handlePurchaseOrderFile(this)">
        </div>
        <table class="table table-bordered">
            <thead><tr><th>ID</th><th>订单号</th><th>日期</th><th>供应商</th><th>入库仓库</th><th>金额</th><th>下浮后</th><th>折减</th><th>最终金额</th><th>状态</th><th>操作</th></tr></thead>
            <tbody id="orderListBody"></tbody>
        </table>

        <div id="pagination" class="mt-3"></div>

        <script>
            let suppliers = [];
            let items = [];

            async function loadSuppliers() {{
                const res = await fetch('/api/supplier/list');
                suppliers = await res.json();
            }}
            loadSuppliers();

            let warehouses = [];
            async function loadWarehouses() {{
                const res = await fetch('/api/warehouse/list');
                warehouses = await res.json();
            }}
            loadWarehouses();

            function showWarehouseDropdown(filter) {{
                const dropdown = document.getElementById('warehouseDropdown');
                if (!dropdown) return;
                let list = warehouses;
                if (filter) {{
                    const kw = filter.toLowerCase();
                    list = warehouses.filter(w => w.name.toLowerCase().includes(kw));
                }}
                if (list.length === 0) {{
                    dropdown.innerHTML = '<div class="p-2 text-muted">无匹配仓库</div>';
                    dropdown.style.display = 'block';
                    return;
                }}
                let html = '<ul class="search-results">';
                list.forEach(w => {{
                    html += '<li onclick="selectWarehouse(this)" data-id="' + w.id + '" data-name="' + w.name.replace(/&/g, '&amp;').replace(/"/g, '&quot;') + '">' + w.name + '</li>';
                }});
                html += '</ul>';
                dropdown.innerHTML = html;
                dropdown.style.display = 'block';
            }}

            function selectWarehouse(li) {{
                const input = document.getElementById('warehouseInput');
                const dropdown = document.getElementById('warehouseDropdown');
                if (li) {{
                    document.getElementById('warehouseId').value = li.getAttribute('data-id');
                    input.value = li.getAttribute('data-name');
                    input.readOnly = true;
                    dropdown.style.display = 'none';
                }}
            }}

            document.getElementById('warehouseInput').addEventListener('click', function() {{
                showWarehouseDropdown('');
            }});
            document.getElementById('warehouseInput').addEventListener('dblclick', function() {{
                this.readOnly = false;
                this.value = '';
                this.focus();
                showWarehouseDropdown('');
            }});
            document.getElementById('warehouseInput').addEventListener('input', function() {{
                showWarehouseDropdown(this.value);
            }});
            document.getElementById('warehouseInput').addEventListener('blur', function() {{
                setTimeout(() => {{
                    const dropdown = document.getElementById('warehouseDropdown');
                    if (dropdown) dropdown.style.display = 'none';
                }}, 200);
            }});

            function showSupplierDropdown(filter) {{
                const dropdown = document.getElementById('supplierDropdown');
                let list = suppliers;
                if (filter) {{
                    const kw = filter.toLowerCase();
                    list = suppliers.filter(s => s.name.toLowerCase().includes(kw));
                }}
                if (list.length === 0) {{
                    dropdown.innerHTML = '<div class="p-2 text-muted">无匹配供应商</div>';
                    dropdown.style.display = 'block';
                    return;
                }}
                let html = '<ul class="search-results">';
                list.forEach(s => {{
                    html += '<li data-id="' + s.id + '" data-name="' + s.name.replace(/&/g, '&amp;').replace(/"/g, '&quot;') + '">' + s.name + '</li>';
                }});
                html += '</ul>';
                dropdown.innerHTML = html;
                dropdown.style.display = 'block';
            }}

            document.getElementById('supplierDropdown').addEventListener('click', function(e) {{
                const li = e.target.closest('li');
                if (li) {{
                    const id = li.getAttribute('data-id');
                    const name = li.getAttribute('data-name');
                    document.getElementById('supplierId').value = id;
                    document.getElementById('supplierInput').value = name;
                    this.style.display = 'none';
                }}
            }});

            document.getElementById('supplierInput').addEventListener('click', function() {{
                this.readOnly = true;
                showSupplierDropdown('');
            }});

            document.getElementById('supplierInput').addEventListener('dblclick', function() {{
                this.readOnly = false;
                this.value = '';
                this.focus();
                showSupplierDropdown('');
            }});

            document.getElementById('supplierInput').addEventListener('input', function() {{
                showSupplierDropdown(this.value.trim());
            }});

            document.getElementById('supplierInput').addEventListener('blur', function() {{
                setTimeout(() => {{
                    document.getElementById('supplierDropdown').style.display = 'none';
                }}, 200);
            }});

            async function generateOrderNo(type) {{
                const date = document.getElementById('orderDateInput').value;
                if (!date) return;
                const res = await fetch('/api/order/generate_no?type=' + type + '&date=' + encodeURIComponent(date));
                const data = await res.json();
                document.getElementById('orderNoInput').value = data.order_no;
            }}

            function updateFinalAmount() {{
                const total = parseFloat(document.getElementById('totalAmount').textContent) || 0;
                const rate = parseFloat(document.getElementById('discountRateInput').value) || 0;
                const reduction = parseFloat(document.getElementById('amountReductionInput').value) || 0;
                const discountAmount = total * (1 - rate / 100);
                const finalAmount = Math.max(0, discountAmount - reduction);
                document.getElementById('discountAmount').textContent = discountAmount.toFixed(2);
                document.getElementById('finalAmount').textContent = finalAmount.toFixed(2);
            }}

            let currentPage = 1;
            let currentKeyword = '';

            function resetSearch() {{
                document.getElementById('searchInput').value = '';
                currentKeyword = '';
                currentPage = 1;
                loadOrders();
            }}

            async function searchOrders() {{
                currentKeyword = document.getElementById('searchInput').value.trim();
                currentPage = 1;
                await loadOrders();
            }}

            async function loadOrders(page) {{
                if (page !== undefined) currentPage = page;
                let url = '/api/purchase_order/list?page=' + currentPage + '&page_size=20';
                if (currentKeyword) {{
                    url += '&keyword=' + encodeURIComponent(currentKeyword);
                }}
                const res = await fetch(url);
                const result = await res.json();
                const orders = result.data || [];
                const tbody = document.getElementById('orderListBody');
                tbody.innerHTML = '';
                orders.forEach(order => {{
                    tbody.innerHTML += '<tr onclick="loadOrderDetail(' + order.id + ')" style="cursor: pointer;">' +
                        '<td>' + order.id + '</td>' +
                        '<td>' + order.order_no + '</td>' +
                        '<td>' + order.order_date + '</td>' +
                        '<td>' + order.supplier_name + '</td>' +
                        '<td>' + (order.warehouse_name || '') + '</td>' +
                        '<td>' + order.total_amount.toFixed(2) + '</td>' +
                        '<td>' + (order.total_amount * (1 - (order.discount_rate || 0) / 100)).toFixed(2) + '</td>' +
                        '<td>' + (order.amount_reduction || 0).toFixed(2) + '</td>' +
                        '<td>' + (order.final_amount || 0).toFixed(2) + '</td>' +
                        '<td>' + order.status + '</td>' +
                        '<td>' +
                        '<button onclick="event.stopPropagation(); deleteOrder(' + order.id + ')" class="btn btn-danger btn-sm">删除</button>' +
                        '</td></tr>';
                }});
                renderPagination(result.page, result.total_pages, result.total);
            }}

            function renderPagination(page, totalPages, total) {{
                const container = document.getElementById('pagination');
                if (!container) return;
                if (totalPages <= 1) {{
                    container.innerHTML = '';
                    return;
                }}
                let html = '<nav aria-label="Page navigation"><ul class="pagination justify-content-center">';
                html += '<li class="page-item ' + (page <= 1 ? 'disabled' : '') + '"><a class="page-link" onclick="loadOrders(' + (page - 1) + ')">上一页</a></li>';
                
                const startPage = Math.max(1, page - 2);
                const endPage = Math.min(totalPages, page + 2);
                
                for (let i = startPage; i <= endPage; i++) {{
                    html += '<li class="page-item ' + (i === page ? 'active' : '') + '"><a class="page-link" onclick="loadOrders(' + i + ')">' + i + '</a></li>';
                }}
                
                html += '<li class="page-item ' + (page >= totalPages ? 'disabled' : '') + '"><a class="page-link" onclick="loadOrders(' + (page + 1) + ')">下一页</a></li>';
                html += '</ul></nav>';
                html += '<p class="text-center text-muted mt-2">共 ' + total + ' 条记录，当前第 ' + page + '/' + totalPages + ' 页</p>';
                container.innerHTML = html;
            }}

            generateOrderNo('purchase');
            loadOrders();

            function addItem() {{
                items.push({{ product_id: 0, product_name: '', alias1: '', alias2: '', spec: '', unit: '', base_unit: '', unit_price: 0, purchase_price: 0, quantity: 0, base_quantity: 0, amount: 0, ratio: 1, units: [] }});
                renderItems();
            }}

            function removeItem(index) {{
                if (!confirm('确定删除该商品行？')) return;
                items.splice(index, 1);
                renderItems();
            }}

            function renderItems() {{
                const table = document.getElementById('itemsTable');
                table.innerHTML = '';
                let total = 0;
                items.forEach((item, index) => {{
                    total += item.amount;
                    let unitOptions = '';
                    unitOptions += '<option value="' + item.base_unit + '" data-ratio="1" data-unit-price="' + (item.base_price || item.unit_price || 0) + '" data-purchase-price="' + (item.purchase_price || item.base_price || item.unit_price || 0) + '"' + (item.unit === item.base_unit ? ' selected' : '') + '>' + item.base_unit + '(基础单位)</option>';
                    item.units.forEach(function(u) {{
                        unitOptions += '<option value="' + u.name + '" data-ratio="' + u.ratio + '" data-unit-price="' + (u.unit_price || 0) + '" data-purchase-price="' + (u.purchase_price || 0) + '" data-base-price="' + (item.base_price || item.unit_price || 0) + '"' + (item.unit === u.name ? ' selected' : '') + '>' + u.name + '</option>';
                    }});
                    table.innerHTML += `
                        <tr>
                            <td>
                                <div class="position-relative">
                                    <input type="text" value="${{item.product_name || ''}}" 
                                           oninput="handleProductSearch(${{index}}, this)" 
                                           onclick="handleProductSearch(${{index}}, this)"
                                           class="form-control-sm product-search-input" 
                                           placeholder="输入商品名称搜索">
                                    <div id="searchDropdown_${{index}}" class="search-dropdown"></div>
                                </div>
                            </td>
                            <td><input type="text" value="${{item.spec}}" onchange="updateSpec(${{index}}, this)" class="form-control-sm"></td>
                            <td>
                                <select onchange="updateUnit(${{index}}, this)" class="form-control-sm">
                                    ${{unitOptions}}
                                </select>
                            </td>
                            <td><input type="number" step="0.01" value="${{item.quantity}}" onchange="updateQty(${{index}}, this)" onkeydown="handleEnterKey(event, ${{index}}, 3)" class="form-control-sm" enterkeyhint="next"></td>
                            <td><input type="number" step="0.01" value="${{item.unit_price}}" onchange="updatePrice(${{index}}, this)" onkeydown="handleEnterKey(event, ${{index}}, 4)" class="form-control-sm" enterkeyhint="next"></td>
                            <td>${{item.amount.toFixed(2)}}</td>
                            <td><input type="text" value="${{item.remark || ''}}" onchange="updateRemark(${{index}}, this)" class="form-control-sm" placeholder="单品备注"></td>
                            <td><button onclick="removeItem(${{index}})" class="btn btn-danger btn-sm">删除</button></td>
                        </tr>
                    `;
                }});
                document.getElementById('totalAmount').textContent = total.toFixed(2);
                updateFinalAmount();
            }}

            let searchTimeout = null;

            async function handleProductSearch(index, input) {{
                const keyword = input.value.trim();
                const dropdown = document.getElementById('searchDropdown_' + index);
                
                if (keyword.length < 1) {{
                    dropdown.innerHTML = '';
                    dropdown.style.display = 'none';
                    return;
                }}
                
                if (searchTimeout) clearTimeout(searchTimeout);
                
                searchTimeout = setTimeout(async () => {{
                    const res = await fetch('/api/product/search?keyword=' + encodeURIComponent(keyword));
                    const products = await res.json();
                    
                    if (products.length > 0) {{
                        let html = '<ul class="search-results">';
                        products.forEach(p => {{
                            let aliases = [];
                            if (p.alias1) aliases.push('别称1: ' + p.alias1);
                            if (p.alias2) aliases.push('别称2: ' + p.alias2);
                            html += '<li onclick="selectProduct(' + index + ', this)" data-id="' + p.id + '" data-name="' + p.name + '" data-alias1="' + (p.alias1 || '') + '" data-alias2="' + (p.alias2 || '') + '" data-spec="' + (p.spec || '') + '" data-unit="' + p.unit + '" data-base-unit="' + p.base_unit + '" data-price="' + p.selling_price + '" data-base-price="' + p.base_price + '" data-purchase-price="' + (p.purchase_price || 0) + '">';
                            html += '<strong>' + p.name + '</strong>';
                            if (p.spec) html += ' (' + p.spec + ')';
                            if (aliases.length > 0) html += '<br><small>' + aliases.join(', ') + '</small>';
                            if (p.category_name) html += '<br><small class="text-muted">分类: ' + p.category_name + '</small>';
                            html += '</li>';
                        }});
                        html += '</ul>';
                        dropdown.innerHTML = html;
                        dropdown.style.display = 'block';
                    }} else {{
                        dropdown.innerHTML = '';
                        dropdown.style.display = 'none';
                    }}
                }}, 300);
            }}

            function selectProduct(index, li) {{
                const input = document.querySelector('#itemsTable tr:nth-child(' + (index + 1) + ') .product-search-input');
                const dropdown = document.getElementById('searchDropdown_' + index);
                
                items[index].product_id = parseInt(li.getAttribute('data-id'));
                items[index].product_name = li.getAttribute('data-name');
                items[index].alias1 = li.getAttribute('data-alias1') || '';
                items[index].alias2 = li.getAttribute('data-alias2') || '';
                items[index].spec = li.getAttribute('data-spec');
                items[index].unit = li.getAttribute('data-base-unit');
                items[index].base_unit = li.getAttribute('data-base-unit');
                items[index].purchase_price = parseFloat(li.getAttribute('data-purchase-price')) || 0;
                items[index].unit_price = items[index].purchase_price || parseFloat(li.getAttribute('data-base-price')) || parseFloat(li.getAttribute('data-price')) || 0;
                items[index].base_price = items[index].purchase_price || parseFloat(li.getAttribute('data-base-price')) || items[index].unit_price;
                if (items[index].quantity === undefined || items[index].quantity === null) items[index].quantity = 0;
                if (items[index].base_quantity === undefined || items[index].base_quantity === null) items[index].base_quantity = 0;
                items[index].amount = (items[index].quantity || 0) * (items[index].unit_price || 0);
                
                input.value = items[index].product_name;
                dropdown.innerHTML = '';
                dropdown.style.display = 'none';
                
                fetch('/api/product/unit/list?product_id=' + items[index].product_id)
                    .then(res => res.json())
                    .then(units => {{
                        items[index].units = units;
                        renderItems();
                    }})
                    .catch(() => {{
                        items[index].units = [];
                        renderItems();
                    }});
            }}

            document.addEventListener('click', function(e) {{
                const dropdowns = document.querySelectorAll('.search-dropdown');
                dropdowns.forEach(d => {{
                    if (!d.contains(e.target) && !e.target.classList.contains('product-search-input') && e.target.id !== 'supplierInput') {{
                        d.style.display = 'none';
                    }}
                }});
            }});

            function updateUnit(index, select) {{
                const opt = select.options[select.selectedIndex];
                const ratio = parseFloat(opt.getAttribute('data-ratio')) || 1;
                const purchasePrice = parseFloat(opt.getAttribute('data-purchase-price')) || 0;
                items[index].unit = opt.value;
                items[index].ratio = ratio;
                if (purchasePrice > 0) {{
                    items[index].unit_price = purchasePrice;
                }} else {{
                    const basePrice = parseFloat(opt.getAttribute('data-base-price')) || items[index].unit_price;
                    items[index].unit_price = basePrice * ratio;
                }}
                items[index].base_quantity = items[index].quantity * ratio;
                items[index].amount = items[index].unit_price * items[index].quantity;
                renderItems();
            }}

            function updateName(index, input) {{ items[index].product_name = input.value; }}
            function updateAlias1(index, input) {{ items[index].alias1 = input.value; }}
            function updateAlias2(index, input) {{ items[index].alias2 = input.value; }}
            function updateSpec(index, input) {{ items[index].spec = input.value; }}
            function updatePrice(index, input) {{ 
                items[index].unit_price = parseFloat(input.value) || 0; 
                items[index].amount = items[index].unit_price * items[index].quantity;
                renderItems();
            }}
            function updateQty(index, input) {{ 
                items[index].quantity = parseFloat(input.value) || 0; 
                items[index].base_quantity = items[index].quantity * (items[index].ratio || 1);
                items[index].amount = items[index].unit_price * items[index].quantity;
                renderItems();
            }}

            function handleEnterKey(event, index, cellIndex) {{
                const enterKeys = ['Enter', 'Next', 'Go', 'Done'];
                if (enterKeys.includes(event.key) || event.keyCode === 13) {{
                    event.preventDefault();
                    
                    const input = event.target;
                    if (cellIndex === 3) {{
                        items[index].quantity = parseFloat(input.value) || 0;
                        items[index].base_quantity = items[index].quantity * (items[index].ratio || 1);
                        items[index].amount = items[index].unit_price * items[index].quantity;
                    }} else if (cellIndex === 4) {{
                        items[index].unit_price = parseFloat(input.value) || 0;
                        items[index].amount = items[index].unit_price * items[index].quantity;
                    }}
                    renderItems();
                    
                    const nextIndex = index + 1;
                    if (nextIndex < items.length) {{
                        const table = document.getElementById('itemsTable');
                        if (table && table.rows[nextIndex]) {{
                            const nextRow = table.rows[nextIndex];
                            if (nextRow.cells[cellIndex]) {{
                                const targetInput = nextRow.cells[cellIndex].querySelector('input');
                                if (targetInput) {{
                                    targetInput.focus();
                                    try {{ targetInput.select(); }} catch(e) {{}}
                                }}
                            }}
                        }}
                    }}
                }}
            }}

            function updateRemark(index, input) {{
                items[index].remark = input.value.trim();
            }}

            let currentOrderId = null;

            async function saveOrder() {{
                const supplierId = document.getElementById('supplierId').value;
                if (!supplierId) {{
                    alert('请选择供应商');
                    return;
                }}
                const data = {{
                    id: currentOrderId,
                    supplier_id: parseInt(supplierId),
                    order_no: document.getElementById('orderNoInput').value,
                    order_date: document.getElementById('orderDateInput').value,
                    total_amount: parseFloat(document.getElementById('totalAmount').textContent),
                    discount_rate: parseFloat(document.getElementById('discountRateInput').value) || 0,
                    amount_reduction: parseFloat(document.getElementById('amountReductionInput').value) || 0,
                    final_amount: parseFloat(document.getElementById('finalAmount').textContent) || 0,
                    warehouse_id: parseInt(document.getElementById('warehouseId').value) || 0,
                    warehouse_name: document.getElementById('warehouseInput').value || '',
                    items: items,
                    remark: document.getElementById('remarkInput').value || null
                }};
                const url = currentOrderId ? '/api/purchase_order/update' : '/api/purchase_order/create';
                const res = await fetch(url, {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify(data)
                }});
                if (res.ok) {{
                    location.reload();
                }}
            }}

            async function loadOrderDetail(id) {{
                const res = await fetch('/api/purchase_order/detail/' + id);
                const order = await res.json();
                currentOrderId = order.id;
                document.getElementById('supplierId').value = order.supplier_id;
                document.getElementById('supplierInput').value = order.supplier_name;
                document.getElementById('warehouseId').value = order.warehouse_id || 0;
                document.getElementById('warehouseInput').value = order.warehouse_name || '';
                document.getElementById('orderNoInput').value = order.order_no;
                document.getElementById('orderDateInput').value = order.order_date;
                document.getElementById('remarkInput').value = order.remark || '';
                document.getElementById('discountRateInput').value = order.discount_rate || 0;
                document.getElementById('amountReductionInput').value = order.amount_reduction || 0;
                
                items = [];
                for (const item of order.items) {{
                    const itemData = {{
                        product_id: item.product_id,
                        product_name: item.product_name,
                        alias1: item.alias1 || '',
                        alias2: item.alias2 || '',
                        spec: item.spec || '',
                        unit: item.unit || '',
                        unit_price: item.unit_price || 0,
                        quantity: item.quantity || 0,
                        base_quantity: item.base_quantity || 0,
                        amount: item.amount || 0,
                        remark: item.remark || '',
                        supplier_id: item.supplier_id || 0,
                        supplier_name: item.supplier_name || '',
                        base_unit: '',
                        base_price: 0,
                        units: []
                    }};
                    items.push(itemData);
                    
                    try {{
                        const productRes = await fetch('/api/product/by_id?id=' + item.product_id);
                        const product = await productRes.json();
                        if (product.id) {{
                            itemData.base_unit = product.base_unit || item.unit || '';
                            itemData.base_price = product.base_price || item.unit_price || 0;
                        }} else {{
                            itemData.base_unit = item.unit || '';
                            itemData.base_price = item.unit_price || 0;
                        }}
                    }} catch (e) {{
                        itemData.base_unit = item.unit || '';
                        itemData.base_price = item.unit_price || 0;
                    }}
                    
                    try {{
                        const unitsRes = await fetch('/api/product/unit/list?product_id=' + item.product_id);
                        itemData.units = await unitsRes.json();
                    }} catch (e) {{
                        itemData.units = [];
                    }}
                }}
                renderItems();
            }}

            async function deleteOrder(id) {{
                if (!confirm('确定删除该订单？')) return;
                const res = await fetch('/api/purchase_order/delete/' + id, {{ method: 'DELETE' }});
                if (res.ok) {{
                    loadOrders();
                    if (currentOrderId === id) {{
                        resetForm();
                    }}
                }}
            }}

            function importPurchaseOrders() {{
                document.getElementById('purchaseOrderFileInput').click();
            }}
            async function handlePurchaseOrderFile(input) {{
                const file = input.files[0];
                if (!file) return;
                const reader = new FileReader();
                reader.onload = async function(e) {{
                    const text = e.target.result;
                    const res = await fetch('/api/purchase_order/import', {{ method: 'POST', body: text }});
                    const result = await res.text();
                    alert(result);
                    if (res.ok) {{ loadOrders(); }}
                }};
                reader.readAsText(file, 'utf-8');
                input.value = '';
            }}

            function cancelOrder() {{
                resetForm();
            }}

            function resetForm() {{
                currentOrderId = null;
                document.getElementById('supplierId').value = '';
                document.getElementById('supplierInput').value = '';
                document.getElementById('warehouseId').value = '';
                document.getElementById('warehouseInput').value = '';
                document.getElementById('orderNoInput').value = '';
                document.getElementById('orderDateInput').value = '';
                document.getElementById('remarkInput').value = '';
                document.getElementById('discountRateInput').value = '0';
                items = [];
                renderItems();
                generateOrderNo('purchase');
            }}
        </script>
    "#, now);
    
    Html(layout_html("采购订单", "/purchase", &content))
}

async fn page_sales(headers: axum::http::HeaderMap) -> Html<String> {
    match check_page_permission(&headers, "/sales").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let now = Local::now().format("%Y-%m-%d").to_string();

    let content = format!(r#"
        <div class="card mb-4">
            <div class="card-body">
                <h4 id="formTitle">新建销售订单</h4>
                <div class="row mb-3">
                    <div class="col-md-3">
                        <label>采购单位：</label>
                        <div class="position-relative">
                            <input type="text" id="purchaserInput" class="form-control" placeholder="单击选择 / 双击搜索" readonly>
                            <input type="hidden" id="purchaserId" value="">
                            <div id="purchaserDropdown" class="search-dropdown"></div>
                        </div>
                    </div>
                    <div class="col-md-3">
                        <label>出库仓库：</label>
                        <div class="position-relative">
                            <input type="text" id="warehouseInput" class="form-control" placeholder="单击选择 / 双击搜索" readonly>
                            <input type="hidden" id="warehouseId" value="">
                            <div id="warehouseDropdown" class="search-dropdown"></div>
                        </div>
                    </div>
                    <div class="col-md-3">
                        <label>订单号：</label>
                        <input type="text" id="orderNoInput" class="form-control" readonly>
                    </div>
                    <div class="col-md-3">
                        <label>订单日期：</label>
                        <input type="date" id="orderDateInput" class="form-control" value="{}" onchange="generateOrderNo('sales')">
                    </div>
                    <div class="col-md-3">
                        <label>备注：</label>
                        <input type="text" id="remarkInput" class="form-control">
                    </div>
                </div>

                <table class="table table-bordered">
                    <thead>
                        <tr><th>商品名称</th><th>规格</th><th>单位</th><th>数量</th><th>单价</th><th>金额</th><th>供应商</th><th>备注</th><th>操作</th></tr>
                    </thead>
                    <tbody id="itemsTable"></tbody>
                </table>

                <div class="d-flex justify-content-between mt-3">
                    <button onclick="addItem()" class="btn btn-primary">新增商品行</button>
                    <div class="font-weight-bold">合计：¥<span id="totalAmount">0.00</span></div>
                </div>

                <div class="d-flex justify-content-end mt-3">
                    <div class="mr-4">
                        <label>下浮率：</label>
                        <input type="number" step="0.1" id="discountRateInput" value="20" oninput="updateFinalAmount()" class="form-control-sm" style="width: 80px;">%
                    </div>
                    <div class="mr-4">
                        <label>下浮后：</label>
                        <span class="font-weight-bold">¥<span id="discountAmount">0.00</span></span>
                    </div>
                    <div class="mr-4">
                        <label>金额折减：</label>
                        <input type="number" step="0.01" id="amountReductionInput" value="0" oninput="updateFinalAmount()" class="form-control-sm" style="width: 80px;">
                    </div>
                    <div>
                        <label>最终合计：</label>
                        <span class="font-weight-bold text-danger">¥<span id="finalAmount">0.00</span></span>
                    </div>
                </div>

                <button onclick="saveOrder()" class="btn btn-success mt-3" id="saveBtn">保存销售订单</button>
                <button onclick="resetForm()" class="btn btn-secondary mt-3 ml-2">新建订单</button>
            </div>
        </div>

        <h4>销售订单列表</h4>
        <div class="mb-3">
            <input type="text" id="searchInput" class="form-control" placeholder="搜索订单号、采购单位、日期..." oninput="searchOrders()" style="width: 300px; display: inline-block;">
            <button onclick="searchOrders()" class="btn btn-primary ml-2">搜索</button>
            <button onclick="resetSearch()" class="btn btn-secondary ml-2">重置</button>
            <button onclick="cancelOrder()" class="btn btn-warning ml-2">取消</button>
            <a href="/api/sales_order/export" class="btn btn-success ml-2">导出</a>
            <button onclick="importSalesOrders()" class="btn btn-warning ml-2">导入</button>
            <input type="file" id="salesOrderFileInput" style="display:none" accept=".csv" onchange="handleSalesOrderFile(this)">
        </div>
        <table class="table table-bordered">
            <thead><tr><th>ID</th><th>订单号</th><th>日期</th><th>采购单位</th><th>金额</th><th>下浮后</th><th>折减</th><th>最终金额</th><th>状态</th><th>操作</th></tr></thead>
            <tbody id="orderListBody"></tbody>
        </table>

        <div id="pagination" class="mt-3"></div>

        <script>
            let purchasers = [];
            let items = [];

            function showPurchaserDropdown(filter) {{
                const dropdown = document.getElementById('purchaserDropdown');
                let list = purchasers;
                if (filter) {{
                    const kw = filter.toLowerCase();
                    list = purchasers.filter(p => p.name.toLowerCase().includes(kw));
                }}
                if (list.length === 0) {{
                    dropdown.innerHTML = '<div class="p-2 text-muted">无匹配采购单位</div>';
                    dropdown.style.display = 'block';
                    return;
                }}
                let html = '<ul class="search-results">';
                list.forEach(p => {{
                    html += '<li data-id="' + p.id + '" data-name="' + p.name.replace(/&/g, '&amp;').replace(/"/g, '&quot;') + '">' + p.name + '</li>';
                }});
                html += '</ul>';
                dropdown.innerHTML = html;
                dropdown.style.display = 'block';
            }}

            document.getElementById('purchaserDropdown').addEventListener('click', function(e) {{
                const li = e.target.closest('li');
                if (li) {{
                    const id = li.getAttribute('data-id');
                    const name = li.getAttribute('data-name');
                    document.getElementById('purchaserId').value = id;
                    document.getElementById('purchaserInput').value = name;
                    this.style.display = 'none';
                }}
            }});

            document.getElementById('purchaserInput').addEventListener('click', function() {{
                this.readOnly = true;
                showPurchaserDropdown('');
            }});

            document.getElementById('purchaserInput').addEventListener('dblclick', function() {{
                this.readOnly = false;
                this.value = '';
                this.focus();
                showPurchaserDropdown('');
            }});

            document.getElementById('purchaserInput').addEventListener('input', function() {{
                showPurchaserDropdown(this.value.trim());
            }});

            document.getElementById('purchaserInput').addEventListener('blur', function() {{
                setTimeout(() => {{
                    document.getElementById('purchaserDropdown').style.display = 'none';
                }}, 200);
            }});

            async function generateOrderNo(type) {{
                const date = document.getElementById('orderDateInput').value;
                if (!date) return;
                const res = await fetch('/api/order/generate_no?type=' + type + '&date=' + encodeURIComponent(date));
                const data = await res.json();
                document.getElementById('orderNoInput').value = data.order_no;
            }}

            function updateFinalAmount() {{
                const total = parseFloat(document.getElementById('totalAmount').textContent) || 0;
                const rate = parseFloat(document.getElementById('discountRateInput').value) || 0;
                const reduction = parseFloat(document.getElementById('amountReductionInput').value) || 0;
                const discountAmount = total * (1 - rate / 100);
                const finalAmount = Math.max(0, discountAmount - reduction);
                document.getElementById('discountAmount').textContent = discountAmount.toFixed(2);
                document.getElementById('finalAmount').textContent = finalAmount.toFixed(2);
            }}

            generateOrderNo('sales');

            function addItem() {{
                items.push({{ product_id: 0, product_name: '', alias1: '', alias2: '', spec: '', unit: '', base_unit: '', unit_price: 0, quantity: 0, base_quantity: 0, amount: 0, ratio: 1, units: [], supplier_id: 0, supplier_name: '' }});
                renderItems();
            }}

            function removeItem(index) {{
                if (!confirm('确定删除该商品行？')) return;
                items.splice(index, 1);
                renderItems();
            }}

            function renderItems() {{
                const table = document.getElementById('itemsTable');
                table.innerHTML = '';
                let total = 0;
                items.forEach((item, index) => {{
                    total += item.amount;
                    let unitOptions = '';
                    unitOptions += '<option value="' + item.base_unit + '" data-ratio="1" data-unit-price="' + (item.base_price || item.unit_price || 0) + '"' + (item.unit === item.base_unit ? ' selected' : '') + '>' + item.base_unit + '(基础单位)</option>';
                    item.units.forEach(function(u) {{
                        unitOptions += '<option value="' + u.name + '" data-ratio="' + u.ratio + '" data-unit-price="' + (u.unit_price || 0) + '" data-base-price="' + (item.base_price || item.unit_price || 0) + '"' + (item.unit === u.name ? ' selected' : '') + '>' + u.name + '</option>';
                    }});
                    let supplierOptions = '<option value="0">请选择供应商</option>';
                    suppliers.forEach(function(s) {{
                        supplierOptions += '<option value="' + s.id + '"' + (item.supplier_id === s.id ? ' selected' : '') + '>' + s.name + '</option>';
                    }});
                    table.innerHTML += `
                        <tr>
                            <td>
                                <div class="position-relative">
                                    <input type="text" value="${{item.product_name || ''}}" 
                                           oninput="handleProductSearch(${{index}}, this)" 
                                           onclick="handleProductSearch(${{index}}, this)"
                                           class="form-control-sm product-search-input" 
                                           placeholder="输入商品名称搜索">
                                    <div id="searchDropdown_${{index}}" class="search-dropdown"></div>
                                </div>
                            </td>
                            <td><input type="text" value="${{item.spec}}" onchange="updateSpec(${{index}}, this)" class="form-control-sm"></td>
                            <td>
                                <select onchange="updateUnit(${{index}}, this)" class="form-control-sm">
                                    ${{unitOptions}}
                                </select>
                            </td>
                            <td><input type="number" step="0.01" value="${{item.quantity}}" onchange="updateQty(${{index}}, this)" onkeydown="handleEnterKey(event, ${{index}}, 3)" class="form-control-sm" enterkeyhint="next"></td>
                            <td><input type="number" step="0.01" value="${{item.unit_price}}" onchange="updatePrice(${{index}}, this)" onkeydown="handleEnterKey(event, ${{index}}, 4)" class="form-control-sm" enterkeyhint="next"></td>
                            <td>${{item.amount.toFixed(2)}}</td>
                            <td>
                                <div class="position-relative">
                                    <input type="text" value="${{item.supplier_name || ''}}" 
                                           onclick="showItemSupplierDropdown(${{index}}, '')"
                                           ondblclick="enableItemSupplierInput(${{index}})"
                                           oninput="showItemSupplierDropdown(${{index}}, this.value)"
                                           onblur="hideItemSupplierDropdown(${{index}})"
                                           class="form-control-sm supplier-input" 
                                           placeholder="单击选择 / 双击搜索"
                                           readonly>
                                    <input type="hidden" id="supplierId_${{index}}" value="${{item.supplier_id || 0}}">
                                    <div id="supplierDropdown_${{index}}" class="search-dropdown"></div>
                                </div>
                            </td>
                            <td><input type="text" value="${{item.remark || ''}}" onchange="updateRemark(${{index}}, this)" class="form-control-sm" placeholder="单品备注"></td>
                            <td><button onclick="removeItem(${{index}})" class="btn btn-danger btn-sm">删除</button></td>
                        </tr>
                    `;
                }});
                document.getElementById('totalAmount').textContent = total.toFixed(2);
                updateFinalAmount();
            }}

            let searchTimeout = null;

            async function handleProductSearch(index, input) {{
                const keyword = input.value.trim();
                const dropdown = document.getElementById('searchDropdown_' + index);
                
                if (keyword.length < 1) {{
                    dropdown.innerHTML = '';
                    dropdown.style.display = 'none';
                    return;
                }}
                
                if (searchTimeout) clearTimeout(searchTimeout);
                
                searchTimeout = setTimeout(async () => {{
                    const res = await fetch('/api/product/search?keyword=' + encodeURIComponent(keyword));
                    const products = await res.json();
                    
                    if (products.length > 0) {{
                        let html = '<ul class="search-results">';
                        products.forEach(p => {{
                            let aliases = [];
                            if (p.alias1) aliases.push('别称1: ' + p.alias1);
                            if (p.alias2) aliases.push('别称2: ' + p.alias2);
                            html += '<li onclick="selectProduct(' + index + ', this)" data-id="' + p.id + '" data-name="' + p.name + '" data-alias1="' + (p.alias1 || '') + '" data-alias2="' + (p.alias2 || '') + '" data-spec="' + (p.spec || '') + '" data-unit="' + p.unit + '" data-base-unit="' + p.base_unit + '" data-price="' + p.selling_price + '" data-base-price="' + p.base_price + '" data-purchase-price="' + (p.purchase_price || 0) + '">';
                            html += '<strong>' + p.name + '</strong>';
                            if (p.spec) html += ' (' + p.spec + ')';
                            if (aliases.length > 0) html += '<br><small>' + aliases.join(', ') + '</small>';
                            if (p.category_name) html += '<br><small class="text-muted">分类: ' + p.category_name + '</small>';
                            html += '</li>';
                        }});
                        html += '</ul>';
                        dropdown.innerHTML = html;
                        dropdown.style.display = 'block';
                    }} else {{
                        dropdown.innerHTML = '';
                        dropdown.style.display = 'none';
                    }}
                }}, 300);
            }}

            function selectProduct(index, li) {{
                const input = document.querySelector('#itemsTable tr:nth-child(' + (index + 1) + ') .product-search-input');
                const dropdown = document.getElementById('searchDropdown_' + index);
                
                items[index].product_id = parseInt(li.getAttribute('data-id'));
                items[index].product_name = li.getAttribute('data-name');
                items[index].alias1 = li.getAttribute('data-alias1') || '';
                items[index].alias2 = li.getAttribute('data-alias2') || '';
                items[index].spec = li.getAttribute('data-spec');
                items[index].unit = li.getAttribute('data-base-unit');
                items[index].base_unit = li.getAttribute('data-base-unit');
                items[index].unit_price = parseFloat(li.getAttribute('data-price')) || parseFloat(li.getAttribute('data-base-price')) || 0;
                items[index].base_price = parseFloat(li.getAttribute('data-price')) || parseFloat(li.getAttribute('data-base-price')) || items[index].unit_price;
                if (items[index].quantity === undefined || items[index].quantity === null) items[index].quantity = 0;
                if (items[index].base_quantity === undefined || items[index].base_quantity === null) items[index].base_quantity = 0;
                items[index].amount = (items[index].quantity || 0) * (items[index].unit_price || 0);
                
                input.value = items[index].product_name;
                dropdown.innerHTML = '';
                dropdown.style.display = 'none';
                
                fetch('/api/product/unit/list?product_id=' + items[index].product_id)
                    .then(res => res.json())
                    .then(units => {{
                        items[index].units = units;
                        renderItems();
                    }})
                    .catch(() => {{
                        items[index].units = [];
                        renderItems();
                    }});
            }}

            document.addEventListener('click', function(e) {{
                const dropdowns = document.querySelectorAll('.search-dropdown');
                dropdowns.forEach(d => {{
                    if (!d.contains(e.target) && !e.target.classList.contains('product-search-input') && e.target.id !== 'purchaserInput') {{
                        d.style.display = 'none';
                    }}
                }});
            }});

            function updateUnit(index, select) {{
                const opt = select.options[select.selectedIndex];
                const ratio = parseFloat(opt.getAttribute('data-ratio')) || 1;
                const unitPrice = parseFloat(opt.getAttribute('data-unit-price')) || 0;
                items[index].unit = opt.value;
                items[index].ratio = ratio;
                if (unitPrice > 0) {{
                    items[index].unit_price = unitPrice;
                }} else {{
                    const basePrice = parseFloat(opt.getAttribute('data-base-price')) || items[index].unit_price;
                    items[index].unit_price = basePrice * ratio;
                }}
                items[index].base_quantity = items[index].quantity * ratio;
                items[index].amount = items[index].unit_price * items[index].quantity;
                renderItems();
            }}

            function updateName(index, input) {{ items[index].product_name = input.value; }}
            function updateAlias1(index, input) {{ items[index].alias1 = input.value; }}
            function updateAlias2(index, input) {{ items[index].alias2 = input.value; }}
            function updateSpec(index, input) {{ items[index].spec = input.value; }}
            function updatePrice(index, input) {{ 
                items[index].unit_price = parseFloat(input.value) || 0; 
                items[index].amount = items[index].unit_price * items[index].quantity;
                renderItems();
            }}
            function updateQty(index, input) {{ 
                items[index].quantity = parseFloat(input.value) || 0; 
                items[index].base_quantity = items[index].quantity * (items[index].ratio || 1);
                items[index].amount = items[index].unit_price * items[index].quantity;
                renderItems();
            }}

            function handleEnterKey(event, index, cellIndex) {{
                const enterKeys = ['Enter', 'Next', 'Go', 'Done'];
                if (enterKeys.includes(event.key) || event.keyCode === 13) {{
                    event.preventDefault();
                    
                    const input = event.target;
                    if (cellIndex === 3) {{
                        items[index].quantity = parseFloat(input.value) || 0;
                        items[index].base_quantity = items[index].quantity * (items[index].ratio || 1);
                        items[index].amount = items[index].unit_price * items[index].quantity;
                    }} else if (cellIndex === 4) {{
                        items[index].unit_price = parseFloat(input.value) || 0;
                        items[index].amount = items[index].unit_price * items[index].quantity;
                    }}
                    renderItems();
                    
                    const nextIndex = index + 1;
                    if (nextIndex < items.length) {{
                        const table = document.getElementById('itemsTable');
                        if (table && table.rows[nextIndex]) {{
                            const nextRow = table.rows[nextIndex];
                            if (nextRow.cells[cellIndex]) {{
                                const targetInput = nextRow.cells[cellIndex].querySelector('input');
                                if (targetInput) {{
                                    targetInput.focus();
                                    try {{ targetInput.select(); }} catch(e) {{}}
                                }}
                            }}
                        }}
                    }}
                }}
            }}

            function updateRemark(index, input) {{ items[index].remark = input.value.trim(); }}

            let suppliers = [];
            async function loadSuppliers() {{
                const res = await fetch('/api/supplier/list');
                suppliers = await res.json();
            }}
            loadSuppliers();

            let warehouses = [];
            async function loadWarehouses() {{
                const res = await fetch('/api/warehouse/list');
                warehouses = await res.json();
            }}
            loadWarehouses();

            function showWarehouseDropdown(filter) {{
                const dropdown = document.getElementById('warehouseDropdown');
                if (!dropdown) return;
                let list = warehouses;
                if (filter) {{
                    const kw = filter.toLowerCase();
                    list = warehouses.filter(w => w.name.toLowerCase().includes(kw));
                }}
                if (list.length === 0) {{
                    dropdown.innerHTML = '<div class="p-2 text-muted">无匹配仓库</div>';
                    dropdown.style.display = 'block';
                    return;
                }}
                let html = '<ul class="search-results">';
                list.forEach(w => {{
                    html += '<li onclick="selectWarehouse(this)" data-id="' + w.id + '" data-name="' + w.name.replace(/&/g, '&amp;').replace(/"/g, '&quot;') + '">' + w.name + '</li>';
                }});
                html += '</ul>';
                dropdown.innerHTML = html;
                dropdown.style.display = 'block';
            }}

            function selectWarehouse(li) {{
                const input = document.getElementById('warehouseInput');
                const dropdown = document.getElementById('warehouseDropdown');
                if (li) {{
                    document.getElementById('warehouseId').value = li.getAttribute('data-id');
                    input.value = li.getAttribute('data-name');
                    input.readOnly = true;
                    dropdown.style.display = 'none';
                }}
            }}

            document.getElementById('warehouseInput').addEventListener('click', function() {{
                showWarehouseDropdown('');
            }});
            document.getElementById('warehouseInput').addEventListener('dblclick', function() {{
                this.readOnly = false;
                this.value = '';
                this.focus();
                showWarehouseDropdown('');
            }});
            document.getElementById('warehouseInput').addEventListener('input', function() {{
                showWarehouseDropdown(this.value);
            }});
            document.getElementById('warehouseInput').addEventListener('blur', function() {{
                setTimeout(() => {{
                    const dropdown = document.getElementById('warehouseDropdown');
                    if (dropdown) dropdown.style.display = 'none';
                }}, 200);
            }});

            function showItemSupplierDropdown(index, filter) {{
                const dropdown = document.getElementById('supplierDropdown_' + index);
                if (!dropdown) return;
                let list = suppliers;
                if (filter) {{
                    const kw = filter.toLowerCase();
                    list = suppliers.filter(s => s.name.toLowerCase().includes(kw));
                }}
                if (list.length === 0) {{
                    dropdown.innerHTML = '<div class="p-2 text-muted">无匹配供应商</div>';
                    dropdown.style.display = 'block';
                    return;
                }}
                let html = '<ul class="search-results">';
                list.forEach(s => {{
                    html += '<li onclick="selectItemSupplier(' + index + ', this)" data-id="' + s.id + '" data-name="' + s.name.replace(/&/g, '&amp;').replace(/"/g, '&quot;') + '">' + s.name + '</li>';
                }});
                html += '</ul>';
                dropdown.innerHTML = html;
                dropdown.style.display = 'block';
            }}

            function enableItemSupplierInput(index) {{
                const input = document.querySelector('#itemsTable tr:nth-child(' + (index + 1) + ') .supplier-input');
                if (input) {{
                    input.readOnly = false;
                    input.value = '';
                    input.focus();
                    showItemSupplierDropdown(index, '');
                }}
            }}

            function hideItemSupplierDropdown(index) {{
                setTimeout(() => {{
                    const dropdown = document.getElementById('supplierDropdown_' + index);
                    if (dropdown) {{
                        dropdown.style.display = 'none';
                    }}
                }}, 200);
            }}

            function selectItemSupplier(index, li) {{
                const input = document.querySelector('#itemsTable tr:nth-child(' + (index + 1) + ') .supplier-input');
                const dropdown = document.getElementById('supplierDropdown_' + index);
                if (li) {{
                    const id = li.getAttribute('data-id');
                    const name = li.getAttribute('data-name');
                    items[index].supplier_id = parseInt(id) || 0;
                    items[index].supplier_name = name;
                    if (input) {{
                        input.value = name;
                        input.readOnly = true;
                    }}
                    dropdown.style.display = 'none';
                }}
            }}

            let currentOrderId = null;

            async function saveOrder() {{
                const purchaserId = document.getElementById('purchaserId').value;
                if (!purchaserId) {{
                    alert('请选择采购单位');
                    return;
                }}
                if (items.length === 0) {{
                    alert('请添加商品明细');
                    return;
                }}
                const data = {{
                    id: currentOrderId,
                    purchaser_id: parseInt(purchaserId),
                    order_no: document.getElementById('orderNoInput').value,
                    order_date: document.getElementById('orderDateInput').value,
                    total_amount: parseFloat(document.getElementById('totalAmount').textContent),
                    discount_rate: parseFloat(document.getElementById('discountRateInput').value) || 0,
                    amount_reduction: parseFloat(document.getElementById('amountReductionInput').value) || 0,
                    final_amount: parseFloat(document.getElementById('finalAmount').textContent) || 0,
                    warehouse_id: parseInt(document.getElementById('warehouseId').value) || 0,
                    warehouse_name: document.getElementById('warehouseInput').value || '',
                    items: items,
                    remark: document.getElementById('remarkInput').value || null
                }};
                const isNew = !currentOrderId;
                const url = isNew ? '/api/sales_order/create' : '/api/sales_order/update';
                const res = await fetch(url, {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify(data)
                }});
                if (res.ok) {{
                    if (isNew) {{
                        resetForm();
                        alert('订单创建成功');
                    }} else {{
                        await loadOrderDetail(currentOrderId);
                        alert('订单保存成功');
                    }}
                }} else {{
                    alert('保存失败');
                }}
            }}

            let currentPage = 1;
            let currentKeyword = '';

            function resetSearch() {{
                document.getElementById('searchInput').value = '';
                currentKeyword = '';
                currentPage = 1;
                loadOrders();
            }}

            async function searchOrders() {{
                currentKeyword = document.getElementById('searchInput').value.trim();
                currentPage = 1;
                await loadOrders();
            }}

            async function loadOrders(page) {{
                if (page !== undefined) currentPage = page;
                let url = '/api/sales_order/list?page=' + currentPage + '&page_size=20';
                if (currentKeyword) {{
                    url += '&keyword=' + encodeURIComponent(currentKeyword);
                }}
                const res = await fetch(url);
                const result = await res.json();
                const orders = result.data || [];
                const tbody = document.getElementById('orderListBody');
                tbody.innerHTML = '';
                orders.forEach(order => {{
                    const selected = currentOrderId === order.id ? ' style="cursor: pointer; background-color: #fff3cd;"' : ' style="cursor: pointer;"';
                    const statusMap = {{
                        'pending': '{{"text":"待分拣","class":"bg-secondary"}}',
                        'sorting': '{{"text":"分拣中","class":"bg-primary"}}',
                        'sorted': '{{"text":"已分拣","class":"bg-success"}}',
                        'delivering': '{{"text":"配送中","class":"bg-warning text-dark"}}',
                        'delivered': '{{"text":"已送达","class":"bg-info text-dark"}}',
                        'accepted': '{{"text":"已验收","class":"bg-teal text-white"}}',
                        'settled': '{{"text":"已结算","class":"bg-purple text-white"}}'
                    }};
                    const statusInfo = JSON.parse(statusMap[order.status] || '{{"text":"未知","class":"bg-gray"}}');
                    const statusBadge = '<span class="badge ' + statusInfo.class + '">' + statusInfo.text + '</span>';
                    const nextStatusMap = {{
                        'pending': '{{"text":"开始分拣","status":"sorting"}}',
                        'sorting': '{{"text":"完成分拣","status":"sorted"}}',
                        'sorted': '{{"text":"开始配送","status":"delivering"}}',
                        'delivering': '{{"text":"确认送达","status":"delivered"}}',
                        'delivered': '{{"text":"确认验收","status":"accepted"}}',
                        'accepted': '{{"text":"确认结算","status":"settled"}}',
                        'settled': '{{"text":"","status":""}}'
                    }};
                    const nextInfo = JSON.parse(nextStatusMap[order.status] || '{{"text":"","status":""}}');
                    const nextBtn = nextInfo.text ? '<button onclick="event.stopPropagation(); updateOrderStatus(' + order.id + ', \'' + nextInfo.status + '\')" class="btn btn-primary btn-sm">' + nextInfo.text + '</button> ' : '';
                    tbody.innerHTML += '<tr onclick="loadOrderDetail(' + order.id + ')"' + selected + '>' +
                        '<td>' + order.id + '</td>' +
                        '<td>' + order.order_no + '</td>' +
                        '<td>' + order.order_date + '</td>' +
                        '<td>' + order.purchaser_name + '</td>' +
                        '<td>' + order.total_amount.toFixed(2) + '</td>' +
                        '<td>' + (order.total_amount * (1 - (order.discount_rate || 0) / 100)).toFixed(2) + '</td>' +
                        '<td>' + (order.amount_reduction || 0).toFixed(2) + '</td>' +
                        '<td>' + (order.final_amount || 0).toFixed(2) + '</td>' +
                        '<td>' + statusBadge + '</td>' +
                        '<td>' +
                        nextBtn +
                        '<button onclick="event.stopPropagation(); exportAcceptExcel(' + order.id + ')" class="btn btn-success btn-sm">导出验收单</button> ' +
                        '<button onclick="event.stopPropagation(); generatePurchaseOrders(' + order.id + ')" class="btn btn-info btn-sm">生成采购订单</button> ' +
                        '<button onclick="event.stopPropagation(); deleteOrder(' + order.id + ')" class="btn btn-danger btn-sm">删除</button>' +
                        '</td></tr>';
                }});
                renderPagination(result.page, result.total_pages, result.total);
            }}

            function renderPagination(page, totalPages, total) {{
                const container = document.getElementById('pagination');
                if (!container) return;
                if (totalPages <= 1) {{
                    container.innerHTML = '';
                    return;
                }}
                let html = '<nav aria-label="Page navigation"><ul class="pagination justify-content-center">';
                html += '<li class="page-item ' + (page <= 1 ? 'disabled' : '') + '"><a class="page-link" onclick="loadOrders(' + (page - 1) + ')">上一页</a></li>';
                
                const startPage = Math.max(1, page - 2);
                const endPage = Math.min(totalPages, page + 2);
                
                for (let i = startPage; i <= endPage; i++) {{
                    html += '<li class="page-item ' + (i === page ? 'active' : '') + '"><a class="page-link" onclick="loadOrders(' + i + ')">' + i + '</a></li>';
                }}
                
                html += '<li class="page-item ' + (page >= totalPages ? 'disabled' : '') + '"><a class="page-link" onclick="loadOrders(' + (page + 1) + ')">下一页</a></li>';
                html += '</ul></nav>';
                html += '<p class="text-center text-muted mt-2">共 ' + total + ' 条记录，当前第 ' + page + '/' + totalPages + ' 页</p>';
                container.innerHTML = html;
            }}

            async function loadOrderDetail(id) {{
                const res = await fetch('/api/sales_order/detail/' + id);
                const order = await res.json();
                currentOrderId = order.id;
                document.getElementById('formTitle').textContent = '编辑销售订单';
                document.getElementById('saveBtn').textContent = '保存修改';
                document.getElementById('purchaserId').value = order.purchaser_id;
                document.getElementById('purchaserInput').value = order.purchaser_name;
                document.getElementById('warehouseId').value = order.warehouse_id || 0;
                document.getElementById('warehouseInput').value = order.warehouse_name || '';
                document.getElementById('orderNoInput').value = order.order_no;
                document.getElementById('orderDateInput').value = order.order_date;
                document.getElementById('remarkInput').value = order.remark || '';
                document.getElementById('discountRateInput').value = order.discount_rate || 0;
                document.getElementById('amountReductionInput').value = order.amount_reduction || 0;
                
                items = [];
                for (const item of order.items) {{
                    const itemData = {{
                        product_id: item.product_id,
                        product_name: item.product_name,
                        alias1: item.alias1 || '',
                        alias2: item.alias2 || '',
                        spec: item.spec || '',
                        unit: item.unit || '',
                        unit_price: item.unit_price || 0,
                        quantity: item.quantity || 0,
                        base_quantity: item.base_quantity || 0,
                        amount: item.amount || 0,
                        remark: item.remark || '',
                        supplier_id: item.supplier_id || 0,
                        supplier_name: item.supplier_name || '',
                        base_unit: '',
                        base_price: 0,
                        units: []
                    }};
                    items.push(itemData);
                    
                    try {{
                        const productRes = await fetch('/api/product/by_id?id=' + item.product_id);
                        const product = await productRes.json();
                        if (product.id) {{
                            itemData.base_unit = product.base_unit || item.unit || '';
                            itemData.base_price = product.base_price || item.unit_price || 0;
                        }} else {{
                            itemData.base_unit = item.unit || '';
                            itemData.base_price = item.unit_price || 0;
                        }}
                    }} catch (e) {{
                        itemData.base_unit = item.unit || '';
                        itemData.base_price = item.unit_price || 0;
                    }}
                    
                    try {{
                        const unitsRes = await fetch('/api/product/unit/list?product_id=' + item.product_id);
                        itemData.units = await unitsRes.json();
                    }} catch (e) {{
                        itemData.units = [];
                    }}
                }}
                renderItems();
                loadOrders();
            }}

            function printAccept(id) {{
                window.open('/accept?order_id=' + id, '_blank');
            }}
            
            function exportAcceptExcel(id) {{
                window.location.href = '/api/sales_order/accept_excel/' + id;
            }}
            
            async function deleteOrder(id) {{
                if (!confirm('确定删除该订单？')) return;
                const res = await fetch('/api/sales_order/delete/' + id, {{ method: 'DELETE' }});
                if (res.ok) {{
                    loadOrders();
                    if (currentOrderId === id) {{
                        resetForm();
                    }}
                }}
            }}

            async function generatePurchaseOrders(id) {{
                const res = await fetch('/api/sales_order/generate_purchase/' + id, {{ method: 'POST' }});
                const data = await res.json();
                if (res.ok) {{
                    alert('成功生成 ' + data.count + ' 张采购订单');
                    loadOrders();
                }} else {{
                    alert('生成失败：' + (data.message || '未知错误'));
                }}
            }}

            function importSalesOrders() {{
                document.getElementById('salesOrderFileInput').click();
            }}
            async function handleSalesOrderFile(input) {{
                const file = input.files[0];
                if (!file) return;
                const reader = new FileReader();
                reader.onload = async function(e) {{
                    const text = e.target.result;
                    const res = await fetch('/api/sales_order/import', {{ method: 'POST', body: text }});
                    const result = await res.text();
                    alert(result);
                    if (res.ok) {{ loadOrders(); }}
                }};
                reader.readAsText(file, 'utf-8');
                input.value = '';
            }}

            async function updateOrderStatus(id, status) {{
                const res = await fetch('/api/sales_order/update_status', {{
                    method: 'POST',
                    headers: {{'Content-Type': 'application/json'}},
                    body: JSON.stringify({{id: id.toString(), status: status}})
                }});
                const text = await res.text();
                if (res.ok) {{
                    alert('状态更新成功');
                    loadOrders();
                }} else {{
                    alert('状态更新失败：' + text);
                }}
            }}

            function cancelOrder() {{
                resetForm();
            }}

            function resetForm() {{
                currentOrderId = null;
                document.getElementById('formTitle').textContent = '新建销售订单';
                document.getElementById('saveBtn').textContent = '保存销售订单';
                document.getElementById('purchaserId').value = '';
                document.getElementById('purchaserInput').value = '';
                document.getElementById('warehouseId').value = '';
                document.getElementById('warehouseInput').value = '';
                document.getElementById('orderNoInput').value = '';
                document.getElementById('orderDateInput').value = '';
                document.getElementById('remarkInput').value = '';
                document.getElementById('discountRateInput').value = '20';
                items = [];
                renderItems();
                generateOrderNo('sales');
                loadOrders();
            }}

            loadPurchasers();
            loadOrders();

            async function loadPurchasers() {{
                const res = await fetch('/api/purchaser/list');
                purchasers = await res.json();
            }}
        </script>
    "#, now);
    
    Html(layout_html("销售订单", "/sales", &content))
}

async fn page_query_purchase_order(headers: axum::http::HeaderMap) -> Html<String> {
    match check_page_permission(&headers, "/query/purchase_order").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let content = r#"
        <div class="card p-4">
            <h3>采购订单查询</h3>
            <div class="row mb-3">
                <div class="col-md-3">
                    <label>供应商：</label>
                    <select id="supplierId" class="form-control">
                        <option value="">全部供应商</option>
                    </select>
                </div>
                <div class="col-md-3">
                    <label>开始日期：</label>
                    <input type="date" id="startDate" class="form-control">
                </div>
                <div class="col-md-3">
                    <label>结束日期：</label>
                    <input type="date" id="endDate" class="form-control">
                </div>
                <div class="col-md-3">
                    <label>状态：</label>
                    <select id="status" class="form-control">
                        <option value="">全部状态</option>
                        <option value="未到货">未到货</option>
                        <option value="部分到货">部分到货</option>
                        <option value="全部到货">全部到货</option>
                        <option value="作废">作废</option>
                    </select>
                </div>
            </div>
            <button onclick="searchPurchaseOrders()" class="btn btn-primary">查询</button>
            <a href="/api/query/purchase_order/export" class="btn btn-success ml-2">导出Excel</a>
        </div>
        <div class="card p-4 mt-4">
            <table class="table table-bordered">
                <thead><tr><th>订单号</th><th>供应商</th><th>日期</th><th>金额</th><th>状态</th><th>操作</th></tr></thead>
                <tbody id="resultTable"></tbody>
            </table>
            <div id="pagination" class="mt-3"></div>
        </div>
        <div class="modal fade" id="detailModal" tabindex="-1" aria-labelledby="detailModalLabel" aria-hidden="true">
            <div class="modal-dialog modal-lg">
                <div class="modal-content">
                    <div class="modal-header">
                        <h5 class="modal-title" id="detailModalLabel">采购订单明细</h5>
                        <button type="button" class="btn-close" data-bs-dismiss="modal" aria-label="Close"></button>
                    </div>
                    <div class="modal-body">
                        <div class="mb-4">
                            <div class="row">
                                <div class="col-md-6"><strong>订单号：</strong><span id="modalOrderNo"></span></div>
                                <div class="col-md-6"><strong>供应商：</strong><span id="modalSupplierName"></span></div>
                            </div>
                            <div class="row mt-2">
                                <div class="col-md-6"><strong>订单日期：</strong><span id="modalOrderDate"></span></div>
                                <div class="col-md-6"><strong>订单状态：</strong><span id="modalStatus"></span></div>
                            </div>
                            <div class="row mt-2">
                                <div class="col-md-6"><strong>订单金额：</strong><span id="modalTotalAmount"></span></div>
                                <div class="col-md-6"><strong>实付金额：</strong><span id="modalFinalAmount"></span></div>
                            </div>
                            <div class="row mt-2">
                                <div class="col-md-6"><strong>入库仓库：</strong><span id="modalWarehouse"></span></div>
                                <div class="col-md-6"><strong>备注：</strong><span id="modalRemark"></span></div>
                            </div>
                        </div>
                        <table class="table table-striped table-bordered">
                            <thead><tr><th>商品名称</th><th>规格</th><th>单位</th><th>数量</th><th>单价</th><th>金额</th></tr></thead>
                            <tbody id="modalItems"></tbody>
                        </table>
                    </div>
                    <div class="modal-footer">
                        <button type="button" class="btn btn-secondary" data-bs-dismiss="modal">关闭</button>
                    </div>
                </div>
            </div>
        </div>
        <script>
            let currentPage = 1;

            async function loadSuppliers() {
                const res = await fetch('/api/supplier/list');
                const suppliers = await res.json();
                const select = document.getElementById('supplierId');
                suppliers.forEach(s => {
                    select.innerHTML += '<option value="' + s.id + '">' + s.name + '</option>';
                });
            }
            async function searchPurchaseOrders() {
                currentPage = 1;
                loadPurchaseOrders();
            }
            async function loadPurchaseOrders(page) {
                if (page !== undefined) currentPage = page;
                const url = '/api/query/purchase_order?supplier_id=' + document.getElementById('supplierId').value + 
                    '&start_date=' + document.getElementById('startDate').value + 
                    '&end_date=' + document.getElementById('endDate').value + 
                    '&status=' + document.getElementById('status').value +
                    '&page=' + currentPage + '&page_size=20';
                const res = await fetch(url);
                const result = await res.json();
                const data = result.data || [];
                const tbody = document.getElementById('resultTable');
                tbody.innerHTML = '';
                if (data.length === 0) {
                    tbody.innerHTML = '<tr><td colspan="6" class="text-center text-muted">暂无数据</td></tr>';
                    renderPagination(result.page, result.total_pages, result.total);
                    return;
                }
                let totalAmount = 0;
                data.forEach(order => {
                    totalAmount += order.final_amount || order.total_amount;
                    const statusBadge = order.status === '已完成' || order.status === '全部到货' 
                        ? '<span class="badge bg-success">' + order.status + '</span>'
                        : order.status === '作废'
                        ? '<span class="badge bg-danger">' + order.status + '</span>'
                        : '<span class="badge bg-warning">' + order.status + '</span>';
                    tbody.innerHTML += '<tr><td>' + order.order_no + '</td><td>' + order.supplier_name + '</td><td>' + order.order_date + '</td><td>¥' + (order.final_amount || order.total_amount).toFixed(2) + '</td><td>' + statusBadge + '</td><td><button onclick="viewDetail(' + order.id + ')" class="btn btn-info btn-sm">查看明细</button></td></tr>';
                });
                tbody.innerHTML += '<tr class="table-active fw-bold"><td colspan="3">合计</td><td>¥' + totalAmount.toFixed(2) + '</td><td colspan="2"></td></tr>';
                renderPagination(result.page, result.total_pages, result.total);
            }
            function renderPagination(page, totalPages, total) {
                const container = document.getElementById('pagination');
                if (!container) return;
                if (totalPages <= 1) {
                    container.innerHTML = '';
                    return;
                }
                let html = '<nav aria-label="Page navigation"><ul class="pagination justify-content-center">';
                html += '<li class="page-item ' + (page <= 1 ? 'disabled' : '') + '"><a class="page-link" onclick="loadPurchaseOrders(' + (page - 1) + ')">上一页</a></li>';
                
                const startPage = Math.max(1, page - 2);
                const endPage = Math.min(totalPages, page + 2);
                
                for (let i = startPage; i <= endPage; i++) {
                    html += '<li class="page-item ' + (i === page ? 'active' : '') + '"><a class="page-link" onclick="loadPurchaseOrders(' + i + ')">' + i + '</a></li>';
                }
                
                html += '<li class="page-item ' + (page >= totalPages ? 'disabled' : '') + '"><a class="page-link" onclick="loadPurchaseOrders(' + (page + 1) + ')">下一页</a></li>';
                html += '</ul></nav>';
                html += '<p class="text-center text-muted mt-2">共 ' + total + ' 条记录，当前第 ' + page + '/' + totalPages + ' 页</p>';
                container.innerHTML = html;
            }
            async function viewDetail(id) {
                const res = await fetch('/api/purchase_order/detail/' + id);
                const data = await res.json();

                document.getElementById('modalOrderNo').textContent = data.order_no;
                document.getElementById('modalSupplierName').textContent = data.supplier_name;
                document.getElementById('modalOrderDate').textContent = data.order_date || '';
                document.getElementById('modalStatus').textContent = data.status || '';
                document.getElementById('modalTotalAmount').textContent = '¥' + (data.total_amount || 0).toFixed(2);
                document.getElementById('modalFinalAmount').textContent = '¥' + (data.final_amount || 0).toFixed(2);
                document.getElementById('modalWarehouse').textContent = data.warehouse_name || '-';
                document.getElementById('modalRemark').textContent = data.remark || '-';

                const tbody = document.getElementById('modalItems');
                tbody.innerHTML = '';
                let itemTotal = 0;
                data.items.forEach(item => {
                    itemTotal += item.amount || 0;
                    tbody.innerHTML += '<tr><td>' + (item.product_name || '') + '</td><td>' + (item.spec || '-') + '</td><td>' + (item.unit || '') + '</td><td>' + (item.quantity || 0).toFixed(2) + '</td><td>¥' + (item.unit_price || 0).toFixed(2) + '</td><td>¥' + (item.amount || 0).toFixed(2) + '</td></tr>';
                });
                tbody.innerHTML += '<tr class="table-active fw-bold"><td colspan="5">合计</td><td>¥' + itemTotal.toFixed(2) + '</td></tr>';

                const modal = new bootstrap.Modal(document.getElementById('detailModal'));
                modal.show();
            }
            loadSuppliers();
        </script>
    "#;
    Html(layout_html("采购订单查询", "/query/purchase_order", &content))
}

async fn page_query_sales_order(headers: axum::http::HeaderMap) -> Html<String> {
    match check_page_permission(&headers, "/query/sales_order").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let content = r#"
        <div class="card p-4">
            <h3>销售订单查询</h3>
            <div class="row mb-3">
                <div class="col-md-3">
                    <label>采购单位：</label>
                    <select id="purchaserId" class="form-control">
                        <option value="">全部采购单位</option>
                    </select>
                </div>
                <div class="col-md-3">
                    <label>开始日期：</label>
                    <input type="date" id="startDate" class="form-control">
                </div>
                <div class="col-md-3">
                    <label>结束日期：</label>
                    <input type="date" id="endDate" class="form-control">
                </div>
                <div class="col-md-3">
                    <label>状态：</label>
                    <select id="status" class="form-control">
                        <option value="">全部状态</option>
                        <option value="未发货">未发货</option>
                        <option value="部分发货">部分发货</option>
                        <option value="已完成">已完成</option>
                        <option value="取消">取消</option>
                    </select>
                </div>
            </div>
            <button onclick="searchSalesOrders()" class="btn btn-primary">查询</button>
            <button onclick="exportSalesOrders()" class="btn btn-success ml-2">导出Excel</button>
        </div>
        <div class="card p-4 mt-4">
            <table class="table table-bordered">
                <thead><tr><th>订单号</th><th>采购单位</th><th>日期</th><th>金额</th><th>下浮后</th><th>状态</th><th>操作</th></tr></thead>
                <tbody id="resultTable"></tbody>
            </table>
            <div id="pagination" class="mt-3"></div>
        </div>
        <div class="modal fade" id="detailModal" tabindex="-1" aria-labelledby="detailModalLabel" aria-hidden="true">
            <div class="modal-dialog modal-lg">
                <div class="modal-content">
                    <div class="modal-header">
                        <h5 class="modal-title" id="detailModalLabel">订单明细</h5>
                        <button type="button" class="btn-close" data-bs-dismiss="modal" aria-label="Close"></button>
                    </div>
                    <div class="modal-body">
                        <div class="mb-4">
                            <div class="row">
                                <div class="col-md-6"><strong>订单号：</strong><span id="modalOrderNo"></span></div>
                                <div class="col-md-6"><strong>采购单位：</strong><span id="modalPurchaserName"></span></div>
                            </div>
                            <div class="row mt-2">
                                <div class="col-md-6"><strong>订单日期：</strong><span id="modalOrderDate"></span></div>
                                <div class="col-md-6"><strong>订单状态：</strong><span id="modalStatus"></span></div>
                            </div>
                            <div class="row mt-2">
                                <div class="col-md-6"><strong>订单金额：</strong><span id="modalTotalAmount"></span></div>
                                <div class="col-md-6"><strong>下浮后金额：</strong><span id="modalFinalAmount"></span></div>
                            </div>
                            <div class="row mt-2">
                                <div class="col-md-12"><strong>备注：</strong><span id="modalRemark"></span></div>
                            </div>
                        </div>
                        <table class="table table-striped table-bordered">
                            <thead><tr><th>商品名称</th><th>规格</th><th>单位</th><th>数量</th><th>单价</th><th>金额</th></tr></thead>
                            <tbody id="modalItems"></tbody>
                        </table>
                    </div>
                    <div class="modal-footer">
                        <button type="button" class="btn btn-secondary" data-bs-dismiss="modal">关闭</button>
                    </div>
                </div>
            </div>
        </div>
        <script>
            let currentPage = 1;
            async function loadPurchasers() {
                const res = await fetch('/api/purchaser/list');
                const purchasers = await res.json();
                const select = document.getElementById('purchaserId');
                purchasers.forEach(p => {
                    select.innerHTML += '<option value="' + p.id + '">' + p.name + '</option>';
                });
            }
            async function searchSalesOrders() {
                currentPage = 1;
                loadData();
            }
            async function loadData(page) {
                if (page !== undefined) currentPage = page;
                const url = '/api/query/sales_order?purchaser_id=' + document.getElementById('purchaserId').value + 
                    '&start_date=' + document.getElementById('startDate').value + 
                    '&end_date=' + document.getElementById('endDate').value + 
                    '&status=' + document.getElementById('status').value +
                    '&page=' + currentPage + '&page_size=20';
                const res = await fetch(url);
                const result = await res.json();
                const data = result.data || [];
                const tbody = document.getElementById('resultTable');
                tbody.innerHTML = '';
                if (data.length === 0) {
                    tbody.innerHTML = '<tr><td colspan="7" class="text-center text-muted">暂无数据</td></tr>';
                    renderPagination(result.page, result.total_pages, result.total);
                    return;
                }
                let totalAmt = 0, totalFinal = 0;
                data.forEach(order => {
                    totalAmt += order.total_amount;
                    totalFinal += order.final_amount || 0;
                    let statusBadge = '';
                    if (order.status === '已完成') {
                        statusBadge = '<span class="badge bg-success">' + order.status + '</span>';
                    } else if (order.status === '未发货') {
                        statusBadge = '<span class="badge bg-secondary">' + order.status + '</span>';
                    } else if (order.status === '部分发货') {
                        statusBadge = '<span class="badge bg-warning text-dark">' + order.status + '</span>';
                    } else if (order.status === '取消') {
                        statusBadge = '<span class="badge bg-danger">' + order.status + '</span>';
                    } else {
                        statusBadge = '<span class="badge bg-info">' + order.status + '</span>';
                    }
                    tbody.innerHTML += '<tr><td>' + order.order_no + '</td><td>' + order.purchaser_name + '</td><td>' + order.order_date + '</td><td>¥' + order.total_amount.toFixed(2) + '</td><td>¥' + (order.final_amount || 0).toFixed(2) + '</td><td>' + statusBadge + '</td><td><button onclick="viewDetail(' + order.id + ')" class="btn btn-info btn-sm">查看明细</button></td></tr>';
                });
                tbody.innerHTML += '<tr class="table-active fw-bold"><td colspan="3">合计</td><td>¥' + totalAmt.toFixed(2) + '</td><td>¥' + totalFinal.toFixed(2) + '</td><td colspan="2"></td></tr>';
                renderPagination(result.page, result.total_pages, result.total);
            }
            function renderPagination(page, totalPages, total) {
                const container = document.getElementById('pagination');
                if (!container) return;
                if (totalPages <= 1) { container.innerHTML = ''; return; }
                let html = '<nav><ul class="pagination justify-content-center">';
                html += '<li class="page-item ' + (page <= 1 ? 'disabled' : '') + '"><a class="page-link" onclick="loadData(' + (page - 1) + ')">上一页</a></li>';
                const startPage = Math.max(1, page - 2);
                const endPage = Math.min(totalPages, page + 2);
                for (let i = startPage; i <= endPage; i++) {
                    html += '<li class="page-item ' + (i === page ? 'active' : '') + '"><a class="page-link" onclick="loadData(' + i + ')">' + i + '</a></li>';
                }
                html += '<li class="page-item ' + (page >= totalPages ? 'disabled' : '') + '"><a class="page-link" onclick="loadData(' + (page + 1) + ')">下一页</a></li>';
                html += '</ul></nav>';
                html += '<p class="text-center text-muted mt-2">共 ' + total + ' 条记录，当前第 ' + page + '/' + totalPages + ' 页</p>';
                container.innerHTML = html;
            }
            async function viewDetail(id) {
                const res = await fetch('/api/sales_order/detail/' + id);
                const data = await res.json();

                document.getElementById('modalOrderNo').textContent = data.order_no;
                document.getElementById('modalPurchaserName').textContent = data.purchaser_name;
                document.getElementById('modalOrderDate').textContent = data.order_date || '';
                document.getElementById('modalStatus').textContent = data.status || '';
                document.getElementById('modalTotalAmount').textContent = '¥' + (data.total_amount || 0).toFixed(2);
                document.getElementById('modalFinalAmount').textContent = '¥' + (data.final_amount || 0).toFixed(2);
                document.getElementById('modalRemark').textContent = data.remark || '-';

                const tbody = document.getElementById('modalItems');
                tbody.innerHTML = '';
                let itemTotal = 0;
                data.items.forEach(item => {
                    itemTotal += item.amount || 0;
                    tbody.innerHTML += '<tr><td>' + (item.product_name || '') + '</td><td>' + (item.spec || '-') + '</td><td>' + (item.unit || '') + '</td><td>' + (item.quantity || 0).toFixed(2) + '</td><td>¥' + (item.unit_price || 0).toFixed(2) + '</td><td>¥' + (item.amount || 0).toFixed(2) + '</td></tr>';
                });
                tbody.innerHTML += '<tr class="table-active fw-bold"><td colspan="5">合计</td><td>¥' + itemTotal.toFixed(2) + '</td></tr>';

                const modal = new bootstrap.Modal(document.getElementById('detailModal'));
                modal.show();
            }
            loadPurchasers();
            searchSalesOrders();
            function exportSalesOrders() {
                const url = '/api/query/sales_order/export?purchaser_id=' + document.getElementById('purchaserId').value + 
                    '&start_date=' + document.getElementById('startDate').value + 
                    '&end_date=' + document.getElementById('endDate').value + 
                    '&status=' + document.getElementById('status').value;
                window.location.href = url;
            }
        </script>
    "#;
    Html(layout_html("销售订单查询", "/query/sales_order", &content))
}

async fn page_query_stock_balance(headers: axum::http::HeaderMap) -> Html<String> {
    match check_page_permission(&headers, "/query/stock_balance").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let content = r#"
        <div class="card p-4">
            <h3>实时库存余额查询</h3>
            <div class="row mb-3">
                <div class="col-md-4">
                    <label>商品名称：</label>
                    <input type="text" id="productName" class="form-control" placeholder="输入商品名称搜索">
                </div>
                <div class="col-md-4">
                    <label>分类：</label>
                    <select id="categoryId" class="form-control">
                        <option value="">全部分类</option>
                    </select>
                </div>
            </div>
            <button onclick="searchStock()" class="btn btn-primary">查询</button>
            <a href="/api/query/stock_balance/export" class="btn btn-success ml-2">导出Excel</a>
        </div>
        <div class="card p-4 mt-4">
            <table class="table table-bordered">
                <thead><tr><th>商品名称</th><th>规格</th><th>单位</th><th>库存数量</th><th>库存金额</th><th>操作</th></tr></thead>
                <tbody id="resultTable"></tbody>
            </table>
        </div>
        <script>
            async function loadCategories() {
                const res = await fetch('/api/category/list');
                const categories = await res.json();
                const select = document.getElementById('categoryId');
                categories.forEach(c => {
                    select.innerHTML += '<option value="' + c.id + '">' + c.name + '</option>';
                });
            }
            async function searchStock() {
                const url = '/api/query/stock_balance?product_name=' + encodeURIComponent(document.getElementById('productName').value) + 
                    '&category_id=' + document.getElementById('categoryId').value;
                const res = await fetch(url);
                const data = await res.json();
                const tbody = document.getElementById('resultTable');
                tbody.innerHTML = '';
                data.forEach(item => {
                    tbody.innerHTML += '<tr><td>' + item.product_name + '</td><td>' + (item.spec || '') + '</td><td>' + (item.unit || '') + '</td><td>' + item.quantity.toFixed(2) + '</td><td>' + item.amount.toFixed(2) + '</td><td><button onclick="viewFlow(' + item.product_id + ')" class="btn btn-info btn-sm">查看流水</button></td></tr>';
                });
            }
            async function viewFlow(productId) {
                const url = '/api/query/stock_flow?product_id=' + productId;
                const res = await fetch(url);
                const data = await res.json();
                let detail = '库存流水:\\n';
                data.forEach(flow => {
                    detail += flow.type + ' ' + flow.quantity.toFixed(2) + ' ' + flow.create_time + '\\n';
                });
                alert(detail);
            }
            loadCategories();
            searchStock();
        </script>
    "#;
    Html(layout_html("实时库存余额查询", "/query/stock_balance", &content))
}

async fn page_query_overview() -> Html<String> {
    let content = r#"
        <div class="card p-4">
            <h3>进销存汇总报表</h3>
            <div class="row mb-3">
                <div class="col-md-3">
                    <label>月份：</label>
                    <input type="month" id="month" class="form-control">
                </div>
            </div>
            <button onclick="loadOverview()" class="btn btn-primary">查询</button>
        </div>
        <div class="row mt-4">
            <div class="col-md-3">
                <div class="card bg-success text-white p-4">
                    <h4>总进货金额</h4>
                    <p class="text-2xl" id="purchaseTotal">¥0.00</p>
                </div>
            </div>
            <div class="col-md-3">
                <div class="card bg-primary text-white p-4">
                    <h4>总销售金额</h4>
                    <p class="text-2xl" id="salesTotal">¥0.00</p>
                </div>
            </div>
            <div class="col-md-3">
                <div class="card bg-warning text-white p-4">
                    <h4>库存金额</h4>
                    <p class="text-2xl" id="stockTotal">¥0.00</p>
                </div>
            </div>
            <div class="col-md-3">
                <div class="card bg-info text-white p-4">
                    <h4>本期毛利</h4>
                    <p class="text-2xl" id="profitTotal">¥0.00</p>
                </div>
            </div>
        </div>
        <div class="card p-4 mt-4">
            <h4>采购汇总</h4>
            <table class="table table-bordered">
                <thead><tr><th>供应商</th><th>采购金额</th><th>采购数量</th></tr></thead>
                <tbody id="purchaseSummary"></tbody>
            </table>
        </div>
        <div class="card p-4 mt-4">
            <h4>销售汇总</h4>
            <table class="table table-bordered">
                <thead><tr><th>采购单位</th><th>销售金额</th><th>销售数量</th></tr></thead>
                <tbody id="salesSummary"></tbody>
            </table>
        </div>
        <script>
            async function loadOverview() {
                const month = document.getElementById('month').value;
                const url = '/api/query/overview?month=' + month;
                const res = await fetch(url);
                const data = await res.json();
                
                document.getElementById('purchaseTotal').textContent = '¥' + data.purchase_total.toFixed(2);
                document.getElementById('salesTotal').textContent = '¥' + data.sales_total.toFixed(2);
                document.getElementById('stockTotal').textContent = '¥' + data.stock_total.toFixed(2);
                document.getElementById('profitTotal').textContent = '¥' + data.profit_total.toFixed(2);
                
                let purchaseHtml = '';
                data.purchase_by_supplier.forEach(item => {
                    purchaseHtml += '<tr><td>' + item.name + '</td><td>' + item.amount.toFixed(2) + '</td><td>' + item.quantity.toFixed(2) + '</td></tr>';
                });
                document.getElementById('purchaseSummary').innerHTML = purchaseHtml;
                
                let salesHtml = '';
                data.sales_by_purchaser.forEach(item => {
                    salesHtml += '<tr><td>' + item.name + '</td><td>' + item.amount.toFixed(2) + '</td><td>' + item.quantity.toFixed(2) + '</td></tr>';
                });
                document.getElementById('salesSummary').innerHTML = salesHtml;
            }
            loadOverview();
        </script>
    "#;
    Html(layout_html("进销存汇总报表", "/query/overview", &content))
}

async fn page_query_purchase_price() -> Html<String> {
    let content = r#"
        <div class="card p-4">
            <h3>采购价格查询</h3>
            <div class="row mb-3">
                <div class="col-md-4">
                    <label>商品名称：</label>
                    <input type="text" id="productName" class="form-control" placeholder="输入商品名称">
                </div>
                <div class="col-md-4">
                    <label>供应商：</label>
                    <select id="supplierId" class="form-control">
                        <option value="">全部供应商</option>
                    </select>
                </div>
            </div>
            <button onclick="searchPurchasePrice()" class="btn btn-primary">查询</button>
        </div>
        <div class="card p-4 mt-4">
            <table class="table table-bordered">
                <thead><tr><th>商品名称</th><th>规格</th><th>供应商</th><th>采购单价</th><th>采购日期</th><th>采购数量</th></tr></thead>
                <tbody id="resultTable"></tbody>
            </table>
            <div id="pagination" class="mt-3"></div>
        </div>
        <script>
            let currentPage = 1;
            async function loadSuppliers() {
                const res = await fetch('/api/supplier/list');
                const suppliers = await res.json();
                const select = document.getElementById('supplierId');
                suppliers.forEach(s => {
                    select.innerHTML += '<option value="' + s.id + '">' + s.name + '</option>';
                });
            }
            async function searchPurchasePrice() {
                currentPage = 1;
                loadData();
            }
            async function loadData(page) {
                if (page !== undefined) currentPage = page;
                const url = '/api/query/purchase_price?product_name=' + encodeURIComponent(document.getElementById('productName').value) + 
                    '&supplier_id=' + document.getElementById('supplierId').value +
                    '&page=' + currentPage + '&page_size=20';
                const res = await fetch(url);
                const result = await res.json();
                const data = result.data || [];
                const tbody = document.getElementById('resultTable');
                tbody.innerHTML = '';
                if (data.length === 0) {
                    tbody.innerHTML = '<tr><td colspan="6" class="text-center text-muted">暂无数据</td></tr>';
                    renderPagination(result.page, result.total_pages, result.total);
                    return;
                }
                data.forEach(item => {
                    tbody.innerHTML += '<tr><td>' + item.product_name + '</td><td>' + (item.spec || '-') + '</td><td>' + item.supplier_name + '</td><td>¥' + item.unit_price.toFixed(2) + '/' + (item.unit || '') + '</td><td>' + item.order_date + '</td><td>' + item.quantity.toFixed(2) + (item.unit || '') + '</td></tr>';
                });
                renderPagination(result.page, result.total_pages, result.total);
            }
            function renderPagination(page, totalPages, total) {
                const container = document.getElementById('pagination');
                if (!container) return;
                if (totalPages <= 1) { container.innerHTML = ''; return; }
                let html = '<nav><ul class="pagination justify-content-center">';
                html += '<li class="page-item ' + (page <= 1 ? 'disabled' : '') + '"><a class="page-link" onclick="loadData(' + (page - 1) + ')">上一页</a></li>';
                const startPage = Math.max(1, page - 2);
                const endPage = Math.min(totalPages, page + 2);
                for (let i = startPage; i <= endPage; i++) {
                    html += '<li class="page-item ' + (i === page ? 'active' : '') + '"><a class="page-link" onclick="loadData(' + i + ')">' + i + '</a></li>';
                }
                html += '<li class="page-item ' + (page >= totalPages ? 'disabled' : '') + '"><a class="page-link" onclick="loadData(' + (page + 1) + ')">下一页</a></li>';
                html += '</ul></nav>';
                html += '<p class="text-center text-muted mt-2">共 ' + total + ' 条记录，当前第 ' + page + '/' + totalPages + ' 页</p>';
                container.innerHTML = html;
            }
            loadSuppliers();
        </script>
    "#;
    Html(layout_html("采购价格查询", "/query/purchase_price", &content))
}

async fn page_query_sales_price(headers: axum::http::HeaderMap) -> Html<String> {
    match check_page_permission(&headers, "/query/sales_price").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let content = r#"
        <div class="card p-4">
            <h3>销售价格查询</h3>
            <div class="row mb-3">
                <div class="col-md-4">
                    <label>商品名称：</label>
                    <input type="text" id="productName" class="form-control" placeholder="输入商品名称">
                </div>
                <div class="col-md-4">
                    <label>采购单位：</label>
                    <select id="purchaserId" class="form-control">
                        <option value="">全部采购单位</option>
                    </select>
                </div>
            </div>
            <button onclick="searchSalesPrice()" class="btn btn-primary">查询</button>
        </div>
        <div class="card p-4 mt-4">
            <table class="table table-bordered">
                <thead><tr><th>商品名称</th><th>规格</th><th>采购单位</th><th>销售单价</th><th>销售日期</th><th>销售数量</th></tr></thead>
                <tbody id="resultTable"></tbody>
            </table>
            <div id="pagination" class="mt-3"></div>
        </div>
        <script>
            let currentPage = 1;
            async function loadPurchasers() {
                const res = await fetch('/api/purchaser/list');
                const purchasers = await res.json();
                const select = document.getElementById('purchaserId');
                purchasers.forEach(p => {
                    select.innerHTML += '<option value="' + p.id + '">' + p.name + '</option>';
                });
            }
            async function searchSalesPrice() {
                currentPage = 1;
                loadData();
            }
            async function loadData(page) {
                if (page !== undefined) currentPage = page;
                const url = '/api/query/sales_price?product_name=' + encodeURIComponent(document.getElementById('productName').value) + 
                    '&purchaser_id=' + document.getElementById('purchaserId').value +
                    '&page=' + currentPage + '&page_size=20';
                const res = await fetch(url);
                const result = await res.json();
                const data = result.data || [];
                const tbody = document.getElementById('resultTable');
                tbody.innerHTML = '';
                if (data.length === 0) {
                    tbody.innerHTML = '<tr><td colspan="6" class="text-center text-muted">暂无数据</td></tr>';
                    renderPagination(result.page, result.total_pages, result.total);
                    return;
                }
                let totalQty = 0;
                data.forEach(item => {
                    totalQty += item.quantity;
                    tbody.innerHTML += '<tr><td>' + item.product_name + '</td><td>' + (item.spec || '-') + '</td><td>' + item.purchaser_name + '</td><td>¥' + item.unit_price.toFixed(2) + '</td><td>' + item.order_date + '</td><td>' + item.quantity.toFixed(2) + '</td></tr>';
                });
                tbody.innerHTML += '<tr class="table-active fw-bold"><td colspan="5">合计</td><td>' + totalQty.toFixed(2) + '</td></tr>';
                renderPagination(result.page, result.total_pages, result.total);
            }
            function renderPagination(page, totalPages, total) {
                const container = document.getElementById('pagination');
                if (!container) return;
                if (totalPages <= 1) { container.innerHTML = ''; return; }
                let html = '<nav><ul class="pagination justify-content-center">';
                html += '<li class="page-item ' + (page <= 1 ? 'disabled' : '') + '"><a class="page-link" onclick="loadData(' + (page - 1) + ')">上一页</a></li>';
                const startPage = Math.max(1, page - 2);
                const endPage = Math.min(totalPages, page + 2);
                for (let i = startPage; i <= endPage; i++) {
                    html += '<li class="page-item ' + (i === page ? 'active' : '') + '"><a class="page-link" onclick="loadData(' + i + ')">' + i + '</a></li>';
                }
                html += '<li class="page-item ' + (page >= totalPages ? 'disabled' : '') + '"><a class="page-link" onclick="loadData(' + (page + 1) + ')">下一页</a></li>';
                html += '</ul></nav>';
                html += '<p class="text-center text-muted mt-2">共 ' + total + ' 条记录，当前第 ' + page + '/' + totalPages + ' 页</p>';
                container.innerHTML = html;
            }
            loadPurchasers();
            searchSalesPrice();
        </script>
    "#;
    Html(layout_html("销售价格查询", "/query/sales_price", &content))
}

async fn page_query_supplier_balance() -> Html<String> {
    let content = r#"
        <div class="card p-4">
            <h3>供应商往来对账</h3>
            <button onclick="searchSupplierBalance()" class="btn btn-primary">查询</button>
            <a href="/api/query/supplier_balance/export" class="btn btn-success ml-2">导出Excel</a>
        </div>
        <div class="card p-4 mt-4">
            <table class="table table-bordered">
                <thead><tr><th>供应商名称</th><th>本期进货总额</th><th>已付款</th><th>未付款</th><th>预付款余额</th></tr></thead>
                <tbody id="resultTable"></tbody>
            </table>
        </div>
        <script>
            async function searchSupplierBalance() {
                const res = await fetch('/api/query/supplier_balance');
                const data = await res.json();
                const tbody = document.getElementById('resultTable');
                tbody.innerHTML = '';
                if (data.length === 0) {
                    tbody.innerHTML = '<tr><td colspan="5" class="text-center text-muted">暂无数据</td></tr>';
                    return;
                }
                let totalPurchase = 0, totalPaid = 0, totalUnpaid = 0;
                data.forEach(item => {
                    totalPurchase += item.purchase_total;
                    totalPaid += item.paid_total;
                    totalUnpaid += item.unpaid;
                    tbody.innerHTML += '<tr><td>' + item.name + '</td><td>¥' + item.purchase_total.toFixed(2) + '</td><td>¥' + item.paid_total.toFixed(2) + '</td><td>¥' + item.unpaid.toFixed(2) + '</td><td>¥' + item.prepay_balance.toFixed(2) + '</td></tr>';
                });
                tbody.innerHTML += '<tr class="table-active fw-bold"><td>合计</td><td>¥' + totalPurchase.toFixed(2) + '</td><td>¥' + totalPaid.toFixed(2) + '</td><td>¥' + totalUnpaid.toFixed(2) + '</td><td></td></tr>';
            }
            searchSupplierBalance();
        </script>
    "#;
    Html(layout_html("供应商往来对账", "/query/supplier_balance", &content))
}

async fn page_query_purchaser_balance(headers: axum::http::HeaderMap) -> Html<String> {
    match check_page_permission(&headers, "/query/purchaser_balance").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let content = r#"
        <div class="card p-4">
            <h3>采购方应收对账</h3>
            <button onclick="searchPurchaserBalance()" class="btn btn-primary">查询</button>
            <a href="/api/query/purchaser_balance/export" class="btn btn-success ml-2">导出Excel</a>
        </div>
        <div class="card p-4 mt-4">
            <table class="table table-bordered">
                <thead><tr><th>采购单位名称</th><th>累计销售</th><th>已收款</th><th>未收款</th><th>预收款余额</th></tr></thead>
                <tbody id="resultTable"></tbody>
            </table>
        </div>
        <script>
            async function searchPurchaserBalance() {
                const res = await fetch('/api/query/purchaser_balance');
                const data = await res.json();
                const tbody = document.getElementById('resultTable');
                tbody.innerHTML = '';
                if (data.length === 0) {
                    tbody.innerHTML = '<tr><td colspan="5" class="text-center text-muted">暂无数据</td></tr>';
                    return;
                }
                let totalSales = 0, totalReceived = 0, totalUnreceived = 0;
                data.forEach(item => {
                    totalSales += item.sales_total;
                    totalReceived += item.received_total;
                    totalUnreceived += item.unreceived;
                    tbody.innerHTML += '<tr><td>' + item.name + '</td><td>¥' + item.sales_total.toFixed(2) + '</td><td>¥' + item.received_total.toFixed(2) + '</td><td>¥' + item.unreceived.toFixed(2) + '</td><td>¥' + item.prepay_balance.toFixed(2) + '</td></tr>';
                });
                tbody.innerHTML += '<tr class="table-active fw-bold"><td>合计</td><td>¥' + totalSales.toFixed(2) + '</td><td>¥' + totalReceived.toFixed(2) + '</td><td>¥' + totalUnreceived.toFixed(2) + '</td><td></td></tr>';
            }
            searchPurchaserBalance();
        </script>
    "#;
    Html(layout_html("采购方应收对账", "/query/purchaser_balance", &content))
}

async fn page_query_purchase_summary() -> Html<String> {
    let content = r#"
        <div class="card p-4">
            <h3>采购汇总统计</h3>
            <div class="row mb-3">
                <div class="col-md-3">
                    <label>开始日期：</label>
                    <input type="date" id="startDate" class="form-control">
                </div>
                <div class="col-md-3">
                    <label>结束日期：</label>
                    <input type="date" id="endDate" class="form-control">
                </div>
            </div>
            <button onclick="searchPurchaseSummary()" class="btn btn-primary">查询</button>
        </div>
        <div class="card p-4 mt-4">
            <h4>按供应商汇总</h4>
            <table class="table table-bordered">
                <thead><tr><th>供应商</th><th>采购数量</th><th>采购金额</th><th>平均成本</th></tr></thead>
                <tbody id="supplierSummary"></tbody>
            </table>
        </div>
        <div class="card p-4 mt-4">
            <h4>按商品汇总</h4>
            <table class="table table-bordered">
                <thead><tr><th>商品名称</th><th>规格</th><th>采购数量</th><th>采购金额</th><th>平均单价</th></tr></thead>
                <tbody id="productSummary"></tbody>
            </table>
        </div>
        <script>
            async function searchPurchaseSummary() {
                const url = '/api/query/purchase_summary?start_date=' + document.getElementById('startDate').value + '&end_date=' + document.getElementById('endDate').value;
                const res = await fetch(url);
                const data = await res.json();
                
                let supplierHtml = '';
                if (data.by_supplier.length === 0) {
                    supplierHtml = '<tr><td colspan="4" class="text-center text-muted">暂无数据</td></tr>';
                } else {
                    let totalQty = 0, totalAmt = 0;
                    data.by_supplier.forEach(item => {
                        totalQty += item.quantity;
                        totalAmt += item.amount;
                        supplierHtml += '<tr><td>' + item.name + '</td><td>' + item.quantity.toFixed(2) + '</td><td>¥' + item.amount.toFixed(2) + '</td><td>¥' + (item.quantity > 0 ? (item.amount / item.quantity).toFixed(2) : '0.00') + '</td></tr>';
                    });
                    supplierHtml += '<tr class="table-active fw-bold"><td>合计</td><td>' + totalQty.toFixed(2) + '</td><td>¥' + totalAmt.toFixed(2) + '</td><td></td></tr>';
                }
                document.getElementById('supplierSummary').innerHTML = supplierHtml;
                
                let productHtml = '';
                if (data.by_product.length === 0) {
                    productHtml = '<tr><td colspan="5" class="text-center text-muted">暂无数据</td></tr>';
                } else {
                    let totalQty = 0, totalAmt = 0;
                    data.by_product.forEach(item => {
                        totalQty += item.quantity;
                        totalAmt += item.amount;
                        productHtml += '<tr><td>' + item.product_name + '</td><td>' + (item.spec || '-') + '</td><td>' + item.quantity.toFixed(2) + '</td><td>¥' + item.amount.toFixed(2) + '</td><td>¥' + (item.quantity > 0 ? (item.amount / item.quantity).toFixed(2) : '0.00') + '</td></tr>';
                    });
                    productHtml += '<tr class="table-active fw-bold"><td colspan="2">合计</td><td>' + totalQty.toFixed(2) + '</td><td>¥' + totalAmt.toFixed(2) + '</td><td></td></tr>';
                }
                document.getElementById('productSummary').innerHTML = productHtml;
            }
        </script>
    "#;
    Html(layout_html("采购汇总统计", "/query/purchase_summary", &content))
}

async fn page_query_sales_summary(headers: axum::http::HeaderMap) -> Html<String> {
    match check_page_permission(&headers, "/query/sales_summary").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let content = r#"
        <div class="card p-4">
            <h3>销售汇总报表</h3>
            <div class="row mb-3">
                <div class="col-md-3">
                    <label>开始日期：</label>
                    <input type="date" id="startDate" class="form-control">
                </div>
                <div class="col-md-3">
                    <label>结束日期：</label>
                    <input type="date" id="endDate" class="form-control">
                </div>
            </div>
            <button onclick="searchSalesSummary()" class="btn btn-primary">查询</button>
        </div>
        <div class="card p-4 mt-4">
            <h4>按采购单位汇总</h4>
            <table class="table table-bordered">
                <thead><tr><th>采购单位</th><th>销售数量</th><th>销售金额</th><th>毛利</th><th>毛利率</th></tr></thead>
                <tbody id="purchaserSummary"></tbody>
            </table>
        </div>
        <div class="card p-4 mt-4">
            <h4>按商品汇总</h4>
            <table class="table table-bordered">
                <thead><tr><th>商品名称</th><th>规格</th><th>销售数量</th><th>销售金额</th><th>毛利</th></tr></thead>
                <tbody id="productSummary"></tbody>
            </table>
        </div>
        <script>
            async function searchSalesSummary() {
                const url = '/api/query/sales_summary?start_date=' + document.getElementById('startDate').value + '&end_date=' + document.getElementById('endDate').value;
                const res = await fetch(url);
                const data = await res.json();
                
                let purchaserHtml = '';
                if (data.by_purchaser.length === 0) {
                    purchaserHtml = '<tr><td colspan="5" class="text-center text-muted">暂无数据</td></tr>';
                } else {
                    let totalQty = 0, totalAmt = 0, totalMargin = 0;
                    data.by_purchaser.forEach(item => {
                        const margin = item.sales_amount - item.cost_amount;
                        const margin_rate = item.sales_amount > 0 ? (margin / item.sales_amount * 100).toFixed(1) : '0';
                        totalQty += item.quantity;
                        totalAmt += item.sales_amount;
                        totalMargin += margin;
                        purchaserHtml += '<tr><td>' + item.name + '</td><td>' + item.quantity.toFixed(2) + '</td><td>¥' + item.sales_amount.toFixed(2) + '</td><td>¥' + margin.toFixed(2) + '</td><td>' + margin_rate + '%</td></tr>';
                    });
                    const totalMarginRate = totalAmt > 0 ? (totalMargin / totalAmt * 100).toFixed(1) : '0';
                    purchaserHtml += '<tr class="table-active fw-bold"><td>合计</td><td>' + totalQty.toFixed(2) + '</td><td>¥' + totalAmt.toFixed(2) + '</td><td>¥' + totalMargin.toFixed(2) + '</td><td>' + totalMarginRate + '%</td></tr>';
                }
                document.getElementById('purchaserSummary').innerHTML = purchaserHtml;
                
                let productHtml = '';
                if (data.by_product.length === 0) {
                    productHtml = '<tr><td colspan="5" class="text-center text-muted">暂无数据</td></tr>';
                } else {
                    let totalQty = 0, totalAmt = 0, totalMargin = 0;
                    data.by_product.forEach(item => {
                        const margin = item.sales_amount - item.cost_amount;
                        totalQty += item.quantity;
                        totalAmt += item.sales_amount;
                        totalMargin += margin;
                        productHtml += '<tr><td>' + item.product_name + '</td><td>' + (item.spec || '-') + '</td><td>' + item.quantity.toFixed(2) + '</td><td>¥' + item.sales_amount.toFixed(2) + '</td><td>¥' + margin.toFixed(2) + '</td></tr>';
                    });
                    productHtml += '<tr class="table-active fw-bold"><td colspan="2">合计</td><td>' + totalQty.toFixed(2) + '</td><td>¥' + totalAmt.toFixed(2) + '</td><td>¥' + totalMargin.toFixed(2) + '</td></tr>';
                }
                document.getElementById('productSummary').innerHTML = productHtml;
            }
            searchSalesSummary();
        </script>
    "#;
    Html(layout_html("销售汇总报表", "/query/sales_summary", &content))
}

async fn page_query_product_rank(headers: axum::http::HeaderMap) -> Html<String> {
    match check_page_permission(&headers, "/query/product_rank").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let content = r#"
        <div class="card p-4">
            <h3>畅销滞销商品查询</h3>
            <div class="row mb-3">
                <div class="col-md-3">
                    <label>开始日期：</label>
                    <input type="date" id="startDate" class="form-control">
                </div>
                <div class="col-md-3">
                    <label>结束日期：</label>
                    <input type="date" id="endDate" class="form-control">
                </div>
            </div>
            <button onclick="searchProductRank()" class="btn btn-primary">查询</button>
        </div>
        <div class="card p-4 mt-4">
            <h4>畅销商品 TOP 10</h4>
            <table class="table table-bordered">
                <thead><tr><th>排名</th><th>商品名称</th><th>规格</th><th>销售数量</th><th>销售金额</th></tr></thead>
                <tbody id="topSelling"></tbody>
            </table>
        </div>
        <div class="card p-4 mt-4">
            <h4>滞销商品（期间无销售）</h4>
            <table class="table table-bordered">
                <thead><tr><th>商品名称</th><th>规格</th><th>当前库存</th><th>最后销售日期</th></tr></thead>
                <tbody id="slowMoving"></tbody>
            </table>
        </div>
        <script>
            async function searchProductRank() {
                const url = '/api/query/product_rank?start_date=' + document.getElementById('startDate').value + '&end_date=' + document.getElementById('endDate').value;
                const res = await fetch(url);
                const data = await res.json();
                
                let topHtml = '';
                if (data.top_selling.length === 0) {
                    topHtml = '<tr><td colspan="5" class="text-center text-muted">暂无数据</td></tr>';
                } else {
                    let totalQty = 0, totalAmt = 0;
                    data.top_selling.forEach((item, idx) => {
                        totalQty += item.quantity;
                        totalAmt += item.amount;
                        topHtml += '<tr><td>' + (idx + 1) + '</td><td>' + item.product_name + '</td><td>' + (item.spec || '-') + '</td><td>' + item.quantity.toFixed(2) + '</td><td>¥' + item.amount.toFixed(2) + '</td></tr>';
                    });
                    topHtml += '<tr class="table-active fw-bold"><td colspan="3">合计</td><td>' + totalQty.toFixed(2) + '</td><td>¥' + totalAmt.toFixed(2) + '</td></tr>';
                }
                document.getElementById('topSelling').innerHTML = topHtml;
                
                let slowHtml = '';
                if (data.slow_moving.length === 0) {
                    slowHtml = '<tr><td colspan="4" class="text-center text-muted">暂无数据</td></tr>';
                } else {
                    data.slow_moving.forEach(item => {
                        slowHtml += '<tr><td>' + item.product_name + '</td><td>' + (item.spec || '-') + '</td><td>' + item.stock_quantity.toFixed(2) + '</td><td>' + (item.last_sale_date || '从未销售') + '</td></tr>';
                    });
                }
                document.getElementById('slowMoving').innerHTML = slowHtml;
            }
            searchProductRank();
        </script>
    "#;
    Html(layout_html("畅销滞销商品查询", "/query/product_rank", &content))
}

async fn page_query_stock_flow() -> Html<String> {
    let content = r#"
        <div class="card p-4">
            <h3>库存明细台账</h3>
            <div class="row mb-3">
                <div class="col-md-4">
                    <label>商品名称：</label>
                    <input type="text" id="productName" class="form-control" placeholder="输入商品名称搜索">
                </div>
                <div class="col-md-3">
                    <label>开始日期：</label>
                    <input type="date" id="startDate" class="form-control">
                </div>
                <div class="col-md-3">
                    <label>结束日期：</label>
                    <input type="date" id="endDate" class="form-control">
                </div>
            </div>
            <button onclick="searchStockFlow()" class="btn btn-primary">查询</button>
            <a href="/api/query/stock_flow/export" class="btn btn-success ml-2">导出Excel</a>
        </div>
        <div class="card p-4 mt-4">
            <table class="table table-bordered">
                <thead><tr><th>日期</th><th>类型</th><th>商品名称</th><th>规格</th><th>入库数量</th><th>出库数量</th><th>余额</th><th>备注</th></tr></thead>
                <tbody id="resultTable"></tbody>
            </table>
        </div>
        <script>
            async function searchStockFlow() {
                const url = '/api/query/stock_flow?product_name=' + encodeURIComponent(document.getElementById('productName').value) + 
                    '&start_date=' + document.getElementById('startDate').value + 
                    '&end_date=' + document.getElementById('endDate').value;
                const res = await fetch(url);
                const data = await res.json();
                const tbody = document.getElementById('resultTable');
                tbody.innerHTML = '';
                let balance = 0;
                data.forEach(item => {
                    balance += (item.in_quantity || 0) - (item.out_quantity || 0);
                    tbody.innerHTML += '<tr><td>' + item.create_time + '</td><td>' + item.type + '</td><td>' + item.product_name + '</td><td>' + (item.spec || '') + '</td><td>' + (item.in_quantity || 0).toFixed(2) + '</td><td>' + (item.out_quantity || 0).toFixed(2) + '</td><td>' + balance.toFixed(2) + '</td><td>' + (item.remark || '') + '</td></tr>';
                });
            }
            searchStockFlow();
        </script>
    "#;
    Html(layout_html("库存明细台账", "/query/stock_flow", &content))
}

async fn page_query_stock_warning() -> Html<String> {
    let content = r#"
        <div class="card p-4">
            <h3>库存上下限预警</h3>
            <button onclick="searchStockWarning()" class="btn btn-primary">查询</button>
        </div>
        <div class="card p-4 mt-4 border-danger">
            <h4>低于最低库存（缺货）</h4>
            <table class="table table-bordered">
                <thead><tr><th>商品名称</th><th>规格</th><th>单位</th><th>当前库存</th><th>最低库存</th><th>缺货数量</th></tr></thead>
                <tbody id="lowStock"></tbody>
            </table>
        </div>
        <div class="card p-4 mt-4 border-warning">
            <h4>高于最高库存（积压）</h4>
            <table class="table table-bordered">
                <thead><tr><th>商品名称</th><th>规格</th><th>单位</th><th>当前库存</th><th>最高库存</th><th>积压数量</th></tr></thead>
                <tbody id="highStock"></tbody>
            </table>
        </div>
        <script>
            async function searchStockWarning() {
                const res = await fetch('/api/query/stock_warning');
                const data = await res.json();
                
                let lowHtml = '';
                data.low_stock.forEach(item => {
                    const shortage = Math.max(0, item.min_stock - item.current_stock);
                    lowHtml += '<tr><td>' + item.product_name + '</td><td>' + (item.spec || '') + '</td><td>' + (item.unit || '') + '</td><td>' + item.current_stock.toFixed(2) + '</td><td>' + item.min_stock.toFixed(2) + '</td><td>' + shortage.toFixed(2) + '</td></tr>';
                });
                document.getElementById('lowStock').innerHTML = lowHtml;
                
                let highHtml = '';
                data.high_stock.forEach(item => {
                    const overstock = Math.max(0, item.current_stock - item.max_stock);
                    highHtml += '<tr><td>' + item.product_name + '</td><td>' + (item.spec || '') + '</td><td>' + (item.unit || '') + '</td><td>' + item.current_stock.toFixed(2) + '</td><td>' + item.max_stock.toFixed(2) + '</td><td>' + overstock.toFixed(2) + '</td></tr>';
                });
                document.getElementById('highStock').innerHTML = highHtml;
            }
            searchStockWarning();
        </script>
    "#;
    Html(layout_html("库存上下限预警", "/query/stock_warning", &content))
}

async fn page_query_slow_stock() -> Html<String> {
    let content = r#"
        <div class="card p-4">
            <h3>呆滞库存查询</h3>
            <div class="row mb-3">
                <div class="col-md-3">
                    <label>无出库天数：</label>
                    <input type="number" id="days" value="30" class="form-control" style="width: 100px;">天
                </div>
            </div>
            <button onclick="searchSlowStock()" class="btn btn-primary">查询</button>
        </div>
        <div class="card p-4 mt-4">
            <table class="table table-bordered">
                <thead><tr><th>商品名称</th><th>规格</th><th>单位</th><th>当前库存</th><th>库存金额</th><th>最后出库日期</th><th>呆滞天数</th></tr></thead>
                <tbody id="resultTable"></tbody>
            </table>
        </div>
        <script>
            async function searchSlowStock() {
                const url = '/api/query/slow_stock?days=' + document.getElementById('days').value;
                const res = await fetch(url);
                const data = await res.json();
                const tbody = document.getElementById('resultTable');
                tbody.innerHTML = '';
                data.forEach(item => {
                    tbody.innerHTML += '<tr><td>' + item.product_name + '</td><td>' + (item.spec || '') + '</td><td>' + (item.unit || '') + '</td><td>' + item.stock_quantity.toFixed(2) + '</td><td>' + item.stock_amount.toFixed(2) + '</td><td>' + (item.last_out_date || '从未出库') + '</td><td>' + item.days + '</td></tr>';
                });
            }
            searchSlowStock();
        </script>
    "#;
    Html(layout_html("呆滞库存查询", "/query/slow_stock", &content))
}

async fn page_query_income_expense() -> Html<String> {
    let content = r#"
        <div class="card p-4">
            <h3>收支流水查询</h3>
            <div class="row mb-3">
                <div class="col-md-3">
                    <label>开始日期：</label>
                    <input type="date" id="startDate" class="form-control">
                </div>
                <div class="col-md-3">
                    <label>结束日期：</label>
                    <input type="date" id="endDate" class="form-control">
                </div>
                <div class="col-md-3">
                    <label>类型：</label>
                    <select id="type" class="form-control">
                        <option value="">全部</option>
                        <option value="收入">收入</option>
                        <option value="支出">支出</option>
                    </select>
                </div>
            </div>
            <button onclick="searchIncomeExpense()" class="btn btn-primary">查询</button>
        </div>
        <div class="row mt-4">
            <div class="col-md-6">
                <div class="card bg-success text-white p-4">
                    <h4>总收入</h4>
                    <p class="text-2xl" id="totalIncome">¥0.00</p>
                </div>
            </div>
            <div class="col-md-6">
                <div class="card bg-danger text-white p-4">
                    <h4>总支出</h4>
                    <p class="text-2xl" id="totalExpense">¥0.00</p>
                </div>
            </div>
        </div>
        <div class="card p-4 mt-4">
            <table class="table table-bordered">
                <thead><tr><th>日期</th><th>类型</th><th>摘要</th><th>金额</th><th>账户</th><th>备注</th></tr></thead>
                <tbody id="resultTable"></tbody>
            </table>
        </div>
        <script>
            async function searchIncomeExpense() {
                const url = '/api/query/income_expense?start_date=' + document.getElementById('startDate').value + 
                    '&end_date=' + document.getElementById('endDate').value + 
                    '&type=' + document.getElementById('type').value;
                const res = await fetch(url);
                const data = await res.json();
                
                document.getElementById('totalIncome').textContent = '¥' + data.total_income.toFixed(2);
                document.getElementById('totalExpense').textContent = '¥' + data.total_expense.toFixed(2);
                
                const tbody = document.getElementById('resultTable');
                tbody.innerHTML = '';
                data.records.forEach(item => {
                    tbody.innerHTML += '<tr><td>' + item.date + '</td><td>' + item.type + '</td><td>' + item.description + '</td><td>' + item.amount.toFixed(2) + '</td><td>' + (item.account || '') + '</td><td>' + (item.remark || '') + '</td></tr>';
                });
            }
        </script>
    "#;
    Html(layout_html("收支流水查询", "/query/income_expense", &content))
}

async fn page_query_profit_detail() -> Html<String> {
    let content = r#"
        <div class="card p-4">
            <h3>毛利明细查询</h3>
            <div class="row mb-3">
                <div class="col-md-3">
                    <label>开始日期：</label>
                    <input type="date" id="startDate" class="form-control">
                </div>
                <div class="col-md-3">
                    <label>结束日期：</label>
                    <input type="date" id="endDate" class="form-control">
                </div>
            </div>
            <button onclick="searchProfitDetail()" class="btn btn-primary">查询</button>
        </div>
        <div class="card p-4 mt-4">
            <table class="table table-bordered">
                <thead><tr><th>订单号</th><th>采购单位</th><th>日期</th><th>销售金额</th><th>成本金额</th><th>毛利</th><th>毛利率</th></tr></thead>
                <tbody id="resultTable"></tbody>
            </table>
        </div>
        <script>
            async function searchProfitDetail() {
                const url = '/api/query/profit_detail?start_date=' + document.getElementById('startDate').value + '&end_date=' + document.getElementById('endDate').value;
                const res = await fetch(url);
                const data = await res.json();
                const tbody = document.getElementById('resultTable');
                tbody.innerHTML = '';
                data.forEach(item => {
                    const margin = item.sales_amount - item.cost_amount;
                    const margin_rate = item.sales_amount > 0 ? (margin / item.sales_amount * 100).toFixed(1) : '0';
                    tbody.innerHTML += '<tr><td>' + item.order_no + '</td><td>' + item.purchaser_name + '</td><td>' + item.order_date + '</td><td>' + item.sales_amount.toFixed(2) + '</td><td>' + item.cost_amount.toFixed(2) + '</td><td>' + margin.toFixed(2) + '</td><td>' + margin_rate + '%</td></tr>';
                });
            }
        </script>
    "#;
    Html(layout_html("毛利明细查询", "/query/profit_detail", &content))
}

async fn page_query_category_stats() -> Html<String> {
    let content = r#"
        <div class="card p-4">
            <h3>品类进销存统计</h3>
            <div class="row mb-3">
                <div class="col-md-3">
                    <label>开始日期：</label>
                    <input type="date" id="startDate" class="form-control">
                </div>
                <div class="col-md-3">
                    <label>结束日期：</label>
                    <input type="date" id="endDate" class="form-control">
                </div>
            </div>
            <button onclick="searchCategoryStats()" class="btn btn-primary">查询</button>
        </div>
        <div class="card p-4 mt-4">
            <table class="table table-bordered">
                <thead><tr><th>品类名称</th><th>采购数量</th><th>采购金额</th><th>销售数量</th><th>销售金额</th><th>库存数量</th><th>库存金额</th><th>毛利</th></tr></thead>
                <tbody id="resultTable"></tbody>
            </table>
        </div>
        <script>
            async function searchCategoryStats() {
                const url = '/api/query/category_stats?start_date=' + document.getElementById('startDate').value + '&end_date=' + document.getElementById('endDate').value;
                const res = await fetch(url);
                const data = await res.json();
                const tbody = document.getElementById('resultTable');
                tbody.innerHTML = '';
                data.forEach(item => {
                    const margin = item.sales_amount - item.purchase_amount;
                    tbody.innerHTML += '<tr><td>' + item.category_name + '</td><td>' + item.purchase_quantity.toFixed(2) + '</td><td>' + item.purchase_amount.toFixed(2) + '</td><td>' + item.sales_quantity.toFixed(2) + '</td><td>' + item.sales_amount.toFixed(2) + '</td><td>' + item.stock_quantity.toFixed(2) + '</td><td>' + item.stock_amount.toFixed(2) + '</td><td>' + margin.toFixed(2) + '</td></tr>';
                });
            }
        </script>
    "#;
    Html(layout_html("品类进销存统计", "/query/category_stats", &content))
}

async fn page_query_document_summary() -> Html<String> {
    let content = r#"
        <div class="card p-4">
            <h3>单据汇总查询</h3>
            <div class="row mb-3">
                <div class="col-md-3">
                    <label>月份：</label>
                    <input type="month" id="month" class="form-control">
                </div>
            </div>
            <button onclick="searchDocumentSummary()" class="btn btn-primary">查询</button>
        </div>
        <div class="card p-4 mt-4">
            <table class="table table-bordered">
                <thead><tr><th>月份</th><th>采购订单数</th><th>销售订单数</th><th>采购金额</th><th>销售金额</th></tr></thead>
                <tbody id="resultTable"></tbody>
            </table>
        </div>
        <script>
            async function searchDocumentSummary() {
                const url = '/api/query/document_summary?month=' + document.getElementById('month').value;
                const res = await fetch(url);
                const data = await res.json();
                const tbody = document.getElementById('resultTable');
                tbody.innerHTML = '';
                data.forEach(item => {
                    tbody.innerHTML += '<tr><td>' + item.month + '</td><td>' + item.purchase_count + '</td><td>' + item.sales_count + '</td><td>' + item.purchase_amount.toFixed(2) + '</td><td>' + item.sales_amount.toFixed(2) + '</td></tr>';
                });
            }
        </script>
    "#;
    Html(layout_html("单据汇总查询", "/query/document_summary", &content))
}

async fn page_system(headers: axum::http::HeaderMap) -> Html<String> {
    match check_page_permission(&headers, "/system").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let rows = sqlx::query("SELECT key, value FROM system_config")
        .fetch_all(pool())
        .await
        .unwrap_or_default();

    let mut config_html = String::new();
    let default_configs = [
        ("system_name", "系统名称", "进销存管理系统"),
        ("company_name", "公司名称", ""),
        ("company_address", "公司地址", ""),
        ("company_phone", "联系电话", ""),
        ("decimal_places", "金额小数位数", "2"),
        ("auto_save_interval", "自动保存间隔(秒)", "30"),
    ];

    for (key, label, default) in default_configs.iter() {
        let value = rows.iter()
            .find(|r| r.get::<String, _>("key") == *key)
            .map(|r| r.get::<String, _>("value"))
            .unwrap_or_else(|| default.to_string());
        config_html.push_str(&format!(
            r#"<div class="row mb-3">
                <div class="col-md-3"><label class="form-label">{}：</label></div>
                <div class="col-md-6"><input type="text" name="{}" value="{}" class="form-control"></div>
            </div>"#,
            label, key, value
        ));
    }

    let content = format!(r#"
        <div class="card p-4">
            <h3>系统参数设置</h3>
            <form id="systemForm" onsubmit="saveConfig(event)">
                {}
                <button type="submit" class="btn btn-primary">保存设置</button>
            </form>
        </div>
        <script>
            async function saveConfig(e) {{
                e.preventDefault();
                const form = e.target;
                const data = {{}};
                const inputs = form.querySelectorAll('input');
                inputs.forEach(input => {{
                    data[input.name] = input.value;
                }});
                const res = await fetch('/api/system/config', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify(data)
                }});
                if (res.ok) {{
                    alert('设置保存成功');
                }} else {{
                    alert('保存失败');
                }}
            }}
        </script>
    "#, config_html);

    Html(layout_html("系统参数", "/system", &content))
}

async fn page_user(headers: axum::http::HeaderMap) -> Html<String> {
    match check_page_permission(&headers, "/user").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let rows = sqlx::query("SELECT id, username, nickname, role, status, last_login_time, create_at FROM user_account ORDER BY id")
        .fetch_all(pool())
        .await
        .unwrap_or_default();

    let mut table_html = String::new();
    for row in rows {
        let id: i64 = row.get("id");
        let username: String = row.get("username");
        let nickname: String = row.get("nickname");
        let role: String = row.get("role");
        let status: i32 = row.get("status");
        let last_login_time: Option<String> = row.get("last_login_time");
        let create_at: String = row.get("create_at");
        
        let role_label = match role.as_str() {
            "super_admin" => "超级管理员",
            "admin" => "管理员",
            "supplier" => "供应商",
            "purchaser" => "采购方",
            _ => "普通用户",
        };
        
        let status_label = if status == 1 {
            "<span class='badge bg-success'>启用</span>"
        } else {
            "<span class='badge bg-danger'>禁用</span>"
        };

        table_html.push_str(&format!(
            r#"<tr>
                <td>{}</td>
                <td>{}</td>
                <td>{}</td>
                <td>{}</td>
                <td>{}</td>
                <td>{}</td>
                <td>{}</td>
                <td>
                    <button onclick="editUser({})" class="btn btn-primary btn-sm">编辑</button>
                    <button onclick="toggleUserStatus({}, {})" class="btn btn-warning btn-sm">{}</button>
                    {}
                </td>
            </tr>"#,
            id,
            username,
            nickname,
            role_label,
            status_label,
            last_login_time.unwrap_or("-".to_string()),
            create_at,
            id,
            id,
            status,
            if status == 1 { "禁用" } else { "启用" },
            if username == "super_admin" { String::new() } else { format!(r#"<button onclick="deleteUser({})" class="btn btn-danger btn-sm">删除</button>"#, id) }
        ));
    }

    let content = format!(r#"
        <div class="card p-4">
            <h3>用户管理</h3>
            <button onclick="showAddModal()" class="btn btn-success mb-4">添加用户</button>
            
            <table class="table table-bordered">
                <thead>
                    <tr>
                        <th>ID</th>
                        <th>用户名</th>
                        <th>昵称</th>
                        <th>角色</th>
                        <th>状态</th>
                        <th>最后登录</th>
                        <th>创建时间</th>
                        <th>操作</th>
                    </tr>
                </thead>
                <tbody>{}</tbody>
            </table>
        </div>

        <div class="modal fade" id="userModal" tabindex="-1">
            <div class="modal-dialog">
                <div class="modal-content">
                    <div class="modal-header">
                        <h5 class="modal-title" id="modalTitle">添加用户</h5>
                        <button type="button" class="btn-close" data-bs-dismiss="modal"></button>
                    </div>
                    <div class="modal-body">
                        <form id="userForm">
                            <input type="hidden" id="userId">
                            <div class="mb-3">
                                <label class="form-label">用户名</label>
                                <input type="text" id="username" class="form-control" required>
                            </div>
                            <div class="mb-3">
                                <label class="form-label">昵称</label>
                                <input type="text" id="nickname" class="form-control">
                            </div>
                            <div class="mb-3">
                                <label class="form-label">密码</label>
                                <input type="password" id="password" class="form-control">
                                <small class="text-muted">编辑时不填则保持原密码</small>
                            </div>
                            <div class="mb-3">
                                <label class="form-label">角色</label>
                                <select id="role" class="form-control">
                                    <option value="admin">管理员</option>
                                    <option value="supplier">供应商</option>
                                    <option value="purchaser">采购方</option>
                                    <option value="user">普通用户</option>
                                </select>
                            </div>
                        </form>
                    </div>
                    <div class="modal-footer">
                        <button type="button" class="btn btn-secondary" data-bs-dismiss="modal">取消</button>
                        <button type="button" onclick="saveUser()" class="btn btn-primary">保存</button>
                    </div>
                </div>
            </div>
        </div>

        <script>
            let currentUserId = null;

            function showAddModal() {{
                currentUserId = null;
                document.getElementById('modalTitle').textContent = '添加用户';
                document.getElementById('userForm').reset();
                document.getElementById('userId').value = '';
                new bootstrap.Modal(document.getElementById('userModal')).show();
            }}

            async function editUser(id) {{
                const res = await fetch('/api/user/' + id);
                const data = await res.json();
                if (data.success) {{
                    currentUserId = data.user.id;
                    document.getElementById('modalTitle').textContent = '编辑用户';
                    document.getElementById('userId').value = data.user.id;
                    document.getElementById('username').value = data.user.username;
                    document.getElementById('nickname').value = data.user.nickname || '';
                    document.getElementById('role').value = data.user.role;
                    document.getElementById('password').value = '';
                    new bootstrap.Modal(document.getElementById('userModal')).show();
                }}
            }}

            async function saveUser() {{
                const id = document.getElementById('userId').value;
                const data = {{
                    username: document.getElementById('username').value,
                    nickname: document.getElementById('nickname').value,
                    password: document.getElementById('password').value,
                    role: document.getElementById('role').value
                }};
                
                const url = id ? '/api/user/' + id : '/api/user';
                const method = id ? 'PUT' : 'POST';
                
                const res = await fetch(url, {{
                    method: method,
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify(data)
                }});
                
                const result = await res.json();
                if (result.success) {{
                    location.reload();
                }} else {{
                    alert(result.message);
                }}
            }}

            async function toggleUserStatus(id, status) {{
                const res = await fetch('/api/user/' + id + '/status', {{
                    method: 'PUT',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ status: status === 1 ? 0 : 1 }})
                }});
                if (res.ok) {{
                    location.reload();
                }}
            }}

            async function deleteUser(id) {{
                if (!confirm('确定删除该用户？')) return;
                const res = await fetch('/api/user/' + id, {{ method: 'DELETE' }});
                if (res.ok) {{
                    location.reload();
                }} else {{
                    alert('删除失败');
                }}
            }}
        </script>
    "#, table_html);

    Html(layout_html("用户管理", "/user", &content))
}

async fn page_backup(headers: axum::http::HeaderMap) -> Html<String> {
    match check_page_permission(&headers, "/backup").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let rows = sqlx::query("SELECT id, backup_time, file_name, size FROM backup_record ORDER BY backup_time DESC")
        .fetch_all(pool())
        .await
        .unwrap_or_default();

    let mut table_html = String::new();
    for row in rows {
        table_html.push_str(&format!(
            r#"<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>
                <a href="/api/backup/download/{}" class="btn btn-info btn-sm">下载</a>
                <button onclick="deleteBackup({})" class="btn btn-danger btn-sm">删除</button>
            </td></tr>"#,
            row.get::<i64, _>("id"),
            row.get::<String, _>("backup_time"),
            row.get::<String, _>("file_name"),
            row.get::<i64, _>("size"),
            row.get::<i64, _>("id"),
            row.get::<i64, _>("id"),
        ));
    }

    let content = format!(r#"
        <div class="card p-4">
            <h3>数据备份</h3>
            <button onclick="doBackup()" class="btn btn-success mb-4">执行备份</button>
            
            <table class="table table-bordered">
                <thead><tr><th>ID</th><th>备份时间</th><th>文件名</th><th>大小(字节)</th><th>操作</th></tr></thead>
                <tbody>{}</tbody>
            </table>
        </div>
        <script>
            async function doBackup() {{
                const res = await fetch('/api/backup', {{ method: 'POST' }});
                const result = await res.text();
                alert(result);
                if (res.ok) {{
                    location.reload();
                }}
            }}
            async function deleteBackup(id) {{
                if (!confirm('确定删除此备份？')) return;
                const res = await fetch('/api/backup/delete/' + id, {{ method: 'DELETE' }});
                if (res.ok) {{
                    location.reload();
                }} else {{
                    alert('删除失败');
                }}
            }}
        </script>
    "#, table_html);

    Html(layout_html("数据备份", "/backup", &content))
}

async fn page_restore(headers: axum::http::HeaderMap) -> Html<String> {
    match check_page_permission(&headers, "/restore").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let rows = sqlx::query("SELECT id, backup_time, file_name FROM backup_record ORDER BY backup_time DESC")
        .fetch_all(pool())
        .await
        .unwrap_or_default();

    let mut options = String::new();
    for row in rows {
        options.push_str(&format!(
            "<option value=\"{}\">{}</option>",
            row.get::<i64, _>("id"),
            row.get::<String, _>("backup_time") + " - " + row.get::<String, _>("file_name").as_str()
        ));
    }

    let content = format!(r#"
        <div class="card p-4">
            <h3>数据恢复</h3>
            <div class="alert alert-warning mb-4">
                <strong>警告！</strong>数据恢复将覆盖当前所有数据，请确保已备份最新数据。
            </div>
            <form id="restoreForm" onsubmit="doRestore(event)">
                <div class="row mb-3">
                    <div class="col-md-3"><label class="form-label">选择备份：</label></div>
                    <div class="col-md-6"><select name="backup_id" class="form-control">{}</select></div>
                </div>
                <button type="submit" class="btn btn-danger">确认恢复</button>
            </form>
            
            <h4 class="mt-4">从文件恢复</h4>
            <input type="file" id="restoreFile" accept=".db" class="form-control mb-3">
            <button onclick="restoreFromFile()" class="btn btn-warning">从文件恢复</button>
        </div>
        <script>
            async function doRestore(e) {{
                e.preventDefault();
                const form = e.target;
                const backupId = form.backup_id.value;
                if (!backupId) {{
                    alert('请选择备份文件');
                    return;
                }}
                if (!confirm('确定要恢复此备份吗？这将覆盖当前所有数据！')) return;
                const res = await fetch('/api/restore/' + backupId, {{ method: 'POST' }});
                const result = await res.text();
                alert(result);
                if (res.ok) {{
                    location.href = '/';
                }}
            }}
            async function restoreFromFile() {{
                const input = document.getElementById('restoreFile');
                const file = input.files[0];
                if (!file) {{
                    alert('请选择备份文件');
                    return;
                }}
                if (!confirm('确定要从文件恢复吗？这将覆盖当前所有数据！')) return;
                const formData = new FormData();
                formData.append('file', file);
                const res = await fetch('/api/restore/file', {{ method: 'POST', body: formData }});
                const result = await res.text();
                alert(result);
                if (res.ok) {{
                    location.href = '/';
                }}
            }}
        </script>
    "#, options);

    Html(layout_html("数据恢复", "/restore", &content))
}

async fn page_mobile_sort() -> Html<String> {
    Html(r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no">
    <title>采购分拣</title>
    <link rel="stylesheet" href="/static/bootstrap.min.css">
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; background: #f5f7fa; }
        .sticky-header { position: sticky; top: 0; z-index: 100; }
        .page-header { background: linear-gradient(135deg, #1e3a8a 0%, #3b82f6 100%); color: white; padding: 16px 20px; box-shadow: 0 2px 8px rgba(0,0,0,0.1); }
        .page-header h1 { font-size: 18px; margin: 0; font-weight: 600; }
        .header-info { font-size: 13px; opacity: 0.9; margin-top: 4px; }
        .switch-link { display: inline-block; margin-top: 8px; padding: 6px 12px; background: rgba(255,255,255,0.2); border-radius: 6px; font-size: 13px; text-decoration: none; color: white; }
        .switch-link:hover { background: rgba(255,255,255,0.3); }
        .stats-bar { display: flex; gap: 12px; margin-top: 12px; }
        .stat-item { background: rgba(255,255,255,0.2); padding: 8px 12px; border-radius: 8px; flex: 1; text-align: center; }
        .stat-value { font-size: 16px; font-weight: bold; }
        .stat-label { font-size: 11px; opacity: 0.8; }
        .content-area { padding: 12px; }
        .sort-card { background: white; border-radius: 12px; padding: 16px; margin-bottom: 12px; box-shadow: 0 2px 6px rgba(0,0,0,0.05); display: flex; align-items: center; gap: 14px; transition: all 0.2s; }
        .sort-card:hover { box-shadow: 0 4px 12px rgba(0,0,0,0.1); }
        .sort-card.checked { background: #ecfdf5; border: 1px solid #10b981; }
        .checkbox-wrapper { flex-shrink: 0; }
        .checkbox-custom { width: 28px; height: 28px; border-radius: 8px; border: 2px solid #ddd; display: flex; align-items: center; justify-content: center; cursor: pointer; transition: all 0.2s; }
        .checkbox-custom.checked { background: #10b981; border-color: #10b981; }
        .checkbox-custom.checked::after { content: '✓'; color: white; font-size: 18px; font-weight: bold; }
        .item-info { flex: 1; min-width: 0; }
        .item-name { font-size: 16px; font-weight: 600; color: #333; margin-bottom: 4px; }
        .item-detail { font-size: 13px; color: #666; display: flex; gap: 16px; flex-wrap: wrap; }
        .item-detail span { background: #f3f4f6; padding: 3px 8px; border-radius: 4px; }
        .quantity-badge { flex-shrink: 0; text-align: right; }
        .quantity-value { font-size: 20px; font-weight: bold; color: #3b82f6; }
        .quantity-unit { font-size: 12px; color: #666; }
        .filter-bar { background: white; padding: 12px; border-bottom: 1px solid #eee; display: flex; gap: 8px; }
        .filter-bar input { flex: 1; padding: 10px 14px; border: 1px solid #ddd; border-radius: 8px; font-size: 14px; }
        .filter-bar button { padding: 10px 16px; border: none; border-radius: 8px; background: #3b82f6; color: white; font-size: 14px; }
        .filter-bar button.clear { background: #f3f4f6; color: #666; }
        .bottom-bar { background: white; padding: 6px 12px; position: fixed; bottom: 0; left: 0; right: 0; display: flex; gap: 6px; box-shadow: 0 -2px 8px rgba(0,0,0,0.05); }
        .bottom-bar button { flex: 1; padding: 6px; border: none; border-radius: 6px; font-size: 11px; font-weight: 600; }
        .btn-select-all { background: #f3f4f6; color: #333; }
        .btn-clear-all { background: #fee2e2; color: #dc2626; }
        .btn-print { background: #10b981; color: white; }
        .empty-state { text-align: center; padding: 60px 20px; color: #999; }
        .empty-icon { font-size: 48px; margin-bottom: 16px; }
        .correction-input { width: 60px; padding: 6px; border: 1px solid #ddd; border-radius: 4px; font-size: 14px; text-align: center; }
        .correction-input:focus { outline: none; border-color: #3b82f6; }
        .corrected-tag { background: #fef3c7; color: #d97706; padding: 2px 5px; border-radius: 3px; font-size: 11px; }
    </style>
</head>
<body>
    <div class="sticky-header">
    <div class="page-header">
        <h1>📦 统筹分拣</h1>
        <div class="header-info">根据销售订单汇总采购清单</div>
        <div class="switch-links">
            <a href="/mobile/sort_by_purchaser" class="switch-link">按单位分拣</a>
            <a href="/mobile/sort_by_category" class="switch-link">按分类分拣</a>
            <a href="/mobile/sort_comprehensive" class="switch-link">综合分拣</a>
        </div>
        <div class="stats-bar">
            <div class="stat-item">
                <div class="stat-value" id="totalCount">0</div>
                <div class="stat-label">商品种类</div>
            </div>
            <div class="stat-item">
                <div class="stat-value" id="checkedCount">0</div>
                <div class="stat-label">已采购</div>
            </div>
            <div class="stat-item">
                <div class="stat-value" id="uncheckedCount">0</div>
                <div class="stat-label">待采购</div>
            </div>
        </div>
    </div>
    
    <div class="filter-bar">
        <input type="text" id="searchInput" placeholder="搜索商品名称..." oninput="filterItems()">
        <button class="clear" onclick="clearSearch()">清除</button>
    </div>
    </div>
    
    <div class="content-area" id="itemsContainer">
        <div class="empty-state">
            <div class="empty-icon">📭</div>
            <div>暂无采购订单</div>
        </div>
    </div>
    
    <div class="bottom-bar">
        <button class="btn-select-all" onclick="toggleSelectAll()">全选</button>
        <button class="btn-clear-all" onclick="clearSelection()">清空</button>
        <button class="btn-clear-all" onclick="clearCorrections()">清除修正</button>
        <button class="btn-print" onclick="saveCorrectionsToServer()">保存修正</button>
        <button class="btn-export" onclick="exportExcel()">导出XLSX</button>
    </div>

    <script>
        let items = [];
        let checkedIds = new Set();
        let correctedQuantities = {};

        async function loadItems() {
            try {
                const res = await fetch('/api/sales_order/sort_items');
                items = await res.json();
                loadCheckedState();
                loadCorrectedQuantities();
                renderItems();
                updateStats();
            } catch (e) {
                console.error('加载失败:', e);
            }
        }

        function loadCheckedState() {
            const saved = localStorage.getItem('sort_checked_ids');
            if (saved) {
                const ids = JSON.parse(saved);
                ids.forEach(id => checkedIds.add(id));
            }
        }

        function saveCheckedState() {
            localStorage.setItem('sort_checked_ids', JSON.stringify([...checkedIds]));
        }

        function loadCorrectedQuantities() {
            const saved = localStorage.getItem('sort_corrections');
            if (saved) {
                correctedQuantities = JSON.parse(saved);
            }
        }

        function saveCorrectedQuantities() {
            localStorage.setItem('sort_corrections', JSON.stringify(correctedQuantities));
        }

        function updateCorrectedQuantity(productId, value) {
            const numValue = parseFloat(value);
            if (numValue && numValue > 0) {
                correctedQuantities[productId] = numValue;
            } else {
                delete correctedQuantities[productId];
            }
            saveCorrectedQuantities();
        }

        function getDisplayQuantity(item) {
            if (correctedQuantities[item.product_id] !== undefined) {
                return correctedQuantities[item.product_id];
            }
            return item.total_quantity;
        }

        function clearCorrections() {
            correctedQuantities = {};
            saveCorrectedQuantities();
            renderItems();
        }

        async function saveCorrectionsToServer() {
            if (Object.keys(correctedQuantities).length === 0) {
                alert('没有需要保存的修正');
                return;
            }
            
            const corrections = [];
            for (const [itemId, quantity] of Object.entries(correctedQuantities)) {
                corrections.push({ id: parseInt(itemId), quantity: quantity });
            }
            
            try {
                const res = await fetch('/api/sales_order/correction', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ corrections })
                });
                const text = await res.text();
                alert(text);
                clearCorrections();
                loadItems();
            } catch (e) {
                console.error('保存失败:', e);
                alert('保存失败，请重试');
            }
        }

        function toggleCheck(productId) {
            if (checkedIds.has(productId)) {
                checkedIds.delete(productId);
            } else {
                checkedIds.add(productId);
            }
            saveCheckedState();
            renderItems();
            updateStats();
        }

        function toggleSelectAll() {
            if (checkedIds.size === items.length) {
                checkedIds.clear();
            } else {
                items.forEach(item => checkedIds.add(item.product_id));
            }
            saveCheckedState();
            renderItems();
            updateStats();
        }

        function clearSelection() {
            checkedIds.clear();
            saveCheckedState();
            renderItems();
            updateStats();
        }

        function filterItems() {
            renderItems();
        }

        function clearSearch() {
            document.getElementById('searchInput').value = '';
            renderItems();
        }

        function updateStats() {
            document.getElementById('totalCount').textContent = items.length;
            document.getElementById('checkedCount').textContent = checkedIds.size;
            document.getElementById('uncheckedCount').textContent = items.length - checkedIds.size;
        }

        function renderItems() {
            const container = document.getElementById('itemsContainer');
            const keyword = document.getElementById('searchInput').value.trim().toLowerCase();
            
            const filtered = items.filter(item => 
                item.product_name.toLowerCase().includes(keyword)
            );
            
            if (filtered.length === 0) {
                container.innerHTML = '<div class="empty-state"><div class="empty-icon">🔍</div><div>没有找到匹配的商品</div></div>';
                return;
            }
            
            container.innerHTML = filtered.map(item => {
                const isChecked = checkedIds.has(item.product_id);
                const displayQty = getDisplayQuantity(item);
                const isCorrected = correctedQuantities[item.product_id] !== undefined;
                return '<div class="sort-card ' + (isChecked ? 'checked' : '') + '" onclick="toggleCheck(' + item.product_id + ')">' +
                    '<div class="checkbox-wrapper">' +
                        '<div class="checkbox-custom ' + (isChecked ? 'checked' : '') + '"></div>' +
                    '</div>' +
                    '<div class="item-info">' +
                        '<div class="item-name">' + item.product_name + '</div>' +
                        '<div class="item-detail">' +
                            '<span>' + item.unit + '</span>' +
                            '<span>采购单位: ' + item.purchaser_names + '</span>' +
                            (item.remarks ? '<span style="color:#d97706;">备注: ' + item.remarks + '</span>' : '') +
                            (isCorrected ? '<span class="corrected-tag">修正: ' + item.total_quantity + '→' + displayQty + '</span>' : '') +
                        '</div>' +
                    '</div>' +
                    '<div class="quantity-badge">' +
                        '<div class="quantity-value">' + displayQty + '</div>' +
                        '<div class="quantity-unit">' + item.unit + '</div>' +
                        '<input type="number" min="0" step="any" class="correction-input" placeholder="修正" ' + (isCorrected ? 'value="' + correctedQuantities[item.product_id] + '"' : '') + ' onchange="updateCorrectedQuantity(' + item.product_id + ', this.value)" onclick="event.stopPropagation()">' +
                    '</div>' +
                '</div>';
            }).join('');
        }

        function exportExcel() {
            window.location.href = '/api/sales_order/sort_items_excel';
        }

        loadItems();
    </script>
</body>
</html>
    "#.to_string())
}

async fn page_mobile_sort_by_purchaser() -> Html<String> {
    Html(r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no">
    <title>按单位分拣</title>
    <link rel="stylesheet" href="/static/bootstrap.min.css">
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; background: #f5f7fa; }
        .sticky-header { position: sticky; top: 0; z-index: 100; }
        .page-header { background: linear-gradient(135deg, #059669 0%, #10b981 100%); color: white; padding: 16px 20px; box-shadow: 0 2px 8px rgba(0,0,0,0.1); }
        .page-header h1 { font-size: 18px; margin: 0; font-weight: 600; }
        .header-info { font-size: 13px; opacity: 0.9; margin-top: 4px; }
        .switch-link { display: inline-block; margin-top: 8px; padding: 6px 12px; background: rgba(255,255,255,0.2); border-radius: 6px; font-size: 13px; text-decoration: none; color: white; }
        .switch-link:hover { background: rgba(255,255,255,0.3); }
        .stats-bar { display: flex; gap: 12px; margin-top: 12px; }
        .stat-item { background: rgba(255,255,255,0.2); padding: 8px 12px; border-radius: 8px; flex: 1; text-align: center; }
        .stat-value { font-size: 16px; font-weight: bold; }
        .stat-label { font-size: 11px; opacity: 0.8; }
        .content-area { padding: 12px; }
        .purchaser-section { margin-bottom: 16px; }
        .purchaser-header { background: #3b82f6; color: white; padding: 12px 16px; border-radius: 10px 10px 0 0; display: flex; justify-content: space-between; align-items: center; }
        .purchaser-header h3 { font-size: 16px; margin: 0; font-weight: 600; }
        .purchaser-stats { font-size: 13px; opacity: 0.9; }
        .sort-card { background: white; padding: 14px; border-bottom: 1px solid #eee; display: flex; align-items: center; gap: 12px; transition: all 0.2s; }
        .sort-card:hover { background: #f9fafb; }
        .sort-card.checked { background: #ecfdf5; }
        .checkbox-wrapper { flex-shrink: 0; }
        .checkbox-custom { width: 26px; height: 26px; border-radius: 6px; border: 2px solid #ddd; display: flex; align-items: center; justify-content: center; cursor: pointer; transition: all 0.2s; }
        .checkbox-custom.checked { background: #10b981; border-color: #10b981; }
        .checkbox-custom.checked::after { content: '✓'; color: white; font-size: 16px; font-weight: bold; }
        .item-info { flex: 1; min-width: 0; }
        .item-name { font-size: 15px; font-weight: 600; color: #333; margin-bottom: 2px; }
        .item-detail { font-size: 12px; color: #666; display: flex; gap: 12px; }
        .item-detail span { background: #f3f4f6; padding: 2px 6px; border-radius: 4px; }
        .quantity-badge { flex-shrink: 0; text-align: right; }
        .quantity-value { font-size: 18px; font-weight: bold; color: #3b82f6; }
        .quantity-unit { font-size: 11px; color: #666; }
        .filter-bar { background: white; padding: 12px; border-bottom: 1px solid #eee; display: flex; gap: 8px; }
        .filter-bar input { flex: 1; padding: 10px 14px; border: 1px solid #ddd; border-radius: 8px; font-size: 14px; }
        .filter-bar button { padding: 10px 16px; border: none; border-radius: 8px; background: #3b82f6; color: white; font-size: 14px; }
        .filter-bar button.clear { background: #f3f4f6; color: #666; }
        .bottom-bar { background: white; padding: 6px 12px; position: fixed; bottom: 0; left: 0; right: 0; display: flex; gap: 6px; box-shadow: 0 -2px 8px rgba(0,0,0,0.05); }
        .bottom-bar button { flex: 1; padding: 6px; border: none; border-radius: 6px; font-size: 11px; font-weight: 600; }
        .btn-select-all { background: #f3f4f6; color: #333; }
        .btn-clear-all { background: #fee2e2; color: #dc2626; }
        .btn-print { background: #10b981; color: white; }
        .empty-state { text-align: center; padding: 60px 20px; color: #999; }
        .empty-icon { font-size: 48px; margin-bottom: 16px; }
        .section-body { background: #fff; border-radius: 0 0 10px 10px; overflow: hidden; }
        .correction-input { width: 60px; padding: 6px; border: 1px solid #ddd; border-radius: 4px; font-size: 14px; text-align: center; }
        .correction-input:focus { outline: none; border-color: #3b82f6; }
        .corrected-tag { background: #fef3c7; color: #d97706; padding: 2px 5px; border-radius: 3px; font-size: 11px; }
    </style>
</head>
<body>
    <div class="sticky-header">
    <div class="page-header">
        <h1>🏢 按单位分拣</h1>
        <div class="header-info">按采购单位分组查看采购清单</div>
        <div class="switch-links">
            <a href="/mobile/sort" class="switch-link">统筹分拣</a>
            <a href="/mobile/sort_by_category" class="switch-link">按分类分拣</a>
            <a href="/mobile/sort_comprehensive" class="switch-link">综合分拣</a>
        </div>
        <div class="stats-bar">
            <div class="stat-item">
                <div class="stat-value" id="totalCount">0</div>
                <div class="stat-label">采购单位</div>
            </div>
            <div class="stat-item">
                <div class="stat-value" id="checkedCount">0</div>
                <div class="stat-label">已采购商品</div>
            </div>
            <div class="stat-item">
                <div class="stat-value" id="uncheckedCount">0</div>
                <div class="stat-label">待采购商品</div>
            </div>
        </div>
    </div>
    
    <div class="filter-bar">
        <input type="text" id="searchInput" placeholder="搜索商品名称..." oninput="filterItems()">
        <button class="clear" onclick="clearSearch()">清除</button>
    </div>
    </div>
    
    <div class="content-area" id="itemsContainer">
        <div class="empty-state">
            <div class="empty-icon">📭</div>
            <div>暂无采购订单</div>
        </div>
    </div>
    
    <div class="bottom-bar">
        <button class="btn-select-all" onclick="toggleSelectAll()">全选</button>
        <button class="btn-clear-all" onclick="clearSelection()">清空</button>
        <button class="btn-clear-all" onclick="clearCorrections()">清除修正</button>
        <button class="btn-print" onclick="saveCorrectionsToServer()">保存修正</button>
        <button class="btn-export" onclick="exportExcel()">导出XLSX</button>
    </div>

    <script>
        let purchasers = [];
        let checkedIds = new Set();
        let correctedQuantities = {};

        async function loadItems() {
            try {
                const res = await fetch('/api/sales_order/sort_items_by_purchaser');
                purchasers = await res.json();
                loadCheckedState();
                loadCorrectedQuantities();
                renderItems();
                updateStats();
            } catch (e) {
                console.error('加载失败:', e);
            }
        }

        function loadCheckedState() {
            const saved = localStorage.getItem('sort_by_purchaser_checked_ids');
            if (saved) {
                const ids = JSON.parse(saved);
                ids.forEach(id => checkedIds.add(id));
            }
        }

        function saveCheckedState() {
            localStorage.setItem('sort_by_purchaser_checked_ids', JSON.stringify([...checkedIds]));
        }

        function loadCorrectedQuantities() {
            const saved = localStorage.getItem('sort_by_purchaser_corrections');
            if (saved) {
                correctedQuantities = JSON.parse(saved);
            }
        }

        function saveCorrectedQuantities() {
            localStorage.setItem('sort_by_purchaser_corrections', JSON.stringify(correctedQuantities));
        }

        function updateCorrectedQuantity(itemId, value) {
            const numValue = parseFloat(value);
            if (numValue && numValue > 0) {
                correctedQuantities[itemId] = numValue;
            } else {
                delete correctedQuantities[itemId];
            }
            saveCorrectedQuantities();
        }

        function getDisplayQuantity(item) {
            if (correctedQuantities[item.id] !== undefined) {
                return correctedQuantities[item.id];
            }
            return item.quantity;
        }

        function clearCorrections() {
            correctedQuantities = {};
            saveCorrectedQuantities();
            renderItems();
        }

        async function saveCorrectionsToServer() {
            if (Object.keys(correctedQuantities).length === 0) {
                alert('没有需要保存的修正');
                return;
            }
            
            const corrections = [];
            for (const [itemId, quantity] of Object.entries(correctedQuantities)) {
                corrections.push({ id: parseInt(itemId), quantity: quantity });
            }
            
            try {
                const res = await fetch('/api/sales_order/correction', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ corrections })
                });
                const text = await res.text();
                alert(text);
                clearCorrections();
                loadItems();
            } catch (e) {
                console.error('保存失败:', e);
                alert('保存失败，请重试');
            }
        }

        function toggleCheck(itemId) {
            if (checkedIds.has(itemId)) {
                checkedIds.delete(itemId);
            } else {
                checkedIds.add(itemId);
            }
            saveCheckedState();
            renderItems();
            updateStats();
        }

        function toggleSelectAll() {
            let allItems = [];
            purchasers.forEach(p => p.items.forEach(item => allItems.push(item.id)));
            
            if (checkedIds.size === allItems.length) {
                checkedIds.clear();
            } else {
                allItems.forEach(id => checkedIds.add(id));
            }
            saveCheckedState();
            renderItems();
            updateStats();
        }

        function clearSelection() {
            checkedIds.clear();
            saveCheckedState();
            renderItems();
            updateStats();
        }

        function filterItems() {
            renderItems();
        }

        function clearSearch() {
            document.getElementById('searchInput').value = '';
            renderItems();
        }

        function updateStats() {
            let totalItems = 0;
            purchasers.forEach(p => totalItems += p.items.length);
            
            document.getElementById('totalCount').textContent = purchasers.length;
            document.getElementById('checkedCount').textContent = checkedIds.size;
            document.getElementById('uncheckedCount').textContent = totalItems - checkedIds.size;
        }

        function renderItems() {
            const container = document.getElementById('itemsContainer');
            const keyword = document.getElementById('searchInput').value.trim().toLowerCase();
            
            let hasItems = false;
            let html = '';
            
            purchasers.forEach(purchaser => {
                let filteredItems = purchaser.items.filter(item => 
                    item.product_name.toLowerCase().includes(keyword)
                );
                
                if (filteredItems.length === 0) return;
                
                hasItems = true;
                
                html += '<div class="purchaser-section">';
                html += '<div class="purchaser-header">';
                html += '<h3>' + purchaser.purchaser_name + '</h3>';
                html += '<div class="purchaser-stats">' + filteredItems.length + '种商品</div>';
                html += '</div>';
                html += '<div class="section-body">';
                
                filteredItems.forEach(item => {
                    const isChecked = checkedIds.has(item.id);
                    const displayQty = getDisplayQuantity(item);
                    const isCorrected = correctedQuantities[item.id] !== undefined;
                    html += '<div class="sort-card ' + (isChecked ? 'checked' : '') + '" onclick="toggleCheck(' + item.id + ')">';
                    html += '<div class="checkbox-wrapper">';
                    html += '<div class="checkbox-custom ' + (isChecked ? 'checked' : '') + '"></div>';
                    html += '</div>';
                    html += '<div class="item-info">';
                    html += '<div class="item-name">' + item.product_name + '</div>';
                    html += '<div class="item-detail">';
                    html += '<span>' + item.unit + '</span>';
                    if (item.remark) {
                        html += '<span style="color:#d97706;">备注: ' + item.remark + '</span>';
                    }
                    if (isCorrected) {
                        html += '<span class="corrected-tag">修正: ' + item.quantity + '→' + displayQty + '</span>';
                    }
                    html += '</div>';
                    html += '</div>';
                    html += '<div class="quantity-badge">';
                    html += '<div class="quantity-value">' + displayQty + '</div>';
                    html += '<div class="quantity-unit">' + item.unit + '</div>';
                    html += '<input type="number" min="0" step="any" class="correction-input" placeholder="修正" ' + (isCorrected ? 'value="' + correctedQuantities[item.id] + '"' : '') + ' onchange="updateCorrectedQuantity(' + item.id + ', this.value)" onclick="event.stopPropagation()">';
                    html += '</div>';
                    html += '</div>';
                });
                
                html += '</div></div>';
            });
            
            if (!hasItems) {
                container.innerHTML = '<div class="empty-state"><div class="empty-icon">🔍</div><div>没有找到匹配的商品</div></div>';
                return;
            }
            
            container.innerHTML = html;
        }

        function exportExcel() {
            window.location.href = '/api/sales_order/sort_items_by_purchaser_excel';
        }

        loadItems();
    </script>
</body>
</html>
    "#.to_string())
}

async fn page_mobile_sort_by_category() -> Html<String> {
    Html(r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no">
    <title>按分类分拣</title>
    <link rel="stylesheet" href="/static/bootstrap.min.css">
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; background: #f5f7fa; }
        .sticky-header { position: sticky; top: 0; z-index: 100; }
        .page-header { background: linear-gradient(135deg, #7c3aed 0%, #a855f7 100%); color: white; padding: 16px 20px; box-shadow: 0 2px 8px rgba(0,0,0,0.1); }
        .page-header h1 { font-size: 18px; margin: 0; font-weight: 600; }
        .header-info { font-size: 13px; opacity: 0.9; margin-top: 4px; }
        .switch-links { display: flex; gap: 8px; margin-top: 8px; }
        .switch-link { padding: 6px 12px; background: rgba(255,255,255,0.2); border-radius: 6px; font-size: 13px; text-decoration: none; color: white; }
        .switch-link:hover { background: rgba(255,255,255,0.3); }
        .stats-bar { display: flex; gap: 12px; margin-top: 12px; }
        .stat-item { background: rgba(255,255,255,0.2); padding: 8px 12px; border-radius: 8px; flex: 1; text-align: center; }
        .stat-value { font-size: 16px; font-weight: bold; }
        .stat-label { font-size: 11px; opacity: 0.8; }
        .content-area { padding: 12px; padding-bottom: 80px; }
        .category-section { border-radius: 12px; margin-bottom: 16px; overflow: hidden; box-shadow: 0 2px 8px rgba(0,0,0,0.08); }
        .category-header { padding: 14px 16px; display: flex; align-items: center; justify-content: space-between; }
        .category-header h3 { font-size: 16px; margin: 0; color: white; font-weight: 600; }
        .category-stats { font-size: 13px; opacity: 0.9; color: white; }
        .category-body { background: white; padding: 0; }
        .sort-card { display: flex; align-items: center; gap: 14px; padding: 14px 16px; border-bottom: 1px solid #f0f0f0; transition: background 0.2s; }
        .sort-card:last-child { border-bottom: none; }
        .sort-card:hover { background: #f9fafb; }
        .sort-card.checked { background: #f0fdf4; }
        .checkbox-wrapper { flex-shrink: 0; }
        .checkbox-custom { width: 26px; height: 26px; border-radius: 6px; border: 2px solid #ddd; display: flex; align-items: center; justify-content: center; cursor: pointer; transition: all 0.2s; }
        .checkbox-custom.checked { background: #10b981; border-color: #10b981; }
        .checkbox-custom.checked::after { content: '✓'; color: white; font-size: 16px; font-weight: bold; }
        .item-info { flex: 1; min-width: 0; }
        .item-name { font-size: 15px; font-weight: 600; color: #333; margin-bottom: 3px; }
        .item-detail { font-size: 12px; color: #666; display: flex; gap: 12px; flex-wrap: wrap; }
        .item-detail span { background: #f3f4f6; padding: 2px 6px; border-radius: 3px; }
        .quantity-badge { flex-shrink: 0; text-align: right; }
        .quantity-value { font-size: 18px; font-weight: bold; color: #3b82f6; }
        .quantity-unit { font-size: 11px; color: #666; }
        .filter-bar { background: white; padding: 12px; border-bottom: 1px solid #eee; display: flex; gap: 8px; }
        .filter-bar input { flex: 1; padding: 10px 14px; border: 1px solid #ddd; border-radius: 8px; font-size: 14px; }
        .filter-bar button { padding: 10px 16px; border: none; border-radius: 8px; background: #3b82f6; color: white; font-size: 14px; }
        .filter-bar button.clear { background: #f3f4f6; color: #666; }
        .bottom-bar { background: white; padding: 6px 12px; position: fixed; bottom: 0; left: 0; right: 0; display: flex; gap: 6px; box-shadow: 0 -2px 8px rgba(0,0,0,0.05); }
        .bottom-bar button { flex: 1; padding: 6px; border: none; border-radius: 6px; font-size: 11px; font-weight: 600; }
        .btn-select-all { background: #f3f4f6; color: #333; }
        .btn-clear-all { background: #fee2e2; color: #dc2626; }
        .btn-export { background: #8b5cf6; color: white; }
        .empty-state { text-align: center; padding: 60px 20px; color: #999; }
        .empty-icon { font-size: 48px; margin-bottom: 16px; }
        .cat-hunxian { background: linear-gradient(135deg, #dc2626 0%, #ef4444 100%); }
        .cat-xianshu { background: linear-gradient(135deg, #16a34a 0%, #22c55e 100%); }
        .cat-liangyou { background: linear-gradient(135deg, #1d4ed8 0%, #3b82f6 100%); }
        .cat-douzhi { background: linear-gradient(135deg, #ca8a04 0%, #eab308 100%); }
        .cat-fenmian { background: linear-gradient(135deg, #64748b 0%, #94a3b8 100%); }
        .cat-shuiguo { background: linear-gradient(135deg, #ea580c 0%, #f97316 100%); }
        .cat-other { background: linear-gradient(135deg, #6b7280 0%, #9ca3af 100%); }
        .correction-input { width: 60px; padding: 6px; border: 1px solid #ddd; border-radius: 4px; font-size: 14px; text-align: center; }
        .correction-input:focus { outline: none; border-color: #3b82f6; }
        .corrected-tag { background: #fef3c7; color: #d97706; padding: 2px 5px; border-radius: 3px; font-size: 11px; }
        .purchaser-section { margin: 0 12px; border-bottom: 1px solid #f0f0f0; padding: 10px 0; }
        .purchaser-section:last-child { border-bottom: none; }
        .purchaser-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px; padding: 8px 12px; background: #f8fafc; border-radius: 6px; }
        .purchaser-name { font-size: 14px; font-weight: 600; color: #333; }
        .purchaser-qty { font-size: 12px; color: #666; }
    </style>
</head>
<body>
    <div class="sticky-header">
    <div class="page-header">
        <h1>🏷️ 按分类分拣</h1>
        <div class="header-info">按商品分类汇总采购清单，便于分发给不同供应商</div>
        <div class="switch-links">
            <a href="/mobile/sort" class="switch-link">统筹分拣</a>
            <a href="/mobile/sort_by_purchaser" class="switch-link">按单位分拣</a>
            <a href="/mobile/sort_comprehensive" class="switch-link">综合分拣</a>
        </div>
        <div class="stats-bar">
            <div class="stat-item">
                <div class="stat-value" id="totalCount">0</div>
                <div class="stat-label">商品种类</div>
            </div>
            <div class="stat-item">
                <div class="stat-value" id="checkedCount">0</div>
                <div class="stat-label">已采购</div>
            </div>
            <div class="stat-item">
                <div class="stat-value" id="uncheckedCount">0</div>
                <div class="stat-label">待采购</div>
            </div>
        </div>
    </div>
    
    <div class="filter-bar">
        <input type="text" id="searchInput" placeholder="搜索商品名称..." oninput="filterItems()">
        <button class="clear" onclick="clearSearch()">清除</button>
    </div>
    </div>
    
    <div class="content-area" id="itemsContainer">
        <div class="empty-state">
            <div class="empty-icon">📭</div>
            <div>暂无采购订单</div>
        </div>
    </div>
    
    <div class="bottom-bar">
        <button class="btn-select-all" onclick="toggleSelectAll()">全选</button>
        <button class="btn-clear-all" onclick="clearSelection()">清空</button>
        <button class="btn-clear-all" onclick="clearCorrections()">清除修正</button>
        <button class="btn-print" onclick="saveCorrectionsToServer()">保存修正</button>
        <button class="btn-export" onclick="exportExcel()">导出XLSX</button>
    </div>

    <script>
        let categories = [];
        let checkedIds = new Set();
        let correctedQuantities = {};

        async function loadItems() {
            try {
                const res = await fetch('/api/sales_order/sort_items_by_category');
                categories = await res.json();
                loadCheckedState();
                loadCorrectedQuantities();
                renderItems();
                updateStats();
            } catch (e) {
                console.error('加载失败:', e);
            }
        }

        function loadCheckedState() {
            const saved = localStorage.getItem('sort_by_category_checked_ids');
            if (saved) {
                const ids = JSON.parse(saved);
                ids.forEach(id => checkedIds.add(id));
            }
        }

        function saveCheckedState() {
            localStorage.setItem('sort_by_category_checked_ids', JSON.stringify([...checkedIds]));
        }

        function loadCorrectedQuantities() {
            const saved = localStorage.getItem('sort_by_category_corrections');
            if (saved) {
                correctedQuantities = JSON.parse(saved);
            }
        }

        function saveCorrectedQuantities() {
            localStorage.setItem('sort_by_category_corrections', JSON.stringify(correctedQuantities));
        }

        function updateCorrectedQuantity(productId, value) {
            const numValue = parseFloat(value);
            if (numValue && numValue > 0) {
                correctedQuantities[productId] = numValue;
            } else {
                delete correctedQuantities[productId];
            }
            saveCorrectedQuantities();
        }

        function getDisplayQuantity(item) {
            if (correctedQuantities[item.item_id] !== undefined) {
                return correctedQuantities[item.item_id];
            }
            return item.quantity;
        }

        function clearCorrections() {
            correctedQuantities = {};
            saveCorrectedQuantities();
            renderItems();
        }

        async function saveCorrectionsToServer() {
            if (Object.keys(correctedQuantities).length === 0) {
                alert('没有需要保存的修正');
                return;
            }
            
            const corrections = [];
            for (const [itemId, quantity] of Object.entries(correctedQuantities)) {
                corrections.push({ id: parseInt(itemId), quantity: quantity });
            }
            
            try {
                const res = await fetch('/api/sales_order/correction', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ corrections })
                });
                const text = await res.text();
                alert(text);
                clearCorrections();
                loadItems();
            } catch (e) {
                console.error('保存失败:', e);
                alert('保存失败，请重试');
            }
        }

        function toggleCheck(productId) {
            if (checkedIds.has(productId)) {
                checkedIds.delete(productId);
            } else {
                checkedIds.add(productId);
            }
            saveCheckedState();
            renderItems();
            updateStats();
        }

        function toggleSelectAll() {
            let allIds = [];
            categories.forEach(cat => {
                if (cat.purchasers) {
                    cat.purchasers.forEach(purchaser => {
                        purchaser.items.forEach(item => allIds.push(item.item_id));
                    });
                }
            });
            if (checkedIds.size === allIds.length) {
                checkedIds.clear();
            } else {
                allIds.forEach(id => checkedIds.add(id));
            }
            saveCheckedState();
            renderItems();
            updateStats();
        }

        function clearSelection() {
            checkedIds.clear();
            saveCheckedState();
            renderItems();
            updateStats();
        }

        function filterItems() {
            renderItems();
        }

        function clearSearch() {
            document.getElementById('searchInput').value = '';
            renderItems();
        }

        function updateStats() {
            let totalCount = 0;
            categories.forEach(cat => {
                if (cat.purchasers) {
                    cat.purchasers.forEach(purchaser => {
                        totalCount += purchaser.items.length;
                    });
                }
            });
            document.getElementById('totalCount').textContent = totalCount;
            document.getElementById('checkedCount').textContent = checkedIds.size;
            document.getElementById('uncheckedCount').textContent = totalCount - checkedIds.size;
        }

        function getCategoryClass(name) {
            if (name.includes('荤鲜')) return 'cat-hunxian';
            if (name.includes('鲜蔬')) return 'cat-xianshu';
            if (name.includes('粮油') || name.includes('干调')) return 'cat-liangyou';
            if (name.includes('豆制品')) return 'cat-douzhi';
            if (name.includes('粉面')) return 'cat-fenmian';
            if (name.includes('水果')) return 'cat-shuiguo';
            return 'cat-other';
        }

        function renderItems() {
            const container = document.getElementById('itemsContainer');
            const keyword = document.getElementById('searchInput').value.trim().toLowerCase();
            
            let hasItems = false;
            let html = '';
            
            categories.forEach(category => {
                let hasPurchaserItems = false;
                let totalQty = 0;
                let catHeaderRendered = false;
                
                html += '<div class="category-section">';
                const catClass = getCategoryClass(category.category_name);
                
                if (category.purchasers) {
                    category.purchasers.forEach(purchaser => {
                        let filteredItems = purchaser.items.filter(item => 
                            item.product_name.toLowerCase().includes(keyword)
                        );
                        
                        if (filteredItems.length === 0) return;
                        
                        hasPurchaserItems = true;
                        totalQty += purchaser.total_quantity;
                        
                        if (!catHeaderRendered) {
                            html += '<div class="category-header ' + catClass + '">';
                            html += '<h3>' + category.category_name + '</h3>';
                            html += '<div class="category-stats" id="cat-stats-' + category.category_name.replace(/\s/g, '') + '">统计中...</div>';
                            html += '</div>';
                            html += '<div class="category-body">';
                            catHeaderRendered = true;
                            hasItems = true;
                        }
                        
                        html += '<div class="purchaser-section">';
                        html += '<div class="purchaser-header">';
                        html += '<div class="purchaser-name">📍 ' + purchaser.purchaser_name + '</div>';
                        html += '<div class="purchaser-qty">共 ' + purchaser.total_quantity.toFixed(0) + ' 件</div>';
                        html += '</div>';
                        
                        filteredItems.forEach(item => {
                            const isChecked = checkedIds.has(item.item_id);
                            const displayQty = getDisplayQuantity(item);
                            const isCorrected = correctedQuantities[item.item_id] !== undefined;
                            html += '<div class="sort-card ' + (isChecked ? 'checked' : '') + '" onclick="toggleCheck(' + item.item_id + ')">';
                            html += '<div class="checkbox-wrapper">';
                            html += '<div class="checkbox-custom ' + (isChecked ? 'checked' : '') + '"></div>';
                            html += '</div>';
                            html += '<div class="item-info">';
                            html += '<div class="item-name">' + item.product_name + '</div>';
                            html += '<div class="item-detail">';
                            html += '<span>' + item.unit + '</span>';
                            if (item.remark && item.remark.trim()) {
                                html += '<span style="color:#d97706;">备注: ' + item.remark + '</span>';
                            }
                            if (isCorrected) {
                                html += '<span class="corrected-tag">修正: ' + item.quantity + '→' + displayQty + '</span>';
                            }
                            html += '</div>';
                            html += '</div>';
                            html += '<div class="quantity-badge">';
                            html += '<div class="quantity-value">' + displayQty + '</div>';
                            html += '<div class="quantity-unit">' + item.unit + '</div>';
                            html += '<input type="number" min="0" step="any" class="correction-input" placeholder="修正" ' + (isCorrected ? 'value="' + correctedQuantities[item.item_id] + '"' : '') + ' onchange="updateCorrectedQuantity(' + item.item_id + ', this.value)" onclick="event.stopPropagation()">';
                            html += '</div>';
                            html += '</div>';
                        });
                        
                        html += '</div>';
                    });
                }
                
                if (hasPurchaserItems) {
                    const catStatsId = 'cat-stats-' + category.category_name.replace(/\s/g, '');
                    setTimeout(() => {
                        const el = document.getElementById(catStatsId);
                        if (el) el.textContent = '共 ' + totalQty.toFixed(0) + ' 件';
                    }, 100);
                }
                
                html += '</div></div>';
            });
            
            if (!hasItems) {
                container.innerHTML = '<div class="empty-state"><div class="empty-icon">🔍</div><div>没有找到匹配的商品</div></div>';
                return;
            }
            
            container.innerHTML = html;
        }

        function exportExcel() {
            window.location.href = '/api/sales_order/sort_items_by_category_excel';
        }

        loadItems();
    </script>
</body>
</html>
    "#.to_string())
}

async fn page_mobile_sort_by_supplier() -> Html<String> {
    Html(r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no">
    <title>按供应商分拣</title>
    <link rel="stylesheet" href="/static/bootstrap.min.css">
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; background: #f5f7fa; }
        .sticky-header { position: sticky; top: 0; z-index: 100; }
        .page-header { background: linear-gradient(135deg, #10b981 0%, #34d399 100%); color: white; padding: 16px 20px; box-shadow: 0 2px 8px rgba(0,0,0,0.1); }
        .page-header h1 { font-size: 18px; margin: 0; font-weight: 600; }
        .header-info { font-size: 13px; opacity: 0.9; margin-top: 4px; }
        .switch-links { display: flex; gap: 8px; margin-top: 8px; }
        .switch-link { padding: 6px 12px; background: rgba(255,255,255,0.2); border-radius: 6px; font-size: 13px; text-decoration: none; color: white; }
        .switch-link:hover { background: rgba(255,255,255,0.3); }
        .stats-bar { display: flex; gap: 12px; margin-top: 12px; }
        .stat-item { background: rgba(255,255,255,0.2); padding: 8px 12px; border-radius: 8px; flex: 1; text-align: center; }
        .stat-value { font-size: 16px; font-weight: bold; }
        .stat-label { font-size: 11px; opacity: 0.8; }
        .content-area { padding: 12px; padding-bottom: 80px; }
        .category-section { border-radius: 12px; margin-bottom: 16px; overflow: hidden; box-shadow: 0 2px 8px rgba(0,0,0,0.08); }
        .category-header { padding: 14px 16px; display: flex; align-items: center; justify-content: space-between; }
        .category-header h3 { font-size: 16px; margin: 0; color: white; font-weight: 600; }
        .category-stats { font-size: 13px; opacity: 0.9; color: white; }
        .category-body { background: white; padding: 0; }
        .sort-card { display: flex; align-items: center; gap: 14px; padding: 14px 16px; border-bottom: 1px solid #f0f0f0; transition: background 0.2s; }
        .sort-card:last-child { border-bottom: none; }
        .sort-card:hover { background: #f9fafb; }
        .sort-card.checked { background: #f0fdf4; }
        .checkbox-wrapper { flex-shrink: 0; }
        .checkbox-custom { width: 26px; height: 26px; border-radius: 6px; border: 2px solid #ddd; display: flex; align-items: center; justify-content: center; cursor: pointer; transition: all 0.2s; }
        .checkbox-custom.checked { background: #10b981; border-color: #10b981; }
        .checkbox-custom.checked::after { content: '✓'; color: white; font-size: 16px; font-weight: bold; }
        .item-info { flex: 1; min-width: 0; }
        .item-name { font-size: 15px; font-weight: 600; color: #333; margin-bottom: 3px; }
        .item-detail { font-size: 12px; color: #666; display: flex; gap: 12px; flex-wrap: wrap; }
        .item-detail span { background: #f3f4f6; padding: 2px 6px; border-radius: 3px; }
        .quantity-badge { flex-shrink: 0; text-align: right; }
        .quantity-value { font-size: 18px; font-weight: bold; color: #3b82f6; }
        .quantity-unit { font-size: 11px; color: #666; }
        .filter-bar { background: white; padding: 12px; border-bottom: 1px solid #eee; display: flex; gap: 8px; }
        .filter-bar input { flex: 1; padding: 10px 14px; border: 1px solid #ddd; border-radius: 8px; font-size: 14px; }
        .filter-bar button { padding: 10px 16px; border: none; border-radius: 8px; background: #3b82f6; color: white; font-size: 14px; }
        .filter-bar button.clear { background: #f3f4f6; color: #666; }
        .bottom-bar { background: white; padding: 6px 12px; position: fixed; bottom: 0; left: 0; right: 0; display: flex; gap: 6px; box-shadow: 0 -2px 8px rgba(0,0,0,0.05); }
        .bottom-bar button { flex: 1; padding: 6px; border: none; border-radius: 6px; font-size: 11px; font-weight: 600; }
        .btn-select-all { background: #f3f4f6; color: #333; }
        .btn-clear-all { background: #fee2e2; color: #dc2626; }
        .btn-export { background: #10b981; color: white; }
        .empty-state { text-align: center; padding: 60px 20px; color: #999; }
        .empty-icon { font-size: 48px; margin-bottom: 16px; }
        .cat-supplier { background: linear-gradient(135deg, #10b981 0%, #34d399 100%); }
        .correction-input { width: 60px; padding: 6px; border: 1px solid #ddd; border-radius: 4px; font-size: 14px; text-align: center; }
        .correction-input:focus { outline: none; border-color: #3b82f6; }
        .corrected-tag { background: #fef3c7; color: #d97706; padding: 2px 5px; border-radius: 3px; font-size: 11px; }
        .purchaser-section { margin: 0 12px; border-bottom: 1px solid #f0f0f0; padding: 10px 0; }
        .purchaser-section:last-child { border-bottom: none; }
        .purchaser-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px; padding: 8px 12px; background: #f8fafc; border-radius: 6px; }
        .purchaser-name { font-size: 14px; font-weight: 600; color: #333; }
        .purchaser-qty { font-size: 12px; color: #666; }
    </style>
</head>
<body>
    <div class="sticky-header">
    <div class="page-header">
        <h1>🏭 按供应商分拣</h1>
        <div class="header-info">按供应商分类汇总采购清单，便于分发给不同供应商</div>
        <div class="switch-links">
            <a href="/mobile/sort" class="switch-link">统筹分拣</a>
            <a href="/mobile/sort_by_category" class="switch-link">按分类分拣</a>
            <a href="/mobile/sort_by_supplier" class="switch-link">按供应商分拣</a>
            <a href="/mobile/sort_by_purchaser" class="switch-link">按单位分拣</a>
            <a href="/mobile/sort_comprehensive" class="switch-link">综合分拣</a>
        </div>
        <div class="stats-bar">
            <div class="stat-item">
                <div class="stat-value" id="totalCount">0</div>
                <div class="stat-label">商品种类</div>
            </div>
            <div class="stat-item">
                <div class="stat-value" id="checkedCount">0</div>
                <div class="stat-label">已采购</div>
            </div>
            <div class="stat-item">
                <div class="stat-value" id="uncheckedCount">0</div>
                <div class="stat-label">待采购</div>
            </div>
        </div>
    </div>
    
    <div class="filter-bar">
        <input type="text" id="searchInput" placeholder="搜索商品名称..." oninput="filterItems()">
        <button class="clear" onclick="clearSearch()">清除</button>
    </div>
    </div>
    
    <div class="content-area" id="itemsContainer">
        <div class="empty-state">
            <div class="empty-icon">📭</div>
            <div>暂无采购订单</div>
        </div>
    </div>
    
    <div class="bottom-bar">
        <button class="btn-select-all" onclick="toggleSelectAll()">全选</button>
        <button class="btn-clear-all" onclick="clearSelection()">清空</button>
        <button class="btn-clear-all" onclick="clearCorrections()">清除修正</button>
        <button class="btn-print" onclick="saveCorrectionsToServer()">保存修正</button>
        <button class="btn-export" onclick="exportExcel()">导出XLSX</button>
    </div>

    <script>
        let suppliers = [];
        let checkedIds = new Set();
        let correctedQuantities = {};

        async function loadItems() {
            try {
                const res = await fetch('/api/sales_order/sort_items_by_supplier');
                suppliers = await res.json();
                loadCheckedState();
                loadCorrectedQuantities();
                renderItems();
                updateStats();
            } catch (e) {
                console.error('加载失败:', e);
            }
        }

        function loadCheckedState() {
            const saved = localStorage.getItem('sort_by_supplier_checked_ids');
            if (saved) {
                const ids = JSON.parse(saved);
                ids.forEach(id => checkedIds.add(id));
            }
        }

        function saveCheckedState() {
            localStorage.setItem('sort_by_supplier_checked_ids', JSON.stringify([...checkedIds]));
        }

        function loadCorrectedQuantities() {
            const saved = localStorage.getItem('sort_by_supplier_corrections');
            if (saved) {
                correctedQuantities = JSON.parse(saved);
            }
        }

        function saveCorrectedQuantities() {
            localStorage.setItem('sort_by_supplier_corrections', JSON.stringify(correctedQuantities));
        }

        function updateCorrectedQuantity(productId, value) {
            const numValue = parseFloat(value);
            if (numValue && numValue > 0) {
                correctedQuantities[productId] = numValue;
            } else {
                delete correctedQuantities[productId];
            }
            saveCorrectedQuantities();
        }

        function getDisplayQuantity(item) {
            if (correctedQuantities[item.item_id] !== undefined) {
                return correctedQuantities[item.item_id];
            }
            return item.quantity;
        }

        function clearCorrections() {
            correctedQuantities = {};
            saveCorrectedQuantities();
            renderItems();
        }

        async function saveCorrectionsToServer() {
            if (Object.keys(correctedQuantities).length === 0) {
                alert('没有需要保存的修正');
                return;
            }
            
            const corrections = [];
            for (const [itemId, quantity] of Object.entries(correctedQuantities)) {
                corrections.push({ id: parseInt(itemId), quantity: quantity });
            }
            
            try {
                const res = await fetch('/api/sales_order/correction', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ corrections })
                });
                const text = await res.text();
                alert(text);
                clearCorrections();
                loadItems();
            } catch (e) {
                console.error('保存失败:', e);
                alert('保存失败，请重试');
            }
        }

        function toggleCheck(productId) {
            if (checkedIds.has(productId)) {
                checkedIds.delete(productId);
            } else {
                checkedIds.add(productId);
            }
            saveCheckedState();
            renderItems();
            updateStats();
        }

        function toggleSelectAll() {
            let allIds = [];
            suppliers.forEach(supplier => {
                if (supplier.purchasers) {
                    supplier.purchasers.forEach(purchaser => {
                        purchaser.items.forEach(item => allIds.push(item.item_id));
                    });
                }
            });
            if (checkedIds.size === allIds.length) {
                checkedIds.clear();
            } else {
                allIds.forEach(id => checkedIds.add(id));
            }
            saveCheckedState();
            renderItems();
            updateStats();
        }

        function clearSelection() {
            checkedIds.clear();
            saveCheckedState();
            renderItems();
            updateStats();
        }

        function filterItems() {
            renderItems();
        }

        function clearSearch() {
            document.getElementById('searchInput').value = '';
            renderItems();
        }

        function updateStats() {
            let totalCount = 0;
            suppliers.forEach(supplier => {
                if (supplier.purchasers) {
                    supplier.purchasers.forEach(purchaser => {
                        totalCount += purchaser.items.length;
                    });
                }
            });
            document.getElementById('totalCount').textContent = totalCount;
            document.getElementById('checkedCount').textContent = checkedIds.size;
            document.getElementById('uncheckedCount').textContent = totalCount - checkedIds.size;
        }

        function renderItems() {
            const container = document.getElementById('itemsContainer');
            const keyword = document.getElementById('searchInput').value.trim().toLowerCase();
            
            let hasItems = false;
            let html = '';
            
            suppliers.forEach(supplier => {
                let hasPurchaserItems = false;
                let totalQty = 0;
                let catHeaderRendered = false;
                
                html += '<div class="category-section">';
                const catClass = 'cat-supplier';
                
                if (supplier.purchasers) {
                    supplier.purchasers.forEach(purchaser => {
                        let filteredItems = purchaser.items.filter(item => 
                            item.product_name.toLowerCase().includes(keyword)
                        );
                        
                        if (filteredItems.length === 0) return;
                        
                        hasPurchaserItems = true;
                        totalQty += purchaser.total_quantity;
                        
                        if (!catHeaderRendered) {
                            html += '<div class="category-header ' + catClass + '">';
                            html += '<h3>' + supplier.supplier_name + '</h3>';
                            html += '<div class="category-stats" id="cat-stats-' + supplier.supplier_name.replace(/\s/g, '') + '">统计中...</div>';
                            html += '</div>';
                            html += '<div class="category-body">';
                            catHeaderRendered = true;
                            hasItems = true;
                        }
                        
                        html += '<div class="purchaser-section">';
                        html += '<div class="purchaser-header">';
                        html += '<div class="purchaser-name">📍 ' + purchaser.purchaser_name + '</div>';
                        html += '<div class="purchaser-qty">共 ' + purchaser.total_quantity.toFixed(0) + ' 件</div>';
                        html += '</div>';
                        
                        filteredItems.forEach(item => {
                            const isChecked = checkedIds.has(item.item_id);
                            const displayQty = getDisplayQuantity(item);
                            const isCorrected = correctedQuantities[item.item_id] !== undefined;
                            html += '<div class="sort-card ' + (isChecked ? 'checked' : '') + '" onclick="toggleCheck(' + item.item_id + ')">';
                            html += '<div class="checkbox-wrapper">';
                            html += '<div class="checkbox-custom ' + (isChecked ? 'checked' : '') + '"></div>';
                            html += '</div>';
                            html += '<div class="item-info">';
                            html += '<div class="item-name">' + item.product_name + '</div>';
                            html += '<div class="item-detail">';
                            html += '<span>' + item.unit + '</span>';
                            if (item.remark && item.remark.trim()) {
                                html += '<span style="color:#d97706;">备注: ' + item.remark + '</span>';
                            }
                            if (isCorrected) {
                                html += '<span class="corrected-tag">修正: ' + item.quantity + '→' + displayQty + '</span>';
                            }
                            html += '</div>';
                            html += '</div>';
                            html += '<div class="quantity-badge">';
                            html += '<div class="quantity-value">' + displayQty + '</div>';
                            html += '<div class="quantity-unit">' + item.unit + '</div>';
                            html += '<input type="number" min="0" step="any" class="correction-input" placeholder="修正" ' + (isCorrected ? 'value="' + correctedQuantities[item.item_id] + '"' : '') + ' onchange="updateCorrectedQuantity(' + item.item_id + ', this.value)" onclick="event.stopPropagation()">';
                            html += '</div>';
                            html += '</div>';
                        });
                        
                        html += '</div>';
                    });
                }
                
                if (hasPurchaserItems) {
                    const catStatsId = 'cat-stats-' + supplier.supplier_name.replace(/\s/g, '');
                    setTimeout(() => {
                        const el = document.getElementById(catStatsId);
                        if (el) el.textContent = '共 ' + totalQty.toFixed(0) + ' 件';
                    }, 100);
                }
                
                html += '</div></div>';
            });
            
            if (!hasItems) {
                container.innerHTML = '<div class="empty-state"><div class="empty-icon">🔍</div><div>没有找到匹配的商品</div></div>';
                return;
            }
            
            container.innerHTML = html;
        }

        function exportExcel() {
            window.location.href = '/api/sales_order/sort_items_by_supplier_excel';
        }

        loadItems();
    </script>
</body>
</html>
    "#.to_string())
}

async fn page_mobile_sort_comprehensive() -> Html<String> {
    Html(r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no">
    <title>综合分拣</title>
    <link rel="stylesheet" href="/static/bootstrap.min.css">
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; background: #f5f7fa; }
        .sticky-header { position: sticky; top: 0; z-index: 100; }
        .page-header { background: linear-gradient(135deg, #06b6d4 0%, #0ea5e9 100%); color: white; padding: 16px 20px; box-shadow: 0 2px 8px rgba(0,0,0,0.1); }
        .page-header h1 { font-size: 18px; margin: 0; font-weight: 600; }
        .header-info { font-size: 13px; opacity: 0.9; margin-top: 4px; }
        .switch-links { display: flex; gap: 8px; margin-top: 8px; flex-wrap: wrap; }
        .switch-link { padding: 6px 12px; background: rgba(255,255,255,0.2); border-radius: 6px; font-size: 13px; text-decoration: none; color: white; }
        .switch-link:hover { background: rgba(255,255,255,0.3); }
        .stats-bar { display: flex; gap: 12px; margin-top: 12px; }
        .stat-item { background: rgba(255,255,255,0.2); padding: 8px 12px; border-radius: 8px; flex: 1; text-align: center; }
        .stat-value { font-size: 16px; font-weight: bold; }
        .stat-label { font-size: 11px; opacity: 0.8; }
        .content-area { padding: 12px; padding-bottom: 80px; }
        .filter-bar { background: white; padding: 12px; border-bottom: 1px solid #eee; display: flex; gap: 8px; }
        .filter-bar input { flex: 1; padding: 10px 14px; border: 1px solid #ddd; border-radius: 8px; font-size: 14px; }
        .filter-bar button { padding: 10px 16px; border: none; border-radius: 8px; background: #3b82f6; color: white; font-size: 14px; }
        .filter-bar button.clear { background: #f3f4f6; color: #666; }
        .purchaser-section { margin-bottom: 16px; border-radius: 12px; overflow: hidden; box-shadow: 0 2px 8px rgba(0,0,0,0.08); }
        .purchaser-header { background: #fff; padding: 14px 16px; display: flex; align-items: center; justify-content: space-between; cursor: pointer; border-bottom: 1px solid #f0f0f0; }
        .purchaser-header h3 { font-size: 16px; margin: 0; font-weight: 600; color: #333; }
        .purchaser-stats { font-size: 13px; color: #666; }
        .expand-icon { font-size: 18px; color: #999; transition: transform 0.2s; }
        .expand-icon.expanded { transform: rotate(180deg); }
        .purchaser-body { background: #fafafa; }
        .category-row { padding: 0 12px; }
        .category-title { padding: 10px 14px; border-radius: 8px; margin: 8px 4px; color: white; font-size: 14px; font-weight: 600; display: flex; align-items: center; justify-content: space-between; }
        .sort-card { display: flex; align-items: center; gap: 12px; padding: 12px 14px; background: white; margin: 4px; border-radius: 8px; border-bottom: 1px solid #f5f5f5; transition: background 0.2s; }
        .sort-card:last-child { border-bottom: none; }
        .sort-card:hover { background: #f9fafb; }
        .sort-card.checked { background: #f0fdf4; }
        .checkbox-wrapper { flex-shrink: 0; }
        .checkbox-custom { width: 24px; height: 24px; border-radius: 6px; border: 2px solid #ddd; display: flex; align-items: center; justify-content: center; cursor: pointer; transition: all 0.2s; }
        .checkbox-custom.checked { background: #10b981; border-color: #10b981; }
        .checkbox-custom.checked::after { content: '✓'; color: white; font-size: 14px; font-weight: bold; }
        .item-info { flex: 1; min-width: 0; }
        .item-name { font-size: 14px; font-weight: 600; color: #333; margin-bottom: 2px; }
        .item-detail { font-size: 11px; color: #666; display: flex; gap: 10px; flex-wrap: wrap; }
        .item-detail span { background: #f3f4f6; padding: 2px 5px; border-radius: 3px; }
        .quantity-badge { flex-shrink: 0; text-align: right; }
        .quantity-value { font-size: 16px; font-weight: bold; color: #3b82f6; }
        .quantity-unit { font-size: 10px; color: #666; }
        .correction-input { width: 60px; padding: 6px; border: 1px solid #ddd; border-radius: 4px; font-size: 14px; text-align: center; }
        .correction-input:focus { outline: none; border-color: #3b82f6; }
        .correction-label { font-size: 11px; color: #666; margin-top: 2px; }
        .corrected-tag { background: #fef3c7; color: #d97706; padding: 2px 5px; border-radius: 3px; font-size: 11px; }
        .bottom-bar { background: white; padding: 6px 12px; position: fixed; bottom: 0; left: 0; right: 0; display: flex; gap: 6px; box-shadow: 0 -2px 8px rgba(0,0,0,0.05); }
        .bottom-bar button { flex: 1; padding: 6px; border: none; border-radius: 6px; font-size: 11px; font-weight: 600; }
        .btn-select-all { background: #f3f4f6; color: #333; }
        .btn-clear-all { background: #fee2e2; color: #dc2626; }
        .btn-export { background: #06b6d4; color: white; }
        .empty-state { text-align: center; padding: 60px 20px; color: #999; }
        .empty-icon { font-size: 48px; margin-bottom: 16px; }
        .cat-hunxian { background: linear-gradient(135deg, #dc2626 0%, #ef4444 100%); }
        .cat-xianshu { background: linear-gradient(135deg, #16a34a 0%, #22c55e 100%); }
        .cat-liangyou { background: linear-gradient(135deg, #1d4ed8 0%, #3b82f6 100%); }
        .cat-douzhi { background: linear-gradient(135deg, #ca8a04 0%, #eab308 100%); }
        .cat-fenmian { background: linear-gradient(135deg, #64748b 0%, #94a3b8 100%); }
        .cat-shuiguo { background: linear-gradient(135deg, #ea580c 0%, #f97316 100%); }
        .cat-other { background: linear-gradient(135deg, #6b7280 0%, #9ca3af 100%); }
    </style>
</head>
<body>
    <div class="sticky-header">
    <div class="page-header">
        <h1>🔄 综合分拣</h1>
        <div class="header-info">先按采购单位，再按商品分类汇总采购清单</div>
        <div class="switch-links">
            <a href="/mobile/sort" class="switch-link">统筹分拣</a>
            <a href="/mobile/sort_by_purchaser" class="switch-link">按单位分拣</a>
            <a href="/mobile/sort_by_category" class="switch-link">按分类分拣</a>
        </div>
        <div class="stats-bar">
            <div class="stat-item">
                <div class="stat-value" id="totalCount">0</div>
                <div class="stat-label">采购单位</div>
            </div>
            <div class="stat-item">
                <div class="stat-value" id="checkedCount">0</div>
                <div class="stat-label">已采购</div>
            </div>
            <div class="stat-item">
                <div class="stat-value" id="uncheckedCount">0</div>
                <div class="stat-label">待采购</div>
            </div>
        </div>
    </div>
    
    <div class="filter-bar">
        <input type="text" id="searchInput" placeholder="搜索商品名称..." oninput="filterItems()">
        <button class="clear" onclick="clearSearch()">清除</button>
    </div>
    </div>
    
    <div class="content-area" id="itemsContainer">
        <div class="empty-state">
            <div class="empty-icon">📭</div>
            <div>暂无采购订单</div>
        </div>
    </div>
    
    <div class="bottom-bar">
        <button class="btn-select-all" onclick="toggleSelectAll()">全选</button>
        <button class="btn-clear-all" onclick="clearSelection()">清空</button>
        <button class="btn-clear-all" onclick="clearCorrections()">清除修正</button>
        <button class="btn-print" onclick="saveCorrectionsToServer()">保存修正</button>
        <button class="btn-export" onclick="exportExcel()">导出XLSX</button>
    </div>

    <script>
        let purchasers = [];
        let checkedIds = new Set();
        let expandedPurchasers = new Set();
        let correctedQuantities = {};

        async function loadItems() {
            try {
                const res = await fetch('/api/sales_order/sort_comprehensive');
                purchasers = await res.json();
                loadCheckedState();
                loadExpandedState();
                loadCorrectedQuantities();
                renderItems();
                updateStats();
            } catch (e) {
                console.error('加载失败:', e);
            }
        }

        function loadCheckedState() {
            const saved = localStorage.getItem('sort_comprehensive_checked_ids');
            if (saved) {
                const ids = JSON.parse(saved);
                ids.forEach(id => checkedIds.add(id));
            }
        }

        function saveCheckedState() {
            localStorage.setItem('sort_comprehensive_checked_ids', JSON.stringify([...checkedIds]));
        }

        function loadExpandedState() {
            const saved = localStorage.getItem('sort_comprehensive_expanded');
            if (saved) {
                const ids = JSON.parse(saved);
                ids.forEach(id => expandedPurchasers.add(id));
            }
        }

        function saveExpandedState() {
            localStorage.setItem('sort_comprehensive_expanded', JSON.stringify([...expandedPurchasers]));
        }

        function loadCorrectedQuantities() {
            const saved = localStorage.getItem('sort_comprehensive_corrections');
            if (saved) {
                correctedQuantities = JSON.parse(saved);
            }
        }

        function saveCorrectedQuantities() {
            localStorage.setItem('sort_comprehensive_corrections', JSON.stringify(correctedQuantities));
        }

        function updateCorrectedQuantity(itemId, value) {
            const numValue = parseFloat(value);
            if (numValue && numValue > 0) {
                correctedQuantities[itemId] = numValue;
            } else {
                delete correctedQuantities[itemId];
            }
            saveCorrectedQuantities();
        }

        function getDisplayQuantity(item) {
            if (correctedQuantities[item.id] !== undefined) {
                return correctedQuantities[item.id];
            }
            return item.quantity;
        }

        function clearCorrections() {
            correctedQuantities = {};
            saveCorrectedQuantities();
            renderItems();
        }

        async function saveCorrectionsToServer() {
            if (Object.keys(correctedQuantities).length === 0) {
                alert('没有需要保存的修正');
                return;
            }
            
            const corrections = [];
            for (const [itemId, quantity] of Object.entries(correctedQuantities)) {
                corrections.push({ id: parseInt(itemId), quantity: quantity });
            }
            
            try {
                const res = await fetch('/api/sales_order/correction', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ corrections })
                });
                const text = await res.text();
                alert(text);
                clearCorrections();
                loadItems();
            } catch (e) {
                console.error('保存失败:', e);
                alert('保存失败，请重试');
            }
        }

        function toggleExpand(purchaserId) {
            if (expandedPurchasers.has(purchaserId)) {
                expandedPurchasers.delete(purchaserId);
            } else {
                expandedPurchasers.add(purchaserId);
            }
            saveExpandedState();
            renderItems();
        }

        function toggleCheck(itemId) {
            if (checkedIds.has(itemId)) {
                checkedIds.delete(itemId);
            } else {
                checkedIds.add(itemId);
            }
            saveCheckedState();
            renderItems();
            updateStats();
        }

        function toggleSelectAll() {
            let allIds = [];
            purchasers.forEach(p => {
                p.categories.forEach(c => {
                    c.items.forEach(item => allIds.push(item.id));
                });
            });
            if (checkedIds.size === allIds.length) {
                checkedIds.clear();
            } else {
                allIds.forEach(id => checkedIds.add(id));
            }
            saveCheckedState();
            renderItems();
            updateStats();
        }

        function clearSelection() {
            checkedIds.clear();
            saveCheckedState();
            renderItems();
            updateStats();
        }

        function filterItems() {
            renderItems();
        }

        function clearSearch() {
            document.getElementById('searchInput').value = '';
            renderItems();
        }

        function updateStats() {
            document.getElementById('totalCount').textContent = purchasers.length;
            let totalItems = 0;
            purchasers.forEach(p => {
                p.categories.forEach(c => {
                    totalItems += c.items.length;
                });
            });
            document.getElementById('checkedCount').textContent = checkedIds.size;
            document.getElementById('uncheckedCount').textContent = totalItems - checkedIds.size;
        }

        function getCategoryClass(name) {
            if (name.includes('荤鲜')) return 'cat-hunxian';
            if (name.includes('鲜蔬')) return 'cat-xianshu';
            if (name.includes('粮油') || name.includes('干调')) return 'cat-liangyou';
            if (name.includes('豆制品')) return 'cat-douzhi';
            if (name.includes('粉面')) return 'cat-fenmian';
            if (name.includes('水果')) return 'cat-shuiguo';
            return 'cat-other';
        }

        function renderItems() {
            const container = document.getElementById('itemsContainer');
            const keyword = document.getElementById('searchInput').value.trim().toLowerCase();
            
            let hasItems = false;
            let html = '';
            
            purchasers.forEach(purchaser => {
                let hasVisibleCategory = false;
                let categoryHtml = '';
                
                purchaser.categories.forEach(category => {
                    let filteredItems = category.items.filter(item => 
                        item.product_name.toLowerCase().includes(keyword)
                    );
                    
                    if (filteredItems.length === 0) return;
                    
                    hasVisibleCategory = true;
                    
                    const catClass = getCategoryClass(category.category_name);
                    const totalQty = filteredItems.reduce((sum, item) => sum + item.quantity, 0);
                    
                    categoryHtml += '<div class="category-row">';
                    categoryHtml += '<div class="category-title ' + catClass + '">';
                    categoryHtml += '<span>' + category.category_name + '</span>';
                    categoryHtml += '<span>' + filteredItems.length + '种 / ' + totalQty.toFixed(0) + '</span>';
                    categoryHtml += '</div>';
                    
                    filteredItems.forEach(item => {
                        const isChecked = checkedIds.has(item.id);
                        const displayQty = getDisplayQuantity(item);
                        const isCorrected = correctedQuantities[item.id] !== undefined;
                        categoryHtml += '<div class="sort-card ' + (isChecked ? 'checked' : '') + '" onclick="toggleCheck(' + item.id + ')">';
                        categoryHtml += '<div class="checkbox-wrapper">';
                        categoryHtml += '<div class="checkbox-custom ' + (isChecked ? 'checked' : '') + '"></div>';
                        categoryHtml += '</div>';
                        categoryHtml += '<div class="item-info">';
                        categoryHtml += '<div class="item-name">' + item.product_name + '</div>';
                        categoryHtml += '<div class="item-detail">';
                        categoryHtml += '<span>' + item.unit + '</span>';
                        if (item.remarks && item.remarks.length > 0) {
                            categoryHtml += '<span style="color:#d97706;">备注: ' + item.remarks.join(', ') + '</span>';
                        }
                        if (isCorrected) {
                            categoryHtml += '<span class="corrected-tag">修正: ' + item.quantity + '→' + displayQty + '</span>';
                        }
                        categoryHtml += '</div>';
                        categoryHtml += '</div>';
                        categoryHtml += '<div class="quantity-badge">';
                        categoryHtml += '<div class="quantity-value">' + displayQty + '</div>';
                        categoryHtml += '<div class="quantity-unit">' + item.unit + '</div>';
                        categoryHtml += '<input type="number" min="0" step="any" class="correction-input" placeholder="修正" ' + (isCorrected ? 'value="' + correctedQuantities[item.id] + '"' : '') + ' onchange="updateCorrectedQuantity(' + item.id + ', this.value)" onclick="event.stopPropagation()">';
                        categoryHtml += '</div>';
                        categoryHtml += '</div>';
                    });
                    
                    categoryHtml += '</div>';
                });
                
                if (!hasVisibleCategory) return;
                
                hasItems = true;
                const isExpanded = expandedPurchasers.has(purchaser.purchaser_id);
                
                html += '<div class="purchaser-section">';
                html += '<div class="purchaser-header" onclick="toggleExpand(' + purchaser.purchaser_id + ')">';
                html += '<h3>' + purchaser.purchaser_name + '</h3>';
                html += '<div class="purchaser-stats">' + purchaser.categories.length + '个分类</div>';
                html += '<div class="expand-icon ' + (isExpanded ? 'expanded' : '') + '">▼</div>';
                html += '</div>';
                
                if (isExpanded) {
                    html += '<div class="purchaser-body">';
                    html += categoryHtml;
                    html += '</div>';
                }
                
                html += '</div>';
            });
            
            if (!hasItems) {
                container.innerHTML = '<div class="empty-state"><div class="empty-icon">🔍</div><div>没有找到匹配的商品</div></div>';
                return;
            }
            
            container.innerHTML = html;
        }

        function exportExcel() {
            window.location.href = '/api/sales_order/sort_comprehensive_excel';
        }

        loadItems();
    </script>
</body>
</html>
    "#.to_string())
}

async fn api_system_config(Json(data): Json<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    for (key, value) in data {
        sqlx::query("INSERT OR REPLACE INTO system_config (key, value) VALUES (?, ?)")
            .bind(&key)
            .bind(&value)
            .execute(pool())
            .await
            .unwrap_or_default();
    }
    (StatusCode::OK, "设置保存成功".to_string())
}

async fn api_user_get(Path(id): Path<i64>) -> impl IntoResponse {
    let rows = sqlx::query("SELECT id, username, nickname, role, status FROM user_account WHERE id = ?")
        .bind(id)
        .fetch_all(pool())
        .await
        .unwrap_or_default();
    
    if rows.is_empty() {
        return (StatusCode::OK, serde_json::to_string(&serde_json::json!({
            "success": false,
            "message": "用户不存在"
        })).unwrap());
    }
    
    let row = &rows[0];
    (StatusCode::OK, serde_json::to_string(&serde_json::json!({
        "success": true,
        "user": {
            "id": row.get::<i64, _>("id"),
            "username": row.get::<String, _>("username"),
            "nickname": row.get::<String, _>("nickname"),
            "role": row.get::<String, _>("role"),
            "status": row.get::<i32, _>("status")
        }
    })).unwrap())
}

async fn api_user_create(Json(data): Json<serde_json::Value>) -> impl IntoResponse {
    let username = data["username"].as_str().unwrap_or("");
    let password = data["password"].as_str().unwrap_or("");
    let nickname = data["nickname"].as_str().unwrap_or("");
    let role = data["role"].as_str().unwrap_or("user");
    
    if username.is_empty() {
        return (StatusCode::OK, serde_json::to_string(&serde_json::json!({
            "success": false,
            "message": "用户名不能为空"
        })).unwrap());
    }
    
    if password.is_empty() {
        return (StatusCode::OK, serde_json::to_string(&serde_json::json!({
            "success": false,
            "message": "密码不能为空"
        })).unwrap());
    }
    
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM user_account WHERE username = ?)")
        .bind(username)
        .fetch_one(pool())
        .await
        .unwrap_or(false);
    
    if exists {
        return (StatusCode::OK, serde_json::to_string(&serde_json::json!({
            "success": false,
            "message": "用户名已存在"
        })).unwrap());
    }
    
    let hashed_pwd = bcrypt::hash(password, bcrypt::DEFAULT_COST).unwrap();
    
    sqlx::query("INSERT INTO user_account (username, password, nickname, role) VALUES (?, ?, ?, ?)")
        .bind(username)
        .bind(hashed_pwd)
        .bind(nickname)
        .bind(role)
        .execute(pool())
        .await
        .ok();
    
    (StatusCode::OK, serde_json::to_string(&serde_json::json!({
        "success": true,
        "message": "用户创建成功"
    })).unwrap())
}

async fn api_user_update(Path(id): Path<i64>, Json(data): Json<serde_json::Value>) -> impl IntoResponse {
    let username = data["username"].as_str().unwrap_or("");
    let password = data["password"].as_str().unwrap_or("");
    let nickname = data["nickname"].as_str().unwrap_or("");
    let role = data["role"].as_str().unwrap_or("");
    
    if username.is_empty() {
        return (StatusCode::OK, serde_json::to_string(&serde_json::json!({
            "success": false,
            "message": "用户名不能为空"
        })).unwrap());
    }
    
    if !password.is_empty() {
        let hashed_pwd = bcrypt::hash(password, bcrypt::DEFAULT_COST).unwrap();
        sqlx::query("UPDATE user_account SET password = ? WHERE id = ?")
            .bind(hashed_pwd)
            .bind(id)
            .execute(pool())
            .await
            .ok();
    }
    
    if role.is_empty() {
        sqlx::query("UPDATE user_account SET username = ?, nickname = ?, update_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(username)
            .bind(nickname)
            .bind(id)
            .execute(pool())
            .await
            .ok();
    } else {
        sqlx::query("UPDATE user_account SET username = ?, nickname = ?, role = ?, update_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(username)
            .bind(nickname)
            .bind(role)
            .bind(id)
            .execute(pool())
            .await
            .ok();
    }
    
    (StatusCode::OK, serde_json::to_string(&serde_json::json!({
        "success": true,
        "message": "用户更新成功"
    })).unwrap())
}

async fn api_user_delete(Path(id): Path<i64>) -> impl IntoResponse {
    sqlx::query("DELETE FROM user_account WHERE id = ?")
        .bind(id)
        .execute(pool())
        .await
        .ok();
    
    (StatusCode::OK, serde_json::to_string(&serde_json::json!({
        "success": true,
        "message": "用户删除成功"
    })).unwrap())
}

async fn api_user_status(Path(id): Path<i64>, Json(data): Json<serde_json::Value>) -> impl IntoResponse {
    let status = data["status"].as_i64().unwrap_or(0);
    
    sqlx::query("UPDATE user_account SET status = ?, update_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(status)
        .bind(id)
        .execute(pool())
        .await
        .ok();
    
    (StatusCode::OK, serde_json::to_string(&serde_json::json!({
        "success": true,
        "message": "状态更新成功"
    })).unwrap())
}

async fn api_backup() -> impl IntoResponse {
    use std::fs;
    use std::path::Path;

    let now = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let backup_dir = "backups";
    if !Path::new(backup_dir).exists() {
        fs::create_dir_all(backup_dir).unwrap_or_default();
    }

    let backup_file = format!("{}/backup_{}.db", backup_dir, now);
    
    let vacuum_sql = format!("VACUUM INTO '{}'", backup_file);
    match sqlx::query(AssertSqlSafe(vacuum_sql.as_str())).execute(pool()).await {
        Ok(_) => {
            if let Ok(size) = fs::metadata(&backup_file) {
                sqlx::query("INSERT INTO backup_record (backup_time, file_name, size) VALUES (?, ?, ?)")
                    .bind(now)
                    .bind(&backup_file)
                    .bind(size.len() as i64)
                    .execute(pool())
                    .await
                    .unwrap_or_default();
                (StatusCode::OK, format!("备份成功，文件大小：{} 字节", size.len()))
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, "备份文件创建失败".to_string())
            }
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("备份失败：{}", e)),
    }
}

async fn api_backup_download(Path(id): Path<i64>) -> impl IntoResponse {
    let row = sqlx::query("SELECT file_name FROM backup_record WHERE id = ?")
        .bind(id)
        .fetch_optional(pool())
        .await
        .unwrap_or_default();

    if let Some(row) = row {
        let file_name: String = row.get("file_name");
        if let Ok(content) = std::fs::read(&file_name) {
            let filename = file_name.split('/').last().unwrap_or(&file_name);
            let headers = [
                ("Content-Type", "application/octet-stream"),
                ("Content-Disposition", &format!("attachment; filename=\"{}\"", filename)),
            ];
            return (StatusCode::OK, headers, content).into_response();
        }
    }
    (StatusCode::NOT_FOUND, "文件不存在".to_string()).into_response()
}

async fn api_backup_delete(Path(id): Path<i64>) -> impl IntoResponse {
    let row = sqlx::query("SELECT file_name FROM backup_record WHERE id = ?")
        .bind(id)
        .fetch_optional(pool())
        .await
        .unwrap_or_default();

    if let Some(row) = row {
        let file_name: String = row.get("file_name");
        std::fs::remove_file(&file_name).unwrap_or_default();
        sqlx::query("DELETE FROM backup_record WHERE id = ?")
            .bind(id)
            .execute(pool())
            .await
            .unwrap_or_default();
        (StatusCode::OK, "删除成功".to_string())
    } else {
        (StatusCode::NOT_FOUND, "备份不存在".to_string())
    }
}

async fn api_restore(Path(id): Path<i64>) -> impl IntoResponse {
    let row = sqlx::query("SELECT file_name FROM backup_record WHERE id = ?")
        .bind(id)
        .fetch_optional(pool())
        .await
        .unwrap_or_default();

    if let Some(row) = row {
        let file_name: String = row.get("file_name");
        match std::fs::copy(&file_name, "food_accept_v3.db") {
            Ok(_) => {
                tokio::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    if let Ok(exe_path) = std::env::current_exe() {
                        let _ = std::process::Command::new(&exe_path)
                            .spawn();
                    }
                    std::process::exit(0);
                });
                (StatusCode::OK, "恢复成功，系统将在2秒后自动重启，请稍后刷新页面".to_string())
            }
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("恢复失败：{}", e)),
        }
    } else {
        (StatusCode::NOT_FOUND, "备份不存在".to_string())
    }
}

async fn api_restore_file(mut multipart: Multipart) -> impl IntoResponse {
    use std::fs;
    
    let mut file_bytes: Option<bytes::Bytes> = None;
    
    while let Some(field) = multipart.next_field().await.unwrap_or(None) {
        if field.name() != Some("file") {
            continue;
        }
        
        let bytes = field.bytes().await.unwrap_or_default();
        if bytes.is_empty() {
            return (StatusCode::BAD_REQUEST, "文件内容为空".to_string());
        }
        
        file_bytes = Some(bytes);
        break;
    }
    
    let bytes = match file_bytes {
        Some(b) => b,
        None => return (StatusCode::BAD_REQUEST, "未找到文件".to_string()),
    };
    
    let backup_dir = "temp_backups";
    if !std::path::Path::new(backup_dir).exists() {
        fs::create_dir_all(backup_dir).unwrap_or_default();
    }
    
    let temp_file = format!("{}/temp_restore.db", backup_dir);
    
    match fs::write(&temp_file, bytes.as_ref()) {
        Ok(_) => {
            match fs::copy(&temp_file, "food_accept_v3.db") {
                Ok(_) => {
                    fs::remove_file(&temp_file).unwrap_or_default();
                    tokio::spawn(async move {
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                        if let Ok(exe_path) = std::env::current_exe() {
                            let _ = std::process::Command::new(&exe_path)
                                .spawn();
                        }
                        std::process::exit(0);
                    });
                    (StatusCode::OK, "恢复成功，系统将在2秒后自动重启，请稍后刷新页面".to_string())
                }
                Err(e) => {
                    fs::remove_file(&temp_file).unwrap_or_default();
                    (StatusCode::INTERNAL_SERVER_ERROR, format!("恢复失败：{}", e))
                }
            }
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("保存临时文件失败：{}", e)),
    }
}

async fn api_inspect_corrupted_items(headers: axum::http::HeaderMap) -> impl IntoResponse {
    match check_api_permission(&headers, "/api/inspect_corrupted_items").await {
        Err(e) => return e,
        Ok(_) => {}
    }

    let rows = sqlx::query(
        "SELECT id, order_id, product_id, product_name, unit, unit_price, quantity, amount FROM sales_order_item WHERE (unit_price = 0 OR quantity = 0 OR amount = 0) AND product_name != '' LIMIT 100"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();

    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| serde_json::json!({
            "id": r.get::<i64, _>("id"),
            "order_id": r.get::<i64, _>("order_id"),
            "product_id": r.get::<i64, _>("product_id"),
            "product_name": r.get::<Option<String>, _>("product_name"),
            "unit": r.get::<Option<String>, _>("unit"),
            "unit_price": r.get::<Option<f64>, _>("unit_price"),
            "quantity": r.get::<Option<f64>, _>("quantity"),
            "amount": r.get::<f64, _>("amount"),
        }))
        .collect();

    match serde_json::to_string(&items) {
        Ok(json_str) => (StatusCode::OK, json_str),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("序列化失败：{}", e)),
    }
}

async fn api_clean_corrupted_items(headers: axum::http::HeaderMap) -> impl IntoResponse {
    match check_api_permission(&headers, "/api/clean_corrupted_items").await {
        Err(e) => return e,
        Ok(_) => {}
    }

    let corrupted_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM sales_order_item WHERE id >= 5527 AND id <= 5619"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();

    let count = corrupted_ids.len();

    for id in &corrupted_ids {
        sqlx::query("DELETE FROM sales_order_item WHERE id = ?")
            .bind(id)
            .execute(pool())
            .await
            .ok();
    }

    let no_item_sales: Vec<i64> = sqlx::query_scalar(
        "SELECT so.id FROM sales_order so LEFT JOIN sales_order_item soi ON so.id = soi.order_id WHERE soi.id IS NULL"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();

    for id in &no_item_sales {
        sqlx::query("DELETE FROM sales_order WHERE id = ?")
            .bind(id)
            .execute(pool())
            .await
            .ok();
    }

    let _ = sqlx::query("VACUUM").execute(pool()).await;

    (StatusCode::OK, format!("清理完成，共删除 {} 条损坏的订单明细记录", count))
}

async fn api_clean_invalid_orders(headers: axum::http::HeaderMap) -> impl IntoResponse {
    match check_api_permission(&headers, "/api/clean_invalid_orders").await {
        Err(e) => return e,
        Ok(_) => {}
    }

    use std::fs;
    use std::path::Path;
    let now = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let backup_dir = "backups";
    if !Path::new(backup_dir).exists() {
        fs::create_dir_all(backup_dir).unwrap_or_default();
    }
    let backup_file = format!("{}/backup_before_clean_{}.db", backup_dir, now);
    let vacuum_sql = format!("VACUUM INTO '{}'", backup_file);
    match sqlx::query(AssertSqlSafe(vacuum_sql.as_str())).execute(pool()).await {
        Ok(_) => {}
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("清理前备份失败：{}", e)),
    }

    let result = sqlx::query("BEGIN TRANSACTION").execute(pool()).await;
    if let Err(e) = result {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("开始事务失败：{}", e));
    }

    let mut deleted_count = 0;

    let no_item_sales: Vec<i64> = sqlx::query_scalar(
        "SELECT so.id FROM sales_order so LEFT JOIN sales_order_item soi ON so.id = soi.order_id WHERE soi.id IS NULL"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();

    for id in &no_item_sales {
        sqlx::query("DELETE FROM sales_order WHERE id = ?")
            .bind(id)
            .execute(pool())
            .await
            .ok();
        deleted_count += 1;
    }

    let no_item_purchase: Vec<i64> = sqlx::query_scalar(
        "SELECT po.id FROM purchase_order po LEFT JOIN purchase_order_item poi ON po.id = poi.order_id WHERE poi.id IS NULL"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();

    for id in &no_item_purchase {
        sqlx::query("DELETE FROM purchase_order WHERE id = ?")
            .bind(id)
            .execute(pool())
            .await
            .ok();
        deleted_count += 1;
    }

    let invalid_purchaser_sales: Vec<i64> = sqlx::query_scalar(
        "SELECT so.id FROM sales_order so LEFT JOIN purchaser p ON so.purchaser_id = p.id WHERE p.id IS NULL"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();

    for id in &invalid_purchaser_sales {
        sqlx::query("DELETE FROM sales_order_item WHERE order_id = ?")
            .bind(id)
            .execute(pool())
            .await
            .ok();
        sqlx::query("DELETE FROM sales_order WHERE id = ?")
            .bind(id)
            .execute(pool())
            .await
            .ok();
        deleted_count += 1;
    }

    let invalid_supplier_purchase: Vec<i64> = sqlx::query_scalar(
        "SELECT po.id FROM purchase_order po LEFT JOIN supplier s ON po.supplier_id = s.id WHERE s.id IS NULL"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();

    for id in &invalid_supplier_purchase {
        sqlx::query("DELETE FROM purchase_order_item WHERE order_id = ?")
            .bind(id)
            .execute(pool())
            .await
            .ok();
        sqlx::query("DELETE FROM purchase_order WHERE id = ?")
            .bind(id)
            .execute(pool())
            .await
            .ok();
        deleted_count += 1;
    }

    let invalid_product_sales_items: Vec<i64> = sqlx::query_scalar(
        "SELECT soi.id FROM sales_order_item soi LEFT JOIN product p ON soi.product_id = p.id WHERE p.id IS NULL"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();

    for id in &invalid_product_sales_items {
        sqlx::query("DELETE FROM sales_order_item WHERE id = ?")
            .bind(id)
            .execute(pool())
            .await
            .ok();
    }

    let invalid_product_purchase_items: Vec<i64> = sqlx::query_scalar(
        "SELECT poi.id FROM purchase_order_item poi LEFT JOIN product p ON poi.product_id = p.id WHERE p.id IS NULL"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();

    for id in &invalid_product_purchase_items {
        sqlx::query("DELETE FROM purchase_order_item WHERE id = ?")
            .bind(id)
            .execute(pool())
            .await
            .ok();
    }

    let no_item_after_clean_sales: Vec<i64> = sqlx::query_scalar(
        "SELECT so.id FROM sales_order so LEFT JOIN sales_order_item soi ON so.id = soi.order_id WHERE soi.id IS NULL"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();

    for id in &no_item_after_clean_sales {
        sqlx::query("DELETE FROM sales_order WHERE id = ?")
            .bind(id)
            .execute(pool())
            .await
            .ok();
        deleted_count += 1;
    }

    let no_item_after_clean_purchase: Vec<i64> = sqlx::query_scalar(
        "SELECT po.id FROM purchase_order po LEFT JOIN purchase_order_item poi ON po.id = poi.order_id WHERE poi.id IS NULL"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();

    for id in &no_item_after_clean_purchase {
        sqlx::query("DELETE FROM purchase_order WHERE id = ?")
            .bind(id)
            .execute(pool())
            .await
            .ok();
        deleted_count += 1;
    }

    match sqlx::query("COMMIT TRANSACTION").execute(pool()).await {
        Ok(_) => {}
        Err(e) => {
            let _ = sqlx::query("ROLLBACK TRANSACTION").execute(pool()).await;
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("提交事务失败：{}", e));
        }
    }

    let _ = sqlx::query("VACUUM").execute(pool()).await;

    (StatusCode::OK, format!("清理完成，共删除 {} 条无效订单。清理前已备份到 {}", deleted_count, backup_file))
}

fn parse_keyword_pattern(params: &std::collections::HashMap<String, String>) -> String {
    match params.get("keyword").filter(|s| !s.is_empty()) {
        Some(k) => format!("%{}%", k),
        None => "%".to_string(),
    }
}

fn parse_csv(content: &str) -> Vec<Vec<String>> {
    let mut result = Vec::new();
    let mut current_row = Vec::new();
    let mut current_field = String::new();
    let mut in_quotes = false;
    let mut chars = content.chars().peekable();
    
    while let Some(c) = chars.next() {
        match c {
            '"' if in_quotes => {
                if let Some(&next) = chars.peek() {
                    if next == '"' {
                        current_field.push('"');
                        chars.next();
                    } else {
                        in_quotes = false;
                    }
                } else {
                    in_quotes = false;
                }
            }
            '"' => {
                in_quotes = true;
            }
            ',' if !in_quotes => {
                current_row.push(current_field);
                current_field = String::new();
            }
            '\n' if !in_quotes => {
                current_row.push(current_field);
                if !current_row.iter().all(|s| s.is_empty()) {
                    result.push(current_row);
                }
                current_row = Vec::new();
                current_field = String::new();
            }
            '\r' => {}
            _ => {
                current_field.push(c);
            }
        }
    }
    
    if !current_field.is_empty() || !current_row.is_empty() {
        current_row.push(current_field);
        result.push(current_row);
    }
    
    result
}

async fn api_supplier_export() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT s.id, s.name, s.contact, s.phone, s.address, s.business_scope, s.remark, c.name as category_name 
         FROM supplier s LEFT JOIN category c ON s.category_id = c.id ORDER BY s.id"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let result: Result<Vec<u8>, XlsxError> = (|| {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();
        
        let header_format = Format::new()
            .set_bold()
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter);
        
        let headers = ["ID", "名称", "联系人", "电话", "地址", "经营范围", "备注", "分类"];
        for (i, &header) in headers.iter().enumerate() {
            worksheet.write_with_format(0, i as u16, header, &header_format)?;
        }
        
        let mut row_idx = 1;
        for row in rows {
            worksheet.write(row_idx, 0, row.get::<i64, _>("id"))?;
            worksheet.write(row_idx, 1, row.get::<String, _>("name"))?;
            worksheet.write(row_idx, 2, row.get::<Option<String>, _>("contact").unwrap_or_default())?;
            worksheet.write(row_idx, 3, row.get::<Option<String>, _>("phone").unwrap_or_default())?;
            worksheet.write(row_idx, 4, row.get::<Option<String>, _>("address").unwrap_or_default())?;
            worksheet.write(row_idx, 5, row.get::<Option<String>, _>("business_scope").unwrap_or_default())?;
            worksheet.write(row_idx, 6, row.get::<Option<String>, _>("remark").unwrap_or_default())?;
            worksheet.write(row_idx, 7, row.get::<Option<String>, _>("category_name").unwrap_or_default())?;
            row_idx += 1;
        }
        
        worksheet.set_column_width(0, 8)?;
        worksheet.set_column_width(1, 18)?;
        worksheet.set_column_width(2, 12)?;
        worksheet.set_column_width(3, 15)?;
        worksheet.set_column_width(4, 25)?;
        worksheet.set_column_width(5, 20)?;
        worksheet.set_column_width(6, 20)?;
        worksheet.set_column_width(7, 12)?;
        
        workbook.save_to_buffer()
    })();
    
    match result {
        Ok(data) => (
            StatusCode::OK,
            [
                ("Content-Type", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
                ("Content-Disposition", "attachment; filename=\"suppliers.xlsx\""),
            ],
            data,
        ).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("导出失败: {}", e)).into_response(),
    }
}

async fn api_supplier_import(content: Bytes) -> impl IntoResponse {
    let rows: Vec<Vec<String>>;
    
    if content.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
        let content_vec = content.to_vec();
        match open_workbook_auto_from_rs(std::io::Cursor::new(content_vec)) {
            Ok(mut workbook) => {
                let sheets = workbook.sheet_names().to_vec();
                if sheets.is_empty() {
                    return (StatusCode::BAD_REQUEST, "Excel文件中没有工作表".to_string()).into_response();
                }
                
                let range = match workbook.worksheet_range(&sheets[0]) {
                    Ok(r) => r,
                    Err(e) => return (StatusCode::BAD_REQUEST, format!("无法读取Excel文件内容: {}", e)).into_response(),
                };
                
                rows = range.rows()
                    .map(|row| {
                        row.iter()
                            .map(|cell| match cell {
                                Data::Empty => "".to_string(),
                                Data::Int(v) => v.to_string(),
                                Data::Float(v) => v.to_string(),
                                Data::String(v) => v.to_string(),
                                Data::Bool(v) => v.to_string(),
                                _ => "".to_string(),
                            })
                            .collect()
                    })
                    .collect();
            }
            Err(e) => {
                return (StatusCode::BAD_REQUEST, format!("读取Excel文件失败: {}", e)).into_response();
            }
        }
    } else {
        let content_str = String::from_utf8_lossy(&content).to_string();
        rows = parse_csv(&content_str);
    }
    
    if rows.len() < 2 {
        return (StatusCode::BAD_REQUEST, "文件至少需要包含标题行和一行数据".to_string()).into_response();
    }
    
    let mut success = 0;
    let mut failed = 0;
    
    for (_i, row) in rows.iter().enumerate().skip(1) {
        if row.len() < 2 {
            failed += 1;
            continue;
        }
        
        let name = row[1].trim();
        if name.is_empty() {
            failed += 1;
            continue;
        }
        
        let category_name = if row.len() > 7 { row[7].trim() } else { "" };
        let category_id = if !category_name.is_empty() {
            let cid: Option<i64> = sqlx::query("SELECT id FROM category WHERE name = ? AND entity_type = 'supplier'")
                .bind(category_name)
                .fetch_optional(pool())
                .await
                .ok()
                .flatten()
                .map(|r| r.get::<i64, _>("id"));
            cid
        } else {
            None
        };
        
        let result = sqlx::query(
            "INSERT OR IGNORE INTO supplier(name, contact, phone, address, business_scope, remark, category_id) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(name)
        .bind(if row.len() > 2 { row[2].trim() } else { "" })
        .bind(if row.len() > 3 { row[3].trim() } else { "" })
        .bind(if row.len() > 4 { row[4].trim() } else { "" })
        .bind(if row.len() > 5 { row[5].trim() } else { "" })
        .bind(if row.len() > 6 { row[6].trim() } else { "" })
        .bind(category_id)
        .execute(pool())
        .await;
        
        match result {
            Ok(res) => {
                if res.rows_affected() > 0 {
                    success += 1;
                } else {
                    failed += 1;
                }
            }
            Err(_) => {
                failed += 1;
            }
        }
    }
    
    (StatusCode::OK, format!("导入完成：成功 {} 条，失败 {} 条", success, failed)).into_response()
}

async fn page_login() -> Html<String> {
    Html(String::from(r#"
<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>用户登录</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); min-height: 100vh; display: flex; align-items: center; justify-content: center; }
        .login-container { background: white; border-radius: 16px; box-shadow: 0 20px 60px rgba(0,0,0,0.3); padding: 48px; width: 100%; max-width: 420px; }
        .login-header { text-align: center; margin-bottom: 32px; }
        .login-header h1 { font-size: 28px; color: #333; margin-bottom: 8px; }
        .login-header p { color: #666; }
        .login-logo { width: 80px; height: 80px; background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); border-radius: 20px; margin: 0 auto 16px; display: flex; align-items: center; justify-content: center; font-size: 40px; }
        .form-group { margin-bottom: 20px; }
        .form-group label { display: block; margin-bottom: 8px; color: #333; font-weight: 500; }
        .form-group input { width: 100%; padding: 12px 16px; border: 2px solid #e0e0e0; border-radius: 10px; font-size: 16px; transition: all 0.3s; }
        .form-group input:focus { outline: none; border-color: #667eea; box-shadow: 0 0 0 3px rgba(102,126,234,0.1); }
        .btn-login { width: 100%; padding: 14px; background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); color: white; border: none; border-radius: 10px; font-size: 18px; font-weight: 600; cursor: pointer; transition: all 0.3s; }
        .btn-login:hover { transform: translateY(-2px); box-shadow: 0 8px 20px rgba(102,126,234,0.4); }
        .btn-login:active { transform: translateY(0); }
        .error-message { background: #fee2e2; color: #dc2626; padding: 12px; border-radius: 8px; margin-bottom: 16px; display: none; }
        .loading { display: inline-block; width: 20px; height: 20px; border: 2px solid white; border-radius: 50%; border-top-color: transparent; animation: spin 0.8s linear infinite; }
        @keyframes spin { to { transform: rotate(360deg); } }
    </style>
</head>
<body>
    <div class="login-container">
        <div class="login-header">
            <div class="login-logo">🍽️</div>
            <h1>食材验收系统</h1>
            <p>欢迎登录管理后台</p>
        </div>
        <div class="error-message" id="errorMsg"></div>
        <form id="loginForm" onsubmit="return false;">
            <div class="form-group">
                <label>用户名</label>
                <input type="text" id="username" placeholder="请输入用户名" autocomplete="username">
            </div>
            <div class="form-group">
                <label>密码</label>
                <input type="password" id="password" placeholder="请输入密码" autocomplete="current-password">
            </div>
            <button type="submit" class="btn-login" id="loginBtn" onclick="handleLogin()">
                <span id="btnText">登 录</span>
                <span id="btnLoading" class="loading" style="display:none;"></span>
            </button>
        </form>
    </div>
    <script>
        async function handleLogin() {
            const username = document.getElementById('username').value.trim();
            const password = document.getElementById('password').value.trim();
            const errorMsg = document.getElementById('errorMsg');
            const btnText = document.getElementById('btnText');
            const btnLoading = document.getElementById('btnLoading');
            const loginBtn = document.getElementById('loginBtn');

            if (!username || !password) {
                showError('请输入用户名和密码');
                return;
            }

            btnText.style.display = 'none';
            btnLoading.style.display = 'inline-block';
            loginBtn.disabled = true;
            errorMsg.style.display = 'none';

            try {
                const response = await fetch('/api/login', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ username, password })
                });
                
                const result = await response.json();
                
                if (result.success) {
                    window.location.href = '/';
                } else {
                    showError(result.message || '登录失败');
                }
            } catch (e) {
                showError('网络错误，请重试');
            } finally {
                btnText.style.display = 'inline';
                btnLoading.style.display = 'none';
                loginBtn.disabled = false;
            }
        }

        function showError(msg) {
            const errorMsg = document.getElementById('errorMsg');
            errorMsg.textContent = msg;
            errorMsg.style.display = 'block';
        }

        document.getElementById('username').addEventListener('keydown', function(e) {
            if (e.key === 'Enter') document.getElementById('password').focus();
        });
        document.getElementById('password').addEventListener('keydown', function(e) {
            if (e.key === 'Enter') handleLogin();
        });
    </script>
</body>
</html>
    "#))
}

async fn api_login(Json(data): Json<LoginReq>) -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT id, username, password, nickname, role, status FROM user_account WHERE username = ?"
    )
    .bind(&data.username)
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    if rows.is_empty() {
        return (StatusCode::OK, serde_json::to_string(&serde_json::json!({
            "success": false,
            "message": "用户名不存在"
        })).unwrap()).into_response();
    }
    
    let row = &rows[0];
    let password_hash: String = row.get("password");
    let status: i32 = row.get("status");
    
    if status != 1 {
        return (StatusCode::OK, serde_json::to_string(&serde_json::json!({
            "success": false,
            "message": "账号已被禁用"
        })).unwrap()).into_response();
    }
    
    if !bcrypt::verify(&data.password, &password_hash).unwrap_or(false) {
        return (StatusCode::OK, serde_json::to_string(&serde_json::json!({
            "success": false,
            "message": "密码错误"
        })).unwrap()).into_response();
    }
    
    let user_id: i64 = row.get("id");
    let nickname: String = row.get("nickname");
    let role: String = row.get("role");
    
    let session_token = format!("{}:{:x}", user_id, rand::random::<u128>());
    
    sqlx::query("UPDATE user_account SET last_login_time = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(user_id)
        .execute(pool())
        .await
        .ok();
    
    let body = serde_json::to_string(&serde_json::json!({
        "success": true,
        "message": "登录成功",
        "user": {
            "id": user_id,
            "username": data.username,
            "nickname": nickname,
            "role": role
        }
    })).unwrap();
    
    axum::response::Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .header("Set-Cookie", format!("session={}; HttpOnly; Path=/", session_token))
        .body(axum::body::Body::from(body))
        .unwrap()
}

async fn api_logout() -> impl IntoResponse {
    let body = serde_json::to_string(&serde_json::json!({
        "success": true,
        "message": "已退出登录"
    })).unwrap();
    
    axum::response::Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .header("Set-Cookie", "session=; HttpOnly; Path=/; Max-Age=0")
        .body(axum::body::Body::from(body))
        .unwrap()
}

async fn api_login_check(headers: axum::http::HeaderMap) -> impl IntoResponse {
    let session_token = headers.get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find(|s| s.trim().starts_with("session="))
                .map(|s| s.trim().strip_prefix("session=").unwrap_or(""))
        })
        .unwrap_or("");
    
    if session_token.is_empty() {
        return (StatusCode::OK, serde_json::to_string(&serde_json::json!({
            "logged_in": false
        })).unwrap());
    }
    
    let parts: Vec<&str> = session_token.split(':').collect();
    if parts.len() < 2 {
        return (StatusCode::OK, serde_json::to_string(&serde_json::json!({
            "logged_in": false
        })).unwrap());
    }
    
    let user_id = match parts[0].parse::<i64>() {
        Ok(id) => id,
        Err(_) => {
            return (StatusCode::OK, serde_json::to_string(&serde_json::json!({
                "logged_in": false
            })).unwrap());
        }
    };
    
    let rows = sqlx::query(
        "SELECT id, username, nickname, role FROM user_account WHERE id = ? AND status = 1"
    )
    .bind(user_id)
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    if rows.is_empty() {
        return (StatusCode::OK, serde_json::to_string(&serde_json::json!({
            "logged_in": false
        })).unwrap());
    }
    
    let row = &rows[0];
    let username: String = row.get("username");
    let nickname: String = row.get("nickname");
    let role: String = row.get("role");
    
    (StatusCode::OK, serde_json::to_string(&serde_json::json!({
        "logged_in": true,
        "user": {
            "id": user_id,
            "username": username,
            "nickname": nickname,
            "role": role
        }
    })).unwrap())
}

async fn api_supplier_list(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let category_id = params.get("category_id").and_then(|v| v.parse::<i64>().ok());
    let keyword_pattern = parse_keyword_pattern(&params);
    
    let rows = if let Some(cid) = category_id {
        sqlx::query(
            "SELECT s.id, s.name, s.contact, s.phone, s.address, s.business_scope, s.remark, s.category_id, c.name as category_name 
             FROM supplier s LEFT JOIN category c ON s.category_id = c.id
             WHERE s.category_id IN (
                 WITH RECURSIVE cat_tree(id) AS (
                     SELECT id FROM category WHERE id = ?
                     UNION ALL
                     SELECT c.id FROM category c 
                     JOIN cat_tree ct ON c.parent_id = ct.id
                 )
                 SELECT id FROM cat_tree
             )
             AND s.name LIKE ?
             ORDER BY s.id DESC"
        )
        .bind(cid)
        .bind(&keyword_pattern)
        .fetch_all(pool())
        .await
        .unwrap_or_default()
    } else {
        sqlx::query(
            "SELECT s.id, s.name, s.contact, s.phone, s.address, s.business_scope, s.remark, s.category_id, c.name as category_name 
             FROM supplier s LEFT JOIN category c ON s.category_id = c.id
             WHERE s.name LIKE ?
             ORDER BY s.id DESC"
        )
        .bind(&keyword_pattern)
        .fetch_all(pool())
        .await
        .unwrap_or_default()
    };
    
    let suppliers: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| serde_json::json!({
            "id": row.get::<i64, _>("id"),
            "name": row.get::<String, _>("name"),
            "contact": row.get::<Option<String>, _>("contact"),
            "phone": row.get::<Option<String>, _>("phone"),
            "address": row.get::<Option<String>, _>("address"),
            "business_scope": row.get::<Option<String>, _>("business_scope"),
            "remark": row.get::<Option<String>, _>("remark"),
            "category_id": row.get::<Option<i64>, _>("category_id"),
            "category_name": row.get::<Option<String>, _>("category_name"),
        }))
        .collect();
    
    (StatusCode::OK, serde_json::to_string(&suppliers).unwrap())
}

async fn api_supplier_create(headers: axum::http::HeaderMap, Json(req): Json<SupplierReq>) -> impl IntoResponse {
    match check_api_permission(&headers, "/api/supplier/create").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    
    let result = sqlx::query(
        "INSERT INTO supplier(name, contact, phone, address, business_scope, remark, category_id) VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&req.name)
    .bind(&req.contact)
    .bind(&req.phone)
    .bind(&req.address)
    .bind(&req.business_scope)
    .bind(&req.remark)
    .bind(&req.category_id)
    .execute(pool())
    .await;
    
    match result {
        Ok(_) => (StatusCode::OK, "创建成功".to_string()),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "创建失败".to_string()),
    }
}

async fn api_supplier_update(headers: axum::http::HeaderMap, Json(req): Json<SupplierReq>) -> impl IntoResponse {
    match check_api_permission(&headers, "/api/supplier/update").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let result = sqlx::query(
        "UPDATE supplier SET name=?, contact=?, phone=?, address=?, business_scope=?, remark=?, category_id=? WHERE id=?"
    )
    .bind(&req.name)
    .bind(&req.contact)
    .bind(&req.phone)
    .bind(&req.address)
    .bind(&req.business_scope)
    .bind(&req.remark)
    .bind(&req.category_id)
    .bind(req.id.unwrap_or(0))
    .execute(pool())
    .await;
    
    match result {
        Ok(_) => (StatusCode::OK, "更新成功".to_string()),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "更新失败".to_string()),
    }
}

async fn api_supplier_delete(headers: axum::http::HeaderMap, Json(req): Json<DeleteReq>) -> impl IntoResponse {
    match check_api_permission(&headers, "/api/supplier/delete").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let result = sqlx::query("DELETE FROM supplier WHERE id=?")
        .bind(req.id)
        .execute(pool())
        .await;
    
    match result {
        Ok(_) => (StatusCode::OK, "删除成功".to_string()),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "删除失败".to_string()),
    }
}

async fn api_purchaser_export() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT p.id, p.name, p.contact, p.phone, p.address, p.business_scope, p.remark, c.name as category_name 
         FROM purchaser p LEFT JOIN category c ON p.category_id = c.id ORDER BY p.id"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let result: Result<Vec<u8>, XlsxError> = (|| {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();
        
        let header_format = Format::new()
            .set_bold()
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter);
        
        let headers = ["ID", "名称", "联系人", "电话", "地址", "经营范围", "备注", "分类"];
        for (i, &header) in headers.iter().enumerate() {
            worksheet.write_with_format(0, i as u16, header, &header_format)?;
        }
        
        let mut row_idx = 1;
        for row in rows {
            worksheet.write(row_idx, 0, row.get::<i64, _>("id"))?;
            worksheet.write(row_idx, 1, row.get::<String, _>("name"))?;
            worksheet.write(row_idx, 2, row.get::<Option<String>, _>("contact").unwrap_or_default())?;
            worksheet.write(row_idx, 3, row.get::<Option<String>, _>("phone").unwrap_or_default())?;
            worksheet.write(row_idx, 4, row.get::<Option<String>, _>("address").unwrap_or_default())?;
            worksheet.write(row_idx, 5, row.get::<Option<String>, _>("business_scope").unwrap_or_default())?;
            worksheet.write(row_idx, 6, row.get::<Option<String>, _>("remark").unwrap_or_default())?;
            worksheet.write(row_idx, 7, row.get::<Option<String>, _>("category_name").unwrap_or_default())?;
            row_idx += 1;
        }
        
        worksheet.set_column_width(0, 8)?;
        worksheet.set_column_width(1, 18)?;
        worksheet.set_column_width(2, 12)?;
        worksheet.set_column_width(3, 15)?;
        worksheet.set_column_width(4, 25)?;
        worksheet.set_column_width(5, 20)?;
        worksheet.set_column_width(6, 20)?;
        worksheet.set_column_width(7, 12)?;
        
        workbook.save_to_buffer()
    })();
    
    match result {
        Ok(data) => (
            StatusCode::OK,
            [
                ("Content-Type", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
                ("Content-Disposition", "attachment; filename=\"purchasers.xlsx\""),
            ],
            data,
        ).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("导出失败: {}", e)).into_response(),
    }
}

async fn api_purchaser_import(content: Bytes) -> impl IntoResponse {
    let rows: Vec<Vec<String>>;
    
    if content.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
        let content_vec = content.to_vec();
        match open_workbook_auto_from_rs(std::io::Cursor::new(content_vec)) {
            Ok(mut workbook) => {
                let sheets = workbook.sheet_names().to_vec();
                if sheets.is_empty() {
                    return (StatusCode::BAD_REQUEST, "Excel文件中没有工作表".to_string()).into_response();
                }
                
                let range = match workbook.worksheet_range(&sheets[0]) {
                    Ok(r) => r,
                    Err(e) => return (StatusCode::BAD_REQUEST, format!("无法读取Excel文件内容: {}", e)).into_response(),
                };
                
                rows = range.rows()
                    .map(|row| {
                        row.iter()
                            .map(|cell| match cell {
                                Data::Empty => "".to_string(),
                                Data::Int(v) => v.to_string(),
                                Data::Float(v) => v.to_string(),
                                Data::String(v) => v.to_string(),
                                Data::Bool(v) => v.to_string(),
                                _ => "".to_string(),
                            })
                            .collect()
                    })
                    .collect();
            }
            Err(e) => {
                return (StatusCode::BAD_REQUEST, format!("读取Excel文件失败: {}", e)).into_response();
            }
        }
    } else {
        let content_str = String::from_utf8_lossy(&content).to_string();
        rows = parse_csv(&content_str);
    }
    
    if rows.len() < 2 {
        return (StatusCode::BAD_REQUEST, "文件至少需要包含标题行和一行数据".to_string()).into_response();
    }
    
    let mut success = 0;
    let mut failed = 0;
    
    for (_i, row) in rows.iter().enumerate().skip(1) {
        if row.len() < 2 {
            failed += 1;
            continue;
        }
        
        let name = row[1].trim();
        if name.is_empty() {
            failed += 1;
            continue;
        }
        
        let category_name = if row.len() > 7 { row[7].trim() } else { "" };
        let category_id = if !category_name.is_empty() {
            let cid: Option<i64> = sqlx::query("SELECT id FROM category WHERE name = ? AND entity_type = 'purchaser'")
                .bind(category_name)
                .fetch_optional(pool())
                .await
                .ok()
                .flatten()
                .map(|r| r.get::<i64, _>("id"));
            cid
        } else {
            None
        };
        
        let result = sqlx::query(
            "INSERT OR IGNORE INTO purchaser(name, contact, phone, address, business_scope, remark, category_id) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(name)
        .bind(if row.len() > 2 { row[2].trim() } else { "" })
        .bind(if row.len() > 3 { row[3].trim() } else { "" })
        .bind(if row.len() > 4 { row[4].trim() } else { "" })
        .bind(if row.len() > 5 { row[5].trim() } else { "" })
        .bind(if row.len() > 6 { row[6].trim() } else { "" })
        .bind(category_id)
        .execute(pool())
        .await;
        
        match result {
            Ok(res) => {
                if res.rows_affected() > 0 {
                    success += 1;
                } else {
                    failed += 1;
                }
            }
            Err(_) => {
                failed += 1;
            }
        }
    }
    
    (StatusCode::OK, format!("导入完成：成功 {} 条，失败 {} 条", success, failed)).into_response()
}

async fn api_purchaser_list(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let category_id = params.get("category_id").and_then(|v| v.parse::<i64>().ok());
    let keyword_pattern = parse_keyword_pattern(&params);
    
    let rows = if let Some(cid) = category_id {
        sqlx::query(
            "SELECT p.id, p.name, p.contact, p.phone, p.address, p.business_scope, p.remark, p.category_id, c.name as category_name
             FROM purchaser p LEFT JOIN category c ON p.category_id = c.id
             WHERE p.category_id IN (
                 WITH RECURSIVE cat_tree(id) AS (
                     SELECT id FROM category WHERE id = ?
                     UNION ALL
                     SELECT c.id FROM category c
                     JOIN cat_tree ct ON c.parent_id = ct.id
                 )
                 SELECT id FROM cat_tree
             )
             AND p.name LIKE ?
             ORDER BY p.id DESC"
        )
        .bind(cid)
        .bind(&keyword_pattern)
        .fetch_all(pool())
        .await
        .unwrap_or_default()
    } else {
        sqlx::query(
            "SELECT p.id, p.name, p.contact, p.phone, p.address, p.business_scope, p.remark, p.category_id, c.name as category_name
             FROM purchaser p LEFT JOIN category c ON p.category_id = c.id
             WHERE p.name LIKE ?
             ORDER BY p.id DESC"
        )
        .bind(&keyword_pattern)
        .fetch_all(pool())
        .await
        .unwrap_or_default()
    };

    let purchasers: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| serde_json::json!({
            "id": row.get::<i64, _>("id"),
            "name": row.get::<String, _>("name"),
            "contact": row.get::<Option<String>, _>("contact"),
            "phone": row.get::<Option<String>, _>("phone"),
            "address": row.get::<Option<String>, _>("address"),
            "business_scope": row.get::<Option<String>, _>("business_scope"),
            "remark": row.get::<Option<String>, _>("remark"),
            "category_id": row.get::<Option<i64>, _>("category_id"),
            "category_name": row.get::<Option<String>, _>("category_name"),
        }))
        .collect();
    
    (StatusCode::OK, serde_json::to_string(&purchasers).unwrap())
}

async fn api_purchaser_create(Json(req): Json<PurchaserReq>) -> impl IntoResponse {
    let result = sqlx::query(
        "INSERT INTO purchaser(name, contact, phone, address, business_scope, remark, category_id) VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&req.name)
    .bind(&req.contact)
    .bind(&req.phone)
    .bind(&req.address)
    .bind(&req.business_scope)
    .bind(&req.remark)
    .bind(&req.category_id)
    .execute(pool())
    .await;
    
    match result {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn api_purchaser_update(headers: axum::http::HeaderMap, Json(req): Json<PurchaserReq>) -> impl IntoResponse {
    match check_api_permission(&headers, "/api/purchaser/update").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let result = sqlx::query(
        "UPDATE purchaser SET name=?, contact=?, phone=?, address=?, business_scope=?, remark=?, category_id=? WHERE id=?"
    )
    .bind(&req.name)
    .bind(&req.contact)
    .bind(&req.phone)
    .bind(&req.address)
    .bind(&req.business_scope)
    .bind(&req.remark)
    .bind(&req.category_id)
    .bind(req.id.unwrap_or(0))
    .execute(pool())
    .await;
    
    match result {
        Ok(_) => (StatusCode::OK, "更新成功".to_string()),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "更新失败".to_string()),
    }
}

async fn api_purchaser_delete(headers: axum::http::HeaderMap, Json(req): Json<DeleteReq>) -> impl IntoResponse {
    match check_api_permission(&headers, "/api/purchaser/delete").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let result = sqlx::query("DELETE FROM purchaser WHERE id=?")
        .bind(req.id)
        .execute(pool())
        .await;
    
    match result {
        Ok(_) => (StatusCode::OK, "删除成功".to_string()),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "删除失败".to_string()),
    }
}

async fn api_product_toggle_status(Path(id): Path<i64>) -> impl IntoResponse {
    let row = sqlx::query("SELECT status FROM product WHERE id = ?")
        .bind(id)
        .fetch_optional(pool())
        .await
        .unwrap_or(None);
    
    if row.is_none() {
        return (StatusCode::NOT_FOUND, "商品不存在".to_string());
    }
    
    let current_status: i64 = row.unwrap().get("status");
    let new_status = if current_status == 1 { 0 } else { 1 };
    
    let result = sqlx::query("UPDATE product SET status = ? WHERE id = ?")
        .bind(new_status)
        .bind(id)
        .execute(pool())
        .await;
    
    match result {
        Ok(_) => {
            let msg = if new_status == 1 { "商品已启用" } else { "商品已停用" };
            (StatusCode::OK, msg.to_string())
        },
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "操作失败".to_string()),
    }
}

async fn api_product_list(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let category_id = params.get("category_id").and_then(|v| v.parse::<i64>().ok());
    let keyword_pattern = parse_keyword_pattern(&params);
    
    let rows = if let Some(cid) = category_id {
        sqlx::query(
            "SELECT p.id, p.name, p.spec, p.alias1, p.alias2, p.unit, p.base_unit, p.base_price, p.purchase_price, p.image_url, p.category_id, p.status, c.name as category_name 
             FROM product p LEFT JOIN category c ON p.category_id = c.id
             WHERE p.category_id IN (
                 WITH RECURSIVE cat_tree(id) AS (
                     SELECT id FROM category WHERE id = ?
                     UNION ALL
                     SELECT c.id FROM category c 
                     JOIN cat_tree ct ON c.parent_id = ct.id
                 )
                 SELECT id FROM cat_tree
             )
             AND (p.name LIKE ? OR p.alias1 LIKE ? OR p.alias2 LIKE ?)
             ORDER BY p.id DESC"
        )
        .bind(cid)
        .bind(&keyword_pattern)
        .bind(&keyword_pattern)
        .bind(&keyword_pattern)
        .fetch_all(pool())
        .await
        .unwrap_or_default()
    } else {
        sqlx::query(
            "SELECT p.id, p.name, p.spec, p.alias1, p.alias2, p.unit, p.base_unit, p.base_price, p.purchase_price, p.image_url, p.category_id, p.status, c.name as category_name 
             FROM product p LEFT JOIN category c ON p.category_id = c.id
             WHERE p.name LIKE ? OR p.alias1 LIKE ? OR p.alias2 LIKE ?
             ORDER BY p.id DESC"
        )
        .bind(&keyword_pattern)
        .bind(&keyword_pattern)
        .bind(&keyword_pattern)
        .fetch_all(pool())
        .await
        .unwrap_or_default()
    };
    
    let mut products: Vec<serde_json::Value> = Vec::new();
    for row in rows {
        let product_id: i64 = row.get("id");
        let unit_rows = sqlx::query(
            "SELECT id, unit_name, ratio, unit_price, purchase_price, sort_order FROM product_unit 
             WHERE product_id = ? ORDER BY sort_order, id"
        )
        .bind(product_id)
        .fetch_all(pool())
        .await
        .unwrap_or_default();
        
        let units: Vec<serde_json::Value> = unit_rows
            .iter()
            .map(|ur| serde_json::json!({
                "id": ur.get::<i64, _>("id"),
                "unit_name": ur.get::<String, _>("unit_name"),
                "ratio": ur.get::<f64, _>("ratio"),
                "unit_price": ur.get::<f64, _>("unit_price"),
                "purchase_price": ur.get::<f64, _>("purchase_price"),
                "sort_order": ur.get::<i32, _>("sort_order"),
            }))
            .collect();
        
        let price_rows = sqlx::query(
            "SELECT price_type, price FROM product_price WHERE product_id = ?"
        )
        .bind(product_id)
        .fetch_all(pool())
        .await
        .unwrap_or_default();
        
        let mut prices: Vec<serde_json::Value> = Vec::new();
        let mut gov_price: Option<f64> = None;
        let mut supermarket_prices: Vec<f64> = Vec::new();
        
        for pr in price_rows {
            let price_type: String = pr.get("price_type");
            let price: f64 = pr.get("price");
            prices.push(serde_json::json!({
                "price_type": price_type.clone(),
                "price": price,
            }));
            
            if price_type == "gov_procurement" {
                gov_price = Some(price);
            } else if price_type.starts_with("supermarket_") {
                supermarket_prices.push(price);
            }
        }
        
        let selling_price = if let Some(gp) = gov_price {
            if gp > 0.0 { gp } else if !supermarket_prices.is_empty() {
                *supermarket_prices.iter().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap()
            } else {
                row.get::<f64, _>("base_price")
            }
        } else if !supermarket_prices.is_empty() {
            *supermarket_prices.iter().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap()
        } else {
            row.get::<f64, _>("base_price")
        };
        
        products.push(serde_json::json!({
            "id": product_id,
            "name": row.get::<String, _>("name"),
            "spec": row.get::<Option<String>, _>("spec"),
            "alias1": row.get::<Option<String>, _>("alias1"),
            "alias2": row.get::<Option<String>, _>("alias2"),
            "unit": row.get::<String, _>("unit"),
            "base_unit": row.get::<String, _>("base_unit"),
            "base_price": row.get::<f64, _>("base_price"),
            "purchase_price": row.get::<f64, _>("purchase_price"),
            "image_url": row.get::<Option<String>, _>("image_url"),
            "category_id": row.get::<Option<i64>, _>("category_id"),
            "status": row.get::<i64, _>("status"),
            "category_name": row.get::<Option<String>, _>("category_name"),
            "units": units,
            "prices": prices,
            "selling_price": selling_price,
        }));
    }
    
    (StatusCode::OK, serde_json::to_string(&products).unwrap())
}

async fn api_product_create(Json(req): Json<ProductReq>) -> impl IntoResponse {
    let base_unit = req.base_unit.clone().unwrap_or_else(|| req.unit.clone().unwrap_or_else(|| "个".to_string()));
    let unit = req.unit.clone().unwrap_or_else(|| "个".to_string());
    let base_price = req.base_price.unwrap_or(0.0);
    
    let purchase_price = req.purchase_price.unwrap_or(0.0);
    
    let result = sqlx::query(
        "INSERT INTO product(name, spec, alias1, alias2, unit, base_unit, base_price, purchase_price, category_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&req.name)
    .bind(&req.spec)
    .bind(&req.alias1)
    .bind(&req.alias2)
    .bind(&unit)
    .bind(&base_unit)
    .bind(base_price)
    .bind(purchase_price)
    .bind(&req.category_id)
    .execute(pool())
    .await;

    match result {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            eprintln!("创建商品失败: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        },
    }
}

async fn api_product_check_name(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let name = params.get("name").filter(|s| !s.is_empty());
    if name.is_none() {
        return (StatusCode::OK, serde_json::to_string(&Vec::<serde_json::Value>::new()).unwrap());
    }
    
    let rows = sqlx::query(
        "SELECT p.id, p.name, p.spec, p.unit, p.base_unit, p.base_price, p.category_id, c.name as category_name 
         FROM product p LEFT JOIN category c ON p.category_id = c.id
         WHERE p.name = ?"
    )
    .bind(name.unwrap())
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let products: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| serde_json::json!({
            "id": row.get::<i64, _>("id"),
            "name": row.get::<String, _>("name"),
            "spec": row.get::<Option<String>, _>("spec"),
            "unit": row.get::<String, _>("unit"),
            "base_unit": row.get::<String, _>("base_unit"),
            "base_price": row.get::<f64, _>("base_price"),
            "category_id": row.get::<Option<i64>, _>("category_id"),
            "category_name": row.get::<Option<String>, _>("category_name"),
        }))
        .collect();
    
    (StatusCode::OK, serde_json::to_string(&products).unwrap())
}

async fn api_product_by_id(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let product_id = params.get("id").and_then(|s| s.parse::<i64>().ok());
    if product_id.is_none() {
        return (StatusCode::OK, serde_json::to_string(&serde_json::json!({})).unwrap());
    }
    
    let row = sqlx::query(
        "SELECT p.id, p.name, p.alias1, p.alias2, p.spec, p.unit, p.base_unit, p.base_price, p.purchase_price,
                COALESCE(NULLIF((SELECT price FROM product_price WHERE product_id = p.id AND price_type = 'gov_procurement'), 0),
                         (SELECT MAX(price) FROM product_price WHERE product_id = p.id AND price_type LIKE 'supermarket_%'),
                         p.base_price) as selling_price,
                c.name as category_name
         FROM product p LEFT JOIN category c ON p.category_id = c.id
         WHERE p.id = ?"
    )
    .bind(product_id.unwrap())
    .fetch_one(pool())
    .await;
    
    match row {
        Ok(r) => {
            let product = serde_json::json!({
                "id": r.get::<i64, _>("id"),
                "name": r.get::<String, _>("name"),
                "alias1": r.get::<Option<String>, _>("alias1").unwrap_or_default(),
                "alias2": r.get::<Option<String>, _>("alias2").unwrap_or_default(),
                "spec": r.get::<Option<String>, _>("spec").unwrap_or_default(),
                "unit": r.get::<Option<String>, _>("unit").unwrap_or_default(),
                "base_unit": r.get::<Option<String>, _>("base_unit").unwrap_or_default(),
                "base_price": r.get::<f64, _>("base_price"),
                "purchase_price": r.get::<f64, _>("purchase_price"),
                "selling_price": r.get::<f64, _>("selling_price"),
                "category_name": r.get::<Option<String>, _>("category_name").unwrap_or_default(),
            });
            (StatusCode::OK, serde_json::to_string(&product).unwrap())
        },
        Err(_) => (StatusCode::OK, serde_json::to_string(&serde_json::json!({})).unwrap()),
    }
}

async fn api_product_search(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let keyword = params.get("keyword").filter(|s| !s.is_empty());
    if keyword.is_none() {
        return (StatusCode::OK, serde_json::to_string(&Vec::<serde_json::Value>::new()).unwrap());
    }
    
    let pattern = format!("%{}%", keyword.unwrap());
    
    let rows = sqlx::query(
        "SELECT p.id, p.name, p.alias1, p.alias2, p.spec, p.unit, p.base_unit, p.base_price, p.purchase_price,
                COALESCE(NULLIF((SELECT price FROM product_price WHERE product_id = p.id AND price_type = 'gov_procurement'), 0),
                         (SELECT MAX(price) FROM product_price WHERE product_id = p.id AND price_type LIKE 'supermarket_%'),
                         p.base_price) as selling_price,
                c.name as category_name
         FROM product p LEFT JOIN category c ON p.category_id = c.id
         WHERE p.status = 1 AND (p.name LIKE ? OR p.alias1 LIKE ? OR p.alias2 LIKE ?)
         ORDER BY p.name"
    )
    .bind(&pattern)
    .bind(&pattern)
    .bind(&pattern)
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let products: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| serde_json::json!({
            "id": row.get::<i64, _>("id"),
            "name": row.get::<String, _>("name"),
            "alias1": row.get::<Option<String>, _>("alias1"),
            "alias2": row.get::<Option<String>, _>("alias2"),
            "spec": row.get::<Option<String>, _>("spec"),
            "unit": row.get::<String, _>("unit"),
            "base_unit": row.get::<String, _>("base_unit"),
            "base_price": row.get::<f64, _>("base_price"),
            "purchase_price": row.get::<f64, _>("purchase_price"),
            "selling_price": row.get::<f64, _>("selling_price"),
            "category_name": row.get::<Option<String>, _>("category_name"),
        }))
        .collect();
    
    (StatusCode::OK, serde_json::to_string(&products).unwrap())
}

#[derive(Deserialize)]
struct ProductUpdateReq {
    id: i64,
    name: String,
    spec: Option<String>,
    alias1: Option<String>,
    alias2: Option<String>,
    unit: Option<String>,
    base_unit: Option<String>,
    base_price: Option<f64>,
    purchase_price: Option<f64>,
    image_url: Option<String>,
    category_id: Option<i64>,
}

async fn api_product_update(headers: axum::http::HeaderMap, Json(req): Json<ProductUpdateReq>) -> impl IntoResponse {
    match check_api_permission(&headers, "/api/product/update").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let result = sqlx::query(
        "UPDATE product SET name = ?, spec = ?, alias1 = ?, alias2 = ?, unit = ?, base_unit = ?, base_price = ?, purchase_price = ?, image_url = ?, category_id = ? WHERE id = ?"
    )
    .bind(&req.name)
    .bind(&req.spec)
    .bind(&req.alias1)
    .bind(&req.alias2)
    .bind(&req.unit)
    .bind(&req.base_unit)
    .bind(&req.base_price)
    .bind(&req.purchase_price)
    .bind(&req.image_url)
    .bind(&req.category_id)
    .bind(req.id)
    .execute(pool())
    .await;

    match result {
        Ok(_) => (StatusCode::OK, "更新成功".to_string()),
        Err(e) => {
            eprintln!("更新商品失败: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "更新失败".to_string())
        }
    }
}

async fn api_product_delete(headers: axum::http::HeaderMap, Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    match check_api_permission(&headers, "/api/product/delete").await {
        Err(e) => return e,
        Ok(_) => {}
    }

    let id = req["id"].as_i64().unwrap_or(0);

    let check_tables = vec![
        ("库存", "SELECT COUNT(*) FROM inventory WHERE product_id = ?"),
        ("采购订单明细", "SELECT COUNT(*) FROM purchase_order_item WHERE product_id = ?"),
        ("销售订单明细", "SELECT COUNT(*) FROM sales_order_item WHERE product_id = ?"),
        ("食品项", "SELECT COUNT(*) FROM food_item WHERE product_id = ?"),
    ];

    for (name, sql) in check_tables {
        let count: i64 = sqlx::query(sql)
            .bind(id)
            .fetch_one(pool())
            .await
            .map(|r| r.get(0))
            .unwrap_or(0);
        if count > 0 {
            return (StatusCode::BAD_REQUEST, format!("该商品存在{}记录（{}条），无法删除，请先处理关联数据", name, count));
        }
    }

    let result = sqlx::query("DELETE FROM product WHERE id = ?")
        .bind(id)
        .execute(pool())
        .await;
    match result {
        Ok(r) => {
            if r.rows_affected() > 0 {
                sqlx::query("DELETE FROM product_unit WHERE product_id = ?")
                    .bind(id)
                    .execute(pool())
                    .await
                    .ok();
                sqlx::query("DELETE FROM product_price WHERE product_id = ?")
                    .bind(id)
                    .execute(pool())
                    .await
                    .ok();
                (StatusCode::OK, "删除成功".to_string())
            } else {
                (StatusCode::NOT_FOUND, "商品不存在".to_string())
            }
        }
        Err(e) => {
            eprintln!("删除商品失败: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "删除失败".to_string())
        }
    }
}

async fn api_product_upload_image(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let product_id = params.get("product_id").and_then(|s| s.parse::<i64>().ok());
    if product_id.is_none() {
        return (StatusCode::BAD_REQUEST, "缺少 product_id 参数".to_string());
    }
    let product_id = product_id.unwrap();

    tokio::fs::create_dir_all("uploads").await.ok();

    let mut file_path = String::new();
    let mut has_file = false;

    while let Some(field) = multipart.next_field().await.unwrap() {
        if field.name() != Some("file") {
            continue;
        }

        has_file = true;

        let filename = field.file_name().unwrap_or_else(|| "unknown.jpg");
        let ext = std::path::Path::new(filename)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("jpg")
            .to_lowercase();

        if !["jpg", "jpeg", "png", "gif", "webp"].contains(&ext.as_str()) {
            return (StatusCode::BAD_REQUEST, "不支持的图片格式".to_string());
        }

        let timestamp = chrono::Utc::now().timestamp_millis();
        let random: u32 = rand::random();
        let new_filename = format!("{}_{}.{}", timestamp, random, ext);
        let path = format!("uploads/{}", new_filename);

        let bytes = field.bytes().await.unwrap_or_default();
        if bytes.len() > 5 * 1024 * 1024 {
            return (StatusCode::BAD_REQUEST, "图片大小不能超过5MB".to_string());
        }

        if tokio::fs::write(&path, bytes).await.is_err() {
            return (StatusCode::INTERNAL_SERVER_ERROR, "保存图片失败".to_string());
        }

        file_path = format!("/api/product/image/{}", new_filename);
    }

    if !has_file {
        return (StatusCode::BAD_REQUEST, "请选择要上传的图片".to_string());
    }

    let _ = sqlx::query("UPDATE product SET image_url = ? WHERE id = ?")
        .bind(&file_path)
        .bind(product_id)
        .execute(pool())
        .await;

    (StatusCode::OK, serde_json::json!({ "url": file_path }).to_string())
}

async fn api_product_delete_image(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> (StatusCode, String) {
    let product_id = params.get("product_id").and_then(|s| s.parse::<i64>().ok());
    if product_id.is_none() {
        return (StatusCode::BAD_REQUEST, "缺少 product_id 参数".to_string());
    }
    let product_id = product_id.unwrap();

    let row = sqlx::query("SELECT image_url FROM product WHERE id = ?")
        .bind(product_id)
        .fetch_optional(pool())
        .await
        .unwrap_or(None);

    if let Some(row) = row {
        let image_url: Option<String> = row.get("image_url");
        if let Some(url) = image_url {
            if url.starts_with("/api/product/image/") {
                let filename = url.replace("/api/product/image/", "");
                let path = format!("uploads/{}", filename);
                let _ = tokio::fs::remove_file(&path).await;
            }
        }
    }

    let _ = sqlx::query("UPDATE product SET image_url = NULL WHERE id = ?")
        .bind(product_id)
        .execute(pool())
        .await;

    (StatusCode::OK, "删除成功".to_string())
}

async fn api_product_get_image(
    Path(filename): Path<String>,
) -> impl IntoResponse {
    let path = format!("uploads/{}", filename);
    let file = tokio::fs::read(&path).await;

    match file {
        Ok(content) => {
            let ext = std::path::Path::new(&filename)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("jpg")
                .to_lowercase();

            let mime_type = match ext.as_str() {
                "jpg" | "jpeg" => "image/jpeg",
                "png" => "image/png",
                "gif" => "image/gif",
                "webp" => "image/webp",
                _ => "image/jpeg",
            };

            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime_type)],
                content,
            )
        }
        Err(_) => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain")],
            "图片不存在".as_bytes().to_vec(),
        ),
    }
}

async fn api_product_export() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT p.id, p.name, p.alias1, p.alias2, p.spec, p.unit, p.base_unit, p.base_price, p.purchase_price, c.name as category_name 
         FROM product p LEFT JOIN category c ON p.category_id = c.id ORDER BY p.id"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let result: Result<Vec<u8>, XlsxError> = (|| {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();
        
        let header_format = Format::new()
            .set_bold()
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter);
        
        let headers = ["ID", "名称", "下订名称(别称1)", "配单名称(别称2)", "规格", "单位", "基本单位", "基准单价", "进价", "分类"];
        for (i, &header) in headers.iter().enumerate() {
            worksheet.write_with_format(0, i as u16, header, &header_format)?;
        }
        
        let mut row_idx = 1;
        for row in rows {
            worksheet.write(row_idx, 0, row.get::<i64, _>("id"))?;
            worksheet.write(row_idx, 1, row.get::<String, _>("name"))?;
            worksheet.write(row_idx, 2, row.get::<Option<String>, _>("alias1").unwrap_or_default())?;
            worksheet.write(row_idx, 3, row.get::<Option<String>, _>("alias2").unwrap_or_default())?;
            worksheet.write(row_idx, 4, row.get::<Option<String>, _>("spec").unwrap_or_default())?;
            worksheet.write(row_idx, 5, row.get::<String, _>("unit"))?;
            worksheet.write(row_idx, 6, row.get::<Option<String>, _>("base_unit").unwrap_or("个".to_string()))?;
            worksheet.write(row_idx, 7, row.get::<f64, _>("base_price"))?;
            worksheet.write(row_idx, 8, row.get::<f64, _>("purchase_price"))?;
            worksheet.write(row_idx, 9, row.get::<Option<String>, _>("category_name").unwrap_or_default())?;
            row_idx += 1;
        }
        
        worksheet.set_column_width(0, 8)?;
        worksheet.set_column_width(1, 20)?;
        worksheet.set_column_width(2, 18)?;
        worksheet.set_column_width(3, 18)?;
        worksheet.set_column_width(4, 12)?;
        worksheet.set_column_width(5, 8)?;
        worksheet.set_column_width(6, 10)?;
        worksheet.set_column_width(7, 12)?;
        worksheet.set_column_width(8, 10)?;
        worksheet.set_column_width(9, 12)?;
        
        workbook.save_to_buffer()
    })();
    
    match result {
        Ok(data) => (
            StatusCode::OK,
            [
                ("Content-Type", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
                ("Content-Disposition", "attachment; filename=\"products.xlsx\""),
            ],
            data,
        ).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("导出失败: {}", e)).into_response(),
    }
}

async fn api_product_import(content: Bytes) -> impl IntoResponse {
    let rows: Vec<Vec<String>>;
    
    if content.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
        let content_vec = content.to_vec();
        match open_workbook_auto_from_rs(std::io::Cursor::new(content_vec)) {
            Ok(mut workbook) => {
                let sheets = workbook.sheet_names().to_vec();
                if sheets.is_empty() {
                    return (StatusCode::BAD_REQUEST, "Excel文件中没有工作表".to_string()).into_response();
                }
                
                let range = match workbook.worksheet_range(&sheets[0]) {
                    Ok(r) => r,
                    Err(e) => return (StatusCode::BAD_REQUEST, format!("无法读取Excel文件内容: {}", e)).into_response(),
                };
                
                rows = range.rows()
                    .map(|row| {
                        row.iter()
                            .map(|cell| match cell {
                                Data::Empty => "".to_string(),
                                Data::Int(v) => v.to_string(),
                                Data::Float(v) => v.to_string(),
                                Data::String(v) => v.to_string(),
                                Data::Bool(v) => v.to_string(),
                                _ => "".to_string(),
                            })
                            .collect()
                    })
                    .collect();
            }
            Err(e) => {
                return (StatusCode::BAD_REQUEST, format!("读取Excel文件失败: {}", e)).into_response();
            }
        }
    } else {
        let content_str = String::from_utf8_lossy(&content).to_string();
        rows = parse_csv(&content_str);
    }
    
    if rows.len() < 2 {
        return (StatusCode::BAD_REQUEST, "文件至少需要包含标题行和一行数据".to_string()).into_response();
    }
    
    let mut success = 0;
    let mut failed = 0;
    
    for (_i, row) in rows.iter().enumerate().skip(1) {
        if row.len() < 2 {
            failed += 1;
            continue;
        }
        
        let name = row[1].trim();
        if name.is_empty() {
            failed += 1;
            continue;
        }
        
        let category_name = if row.len() > 9 { row[9].trim() } else { "" };
        let category_id = if !category_name.is_empty() {
            let cid: Option<i64> = sqlx::query("SELECT id FROM category WHERE name = ? AND entity_type = 'product'")
                .bind(category_name)
                .fetch_optional(pool())
                .await
                .ok()
                .flatten()
                .map(|r| r.get::<i64, _>("id"));
            cid
        } else {
            None
        };
        
        let spec = if row.len() > 4 { row[4].trim() } else { "" };
        let base_price: f64 = if row.len() > 7 { row[7].trim().parse().unwrap_or(0.0) } else { 0.0 };
        let purchase_price: f64 = if row.len() > 8 { row[8].trim().parse().unwrap_or(0.0) } else { 0.0 };
        
        let result = sqlx::query(
            "INSERT OR IGNORE INTO product(name, alias1, alias2, spec, unit, base_unit, base_price, purchase_price, category_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(name)
        .bind(if row.len() > 2 { row[2].trim() } else { "" })
        .bind(if row.len() > 3 { row[3].trim() } else { "" })
        .bind(spec)
        .bind(if row.len() > 5 { row[5].trim() } else { "个" })
        .bind(if row.len() > 6 { row[6].trim() } else { "个" })
        .bind(base_price)
        .bind(purchase_price)
        .bind(category_id)
        .execute(pool())
        .await;
        
        match result {
            Ok(res) => {
                if res.rows_affected() > 0 {
                    success += 1;
                } else {
                    failed += 1;
                }
            }
            Err(_) => {
                failed += 1;
            }
        }
    }
    
    (StatusCode::OK, format!("导入完成：成功 {} 条，失败 {} 条", success, failed)).into_response()
}

async fn api_product_unit_create(Json(req): Json<ProductUnitReq>) -> impl IntoResponse {
    let result = sqlx::query(
        "INSERT INTO product_unit(product_id, unit_name, ratio, unit_price, purchase_price, sort_order) VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(req.product_id)
    .bind(&req.unit_name)
    .bind(req.ratio)
    .bind(req.unit_price.unwrap_or(0.0))
    .bind(req.purchase_price.unwrap_or(0.0))
    .bind(req.sort_order.unwrap_or(0))
    .execute(pool())
    .await;

    match result {
        Ok(_) => (StatusCode::OK, "创建成功".to_string()),
        Err(e) => {
            eprintln!("创建单位失败: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("创建单位失败: {:?}", e))
        }
    }
}

async fn api_product_unit_update(Json(req): Json<ProductUnitReq>) -> (StatusCode, String) {
    let result = sqlx::query(
        "UPDATE product_unit SET unit_name = ?, ratio = ?, unit_price = ?, purchase_price = ?, sort_order = ? WHERE id = ?"
    )
    .bind(&req.unit_name)
    .bind(req.ratio)
    .bind(req.unit_price.unwrap_or(0.0))
    .bind(req.purchase_price.unwrap_or(0.0))
    .bind(req.sort_order.unwrap_or(0))
    .bind(req.product_id)
    .execute(pool())
    .await;

    match result {
        Ok(_) => (StatusCode::OK, "更新成功".to_string()),
        Err(e) => {
            eprintln!("更新单位失败: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "更新失败".to_string())
        }
    }
}

async fn api_product_unit_delete(Json(req): Json<DeleteReq>) -> (StatusCode, String) {
    let result = sqlx::query("DELETE FROM product_unit WHERE id = ?")
        .bind(req.id)
        .execute(pool())
        .await;
    match result {
        Ok(r) => {
            if r.rows_affected() > 0 {
                (StatusCode::OK, "删除成功".to_string())
            } else {
                (StatusCode::NOT_FOUND, "单位不存在".to_string())
            }
        }
        Err(e) => {
            eprintln!("删除单位失败: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "删除失败".to_string())
        }
    }
}

async fn api_product_unit_delete_by_product(Json(req): Json<serde_json::Value>) -> (StatusCode, String) {
    let product_id = req["product_id"].as_i64().unwrap_or(0);
    let result = sqlx::query("DELETE FROM product_unit WHERE product_id = ?")
        .bind(product_id)
        .execute(pool())
        .await;
    match result {
        Ok(_) => (StatusCode::OK, "删除成功".to_string()),
        Err(e) => {
            eprintln!("删除单位失败: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "删除失败".to_string())
        }
    }
}

async fn api_product_unit_list(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let product_id = params.get("product_id").and_then(|s| s.parse::<i64>().ok());
    if product_id.is_none() {
        return (StatusCode::OK, serde_json::to_string(&Vec::<serde_json::Value>::new()).unwrap());
    }
    
    let rows = sqlx::query(
        "SELECT unit_name, ratio, unit_price, purchase_price FROM product_unit WHERE product_id = ? ORDER BY sort_order, id"
    )
    .bind(product_id.unwrap())
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let units: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| serde_json::json!({
            "name": row.get::<String, _>("unit_name"),
            "ratio": row.get::<f64, _>("ratio"),
            "unit_price": row.get::<f64, _>("unit_price"),
            "purchase_price": row.get::<f64, _>("purchase_price"),
        }))
        .collect();
    
    (StatusCode::OK, serde_json::to_string(&units).unwrap())
}

async fn api_product_price_upsert(Json(req): Json<ProductPriceReq>) -> impl IntoResponse {
    let result = sqlx::query(
        "INSERT OR REPLACE INTO product_price(product_id, price_type, price, collected_at, source) VALUES (?, ?, ?, ?, ?)"
    )
    .bind(req.product_id)
    .bind(&req.price_type)
    .bind(req.price.unwrap_or(0.0))
    .bind(&req.collected_at)
    .bind(&req.source)
    .execute(pool())
    .await;

    match result {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            eprintln!("保存价格失败: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

async fn api_product_price_list(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let product_id = match params.get("product_id") {
        Some(v) => v.parse::<i64>().unwrap_or(0),
        None => 0,
    };
    
    let rows = sqlx::query("SELECT id, product_id, price_type, price, collected_at, source FROM product_price WHERE product_id = ?")
        .bind(product_id)
        .fetch_all(pool())
        .await
        .unwrap_or_default();
    
    let prices: Vec<serde_json::Value> = rows.iter().map(|r| serde_json::json!({
        "id": r.get::<i64, _>("id"),
        "product_id": r.get::<i64, _>("product_id"),
        "price_type": r.get::<String, _>("price_type"),
        "price": r.get::<f64, _>("price"),
        "collected_at": r.get::<Option<String>, _>("collected_at"),
        "source": r.get::<Option<String>, _>("source"),
    })).collect();
    
    (StatusCode::OK, serde_json::to_string(&prices).unwrap())
}

async fn api_product_price_delete(Json(req): Json<DeleteReq>) -> (StatusCode, String) {
    let result = sqlx::query("DELETE FROM product_price WHERE id = ?")
        .bind(req.id)
        .execute(pool())
        .await;
    match result {
        Ok(r) => {
            if r.rows_affected() > 0 {
                (StatusCode::OK, "删除成功".to_string())
            } else {
                (StatusCode::NOT_FOUND, "价格记录不存在".to_string())
            }
        }
        Err(e) => {
            eprintln!("删除价格失败: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "删除失败".to_string())
        }
    }
}

async fn api_product_price_delete_by_product(Json(req): Json<std::collections::HashMap<String, i64>>) -> impl IntoResponse {
    let product_id = match req.get("product_id") {
        Some(&id) => id,
        None => return StatusCode::BAD_REQUEST,
    };
    sqlx::query("DELETE FROM product_price WHERE product_id = ?")
        .bind(product_id)
        .execute(pool())
        .await
        .ok();
    StatusCode::OK
}

async fn api_product_sync_base_price(Json(req): Json<std::collections::HashMap<String, i64>>) -> impl IntoResponse {
    let product_id = match req.get("product_id") {
        Some(&id) => id,
        None => return StatusCode::BAD_REQUEST,
    };

    let gov_row = sqlx::query("SELECT price FROM product_price WHERE product_id = ? AND price_type = 'gov_procurement'")
        .bind(product_id)
        .fetch_optional(pool())
        .await
        .ok()
        .flatten();
    let gov_price: Option<f64> = gov_row.map(|r| r.get("price"));

    let selling_price: f64 = if let Some(gp) = gov_price {
        if gp > 0.0 {
            gp
        } else {
            let max_row = sqlx::query("SELECT MAX(price) as max_price FROM product_price WHERE product_id = ? AND price_type LIKE 'supermarket_%'")
                .bind(product_id)
                .fetch_optional(pool())
                .await
                .ok()
                .flatten();
            if let Some(row) = max_row {
                let mp: Option<f64> = row.get("max_price");
                mp.unwrap_or(0.0)
            } else {
                0.0
            }
        }
    } else {
        let max_row = sqlx::query("SELECT MAX(price) as max_price FROM product_price WHERE product_id = ? AND price_type LIKE 'supermarket_%'")
            .bind(product_id)
            .fetch_optional(pool())
            .await
            .ok()
            .flatten();
        if let Some(row) = max_row {
            let mp: Option<f64> = row.get("max_price");
            mp.unwrap_or(0.0)
        } else {
            0.0
        }
    };

    if selling_price > 0.0 {
        let _ = sqlx::query("UPDATE product SET base_price = ? WHERE id = ?")
            .bind(selling_price)
            .bind(product_id)
            .execute(pool())
            .await;
    }

    StatusCode::OK
}

async fn api_category_list() -> impl IntoResponse {
    let rows = sqlx::query("SELECT id, name, parent_id, entity_type, sort_order FROM category ORDER BY entity_type, sort_order, id")
        .fetch_all(pool())
        .await
        .unwrap_or_default();
    
    let categories: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| serde_json::json!({
            "id": row.get::<i64, _>("id"),
            "name": row.get::<String, _>("name"),
            "parent_id": row.get::<Option<i64>, _>("parent_id"),
            "entity_type": row.get::<String, _>("entity_type"),
            "sort_order": row.get::<i32, _>("sort_order"),
        }))
        .collect();
    
    (StatusCode::OK, serde_json::to_string(&categories).unwrap())
}

async fn api_category_create(Json(req): Json<CategoryReq>) -> impl IntoResponse {
    let result = sqlx::query(
        "INSERT INTO category(name, parent_id, entity_type, sort_order) VALUES (?, ?, ?, ?)"
    )
    .bind(&req.name)
    .bind(&req.parent_id)
    .bind(&req.entity_type)
    .bind(&req.sort_order)
    .execute(pool())
    .await;
    
    match result {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            eprintln!("创建分类失败: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

async fn api_category_delete(Json(req): Json<serde_json::Value>) -> (StatusCode, String) {
    let id = req["id"].as_i64().unwrap_or(0);
    // 先检查是否有子分类
    let child_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM category WHERE parent_id = ?")
        .bind(id)
        .fetch_one(pool())
        .await
        .unwrap_or(0);
    if child_count > 0 {
        return (StatusCode::BAD_REQUEST, "该分类下有子分类，无法删除".to_string());
    }
    // 检查是否有实体引用
    for table in &["supplier", "purchaser", "product"] {
        let count = match *table {
            "supplier" => sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM supplier WHERE category_id = ?").bind(id).fetch_one(pool()).await.unwrap_or(0),
            "purchaser" => sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM purchaser WHERE category_id = ?").bind(id).fetch_one(pool()).await.unwrap_or(0),
            "product" => sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM product WHERE category_id = ?").bind(id).fetch_one(pool()).await.unwrap_or(0),
            _ => 0,
        };
        if count > 0 {
            return (StatusCode::BAD_REQUEST, format!("该分类已被{}引用，无法删除", table));
        }
    }
    let result = sqlx::query("DELETE FROM category WHERE id = ?").bind(id).execute(pool()).await;
    match result {
        Ok(_) => (StatusCode::OK, "删除成功".to_string()),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "删除失败".to_string()),
    }
}

#[derive(Deserialize)]
struct CategoryRenameReq {
    id: i64,
    name: String,
}

async fn api_category_rename(Json(req): Json<CategoryRenameReq>) -> (StatusCode, String) {
    let result = sqlx::query("UPDATE category SET name = ? WHERE id = ?")
        .bind(&req.name)
        .bind(req.id)
        .execute(pool())
        .await;
    match result {
        Ok(_) => (StatusCode::OK, "重命名成功".to_string()),
        Err(e) => {
            eprintln!("重命名分类失败: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "重命名失败".to_string())
        }
    }
}

async fn api_category_tree(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let entity_type = params.get("entity_type").cloned().unwrap_or_else(|| "product".to_string());
    let rows = sqlx::query("SELECT id, name, parent_id, entity_type, sort_order FROM category ORDER BY sort_order, id")
        .fetch_all(pool())
        .await
        .unwrap_or_default();
    
    let tree = build_category_tree_json(&rows, None, &entity_type);
    (StatusCode::OK, serde_json::to_string(&tree).unwrap())
}

fn build_category_tree_json(rows: &[sqlx::sqlite::SqliteRow], parent_id: Option<i64>, entity_type: &str) -> Vec<serde_json::Value> {
    let mut result = vec![];
    for row in rows {
        let et: String = row.get("entity_type");
        let pid: Option<i64> = row.get("parent_id");
        if et != entity_type { continue; }
        if pid != parent_id { continue; }
        let id: i64 = row.get("id");
        let children = build_category_tree_json(rows, Some(id), entity_type);
        result.push(serde_json::json!({
            "id": id,
            "name": row.get::<String, _>("name"),
            "parent_id": pid,
            "sort_order": row.get::<i32, _>("sort_order"),
            "children": children,
        }));
    }
    result.sort_by_key(|x| (x["sort_order"].as_i64().unwrap_or(0), x["id"].as_i64().unwrap_or(0)));
    result
}

async fn api_inventory_list() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT i.id, i.product_id, i.warehouse_id, p.name, p.spec, i.quantity, i.min_stock, i.max_stock, w.name as warehouse_name
         FROM inventory i JOIN product p ON i.product_id = p.id LEFT JOIN warehouse w ON i.warehouse_id = w.id"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let inventory: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| serde_json::json!({
            "id": row.get::<i64, _>("id"),
            "product_id": row.get::<i64, _>("product_id"),
            "warehouse_id": row.get::<i64, _>("warehouse_id"),
            "warehouse_name": row.get::<Option<String>, _>("warehouse_name"),
            "name": row.get::<String, _>("name"),
            "spec": row.get::<Option<String>, _>("spec"),
            "quantity": row.get::<f64, _>("quantity"),
            "min_stock": row.get::<f64, _>("min_stock"),
            "max_stock": row.get::<f64, _>("max_stock"),
        }))
        .collect();
    
    (StatusCode::OK, serde_json::to_string(&inventory).unwrap())
}

async fn api_warehouse_list() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT id, name, code, address, contact, phone, status, sort_order, create_at, update_at FROM warehouse ORDER BY sort_order, id"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let warehouses: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| serde_json::json!({
            "id": row.get::<i64, _>("id"),
            "name": row.get::<String, _>("name"),
            "code": row.get::<Option<String>, _>("code"),
            "address": row.get::<Option<String>, _>("address"),
            "contact": row.get::<Option<String>, _>("contact"),
            "phone": row.get::<Option<String>, _>("phone"),
            "status": row.get::<i32, _>("status"),
            "sort_order": row.get::<i32, _>("sort_order"),
            "create_at": row.get::<Option<String>, _>("create_at"),
            "update_at": row.get::<Option<String>, _>("update_at"),
        }))
        .collect();
    
    (StatusCode::OK, serde_json::to_string(&warehouses).unwrap())
}

#[derive(Deserialize)]
struct WarehouseCreateReq {
    name: String,
    code: Option<String>,
    address: Option<String>,
    contact: Option<String>,
    phone: Option<String>,
    sort_order: Option<i32>,
}

async fn api_warehouse_create(Json(req): Json<WarehouseCreateReq>) -> (StatusCode, String) {
    let result = sqlx::query(
        "INSERT INTO warehouse (name, code, address, contact, phone, sort_order) VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(&req.name)
    .bind(req.code)
    .bind(req.address)
    .bind(req.contact)
    .bind(req.phone)
    .bind(req.sort_order.unwrap_or(0))
    .execute(pool())
    .await;
    
    match result {
        Ok(_) => (StatusCode::OK, "创建成功".to_string()),
        Err(e) => {
            eprintln!("创建仓库失败: {}", e);
            if e.to_string().contains("UNIQUE constraint failed") {
                (StatusCode::BAD_REQUEST, "仓库名称或编号已存在".to_string())
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, "创建失败".to_string())
            }
        }
    }
}

#[derive(Deserialize)]
struct WarehouseUpdateReq {
    id: i64,
    name: String,
    code: Option<String>,
    address: Option<String>,
    contact: Option<String>,
    phone: Option<String>,
    status: Option<i32>,
    sort_order: Option<i32>,
}

async fn api_warehouse_update(Json(req): Json<WarehouseUpdateReq>) -> (StatusCode, String) {
    let result = sqlx::query(
        "UPDATE warehouse SET name = ?, code = ?, address = ?, contact = ?, phone = ?, status = ?, sort_order = ?, update_at = CURRENT_TIMESTAMP WHERE id = ?"
    )
    .bind(&req.name)
    .bind(req.code)
    .bind(req.address)
    .bind(req.contact)
    .bind(req.phone)
    .bind(req.status.unwrap_or(1))
    .bind(req.sort_order.unwrap_or(0))
    .bind(req.id)
    .execute(pool())
    .await;
    
    match result {
        Ok(_) => (StatusCode::OK, "更新成功".to_string()),
        Err(e) => {
            eprintln!("更新仓库失败: {}", e);
            if e.to_string().contains("UNIQUE constraint failed") {
                (StatusCode::BAD_REQUEST, "仓库名称或编号已存在".to_string())
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, "更新失败".to_string())
            }
        }
    }
}

async fn api_warehouse_delete(Json(req): Json<std::collections::HashMap<String, i64>>) -> (StatusCode, String) {
    let id = req.get("id").copied().unwrap_or(0);
    if id == 1 {
        return (StatusCode::BAD_REQUEST, "默认仓库无法删除".to_string());
    }
    
    let count: i64 = sqlx::query("SELECT COUNT(*) FROM inventory WHERE warehouse_id = ?")
        .bind(id)
        .fetch_one(pool())
        .await
        .map(|r| r.get(0))
        .unwrap_or(0);
    
    if count > 0 {
        return (StatusCode::BAD_REQUEST, "该仓库存在库存记录，无法删除".to_string());
    }
    
    let result = sqlx::query("DELETE FROM warehouse WHERE id = ?")
        .bind(id)
        .execute(pool())
        .await;
    
    match result {
        Ok(_) => (StatusCode::OK, "删除成功".to_string()),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "删除失败".to_string()),
    }
}

async fn generate_order_no(order_type: &str, order_date: &str) -> String {
    let prefix = if order_type == "sales" { "SO" } else { "PO" };
    
    let date_str: Vec<&str> = order_date.split('-').collect();
    let date_part = format!("{}{}{}", date_str[0], date_str[1], date_str[2]);
    
    let max_seq: i64 = if order_type == "sales" {
        sqlx::query_scalar(
            "SELECT COALESCE(MAX(CAST(SUBSTR(order_no, 11, 3) AS INTEGER)), 0) FROM sales_order WHERE order_date = ?"
        )
        .bind(order_date)
        .fetch_one(pool())
        .await
        .unwrap_or(0)
    } else {
        sqlx::query_scalar(
            "SELECT COALESCE(MAX(CAST(SUBSTR(order_no, 11, 3) AS INTEGER)), 0) FROM purchase_order WHERE order_date = ?"
        )
        .bind(order_date)
        .fetch_one(pool())
        .await
        .unwrap_or(0)
    };
    
    format!("{}{}{:03}", prefix, date_part, max_seq + 1)
}

async fn api_order_generate_no(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let default_type = "purchase".to_string();
    let order_type = params.get("type").unwrap_or(&default_type);
    let default_date = Local::now().format("%Y-%m-%d").to_string();
    let order_date = params.get("date").unwrap_or(&default_date);
    
    let order_no = generate_order_no(order_type, order_date).await;
    
    (StatusCode::OK, serde_json::to_string(&serde_json::json!({ "order_no": order_no })).unwrap())
}

async fn api_purchase_order_create(Json(req): Json<PurchaseOrderReq>) -> impl IntoResponse {
    let result = sqlx::query(
        "INSERT INTO purchase_order(supplier_id, order_no, order_date, total_amount, discount_rate, amount_reduction, final_amount, warehouse_id, warehouse_name, remark) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(req.supplier_id)
    .bind(&req.order_no)
    .bind(&req.order_date)
    .bind(req.total_amount)
    .bind(req.discount_rate)
    .bind(req.amount_reduction)
    .bind(req.final_amount)
    .bind(req.warehouse_id)
    .bind(&req.warehouse_name)
    .bind(&req.remark)
    .execute(pool())
    .await;
    
    match result {
        Ok(res) => {
            let order_id = res.last_insert_rowid();
            for item in req.items {
                sqlx::query(
                    "INSERT INTO purchase_order_item(order_id, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, base_quantity, amount, remark) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                )
                .bind(order_id)
                .bind(item.product_id)
                .bind(&item.product_name)
                .bind(&item.alias1)
                .bind(&item.alias2)
                .bind(&item.spec)
                .bind(&item.unit)
                .bind(item.unit_price)
                .bind(item.quantity)
                .bind(item.base_quantity.unwrap_or(0.0))
                .bind(item.amount)
                .bind(&item.remark)
                .execute(pool())
                .await
                .ok();
            }
            StatusCode::OK
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn api_purchase_order_list(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let keyword_pattern = parse_keyword_pattern(&params);
    
    let page: i64 = params.get("page").and_then(|s| s.parse().ok()).unwrap_or(1);
    let page_size: i64 = params.get("page_size").and_then(|s| s.parse().ok()).unwrap_or(20);
    let offset = (page - 1) * page_size;
    
    let total_rows = sqlx::query(
        "SELECT COUNT(*) as count FROM purchase_order po JOIN supplier s ON po.supplier_id = s.id 
         WHERE po.order_no LIKE ? OR s.name LIKE ? OR po.order_date LIKE ?"
    )
    .bind(&keyword_pattern)
    .bind(&keyword_pattern)
    .bind(&keyword_pattern)
    .fetch_one(pool())
    .await
    .unwrap();
    let total: i64 = total_rows.get("count");
    
    let rows = sqlx::query(
        "SELECT po.id, po.order_no, po.order_date, po.total_amount, po.discount_rate, po.amount_reduction, po.final_amount, po.status, po.remark, po.warehouse_id, po.warehouse_name, s.name as supplier_name 
         FROM purchase_order po JOIN supplier s ON po.supplier_id = s.id 
         WHERE po.order_no LIKE ? OR s.name LIKE ? OR po.order_date LIKE ?
         ORDER BY po.id DESC LIMIT ? OFFSET ?"
    )
    .bind(&keyword_pattern)
    .bind(&keyword_pattern)
    .bind(&keyword_pattern)
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let orders: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| serde_json::json!({
            "id": row.get::<i64, _>("id"),
            "order_no": row.get::<String, _>("order_no"),
            "order_date": row.get::<String, _>("order_date"),
            "total_amount": row.get::<f64, _>("total_amount"),
            "discount_rate": row.get::<f64, _>("discount_rate"),
            "amount_reduction": row.get::<f64, _>("amount_reduction"),
            "final_amount": row.get::<f64, _>("final_amount"),
            "warehouse_id": row.get::<i64, _>("warehouse_id"),
            "warehouse_name": row.get::<Option<String>, _>("warehouse_name"),
            "status": row.get::<String, _>("status"),
            "remark": row.get::<Option<String>, _>("remark"),
            "supplier_name": row.get::<String, _>("supplier_name"),
        }))
        .collect();
    
    let result = serde_json::json!({
        "data": orders,
        "page": page,
        "page_size": page_size,
        "total": total,
        "total_pages": (total + page_size - 1) / page_size
    });
    
    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

async fn api_purchase_order_detail(Path(id): Path<i64>) -> impl IntoResponse {
    let order_row = sqlx::query(
        "SELECT po.id, po.supplier_id, po.order_no, po.order_date, po.total_amount, po.discount_rate, po.amount_reduction, po.final_amount, po.status, po.remark, po.warehouse_id, po.warehouse_name, s.name as supplier_name
         FROM purchase_order po JOIN supplier s ON po.supplier_id = s.id WHERE po.id = ?"
    )
    .bind(id)
    .fetch_optional(pool())
    .await
    .unwrap_or(None);
    
    if order_row.is_none() {
        return (StatusCode::NOT_FOUND, "订单不存在".to_string());
    }
    
    let row = order_row.unwrap();
    
    let item_rows = sqlx::query(
        "SELECT id, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, base_quantity, amount, remark FROM purchase_order_item WHERE order_id = ?"
    )
    .bind(id)
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let items: Vec<serde_json::Value> = item_rows
        .iter()
        .map(|r| serde_json::json!({
            "id": r.get::<i64, _>("id"),
            "product_id": r.get::<i64, _>("product_id"),
            "product_name": r.get::<String, _>("product_name"),
            "alias1": r.get::<Option<String>, _>("alias1"),
            "alias2": r.get::<Option<String>, _>("alias2"),
            "spec": r.get::<Option<String>, _>("spec"),
            "unit": r.get::<Option<String>, _>("unit"),
            "unit_price": r.get::<f64, _>("unit_price"),
            "quantity": r.get::<f64, _>("quantity"),
            "base_quantity": r.get::<Option<f64>, _>("base_quantity"),
            "amount": r.get::<f64, _>("amount"),
            "remark": r.get::<Option<String>, _>("remark"),
        }))
        .collect();
    
    let order = serde_json::json!({
        "id": row.get::<i64, _>("id"),
        "supplier_id": row.get::<i64, _>("supplier_id"),
        "order_no": row.get::<String, _>("order_no"),
        "order_date": row.get::<String, _>("order_date"),
        "total_amount": row.get::<f64, _>("total_amount"),
        "discount_rate": row.get::<f64, _>("discount_rate"),
        "amount_reduction": row.get::<f64, _>("amount_reduction"),
        "final_amount": row.get::<f64, _>("final_amount"),
        "warehouse_id": row.get::<i64, _>("warehouse_id"),
        "warehouse_name": row.get::<Option<String>, _>("warehouse_name"),
        "status": row.get::<String, _>("status"),
        "remark": row.get::<Option<String>, _>("remark"),
        "supplier_name": row.get::<String, _>("supplier_name"),
        "items": items,
    });
    
    (StatusCode::OK, serde_json::to_string(&order).unwrap())
}

async fn api_purchase_order_update(headers: axum::http::HeaderMap, Json(req): Json<PurchaseOrderReq>) -> impl IntoResponse {
    match check_api_permission(&headers, "/api/purchase_order/update").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let result = sqlx::query(
        "UPDATE purchase_order SET supplier_id = ?, order_no = ?, order_date = ?, total_amount = ?, discount_rate = ?, amount_reduction = ?, final_amount = ?, warehouse_id = ?, warehouse_name = ?, remark = ? WHERE id = ?"
    )
    .bind(req.supplier_id)
    .bind(&req.order_no)
    .bind(&req.order_date)
    .bind(req.total_amount)
    .bind(req.discount_rate)
    .bind(req.amount_reduction)
    .bind(req.final_amount)
    .bind(req.warehouse_id)
    .bind(&req.warehouse_name)
    .bind(&req.remark)
    .bind(req.id)
    .execute(pool())
    .await;
    
    match result {
        Ok(_) => {
            sqlx::query("DELETE FROM purchase_order_item WHERE order_id = ?")
                .bind(req.id)
                .execute(pool())
                .await
                .ok();
            
            for item in req.items {
                sqlx::query(
                    "INSERT INTO purchase_order_item(order_id, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, base_quantity, amount, remark) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                )
                .bind(req.id)
                .bind(item.product_id)
                .bind(&item.product_name)
                .bind(&item.alias1)
                .bind(&item.alias2)
                .bind(&item.spec)
                .bind(&item.unit)
                .bind(item.unit_price)
                .bind(item.quantity)
                .bind(item.base_quantity.unwrap_or(0.0))
                .bind(item.amount)
                .bind(&item.remark)
                .execute(pool())
                .await
                .ok();
            }
            (StatusCode::OK, "更新成功".to_string())
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "更新失败".to_string()),
    }
}

async fn api_purchase_order_delete(headers: axum::http::HeaderMap, Path(id): Path<i64>) -> impl IntoResponse {
    match check_api_permission(&headers, "/api/purchase_order/delete").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    sqlx::query("DELETE FROM purchase_order_item WHERE order_id = ?")
        .bind(id)
        .execute(pool())
        .await
        .ok();
    
    let result = sqlx::query("DELETE FROM purchase_order WHERE id = ?")
        .bind(id)
        .execute(pool())
        .await;
    
    match result {
        Ok(_) => (StatusCode::OK, "删除成功".to_string()),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "删除失败".to_string()),
    }
}

async fn api_purchase_order_export() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT po.id, po.order_no, po.order_date, po.total_amount, po.discount_rate, po.final_amount, po.status, po.remark, s.name as supplier_name,
                poi.product_name, poi.alias1, poi.alias2, poi.spec, poi.unit, poi.unit_price, poi.quantity, poi.base_quantity, poi.amount, poi.remark as item_remark
         FROM purchase_order po 
         JOIN supplier s ON po.supplier_id = s.id
         LEFT JOIN purchase_order_item poi ON po.id = poi.order_id
         ORDER BY po.id, poi.id"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let result: Result<Vec<u8>, XlsxError> = (|| {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();
        
        let header_format = Format::new()
            .set_bold()
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter);
        
        let headers = ["订单ID", "订单号", "订单日期", "供应商", "总金额", "下浮率(%)", "下浮后合计", "状态", "备注", "商品名称", "下订名称(别称1)", "配单名称(别称2)", "规格", "单位", "数量", "单价", "基本数量", "金额", "商品备注"];
        for (i, &header) in headers.iter().enumerate() {
            worksheet.write_with_format(0, i as u16, header, &header_format)?;
        }
        
        let mut row_idx = 1;
        for row in rows {
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
            worksheet.write(row_idx, 14, row.get::<Option<f64>, _>("quantity").unwrap_or(0.0))?;
            worksheet.write(row_idx, 15, row.get::<Option<f64>, _>("unit_price").unwrap_or(0.0))?;
            worksheet.write(row_idx, 16, row.get::<Option<f64>, _>("base_quantity").unwrap_or(0.0))?;
            worksheet.write(row_idx, 17, row.get::<Option<f64>, _>("amount").unwrap_or(0.0))?;
            worksheet.write(row_idx, 18, row.get::<Option<String>, _>("item_remark").unwrap_or_default())?;
            row_idx += 1;
        }
        
        worksheet.set_column_width(0, 8)?;
        worksheet.set_column_width(1, 18)?;
        worksheet.set_column_width(2, 12)?;
        worksheet.set_column_width(3, 15)?;
        worksheet.set_column_width(4, 10)?;
        worksheet.set_column_width(5, 12)?;
        worksheet.set_column_width(6, 12)?;
        worksheet.set_column_width(7, 8)?;
        worksheet.set_column_width(8, 15)?;
        worksheet.set_column_width(9, 15)?;
        worksheet.set_column_width(10, 15)?;
        worksheet.set_column_width(11, 15)?;
        worksheet.set_column_width(12, 10)?;
        worksheet.set_column_width(13, 8)?;
        worksheet.set_column_width(14, 8)?;
        worksheet.set_column_width(15, 8)?;
        worksheet.set_column_width(16, 10)?;
        worksheet.set_column_width(17, 10)?;
        worksheet.set_column_width(18, 15)?;
        
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

async fn api_purchase_order_import(content: Bytes) -> impl IntoResponse {
    let rows: Vec<Vec<String>>;
    
    if content.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
        let content_vec = content.to_vec();
        match open_workbook_auto_from_rs(std::io::Cursor::new(content_vec)) {
            Ok(mut workbook) => {
                let sheets = workbook.sheet_names().to_vec();
                if sheets.is_empty() {
                    return (StatusCode::BAD_REQUEST, "Excel文件中没有工作表".to_string()).into_response();
                }
                
                let range = match workbook.worksheet_range(&sheets[0]) {
                    Ok(r) => r,
                    Err(e) => return (StatusCode::BAD_REQUEST, format!("无法读取Excel文件内容: {}", e)).into_response(),
                };
                
                rows = range.rows()
                    .map(|row| {
                        row.iter()
                            .map(|cell| match cell {
                                Data::Empty => "".to_string(),
                                Data::Int(v) => v.to_string(),
                                Data::Float(v) => v.to_string(),
                                Data::String(v) => v.to_string(),
                                Data::Bool(v) => v.to_string(),
                                _ => "".to_string(),
                            })
                            .collect()
                    })
                    .collect();
            }
            Err(e) => {
                return (StatusCode::BAD_REQUEST, format!("读取Excel文件失败: {}", e)).into_response();
            }
        }
    } else {
        let content_str = String::from_utf8_lossy(&content).to_string();
        rows = parse_csv(&content_str);
    }
    
    if rows.len() < 2 {
        return (StatusCode::BAD_REQUEST, "文件至少需要包含标题行和一行数据".to_string()).into_response();
    }
    
    let mut orders: std::collections::HashMap<String, (Vec<String>, Vec<Vec<String>>)> = std::collections::HashMap::new();
    
    for row in rows.iter().skip(1) {
        if row.len() < 3 {
            continue;
        }
        
        let order_no = row[1].trim().to_string();
        if order_no.is_empty() {
            continue;
        }
        
        if !orders.contains_key(&order_no) {
            orders.insert(order_no.clone(), (row.clone(), Vec::new()));
        }
        
        if row.len() > 9 && !row[9].trim().is_empty() {
            let item: Vec<String> = row[9..].to_vec();
            orders.get_mut(&order_no).unwrap().1.push(item);
        }
    }
    
    let mut success = 0;
    let mut failed = 0;
    
    for (order_no, (order_row, items)) in orders {
        let supplier_name = if order_row.len() > 3 { order_row[3].trim() } else { "" };
        let supplier_id = if !supplier_name.is_empty() {
            let sid: Option<i64> = sqlx::query("SELECT id FROM supplier WHERE name = ?")
                .bind(supplier_name)
                .fetch_optional(pool())
                .await
                .ok()
                .flatten()
                .map(|r| r.get::<i64, _>("id"));
            sid
        } else {
            None
        };
        
        if supplier_id.is_none() {
            failed += 1;
            continue;
        }
        
        let order_date = if order_row.len() > 2 { order_row[2].trim() } else { "" };
        if order_date.is_empty() {
            failed += 1;
            continue;
        }
        
        let total_amount: f64 = if order_row.len() > 4 { order_row[4].trim().parse().unwrap_or(0.0) } else { 0.0 };
        let discount_rate: f64 = if order_row.len() > 5 { order_row[5].trim().parse().unwrap_or(0.0) } else { 0.0 };
        let final_amount: f64 = if order_row.len() > 6 { order_row[6].trim().parse().unwrap_or(0.0) } else { 0.0 };
        let remark = if order_row.len() > 8 { order_row[8].trim() } else { "" };
        
        let result = sqlx::query(
            "INSERT OR IGNORE INTO purchase_order(order_no, supplier_id, order_date, total_amount, discount_rate, final_amount, remark, status) VALUES (?, ?, ?, ?, ?, ?, ?, 'pending')"
        )
        .bind(&order_no)
        .bind(supplier_id.unwrap())
        .bind(order_date)
        .bind(total_amount)
        .bind(discount_rate)
        .bind(final_amount)
        .bind(remark)
        .execute(pool())
        .await;
        
        match result {
            Ok(res) => {
                if res.rows_affected() > 0 {
                    let order_id = res.last_insert_rowid();
                    for item in items {
                        if item.len() < 1 {
                            continue;
                        }
                        
                        let product_name = if item.len() > 0 { item[0].trim() } else { "" };
                        let alias1 = if item.len() > 1 { item[1].trim() } else { "" };
                        let alias2 = if item.len() > 2 { item[2].trim() } else { "" };
                        let spec = if item.len() > 3 { item[3].trim() } else { "" };
                        let unit = if item.len() > 4 { item[4].trim() } else { "个" };
                        let unit_price: f64 = if item.len() > 5 { item[5].trim().parse().unwrap_or(0.0) } else { 0.0 };
                        let quantity: f64 = if item.len() > 6 { item[6].trim().parse().unwrap_or(0.0) } else { 0.0 };
                        let base_quantity: f64 = if item.len() > 7 { item[7].trim().parse().unwrap_or(0.0) } else { 0.0 };
                        let amount: f64 = if item.len() > 8 { item[8].trim().parse().unwrap_or(0.0) } else { 0.0 };
                        let item_remark = if item.len() > 9 { item[9].trim() } else { "" };
                        
                        let product_id: i64 = sqlx::query("SELECT id FROM product WHERE name = ? AND (spec IS NULL OR spec = ?)")
                            .bind(product_name)
                            .bind(spec)
                            .fetch_optional(pool())
                            .await
                            .ok()
                            .flatten()
                            .map(|r| r.get::<i64, _>("id"))
                            .unwrap_or(0);
                        
                        sqlx::query(
                            "INSERT INTO purchase_order_item(order_id, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, base_quantity, amount, remark) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                        )
                        .bind(order_id)
                        .bind(product_id)
                        .bind(product_name)
                        .bind(alias1)
                        .bind(alias2)
                        .bind(spec)
                        .bind(unit)
                        .bind(unit_price)
                        .bind(quantity)
                        .bind(base_quantity)
                        .bind(amount)
                        .bind(item_remark)
                        .execute(pool())
                        .await
                        .ok();
                    }
                    success += 1;
                } else {
                    failed += 1;
                }
            }
            Err(_) => {
                failed += 1;
            }
        }
    }
    
    (StatusCode::OK, format!("导入完成：成功 {} 条，失败 {} 条", success, failed)).into_response()
}

async fn api_sales_order_detail(Path(id): Path<i64>) -> impl IntoResponse {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sales_order WHERE id = ?)")
        .bind(id)
        .fetch_one(pool())
        .await
        .unwrap_or(false);

    if !exists {
        return (StatusCode::NOT_FOUND, format!("订单不存在 (ID: {})", id).to_string());
    }

    let order_row = sqlx::query(
        "SELECT so.id, so.purchaser_id, so.order_no, so.order_date, so.total_amount, so.discount_rate, so.amount_reduction, so.final_amount, so.status, so.remark, so.warehouse_id, so.warehouse_name, COALESCE(p.name, '') as purchaser_name
         FROM sales_order so LEFT JOIN purchaser p ON so.purchaser_id = p.id WHERE so.id = ?"
    )
    .bind(id)
    .fetch_optional(pool())
    .await;

    let row = match order_row {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("订单存在但JOIN查询失败 (ID: {})", id).to_string());
        }
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("查询失败：{}", e).to_string());
        }
    };
    
    let item_rows = match sqlx::query(
        "SELECT id, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, base_quantity, amount, supplier_id, supplier_name, remark FROM sales_order_item WHERE order_id = ?"
    )
    .bind(id)
    .fetch_all(pool())
    .await {
        Ok(rows) => rows,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("查询订单明细失败：{}", e).to_string());
        }
    };
    
    let items: Vec<serde_json::Value> = item_rows
        .iter()
        .map(|r| serde_json::json!({
            "id": r.get::<i64, _>("id"),
            "product_id": r.get::<i64, _>("product_id"),
            "product_name": r.get::<String, _>("product_name"),
            "alias1": r.get::<Option<String>, _>("alias1"),
            "alias2": r.get::<Option<String>, _>("alias2"),
            "spec": r.get::<Option<String>, _>("spec"),
            "unit": r.get::<Option<String>, _>("unit"),
            "unit_price": r.get::<f64, _>("unit_price"),
            "quantity": r.get::<f64, _>("quantity"),
            "base_quantity": r.get::<Option<f64>, _>("base_quantity"),
            "amount": r.get::<f64, _>("amount"),
            "supplier_id": r.get::<Option<i64>, _>("supplier_id"),
            "supplier_name": r.get::<Option<String>, _>("supplier_name"),
            "remark": r.get::<Option<String>, _>("remark"),
        }))
        .collect();
    
    let order = serde_json::json!({
        "id": row.get::<i64, _>("id"),
        "purchaser_id": row.get::<i64, _>("purchaser_id"),
        "order_no": row.get::<String, _>("order_no"),
        "order_date": row.get::<String, _>("order_date"),
        "total_amount": row.get::<f64, _>("total_amount"),
        "discount_rate": row.get::<f64, _>("discount_rate"),
        "amount_reduction": row.get::<f64, _>("amount_reduction"),
        "final_amount": row.get::<f64, _>("final_amount"),
        "warehouse_id": row.get::<Option<i64>, _>("warehouse_id"),
        "warehouse_name": row.get::<Option<String>, _>("warehouse_name"),
        "status": row.get::<String, _>("status"),
        "remark": row.get::<Option<String>, _>("remark"),
        "purchaser_name": row.get::<String, _>("purchaser_name"),
        "items": items,
    });
    
    match serde_json::to_string(&order) {
        Ok(json_str) => (StatusCode::OK, json_str),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("序列化订单JSON失败：{}", e).to_string()),
    }
}

async fn api_sales_order_update(headers: axum::http::HeaderMap, Json(req): Json<SalesOrderReq>) -> impl IntoResponse {
    match check_api_permission(&headers, "/api/sales_order/update").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let result = sqlx::query(
        "UPDATE sales_order SET purchaser_id = ?, order_no = ?, order_date = ?, total_amount = ?, discount_rate = ?, amount_reduction = ?, final_amount = ?, warehouse_id = ?, warehouse_name = ?, remark = ? WHERE id = ?"
    )
    .bind(req.purchaser_id)
    .bind(&req.order_no)
    .bind(&req.order_date)
    .bind(req.total_amount)
    .bind(req.discount_rate)
    .bind(req.amount_reduction)
    .bind(req.final_amount)
    .bind(req.warehouse_id)
    .bind(&req.warehouse_name)
    .bind(&req.remark)
    .bind(req.id)
    .execute(pool())
    .await;
    
    match result {
        Ok(_) => {
            sqlx::query("DELETE FROM sales_order_item WHERE order_id = ?")
                .bind(req.id)
                .execute(pool())
                .await
                .ok();
            
            for item in req.items {
                sqlx::query(
                    "INSERT INTO sales_order_item(order_id, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, base_quantity, amount, supplier_id, supplier_name, remark) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                )
                .bind(req.id)
                .bind(item.product_id)
                .bind(&item.product_name)
                .bind(&item.alias1)
                .bind(&item.alias2)
                .bind(&item.spec)
                .bind(&item.unit)
                .bind(item.unit_price)
                .bind(item.quantity)
                .bind(item.base_quantity.unwrap_or(0.0))
                .bind(item.amount)
                .bind(item.supplier_id)
                .bind(&item.supplier_name)
                .bind(&item.remark)
                .execute(pool())
                .await
                .ok();
            }
            (StatusCode::OK, "更新成功".to_string())
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "更新失败".to_string()),
    }
}

async fn api_sales_order_delete(headers: axum::http::HeaderMap, Path(id): Path<i64>) -> impl IntoResponse {
    match check_api_permission(&headers, "/api/sales_order/delete").await {
        Err(e) => return e,
        Ok(_) => {}
    }

    let order_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sales_order WHERE id = ?)")
        .bind(id)
        .fetch_one(pool())
        .await
        .unwrap_or(false);

    if !order_exists {
        return (StatusCode::NOT_FOUND, "订单不存在".to_string());
    }

    let delete_items_result = sqlx::query("DELETE FROM sales_order_item WHERE order_id = ?")
        .bind(id)
        .execute(pool())
        .await;

    if let Err(e) = delete_items_result {
        let err_str = e.to_string();
        if err_str.contains("foreign key constraint") || err_str.contains("FOREIGN KEY") {
            return (StatusCode::BAD_REQUEST, format!("删除失败：订单明细存在外键约束冲突，请检查关联数据"));
        }
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("删除订单明细失败：{}", e));
    }

    let result = sqlx::query("DELETE FROM sales_order WHERE id = ?")
        .bind(id)
        .execute(pool())
        .await;
    
    match result {
        Ok(_) => (StatusCode::OK, "删除成功".to_string()),
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("foreign key constraint") || err_str.contains("FOREIGN KEY") {
                (StatusCode::BAD_REQUEST, format!("删除失败：存在外键约束冲突，请检查关联数据"))
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("删除失败：{}", e))
            }
        }
    }
}

async fn api_sales_order_export() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT so.id, so.order_no, so.order_date, so.total_amount, so.discount_rate, so.final_amount, so.status, so.remark, p.name as purchaser_name,
                soi.product_name, soi.alias1, soi.alias2, soi.spec, soi.unit, soi.unit_price, soi.quantity, soi.base_quantity, soi.amount, soi.remark as item_remark
         FROM sales_order so 
         JOIN purchaser p ON so.purchaser_id = p.id
         LEFT JOIN sales_order_item soi ON so.id = soi.order_id
         ORDER BY so.id, soi.id"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let result: Result<Vec<u8>, XlsxError> = (|| {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();
        
        let header_format = Format::new()
            .set_bold()
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter);
        
        let headers = ["订单ID", "订单号", "订单日期", "采购单位", "总金额", "下浮率(%)", "下浮后合计", "状态", "备注", "商品名称", "下订名称(别称1)", "配单名称(别称2)", "规格", "单位", "数量", "单价", "基本数量", "金额", "商品备注"];
        for (i, &header) in headers.iter().enumerate() {
            worksheet.write_with_format(0, i as u16, header, &header_format)?;
        }
        
        let mut row_idx = 1;
        for row in rows {
            worksheet.write(row_idx, 0, row.get::<i64, _>("id"))?;
            worksheet.write(row_idx, 1, row.get::<String, _>("order_no"))?;
            worksheet.write(row_idx, 2, row.get::<String, _>("order_date"))?;
            worksheet.write(row_idx, 3, row.get::<String, _>("purchaser_name"))?;
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
            worksheet.write(row_idx, 14, row.get::<Option<f64>, _>("quantity").unwrap_or(0.0))?;
            worksheet.write(row_idx, 15, row.get::<Option<f64>, _>("unit_price").unwrap_or(0.0))?;
            worksheet.write(row_idx, 16, row.get::<Option<f64>, _>("base_quantity").unwrap_or(0.0))?;
            worksheet.write(row_idx, 17, row.get::<Option<f64>, _>("amount").unwrap_or(0.0))?;
            worksheet.write(row_idx, 18, row.get::<Option<String>, _>("item_remark").unwrap_or_default())?;
            row_idx += 1;
        }
        
        worksheet.set_column_width(0, 8)?;
        worksheet.set_column_width(1, 18)?;
        worksheet.set_column_width(2, 12)?;
        worksheet.set_column_width(3, 15)?;
        worksheet.set_column_width(4, 10)?;
        worksheet.set_column_width(5, 12)?;
        worksheet.set_column_width(6, 12)?;
        worksheet.set_column_width(7, 8)?;
        worksheet.set_column_width(8, 15)?;
        worksheet.set_column_width(9, 15)?;
        worksheet.set_column_width(10, 15)?;
        worksheet.set_column_width(11, 15)?;
        worksheet.set_column_width(12, 10)?;
        worksheet.set_column_width(13, 8)?;
        worksheet.set_column_width(14, 8)?;
        worksheet.set_column_width(15, 8)?;
        worksheet.set_column_width(16, 10)?;
        worksheet.set_column_width(17, 10)?;
        worksheet.set_column_width(18, 15)?;
        
        workbook.save_to_buffer()
    })();
    
    match result {
        Ok(data) => (
            StatusCode::OK,
            [
                ("Content-Type", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
                ("Content-Disposition", "attachment; filename=\"sales_orders.xlsx\""),
            ],
            data,
        ).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("导出失败: {}", e)).into_response(),
    }
}

async fn api_sales_order_import(content: Bytes) -> impl IntoResponse {
    let rows: Vec<Vec<String>>;
    
    if content.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
        let content_vec = content.to_vec();
        match open_workbook_auto_from_rs(std::io::Cursor::new(content_vec)) {
            Ok(mut workbook) => {
                let sheets = workbook.sheet_names().to_vec();
                if sheets.is_empty() {
                    return (StatusCode::BAD_REQUEST, "Excel文件中没有工作表".to_string()).into_response();
                }
                
                let range = match workbook.worksheet_range(&sheets[0]) {
                    Ok(r) => r,
                    Err(e) => return (StatusCode::BAD_REQUEST, format!("无法读取Excel文件内容: {}", e)).into_response(),
                };
                
                rows = range.rows()
                    .map(|row| {
                        row.iter()
                            .map(|cell| match cell {
                                Data::Empty => "".to_string(),
                                Data::Int(v) => v.to_string(),
                                Data::Float(v) => v.to_string(),
                                Data::String(v) => v.to_string(),
                                Data::Bool(v) => v.to_string(),
                                _ => "".to_string(),
                            })
                            .collect()
                    })
                    .collect();
            }
            Err(e) => {
                return (StatusCode::BAD_REQUEST, format!("读取Excel文件失败: {}", e)).into_response();
            }
        }
    } else {
        let content_str = String::from_utf8_lossy(&content).to_string();
        rows = parse_csv(&content_str);
    }
    
    if rows.len() < 2 {
        return (StatusCode::BAD_REQUEST, "文件至少需要包含标题行和一行数据".to_string()).into_response();
    }
    
    let mut orders: std::collections::HashMap<String, (Vec<String>, Vec<Vec<String>>)> = std::collections::HashMap::new();
    
    for row in rows.iter().skip(1) {
        if row.len() < 3 {
            continue;
        }
        
        let order_no = row[1].trim().to_string();
        if order_no.is_empty() {
            continue;
        }
        
        if !orders.contains_key(&order_no) {
            orders.insert(order_no.clone(), (row.clone(), Vec::new()));
        }
        
        if row.len() > 9 && !row[9].trim().is_empty() {
            let item: Vec<String> = row[9..].to_vec();
            orders.get_mut(&order_no).unwrap().1.push(item);
        }
    }
    
    let mut success = 0;
    let mut failed = 0;
    
    for (order_no, (order_row, items)) in orders {
        let purchaser_name = if order_row.len() > 3 { order_row[3].trim() } else { "" };
        let purchaser_id = if !purchaser_name.is_empty() {
            let pid: Option<i64> = sqlx::query("SELECT id FROM purchaser WHERE name = ?")
                .bind(purchaser_name)
                .fetch_optional(pool())
                .await
                .ok()
                .flatten()
                .map(|r| r.get::<i64, _>("id"));
            pid
        } else {
            None
        };
        
        if purchaser_id.is_none() {
            failed += 1;
            continue;
        }
        
        let order_date = if order_row.len() > 2 { order_row[2].trim() } else { "" };
        if order_date.is_empty() {
            failed += 1;
            continue;
        }
        
        let total_amount: f64 = if order_row.len() > 4 { order_row[4].trim().parse().unwrap_or(0.0) } else { 0.0 };
        let discount_rate: f64 = if order_row.len() > 5 { order_row[5].trim().parse().unwrap_or(0.0) } else { 0.0 };
        let final_amount: f64 = if order_row.len() > 6 { order_row[6].trim().parse().unwrap_or(0.0) } else { 0.0 };
        let remark = if order_row.len() > 8 { order_row[8].trim() } else { "" };
        
        let result = sqlx::query(
            "INSERT OR IGNORE INTO sales_order(order_no, purchaser_id, order_date, total_amount, discount_rate, final_amount, remark, status) VALUES (?, ?, ?, ?, ?, ?, ?, 'pending')"
        )
        .bind(&order_no)
        .bind(purchaser_id.unwrap())
        .bind(order_date)
        .bind(total_amount)
        .bind(discount_rate)
        .bind(final_amount)
        .bind(remark)
        .execute(pool())
        .await;
        
        match result {
            Ok(res) => {
                if res.rows_affected() > 0 {
                    let order_id = res.last_insert_rowid();
                    for item in items {
                        if item.len() < 1 {
                            continue;
                        }
                        
                        let product_name = if item.len() > 0 { item[0].trim() } else { "" };
                        let alias1 = if item.len() > 1 { item[1].trim() } else { "" };
                        let alias2 = if item.len() > 2 { item[2].trim() } else { "" };
                        let spec = if item.len() > 3 { item[3].trim() } else { "" };
                        let unit = if item.len() > 4 { item[4].trim() } else { "个" };
                        let unit_price: f64 = if item.len() > 5 { item[5].trim().parse().unwrap_or(0.0) } else { 0.0 };
                        let quantity: f64 = if item.len() > 6 { item[6].trim().parse().unwrap_or(0.0) } else { 0.0 };
                        let base_quantity: f64 = if item.len() > 7 { item[7].trim().parse().unwrap_or(0.0) } else { 0.0 };
                        let amount: f64 = if item.len() > 8 { item[8].trim().parse().unwrap_or(0.0) } else { 0.0 };
                        let item_remark = if item.len() > 9 { item[9].trim() } else { "" };
                        
                        let product_id: i64 = sqlx::query("SELECT id FROM product WHERE name = ? AND (spec IS NULL OR spec = ?)")
                            .bind(product_name)
                            .bind(spec)
                            .fetch_optional(pool())
                            .await
                            .ok()
                            .flatten()
                            .map(|r| r.get::<i64, _>("id"))
                            .unwrap_or(0);
                        
                        sqlx::query(
                            "INSERT INTO sales_order_item(order_id, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, base_quantity, amount, remark) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                        )
                        .bind(order_id)
                        .bind(product_id)
                        .bind(product_name)
                        .bind(alias1)
                        .bind(alias2)
                        .bind(spec)
                        .bind(unit)
                        .bind(unit_price)
                        .bind(quantity)
                        .bind(base_quantity)
                        .bind(amount)
                        .bind(item_remark)
                        .execute(pool())
                        .await
                        .ok();
                    }
                    success += 1;
                } else {
                    failed += 1;
                }
            }
            Err(_) => {
                failed += 1;
            }
        }
    }
    
    (StatusCode::OK, format!("导入完成：成功 {} 条，失败 {} 条", success, failed)).into_response()
}

async fn api_query_purchase_order(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let supplier_id = params.get("supplier_id").map(|s| s.as_str()).unwrap_or("");
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or("");
    let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    let status = params.get("status").map(|s| s.as_str()).unwrap_or("");
    
    let page: i64 = params.get("page").and_then(|s| s.parse().ok()).unwrap_or(1);
    let page_size: i64 = params.get("page_size").and_then(|s| s.parse().ok()).unwrap_or(20);
    let offset = (page - 1) * page_size;
    
    let mut sql = String::from(
        "SELECT po.id, po.order_no, po.order_date, po.total_amount, po.final_amount, po.status, po.remark, s.name as supplier_name 
         FROM purchase_order po JOIN supplier s ON po.supplier_id = s.id WHERE 1=1"
    );
    let mut count_sql = String::from(
        "SELECT COUNT(*) as count FROM purchase_order po JOIN supplier s ON po.supplier_id = s.id WHERE 1=1"
    );
    let mut binds: Vec<String> = Vec::new();
    
    if !supplier_id.is_empty() {
        sql.push_str(" AND po.supplier_id = ?");
        count_sql.push_str(" AND po.supplier_id = ?");
        binds.push(supplier_id.to_string());
    }
    if !start_date.is_empty() {
        sql.push_str(" AND po.order_date >= ?");
        count_sql.push_str(" AND po.order_date >= ?");
        binds.push(start_date.to_string());
    }
    if !end_date.is_empty() {
        sql.push_str(" AND po.order_date <= ?");
        count_sql.push_str(" AND po.order_date <= ?");
        binds.push(end_date.to_string());
    }
    if !status.is_empty() {
        sql.push_str(" AND po.status = ?");
        count_sql.push_str(" AND po.status = ?");
        binds.push(status.to_string());
    }
    sql.push_str(" ORDER BY po.id DESC LIMIT ? OFFSET ?");
    
    let mut count_query = sqlx::query(AssertSqlSafe(count_sql.as_str()));
    for b in &binds {
        count_query = count_query.bind(b);
    }
    let total_rows = count_query.fetch_one(pool()).await.unwrap();
    let total: i64 = total_rows.get("count");
    
    let mut query = sqlx::query(AssertSqlSafe(sql.as_str()));
    for b in &binds {
        query = query.bind(b);
    }
    query = query.bind(page_size).bind(offset);
    
    let rows = query.fetch_all(pool()).await.unwrap_or_default();
    
    let orders: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let total_amount: f64 = row.get("total_amount");
            let final_amount: f64 = row.get("final_amount");
            serde_json::json!({
                "id": row.get::<i64, _>("id"),
                "order_no": row.get::<String, _>("order_no"),
                "order_date": row.get::<String, _>("order_date"),
                "total_amount": total_amount,
                "final_amount": final_amount,
                "status": row.get::<String, _>("status"),
                "remark": row.get::<Option<String>, _>("remark"),
                "supplier_name": row.get::<String, _>("supplier_name"),
            })
        })
        .collect();
    
    let result = serde_json::json!({
        "data": orders,
        "page": page,
        "page_size": page_size,
        "total": total,
        "total_pages": (total + page_size - 1) / page_size
    });
    
    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

async fn api_query_purchase_order_export(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let supplier_id = params.get("supplier_id").map(|s| s.as_str()).unwrap_or("");
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or("");
    let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    let status = params.get("status").map(|s| s.as_str()).unwrap_or("");
    
    let mut sql = String::from(
        "SELECT po.id, po.order_no, po.order_date, po.total_amount, po.final_amount, po.status, po.remark, s.name as supplier_name 
         FROM purchase_order po JOIN supplier s ON po.supplier_id = s.id WHERE 1=1"
    );
    let mut binds: Vec<String> = Vec::new();
    
    if !supplier_id.is_empty() {
        sql.push_str(" AND po.supplier_id = ?");
        binds.push(supplier_id.to_string());
    }
    if !start_date.is_empty() {
        sql.push_str(" AND po.order_date >= ?");
        binds.push(start_date.to_string());
    }
    if !end_date.is_empty() {
        sql.push_str(" AND po.order_date <= ?");
        binds.push(end_date.to_string());
    }
    if !status.is_empty() {
        sql.push_str(" AND po.status = ?");
        binds.push(status.to_string());
    }
    sql.push_str(" ORDER BY po.id DESC");
    
    let mut query = sqlx::query(AssertSqlSafe(sql.as_str()));
    for b in &binds {
        query = query.bind(b);
    }
    
    let rows = query.fetch_all(pool()).await.unwrap_or_default();
    
    let mut workbook = rust_xlsxwriter::Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("采购订单查询").unwrap();
    
    let header_format = rust_xlsxwriter::Format::new()
        .set_bold()
        .set_background_color(rust_xlsxwriter::Color::RGB(0x4472C4))
        .set_font_color(rust_xlsxwriter::Color::White)
        .set_align(rust_xlsxwriter::FormatAlign::Center)
        .set_border(rust_xlsxwriter::FormatBorder::Thin);
    
    let headers = ["订单号", "供应商", "日期", "订单金额", "实付金额", "状态", "备注"];
    for (col, header) in headers.iter().enumerate() {
        worksheet.write_with_format(0, col as u16, *header, &header_format).unwrap();
    }
    
    for (row_idx, row) in rows.iter().enumerate() {
        let order_no: String = row.get("order_no");
        let supplier_name: String = row.get("supplier_name");
        let order_date: String = row.get("order_date");
        let total_amount: f64 = row.get("total_amount");
        let final_amount: f64 = row.get("final_amount");
        let status: String = row.get("status");
        let remark: Option<String> = row.get("remark");
        
        worksheet.write(row_idx as u32 + 1, 0, &order_no).unwrap();
        worksheet.write(row_idx as u32 + 1, 1, &supplier_name).unwrap();
        worksheet.write(row_idx as u32 + 1, 2, &order_date).unwrap();
        worksheet.write(row_idx as u32 + 1, 3, total_amount).unwrap();
        worksheet.write(row_idx as u32 + 1, 4, final_amount).unwrap();
        worksheet.write(row_idx as u32 + 1, 5, &status).unwrap();
        worksheet.write(row_idx as u32 + 1, 6, remark.unwrap_or_default()).unwrap();
    }
    
    let buf = workbook.save_to_buffer().unwrap();
    (
        [
            (header::CONTENT_TYPE, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"purchase_orders.xlsx\""),
        ],
        buf,
    ).into_response()
}

async fn api_query_purchase_price(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let product_name = params.get("product_name").map(|s| s.as_str()).unwrap_or("");
    let supplier_id = params.get("supplier_id").map(|s| s.as_str()).unwrap_or("");
    
    let page: i64 = params.get("page").and_then(|s| s.parse().ok()).unwrap_or(1);
    let page_size: i64 = params.get("page_size").and_then(|s| s.parse().ok()).unwrap_or(20);
    let offset = (page - 1) * page_size;
    
    let mut base_sql = String::from(
        " FROM purchase_order_item poi 
         JOIN purchase_order po ON poi.order_id = po.id 
         JOIN supplier s ON po.supplier_id = s.id WHERE 1=1"
    );
    let mut binds: Vec<String> = Vec::new();
    
    if !product_name.is_empty() {
        base_sql.push_str(" AND poi.product_name LIKE ?");
        binds.push(format!("%{}%", product_name));
    }
    if !supplier_id.is_empty() {
        base_sql.push_str(" AND po.supplier_id = ?");
        binds.push(supplier_id.to_string());
    }
    
    let count_query = format!("SELECT COUNT(*){count_sql}", count_sql = base_sql);
    let mut count_q = sqlx::query(AssertSqlSafe(count_query.as_str()));
    for b in &binds {
        count_q = count_q.bind(b);
    }
    let total_rows = count_q.fetch_one(pool()).await.unwrap();
    let total: i64 = total_rows.get("COUNT(*)");
    
    let data_sql = format!(
        "SELECT poi.product_name, poi.spec, poi.unit_price, poi.quantity, poi.unit, po.order_date, s.name as supplier_name {data_sql} ORDER BY po.order_date DESC LIMIT ? OFFSET ?",
        data_sql = base_sql
    );
    let mut query = sqlx::query(AssertSqlSafe(data_sql.as_str()));
    for b in &binds {
        query = query.bind(b);
    }
    query = query.bind(page_size).bind(offset);
    
    let rows = query.fetch_all(pool()).await.unwrap_or_default();
    
    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let unit_price: f64 = row.get("unit_price");
            let quantity: f64 = row.get("quantity");
            serde_json::json!({
                "product_name": row.get::<String, _>("product_name"),
                "spec": row.get::<Option<String>, _>("spec"),
                "unit": row.get::<Option<String>, _>("unit"),
                "supplier_name": row.get::<String, _>("supplier_name"),
                "unit_price": unit_price,
                "order_date": row.get::<String, _>("order_date"),
                "quantity": quantity,
            })
        })
        .collect();
    
    let result = serde_json::json!({
        "data": items,
        "page": page,
        "page_size": page_size,
        "total": total,
        "total_pages": (total + page_size - 1) / page_size
    });
    
    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

async fn api_query_purchase_summary(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or("");
    let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    
    let mut supplier_sql = String::from(
        "SELECT s.name, SUM(poi.quantity) as quantity, SUM(poi.amount) as amount 
         FROM purchase_order_item poi 
         JOIN purchase_order po ON poi.order_id = po.id 
         JOIN supplier s ON po.supplier_id = s.id WHERE 1=1"
    );
    let mut product_sql = String::from(
        "SELECT poi.product_name, poi.spec, SUM(poi.quantity) as quantity, SUM(poi.amount) as amount 
         FROM purchase_order_item poi 
         JOIN purchase_order po ON poi.order_id = po.id WHERE 1=1"
    );
    let mut binds: Vec<String> = Vec::new();
    
    if !start_date.is_empty() {
        supplier_sql.push_str(" AND po.order_date >= ?");
        product_sql.push_str(" AND po.order_date >= ?");
        binds.push(start_date.to_string());
    }
    let mut binds2 = binds.clone();
    if !end_date.is_empty() {
        supplier_sql.push_str(" AND po.order_date <= ?");
        product_sql.push_str(" AND po.order_date <= ?");
        binds.push(end_date.to_string());
        binds2.push(end_date.to_string());
    }
    supplier_sql.push_str(" GROUP BY s.id ORDER BY amount DESC");
    product_sql.push_str(" GROUP BY poi.product_name, poi.spec ORDER BY amount DESC");
    
    let mut query1 = sqlx::query(AssertSqlSafe(supplier_sql.as_str()));
    for b in &binds {
        query1 = query1.bind(b);
    }
    let supplier_rows = query1.fetch_all(pool()).await.unwrap_or_default();
    
    let mut query2 = sqlx::query(AssertSqlSafe(product_sql.as_str()));
    for b in &binds2 {
        query2 = query2.bind(b);
    }
    let product_rows = query2.fetch_all(pool()).await.unwrap_or_default();
    
    let by_supplier: Vec<serde_json::Value> = supplier_rows
        .iter()
        .map(|row| {
            let quantity: f64 = row.get("quantity");
            let amount: f64 = row.get("amount");
            serde_json::json!({
                "name": row.get::<String, _>("name"),
                "quantity": quantity,
                "amount": amount,
            })
        })
        .collect();
    
    let by_product: Vec<serde_json::Value> = product_rows
        .iter()
        .map(|row| {
            let quantity: f64 = row.get("quantity");
            let amount: f64 = row.get("amount");
            serde_json::json!({
                "product_name": row.get::<String, _>("product_name"),
                "spec": row.get::<Option<String>, _>("spec"),
                "quantity": quantity,
                "amount": amount,
            })
        })
        .collect();
    
    let result = serde_json::json!({
        "by_supplier": by_supplier,
        "by_product": by_product,
    });
    
    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

async fn api_query_supplier_balance() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT s.id, s.name, 
                COALESCE(SUM(po.final_amount), 0.0) as purchase_total,
                COALESCE(SUM(po.final_amount), 0.0) as unpaid
         FROM supplier s 
         LEFT JOIN purchase_order po ON po.supplier_id = s.id 
         GROUP BY s.id, s.name 
         ORDER BY purchase_total DESC"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let balances: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let purchase_total: f64 = row.get("purchase_total");
            let unpaid: f64 = row.get("unpaid");
            serde_json::json!({
                "id": row.get::<i64, _>("id"),
                "name": row.get::<String, _>("name"),
                "purchase_total": purchase_total,
                "paid_total": 0.0,
                "unpaid": unpaid,
                "prepay_balance": 0.0,
            })
        })
        .collect();
    
    (StatusCode::OK, serde_json::to_string(&balances).unwrap())
}

async fn api_query_supplier_balance_export() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT s.id, s.name, 
                COALESCE(SUM(po.final_amount), 0.0) as purchase_total,
                COALESCE(SUM(po.final_amount), 0.0) as unpaid
         FROM supplier s 
         LEFT JOIN purchase_order po ON po.supplier_id = s.id 
         GROUP BY s.id, s.name 
         ORDER BY purchase_total DESC"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let mut workbook = rust_xlsxwriter::Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("供应商往来对账").unwrap();
    
    let header_format = rust_xlsxwriter::Format::new()
        .set_bold()
        .set_background_color(rust_xlsxwriter::Color::RGB(0x4472C4))
        .set_font_color(rust_xlsxwriter::Color::White)
        .set_align(rust_xlsxwriter::FormatAlign::Center)
        .set_border(rust_xlsxwriter::FormatBorder::Thin);
    
    let headers = ["供应商名称", "本期进货总额", "已付款", "未付款", "预付款余额"];
    for (col, header) in headers.iter().enumerate() {
        worksheet.write_with_format(0, col as u16, *header, &header_format).unwrap();
    }
    
    for (row_idx, row) in rows.iter().enumerate() {
        let name: String = row.get("name");
        let purchase_total: f64 = row.get("purchase_total");
        
        worksheet.write(row_idx as u32 + 1, 0, &name).unwrap();
        worksheet.write(row_idx as u32 + 1, 1, purchase_total).unwrap();
        worksheet.write(row_idx as u32 + 1, 2, 0.0).unwrap();
        worksheet.write(row_idx as u32 + 1, 3, purchase_total).unwrap();
        worksheet.write(row_idx as u32 + 1, 4, 0.0).unwrap();
    }
    
    let buf = workbook.save_to_buffer().unwrap();
    (
        [
            (header::CONTENT_TYPE, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"supplier_balance.xlsx\""),
        ],
        buf,
    ).into_response()
}

async fn api_query_sales_order(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let purchaser_id = params.get("purchaser_id").map(|s| s.as_str()).unwrap_or("");
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or("");
    let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    let status = params.get("status").map(|s| s.as_str()).unwrap_or("");
    
    let page: i64 = params.get("page").and_then(|s| s.parse().ok()).unwrap_or(1);
    let page_size: i64 = params.get("page_size").and_then(|s| s.parse().ok()).unwrap_or(20);
    let offset = (page - 1) * page_size;
    
    let mut base_sql = String::from(
        " FROM sales_order so JOIN purchaser p ON so.purchaser_id = p.id WHERE 1=1"
    );
    let mut binds: Vec<String> = Vec::new();
    
    if !purchaser_id.is_empty() {
        base_sql.push_str(" AND so.purchaser_id = ?");
        binds.push(purchaser_id.to_string());
    }
    if !start_date.is_empty() {
        base_sql.push_str(" AND so.order_date >= ?");
        binds.push(start_date.to_string());
    }
    if !end_date.is_empty() {
        base_sql.push_str(" AND so.order_date <= ?");
        binds.push(end_date.to_string());
    }
    if !status.is_empty() {
        base_sql.push_str(" AND so.status = ?");
        binds.push(status.to_string());
    }
    
    let count_query = format!("SELECT COUNT(*){}", base_sql);
    let mut count_q = sqlx::query(AssertSqlSafe(count_query.as_str()));
    for b in &binds {
        count_q = count_q.bind(b);
    }
    let total_rows = count_q.fetch_one(pool()).await.unwrap();
    let total: i64 = total_rows.get("COUNT(*)");
    
    let data_sql = format!(
        "SELECT so.id, so.order_no, so.order_date, so.total_amount, so.discount_rate, so.final_amount, so.status, so.remark, p.name as purchaser_name {} ORDER BY so.id DESC LIMIT ? OFFSET ?",
        base_sql
    );
    let mut query = sqlx::query(AssertSqlSafe(data_sql.as_str()));
    for b in &binds {
        query = query.bind(b);
    }
    query = query.bind(page_size).bind(offset);
    
    let rows = query.fetch_all(pool()).await.unwrap_or_default();
    
    let orders: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let total_amount: f64 = row.get("total_amount");
            let final_amount: f64 = row.get("final_amount");
            serde_json::json!({
                "id": row.get::<i64, _>("id"),
                "order_no": row.get::<String, _>("order_no"),
                "order_date": row.get::<String, _>("order_date"),
                "total_amount": total_amount,
                "discount_rate": row.get::<f64, _>("discount_rate"),
                "final_amount": final_amount,
                "status": row.get::<String, _>("status"),
                "remark": row.get::<Option<String>, _>("remark"),
                "purchaser_name": row.get::<String, _>("purchaser_name"),
            })
        })
        .collect();
    
    let result = serde_json::json!({
        "data": orders,
        "page": page,
        "page_size": page_size,
        "total": total,
        "total_pages": (total + page_size - 1) / page_size
    });
    
    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

async fn api_query_sales_order_export(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let purchaser_id = params.get("purchaser_id").map(|s| s.as_str()).unwrap_or("");
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or("");
    let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    let status = params.get("status").map(|s| s.as_str()).unwrap_or("");
    
    let mut sql = String::from(
        "SELECT so.id, so.order_no, so.order_date, so.total_amount, so.discount_rate, so.final_amount, so.status, so.remark, p.name as purchaser_name 
         FROM sales_order so JOIN purchaser p ON so.purchaser_id = p.id WHERE 1=1"
    );
    let mut binds: Vec<String> = Vec::new();
    
    if !purchaser_id.is_empty() {
        sql.push_str(" AND so.purchaser_id = ?");
        binds.push(purchaser_id.to_string());
    }
    if !start_date.is_empty() {
        sql.push_str(" AND so.order_date >= ?");
        binds.push(start_date.to_string());
    }
    if !end_date.is_empty() {
        sql.push_str(" AND so.order_date <= ?");
        binds.push(end_date.to_string());
    }
    if !status.is_empty() {
        sql.push_str(" AND so.status = ?");
        binds.push(status.to_string());
    }
    sql.push_str(" ORDER BY so.id DESC");
    
    let mut query = sqlx::query(AssertSqlSafe(sql.as_str()));
    for b in &binds {
        query = query.bind(b);
    }
    
    let rows = query.fetch_all(pool()).await.unwrap_or_default();
    
    let mut workbook = rust_xlsxwriter::Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("销售订单查询").unwrap();
    
    let header_format = rust_xlsxwriter::Format::new()
        .set_bold()
        .set_background_color(rust_xlsxwriter::Color::RGB(0x70AD47))
        .set_font_color(rust_xlsxwriter::Color::White)
        .set_align(rust_xlsxwriter::FormatAlign::Center)
        .set_border(rust_xlsxwriter::FormatBorder::Thin);
    
    let headers = ["订单号", "采购单位", "日期", "订单金额", "下浮比例", "下浮后金额", "状态", "备注"];
    for (col, header) in headers.iter().enumerate() {
        worksheet.write_with_format(0, col as u16, *header, &header_format).unwrap();
    }
    
    for (row_idx, row) in rows.iter().enumerate() {
        let order_no: String = row.get("order_no");
        let purchaser_name: String = row.get("purchaser_name");
        let order_date: String = row.get("order_date");
        let total_amount: f64 = row.get("total_amount");
        let discount_rate: f64 = row.get("discount_rate");
        let final_amount: f64 = row.get("final_amount");
        let status: String = row.get("status");
        let remark: Option<String> = row.get("remark");
        
        worksheet.write(row_idx as u32 + 1, 0, &order_no).unwrap();
        worksheet.write(row_idx as u32 + 1, 1, &purchaser_name).unwrap();
        worksheet.write(row_idx as u32 + 1, 2, &order_date).unwrap();
        worksheet.write(row_idx as u32 + 1, 3, total_amount).unwrap();
        worksheet.write(row_idx as u32 + 1, 4, discount_rate).unwrap();
        worksheet.write(row_idx as u32 + 1, 5, final_amount).unwrap();
        worksheet.write(row_idx as u32 + 1, 6, &status).unwrap();
        worksheet.write(row_idx as u32 + 1, 7, remark.unwrap_or_default()).unwrap();
    }
    
    let buf = workbook.save_to_buffer().unwrap();
    (
        [
            (header::CONTENT_TYPE, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"sales_orders.xlsx\""),
        ],
        buf,
    ).into_response()
}

async fn api_query_sales_price(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let product_name = params.get("product_name").map(|s| s.as_str()).unwrap_or("");
    let purchaser_id = params.get("purchaser_id").map(|s| s.as_str()).unwrap_or("");
    
    let page: i64 = params.get("page").and_then(|s| s.parse().ok()).unwrap_or(1);
    let page_size: i64 = params.get("page_size").and_then(|s| s.parse().ok()).unwrap_or(20);
    let offset = (page - 1) * page_size;
    
    let mut base_sql = String::from(
        " FROM sales_order_item soi 
         JOIN sales_order so ON soi.order_id = so.id 
         JOIN purchaser p ON so.purchaser_id = p.id WHERE 1=1"
    );
    let mut binds: Vec<String> = Vec::new();
    
    if !product_name.is_empty() {
        base_sql.push_str(" AND soi.product_name LIKE ?");
        binds.push(format!("%{}%", product_name));
    }
    if !purchaser_id.is_empty() {
        base_sql.push_str(" AND so.purchaser_id = ?");
        binds.push(purchaser_id.to_string());
    }
    
    let count_query = format!("SELECT COUNT(*){}", base_sql);
    let mut count_q = sqlx::query(AssertSqlSafe(count_query.as_str()));
    for b in &binds {
        count_q = count_q.bind(b);
    }
    let total_rows = count_q.fetch_one(pool()).await.unwrap();
    let total: i64 = total_rows.get("COUNT(*)");
    
    let data_sql = format!(
        "SELECT soi.product_name, soi.spec, soi.unit_price, soi.quantity, soi.unit, so.order_date, p.name as purchaser_name {} ORDER BY so.order_date DESC LIMIT ? OFFSET ?",
        base_sql
    );
    let mut query = sqlx::query(AssertSqlSafe(data_sql.as_str()));
    for b in &binds {
        query = query.bind(b);
    }
    query = query.bind(page_size).bind(offset);
    
    let rows = query.fetch_all(pool()).await.unwrap_or_default();
    
    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let unit_price: f64 = row.get("unit_price");
            let quantity: f64 = row.get("quantity");
            serde_json::json!({
                "product_name": row.get::<String, _>("product_name"),
                "spec": row.get::<Option<String>, _>("spec"),
                "unit": row.get::<Option<String>, _>("unit"),
                "purchaser_name": row.get::<String, _>("purchaser_name"),
                "unit_price": unit_price,
                "order_date": row.get::<String, _>("order_date"),
                "quantity": quantity,
            })
        })
        .collect();
    
    let result = serde_json::json!({
        "data": items,
        "page": page,
        "page_size": page_size,
        "total": total,
        "total_pages": (total + page_size - 1) / page_size
    });
    
    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

async fn api_query_sales_summary(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or("");
    let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    
    let mut purchaser_sql = String::from(
        "SELECT p.name, SUM(soi.quantity) as quantity, SUM(soi.amount) as sales_amount
         FROM sales_order_item soi 
         JOIN sales_order so ON soi.order_id = so.id 
         JOIN purchaser p ON so.purchaser_id = p.id WHERE 1=1"
    );
    let mut product_sql = String::from(
        "SELECT soi.product_name, soi.spec, SUM(soi.quantity) as quantity, SUM(soi.amount) as sales_amount
         FROM sales_order_item soi 
         JOIN sales_order so ON soi.order_id = so.id WHERE 1=1"
    );
    let mut binds: Vec<String> = Vec::new();
    
    if !start_date.is_empty() {
        purchaser_sql.push_str(" AND so.order_date >= ?");
        product_sql.push_str(" AND so.order_date >= ?");
        binds.push(start_date.to_string());
    }
    let mut binds2 = binds.clone();
    if !end_date.is_empty() {
        purchaser_sql.push_str(" AND so.order_date <= ?");
        product_sql.push_str(" AND so.order_date <= ?");
        binds.push(end_date.to_string());
        binds2.push(end_date.to_string());
    }
    purchaser_sql.push_str(" GROUP BY p.id ORDER BY sales_amount DESC");
    product_sql.push_str(" GROUP BY soi.product_name, soi.spec ORDER BY sales_amount DESC");
    
    let mut query1 = sqlx::query(AssertSqlSafe(purchaser_sql.as_str()));
    for b in &binds {
        query1 = query1.bind(b);
    }
    let purchaser_rows = query1.fetch_all(pool()).await.unwrap_or_default();
    
    let mut query2 = sqlx::query(AssertSqlSafe(product_sql.as_str()));
    for b in &binds2 {
        query2 = query2.bind(b);
    }
    let product_rows = query2.fetch_all(pool()).await.unwrap_or_default();
    
    let by_purchaser: Vec<serde_json::Value> = purchaser_rows
        .iter()
        .map(|row| {
            let quantity: f64 = row.get("quantity");
            let sales_amount: f64 = row.get("sales_amount");
            serde_json::json!({
                "name": row.get::<String, _>("name"),
                "quantity": quantity,
                "sales_amount": sales_amount,
                "cost_amount": 0.0,
            })
        })
        .collect();
    
    let by_product: Vec<serde_json::Value> = product_rows
        .iter()
        .map(|row| {
            let quantity: f64 = row.get("quantity");
            let sales_amount: f64 = row.get("sales_amount");
            serde_json::json!({
                "product_name": row.get::<String, _>("product_name"),
                "spec": row.get::<Option<String>, _>("spec"),
                "quantity": quantity,
                "sales_amount": sales_amount,
                "cost_amount": 0.0,
            })
        })
        .collect();
    
    let result = serde_json::json!({
        "by_purchaser": by_purchaser,
        "by_product": by_product,
    });
    
    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

async fn api_query_purchaser_balance() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT p.id, p.name, 
                COALESCE(SUM(so.final_amount), 0) as sales_total,
                COALESCE(SUM(so.final_amount), 0) as unreceived
         FROM purchaser p 
         LEFT JOIN sales_order so ON so.purchaser_id = p.id 
         GROUP BY p.id, p.name 
         ORDER BY sales_total DESC"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let balances: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let sales_total: f64 = row.get("sales_total");
            let unreceived: f64 = row.get("unreceived");
            serde_json::json!({
                "id": row.get::<i64, _>("id"),
                "name": row.get::<String, _>("name"),
                "sales_total": sales_total,
                "received_total": 0.0,
                "unreceived": unreceived,
                "prepay_balance": 0.0,
            })
        })
        .collect();
    
    (StatusCode::OK, serde_json::to_string(&balances).unwrap())
}

async fn api_query_purchaser_balance_export() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT p.id, p.name, 
                COALESCE(SUM(so.final_amount), 0) as sales_total,
                COALESCE(SUM(so.final_amount), 0) as unreceived
         FROM purchaser p 
         LEFT JOIN sales_order so ON so.purchaser_id = p.id 
         GROUP BY p.id, p.name 
         ORDER BY sales_total DESC"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let mut workbook = rust_xlsxwriter::Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("采购方应收对账").unwrap();
    
    let header_format = rust_xlsxwriter::Format::new()
        .set_bold()
        .set_background_color(rust_xlsxwriter::Color::RGB(0x70AD47))
        .set_font_color(rust_xlsxwriter::Color::White)
        .set_align(rust_xlsxwriter::FormatAlign::Center)
        .set_border(rust_xlsxwriter::FormatBorder::Thin);
    
    let headers = ["采购单位名称", "累计销售", "已收款", "未收款", "预收款余额"];
    for (col, header) in headers.iter().enumerate() {
        worksheet.write_with_format(0, col as u16, *header, &header_format).unwrap();
    }
    
    for (row_idx, row) in rows.iter().enumerate() {
        let name: String = row.get("name");
        let sales_total: f64 = row.get("sales_total");
        
        worksheet.write(row_idx as u32 + 1, 0, &name).unwrap();
        worksheet.write(row_idx as u32 + 1, 1, sales_total).unwrap();
        worksheet.write(row_idx as u32 + 1, 2, 0.0).unwrap();
        worksheet.write(row_idx as u32 + 1, 3, sales_total).unwrap();
        worksheet.write(row_idx as u32 + 1, 4, 0.0).unwrap();
    }
    
    let buf = workbook.save_to_buffer().unwrap();
    (
        [
            (header::CONTENT_TYPE, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"purchaser_balance.xlsx\""),
        ],
        buf,
    ).into_response()
}

async fn api_sales_order_generate_purchase(Path(id): Path<i64>) -> impl IntoResponse {
    let order_row = sqlx::query(
        "SELECT so.id, so.order_date, so.warehouse_id, so.warehouse_name
         FROM sales_order so WHERE so.id = ?"
    )
    .bind(id)
    .fetch_optional(pool())
    .await
    .unwrap_or(None);
    
    if order_row.is_none() {
        return (StatusCode::NOT_FOUND, serde_json::json!({ "message": "销售订单不存在" }).to_string()).into_response();
    }
    
    let row = order_row.unwrap();
    let order_date = row.get::<String, _>("order_date");
    let warehouse_id = row.get::<i64, _>("warehouse_id");
    let warehouse_name = row.get::<Option<String>, _>("warehouse_name").unwrap_or_default();
    
    let item_rows = sqlx::query(
        "SELECT soi.product_id, soi.product_name, soi.alias1, soi.alias2, soi.spec, soi.unit, soi.quantity, soi.supplier_id, soi.supplier_name, p.purchase_price, p.base_unit, p.base_price
         FROM sales_order_item soi LEFT JOIN product p ON soi.product_id = p.id
         WHERE soi.order_id = ?"
    )
    .bind(id)
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let mut supplier_items: std::collections::HashMap<i64, Vec<(i64, String, String, String, String, String, f64, f64, String, f64)>> = std::collections::HashMap::new();
    
    for r in &item_rows {
        let supplier_id = r.get::<i64, _>("supplier_id");
        if supplier_id == 0 {
            continue;
        }
        let product_id = r.get::<i64, _>("product_id");
        let product_name = r.get::<String, _>("product_name");
        let alias1 = r.get::<Option<String>, _>("alias1").unwrap_or_default();
        let alias2 = r.get::<Option<String>, _>("alias2").unwrap_or_default();
        let spec = r.get::<Option<String>, _>("spec").unwrap_or_default();
        let unit = r.get::<Option<String>, _>("unit").unwrap_or_default();
        let quantity = r.get::<f64, _>("quantity");
        let purchase_price = r.get::<f64, _>("purchase_price");
        let base_unit = r.get::<Option<String>, _>("base_unit").unwrap_or_default();
        let base_price = r.get::<f64, _>("base_price");
        
        let unit_price = if purchase_price > 0.0 { purchase_price } else { base_price };
        
        supplier_items.entry(supplier_id).or_insert_with(Vec::new).push(
            (product_id, product_name, alias1, alias2, spec, unit, quantity, unit_price, base_unit, base_price)
        );
    }
    
    if supplier_items.is_empty() {
        return (StatusCode::BAD_REQUEST, serde_json::json!({ "message": "销售订单中没有供应商信息，无法生成采购订单" }).to_string()).into_response();
    }
    
    let mut created_count = 0;
    
    for (supplier_id, items) in supplier_items {
        let supplier_name_result = sqlx::query("SELECT name FROM supplier WHERE id = ?")
            .bind(supplier_id)
            .fetch_optional(pool())
            .await
            .unwrap_or(None);
        
        let _supplier_name = match supplier_name_result {
            Some(sr) => sr.get::<String, _>("name"),
            None => "未知供应商".to_string(),
        };
        
        let order_no = generate_order_no("purchase", &order_date).await;
        
        let mut total_amount = 0.0;
        for (_, _, _, _, _, _, qty, price, _, _) in &items {
            total_amount += qty * price;
        }
        
        let result = sqlx::query(
            "INSERT INTO purchase_order(supplier_id, order_no, order_date, total_amount, discount_rate, amount_reduction, final_amount, warehouse_id, warehouse_name, remark) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(supplier_id)
        .bind(&order_no)
        .bind(&order_date)
        .bind(total_amount)
        .bind(0.0)
        .bind(0.0)
        .bind(total_amount)
        .bind(warehouse_id)
        .bind(&warehouse_name)
        .bind(None::<String>)
        .execute(pool())
        .await;
        
        if let Ok(res) = result {
            let po_id = res.last_insert_rowid();
            for (product_id, product_name, alias1, alias2, spec, unit, quantity, unit_price, _base_unit, _base_price) in items {
                let amount = quantity * unit_price;
                let base_quantity = quantity;
                sqlx::query(
                    "INSERT INTO purchase_order_item(order_id, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, base_quantity, amount, remark) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                )
                .bind(po_id)
                .bind(product_id)
                .bind(&product_name)
                .bind(&alias1)
                .bind(&alias2)
                .bind(&spec)
                .bind(&unit)
                .bind(unit_price)
                .bind(quantity)
                .bind(base_quantity)
                .bind(amount)
                .bind(None::<String>)
                .execute(pool())
                .await
                .ok();
            }
            created_count += 1;
        }
    }
    
    (StatusCode::OK, serde_json::json!({ "count": created_count, "message": format!("成功生成 {} 张采购订单", created_count) }).to_string()).into_response()
}

async fn api_query_product_rank(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or("");
    let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    
    let mut top_sql = String::from(
        "SELECT soi.product_name, soi.spec, SUM(soi.quantity) as quantity, SUM(soi.amount) as amount
         FROM sales_order_item soi 
         JOIN sales_order so ON soi.order_id = so.id WHERE 1=1"
    );
    let mut binds: Vec<String> = Vec::new();
    
    if !start_date.is_empty() {
        top_sql.push_str(" AND so.order_date >= ?");
        binds.push(start_date.to_string());
    }
    if !end_date.is_empty() {
        top_sql.push_str(" AND so.order_date <= ?");
        binds.push(end_date.to_string());
    }
    top_sql.push_str(" GROUP BY soi.product_name, soi.spec ORDER BY quantity DESC LIMIT 10");
    
    let mut query1 = sqlx::query(AssertSqlSafe(top_sql.as_str()));
    for b in &binds {
        query1 = query1.bind(b);
    }
    let top_rows = query1.fetch_all(pool()).await.unwrap_or_default();
    
    let slow_sql = String::from(
        "SELECT pr.id, pr.name as product_name, pr.spec, pr.stock_quantity, 
                (SELECT MAX(so.order_date) FROM sales_order so 
                 JOIN sales_order_item soi ON so.id = soi.order_id 
                 WHERE soi.product_id = pr.id) as last_sale_date
         FROM product pr 
         WHERE pr.status = 1 
         AND pr.id NOT IN (
             SELECT DISTINCT soi.product_id 
             FROM sales_order_item soi 
             JOIN sales_order so ON soi.order_id = so.id 
             WHERE 1=1"
    );
    let mut slow_sql_complete = slow_sql.clone();
    let binds2 = binds.clone();
    if !start_date.is_empty() {
        slow_sql_complete.push_str(" AND so.order_date >= ?");
    }
    if !end_date.is_empty() {
        slow_sql_complete.push_str(" AND so.order_date <= ?");
    }
    slow_sql_complete.push_str(") ORDER BY pr.id LIMIT 50");
    
    let mut query2 = sqlx::query(AssertSqlSafe(slow_sql_complete.as_str()));
    for b in &binds2 {
        query2 = query2.bind(b);
    }
    let slow_rows = query2.fetch_all(pool()).await.unwrap_or_default();
    
    let top_selling: Vec<serde_json::Value> = top_rows
        .iter()
        .map(|row| {
            let quantity: f64 = row.get("quantity");
            let amount: f64 = row.get("amount");
            serde_json::json!({
                "product_name": row.get::<String, _>("product_name"),
                "spec": row.get::<Option<String>, _>("spec"),
                "quantity": quantity,
                "amount": amount,
            })
        })
        .collect();
    
    let slow_moving: Vec<serde_json::Value> = slow_rows
        .iter()
        .map(|row| {
            let stock_quantity: f64 = row.try_get("stock_quantity").unwrap_or(0.0);
            let last_sale_date: Option<String> = row.try_get("last_sale_date").unwrap_or(None);
            serde_json::json!({
                "product_name": row.get::<String, _>("product_name"),
                "spec": row.get::<Option<String>, _>("spec"),
                "stock_quantity": stock_quantity,
                "last_sale_date": last_sale_date,
            })
        })
        .collect();
    
    let result = serde_json::json!({
        "top_selling": top_selling,
        "slow_moving": slow_moving,
    });
    
    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

async fn api_query_overview(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let month = params.get("month").map(|s| s.as_str()).unwrap_or("");
    
    let purchase_total: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(po.total_amount), 0) FROM purchase_order po WHERE strftime('%Y-%m', po.order_date) = ?"
    )
    .bind(month)
    .fetch_one(pool())
    .await
    .unwrap_or(0.0);
    
    let sales_total: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(so.total_amount), 0) FROM sales_order so WHERE strftime('%Y-%m', so.order_date) = ?"
    )
    .bind(month)
    .fetch_one(pool())
    .await
    .unwrap_or(0.0);
    
    let stock_total: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(i.quantity * pr.selling_price), 0) FROM inventory i JOIN product pr ON i.product_id = pr.id"
    )
    .fetch_one(pool())
    .await
    .unwrap_or(0.0);
    
    let profit_total = sales_total - purchase_total;
    
    let purchase_by_supplier_rows = sqlx::query(
        "SELECT s.name, COALESCE(SUM(poi.amount), 0) as amount, COALESCE(SUM(poi.quantity), 0) as quantity
         FROM purchase_order_item poi
         JOIN purchase_order po ON poi.order_id = po.id
         JOIN supplier s ON po.supplier_id = s.id
         WHERE strftime('%Y-%m', po.order_date) = ?
         GROUP BY s.id, s.name
         ORDER BY amount DESC"
    )
    .bind(month)
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let purchase_by_supplier: Vec<serde_json::Value> = purchase_by_supplier_rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "name": row.get::<String, _>("name"),
                "amount": row.get::<f64, _>("amount"),
                "quantity": row.get::<f64, _>("quantity"),
            })
        })
        .collect();
    
    let sales_by_purchaser_rows = sqlx::query(
        "SELECT p.name, COALESCE(SUM(soi.amount), 0) as amount, COALESCE(SUM(soi.quantity), 0) as quantity
         FROM sales_order_item soi
         JOIN sales_order so ON soi.order_id = so.id
         JOIN purchaser p ON so.purchaser_id = p.id
         WHERE strftime('%Y-%m', so.order_date) = ?
         GROUP BY p.id, p.name
         ORDER BY amount DESC"
    )
    .bind(month)
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let sales_by_purchaser: Vec<serde_json::Value> = sales_by_purchaser_rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "name": row.get::<String, _>("name"),
                "amount": row.get::<f64, _>("amount"),
                "quantity": row.get::<f64, _>("quantity"),
            })
        })
        .collect();
    
    let result = serde_json::json!({
        "purchase_total": purchase_total,
        "sales_total": sales_total,
        "stock_total": stock_total,
        "profit_total": profit_total,
        "purchase_by_supplier": purchase_by_supplier,
        "sales_by_purchaser": sales_by_purchaser,
    });
    
    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

async fn api_query_category_stats(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or("");
    let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    
    let category_rows = sqlx::query(
        "SELECT pc.id, pc.name as category_name,
                COALESCE((SELECT SUM(poi.quantity) FROM purchase_order_item poi 
                          JOIN purchase_order po ON poi.order_id = po.id
                          JOIN product pr ON poi.product_id = pr.id
                          WHERE pr.category_id = pc.id AND po.order_date >= ? AND po.order_date <= ?), 0) as purchase_quantity,
                COALESCE((SELECT SUM(poi.amount) FROM purchase_order_item poi 
                          JOIN purchase_order po ON poi.order_id = po.id
                          JOIN product pr ON poi.product_id = pr.id
                          WHERE pr.category_id = pc.id AND po.order_date >= ? AND po.order_date <= ?), 0) as purchase_amount,
                COALESCE((SELECT SUM(soi.quantity) FROM sales_order_item soi 
                          JOIN sales_order so ON soi.order_id = so.id
                          JOIN product pr ON soi.product_id = pr.id
                          WHERE pr.category_id = pc.id AND so.order_date >= ? AND so.order_date <= ?), 0) as sales_quantity,
                COALESCE((SELECT SUM(soi.amount) FROM sales_order_item soi 
                          JOIN sales_order so ON soi.order_id = so.id
                          JOIN product pr ON soi.product_id = pr.id
                          WHERE pr.category_id = pc.id AND so.order_date >= ? AND so.order_date <= ?), 0) as sales_amount,
                COALESCE((SELECT SUM(i.quantity) FROM inventory i JOIN product pr ON i.product_id = pr.id WHERE pr.category_id = pc.id), 0) as stock_quantity,
                COALESCE((SELECT SUM(i.quantity * pr.selling_price) FROM inventory i JOIN product pr ON i.product_id = pr.id WHERE pr.category_id = pc.id), 0) as stock_amount
         FROM category pc
         WHERE pc.entity_type = 'product' AND pc.parent_id IS NULL
         ORDER BY pc.id"
    )
    .bind(start_date)
    .bind(end_date)
    .bind(start_date)
    .bind(end_date)
    .bind(start_date)
    .bind(end_date)
    .bind(start_date)
    .bind(end_date)
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let result: Vec<serde_json::Value> = category_rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "category_name": row.get::<String, _>("category_name"),
                "purchase_quantity": row.get::<f64, _>("purchase_quantity"),
                "purchase_amount": row.get::<f64, _>("purchase_amount"),
                "sales_quantity": row.get::<f64, _>("sales_quantity"),
                "sales_amount": row.get::<f64, _>("sales_amount"),
                "stock_quantity": row.get::<f64, _>("stock_quantity"),
                "stock_amount": row.get::<f64, _>("stock_amount"),
            })
        })
        .collect();
    
    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

async fn api_query_document_summary(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let month = params.get("month").map(|s| s.as_str()).unwrap_or("");
    
    let document_rows = sqlx::query(
        "SELECT strftime('%Y-%m', po.order_date) as month,
                COUNT(DISTINCT po.id) as purchase_count,
                COALESCE(SUM(po.total_amount), 0) as purchase_amount,
                COALESCE((SELECT COUNT(DISTINCT so.id) FROM sales_order so WHERE strftime('%Y-%m', so.order_date) = strftime('%Y-%m', po.order_date)), 0) as sales_count,
                COALESCE((SELECT SUM(so.total_amount) FROM sales_order so WHERE strftime('%Y-%m', so.order_date) = strftime('%Y-%m', po.order_date)), 0) as sales_amount
         FROM purchase_order po
         WHERE strftime('%Y-%m', po.order_date) = ?
         GROUP BY strftime('%Y-%m', po.order_date)
         UNION ALL
         SELECT strftime('%Y-%m', so.order_date) as month,
                COALESCE((SELECT COUNT(DISTINCT po.id) FROM purchase_order po WHERE strftime('%Y-%m', po.order_date) = strftime('%Y-%m', so.order_date)), 0) as purchase_count,
                COALESCE((SELECT SUM(po.total_amount) FROM purchase_order po WHERE strftime('%Y-%m', po.order_date) = strftime('%Y-%m', so.order_date)), 0) as purchase_amount,
                COUNT(DISTINCT so.id) as sales_count,
                COALESCE(SUM(so.total_amount), 0) as sales_amount
         FROM sales_order so
         WHERE strftime('%Y-%m', so.order_date) = ?
         GROUP BY strftime('%Y-%m', so.order_date)"
    )
    .bind(month)
    .bind(month)
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let mut month_map: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
    
    for row in &document_rows {
        let m = row.get::<String, _>("month");
        let purchase_count: i64 = row.get("purchase_count");
        let purchase_amount: f64 = row.get("purchase_amount");
        let sales_count: i64 = row.get("sales_count");
        let sales_amount: f64 = row.get("sales_amount");
        
        if let Some(existing) = month_map.get_mut(&m) {
            let current_purchase_count = existing["purchase_count"].as_i64().unwrap_or(0);
            let current_purchase_amount = existing["purchase_amount"].as_f64().unwrap_or(0.0);
            let current_sales_count = existing["sales_count"].as_i64().unwrap_or(0);
            let current_sales_amount = existing["sales_amount"].as_f64().unwrap_or(0.0);
            
            existing["purchase_count"] = serde_json::json!(std::cmp::max(current_purchase_count, purchase_count));
            existing["purchase_amount"] = serde_json::json!(current_purchase_amount.max(purchase_amount));
            existing["sales_count"] = serde_json::json!(std::cmp::max(current_sales_count, sales_count));
            existing["sales_amount"] = serde_json::json!(current_sales_amount.max(sales_amount));
        } else {
            month_map.insert(m.clone(), serde_json::json!({
                "month": m,
                "purchase_count": purchase_count,
                "purchase_amount": purchase_amount,
                "sales_count": sales_count,
                "sales_amount": sales_amount,
            }));
        }
    }
    
    let mut result: Vec<serde_json::Value> = month_map.values().cloned().collect();
    result.sort_by(|a, b| a["month"].as_str().unwrap_or("").cmp(b["month"].as_str().unwrap_or("")));
    
    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

async fn api_sales_order_create(Json(req): Json<SalesOrderReq>) -> impl IntoResponse {
    let result = sqlx::query(
        "INSERT INTO sales_order(purchaser_id, order_no, order_date, total_amount, discount_rate, amount_reduction, final_amount, warehouse_id, warehouse_name, remark) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(req.purchaser_id)
    .bind(&req.order_no)
    .bind(&req.order_date)
    .bind(req.total_amount)
    .bind(req.discount_rate)
    .bind(req.amount_reduction)
    .bind(req.final_amount)
    .bind(req.warehouse_id)
    .bind(&req.warehouse_name)
    .bind(&req.remark)
    .execute(pool())
    .await;
    
    match result {
        Ok(res) => {
            let order_id = res.last_insert_rowid();
            for item in req.items {
                sqlx::query(
                    "INSERT INTO sales_order_item(order_id, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, base_quantity, amount, supplier_id, supplier_name, remark) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                )
                .bind(order_id)
                .bind(item.product_id)
                .bind(&item.product_name)
                .bind(&item.alias1)
                .bind(&item.alias2)
                .bind(&item.spec)
                .bind(&item.unit)
                .bind(item.unit_price)
                .bind(item.quantity)
                .bind(item.base_quantity.unwrap_or(0.0))
                .bind(item.amount)
                .bind(item.supplier_id)
                .bind(&item.supplier_name)
                .bind(&item.remark)
                .execute(pool())
                .await
                .ok();
            }
            StatusCode::OK
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn api_sales_order_list(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let keyword_pattern = parse_keyword_pattern(&params);
    
    let page: i64 = params.get("page").and_then(|s| s.parse().ok()).unwrap_or(1);
    let page_size: i64 = params.get("page_size").and_then(|s| s.parse().ok()).unwrap_or(20);
    let offset = (page - 1) * page_size;
    
    let total_rows = sqlx::query(
        "SELECT COUNT(*) as count FROM sales_order so JOIN purchaser p ON so.purchaser_id = p.id 
         WHERE so.order_no LIKE ? OR p.name LIKE ? OR so.order_date LIKE ?"
    )
    .bind(&keyword_pattern)
    .bind(&keyword_pattern)
    .bind(&keyword_pattern)
    .fetch_one(pool())
    .await
    .unwrap();
    let total: i64 = total_rows.get("count");
    
    let rows = sqlx::query(
        "SELECT so.id, so.order_no, so.order_date, so.total_amount, so.discount_rate, so.amount_reduction, so.final_amount, so.status, so.remark, so.warehouse_id, so.warehouse_name, p.name as purchaser_name 
         FROM sales_order so JOIN purchaser p ON so.purchaser_id = p.id 
         WHERE so.order_no LIKE ? OR p.name LIKE ? OR so.order_date LIKE ?
         ORDER BY so.id DESC LIMIT ? OFFSET ?"
    )
    .bind(&keyword_pattern)
    .bind(&keyword_pattern)
    .bind(&keyword_pattern)
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let orders: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| serde_json::json!({
            "id": row.get::<i64, _>("id"),
            "order_no": row.get::<String, _>("order_no"),
            "order_date": row.get::<String, _>("order_date"),
            "total_amount": row.get::<f64, _>("total_amount"),
            "discount_rate": row.get::<f64, _>("discount_rate"),
            "amount_reduction": row.get::<f64, _>("amount_reduction"),
            "final_amount": row.get::<f64, _>("final_amount"),
            "warehouse_id": row.get::<i64, _>("warehouse_id"),
            "warehouse_name": row.get::<Option<String>, _>("warehouse_name"),
            "status": row.get::<String, _>("status"),
            "remark": row.get::<Option<String>, _>("remark"),
            "purchaser_name": row.get::<String, _>("purchaser_name"),
        }))
        .collect();
    
    let result = serde_json::json!({
        "data": orders,
        "page": page,
        "page_size": page_size,
        "total": total,
        "total_pages": (total + page_size - 1) / page_size
    });
    
    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

async fn api_sales_order_accept(Path(id): Path<i64>) -> impl IntoResponse {
    let order_row = sqlx::query(
        "SELECT so.id, so.purchaser_id, so.order_no, so.order_date, so.total_amount, so.discount_rate, so.final_amount, so.remark,
                p.name as purchaser_name, p.address as purchaser_address
         FROM sales_order so JOIN purchaser p ON so.purchaser_id = p.id WHERE so.id = ?"
    )
    .bind(id)
    .fetch_optional(pool())
    .await
    .unwrap_or(None);
    
    if order_row.is_none() {
        return (StatusCode::NOT_FOUND, "订单不存在".to_string());
    }
    
    let row = order_row.unwrap();
    
    let item_rows = sqlx::query(
        "SELECT soi.id, soi.product_id, soi.product_name, soi.alias1, soi.alias2, soi.spec, soi.unit, soi.unit_price, soi.quantity, soi.amount, soi.remark
         FROM sales_order_item soi WHERE soi.order_id = ?"
    )
    .bind(id)
    .fetch_all(pool())
    .await
    .unwrap_or_default();

    let items: Vec<serde_json::Value> = item_rows
        .iter()
        .map(|r| {
            let food_name = r.get::<Option<String>, _>("alias2").unwrap_or_default();
            let unit = r.get::<Option<String>, _>("unit").unwrap_or_default();
            let remark = r.get::<Option<String>, _>("remark").unwrap_or_default();
            serde_json::json!({
                "id": r.get::<i64, _>("id"),
                "product_id": r.get::<i64, _>("product_id"),
                "product_name": r.get::<String, _>("product_name"),
                "food_name": if food_name.is_empty() { r.get::<String, _>("product_name") } else { food_name },
                "alias2": r.get::<Option<String>, _>("alias2"),
                "spec": unit,
                "unit": unit,
                "unit_price": r.get::<f64, _>("unit_price"),
                "quantity": r.get::<f64, _>("quantity"),
                "amount": r.get::<f64, _>("amount"),
                "remark": remark,
            })
        })
        .collect();
    
    let supplier_name = "湖南食全味美餐饮管理有限公司".to_string();
    let car_no = "湘A·NY360".to_string();
    
    let accept_data = serde_json::json!({
        "id": row.get::<i64, _>("id"),
        "order_no": row.get::<String, _>("order_no"),
        "order_date": row.get::<String, _>("order_date"),
        "total_amount": row.get::<f64, _>("total_amount"),
        "discount_rate": row.get::<f64, _>("discount_rate"),
        "final_amount": row.get::<f64, _>("final_amount"),
        "remark": row.get::<Option<String>, _>("remark"),
        "purchaser_name": row.get::<String, _>("purchaser_name"),
        "purchaser_address": row.get::<Option<String>, _>("purchaser_address"),
        "supplier_name": supplier_name,
        "car_no": car_no,
        "items": items,
    });
    
    (StatusCode::OK, serde_json::to_string(&accept_data).unwrap())
}

fn get_category_sort_key(category_name: &str, parent_name: &str) -> i64 {
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

async fn api_sales_order_accept_excel(Path(id): Path<i64>) -> impl IntoResponse {
    let order_row = sqlx::query(
        "SELECT so.id, so.purchaser_id, so.order_no, so.order_date, so.total_amount, so.discount_rate, so.final_amount, so.remark,
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
    let total_amount = row.get::<f64, _>("total_amount");
    let discount_rate = row.get::<f64, _>("discount_rate");
    let final_amount = row.get::<f64, _>("final_amount");
    let purchaser_name = row.get::<String, _>("purchaser_name");

    let supplier_name = "湖南食全味美餐饮管理有限公司".to_string();
    let car_no = "湘A·NY360".to_string();

    let item_rows = sqlx::query(
        "SELECT soi.id, soi.product_id, soi.product_name, soi.alias1, soi.alias2, soi.spec, soi.unit, soi.unit_price, soi.quantity, soi.amount, soi.remark,
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

    let mut items: Vec<(i64, String, String, f64, f64, f64, String)> = Vec::new();
    for r in &item_rows {
        let alias2 = r.get::<Option<String>, _>("alias2").unwrap_or_default();
        let food_name = if alias2.is_empty() {
            r.get::<String, _>("product_name")
        } else {
            alias2
        };
        let unit = r.get::<Option<String>, _>("unit").unwrap_or_default();
        let spec = r.get::<Option<String>, _>("spec").unwrap_or_default();
        let original_remark = r.get::<Option<String>, _>("remark").unwrap_or_default();
        let remark = if spec.is_empty() {
            original_remark
        } else if original_remark.is_empty() {
            spec
        } else {
            format!("{}; {}", spec, original_remark)
        };
        let category_name = r.get::<Option<String>, _>("category_name").unwrap_or_default();
        let parent_name = r.get::<Option<String>, _>("parent_name").unwrap_or_default();
        let sort_key = get_category_sort_key(&category_name, &parent_name);
        items.push((
            sort_key,
            food_name,
            unit,
            r.get::<f64, _>("unit_price"),
            r.get::<f64, _>("quantity"),
            r.get::<f64, _>("amount"),
            remark,
        ));
    }

    items.sort_by(|a, b| a.0.cmp(&b.0));

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

        let col_widths = [4.0, 14.0, 7.0, 7.0, 7.0, 8.0, 10.0, 7.0, 11.0, 11.0, 11.0, 14.0, 10.0];
        for (i, w) in col_widths.iter().enumerate() {
            worksheet.set_column_width(i as u16, *w)?;
        }

        let headers = [
            "序号".to_string(), "食材名称".to_string(), "规格".to_string(), "数量".to_string(), "单价".to_string(), "总价".to_string(),
            "生产日期/批号".to_string(), "保质期".to_string(), "是否有蔬菜农残检测报告单".to_string(),
            "是否有肉类检疫合格证".to_string(), "是否异常(异味、异色)".to_string(),
            "检验情况是否合格".to_string(), "备注".to_string(),
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
                worksheet.write_with_format(current_row, 3, *quantity, &cell_format)?;
                worksheet.write_with_format(current_row, 4, *unit_price, &money_format)?;
                worksheet.write_with_format(current_row, 5, *amount, &money_format)?;
                worksheet.write_with_format(current_row, 6, "", &cell_format)?;
                worksheet.write_with_format(current_row, 7, "", &cell_format)?;
                worksheet.write_with_format(current_row, 8, "□有  □无", &cell_format)?;
                worksheet.write_with_format(current_row, 9, "□有  □无", &cell_format)?;
                worksheet.write_with_format(current_row, 10, "□有  □无", &cell_format)?;
                worksheet.write_with_format(current_row, 11, "□合格  □不合格", &cell_format)?;
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

            worksheet.write_with_format(current_row, 3, total_amount, &money_format)?;
            current_row += 1;

            worksheet.merge_range(current_row, 0, current_row, 2, "下浮率：", &label_format)?;
            worksheet.merge_range(current_row, 3, current_row, 5, "", &cell_format)?;
            worksheet.write_with_format(current_row, 3, discount_rate, &percent_format)?;
            current_row += 1;

            worksheet.merge_range(current_row, 0, current_row, 2, "下浮后总价：", &label_format)?;
            worksheet.merge_range(current_row, 3, current_row, 5, "", &cell_format)?;
            worksheet.write_with_format(current_row, 3, final_amount, &money_format)?;
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
                worksheet.set_row_height(row, 20)?;
                if sig_row == 0 {
                    worksheet.merge_range(row, 0, row, 1, "食材供应人员①：", &label_fmt)?;
                    worksheet.merge_range(row, 2, row, 3, "联系方式：", &contact_fmt)?;
                    worksheet.merge_range(row, 4, row, 5, "", &cell_fmt)?;
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
            let filename = format!("验收单_{}.xlsx", order_no);
            let content_disposition = format!("attachment; filename=\"{}\"", filename);
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
                    (header::CONTENT_DISPOSITION, content_disposition.as_str()),
                ],
                buf,
            ).into_response()
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, format!("生成Excel失败: {}", e)).into_response()
        }
    }
}

async fn api_sales_order_sort_items() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT soi.id, soi.product_id, soi.product_name, soi.unit, soi.unit_price, soi.quantity, soi.amount, soi.remark,
                p.name as purchaser_name, so.order_no
         FROM sales_order_item soi 
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         WHERE so.status IN ('pending', 'sorting')
         ORDER BY soi.product_name"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let mut items_map: std::collections::HashMap<i64, serde_json::Value> = std::collections::HashMap::new();
    
    for r in &rows {
        let product_id = r.get::<i64, _>("product_id");
        let existing = items_map.entry(product_id).or_insert_with(|| {
            serde_json::json!({
                "id": r.get::<i64, _>("id"),
                "product_id": product_id,
                "product_name": r.get::<String, _>("product_name"),
                "unit": r.get::<Option<String>, _>("unit").unwrap_or_default(),
                "unit_price": r.get::<f64, _>("unit_price"),
                "total_quantity": 0.0,
                "total_amount": 0.0,
                "purchaser_names": Vec::new() as Vec<String>,
                "order_nos": Vec::new() as Vec<String>,
                "remarks": Vec::new() as Vec<String>,
            })
        });
        
        existing["total_quantity"] = serde_json::json!(existing["total_quantity"].as_f64().unwrap_or(0.0) + r.get::<f64, _>("quantity"));
        existing["total_amount"] = serde_json::json!(existing["total_amount"].as_f64().unwrap_or(0.0) + r.get::<f64, _>("amount"));
        
        let purchaser_name = r.get::<Option<String>, _>("purchaser_name").unwrap_or_default();
        let purchasers = existing["purchaser_names"].as_array_mut().unwrap();
        if !purchasers.contains(&serde_json::json!(purchaser_name)) {
            purchasers.push(serde_json::json!(purchaser_name));
        }
        
        let order_no = r.get::<Option<String>, _>("order_no").unwrap_or_default();
        let orders = existing["order_nos"].as_array_mut().unwrap();
        if !orders.contains(&serde_json::json!(order_no)) {
            orders.push(serde_json::json!(order_no));
        }
        
        let remark = r.get::<Option<String>, _>("remark").unwrap_or_default();
        if !remark.is_empty() {
            let remarks = existing["remarks"].as_array_mut().unwrap();
            if !remarks.contains(&serde_json::json!(remark)) {
                remarks.push(serde_json::json!(remark));
            }
        }
    }
    
    let items: Vec<serde_json::Value> = items_map.values()
        .map(|v| {
            let mut v = v.clone();
            let purchasers: Vec<String> = v["purchaser_names"].as_array().unwrap()
                .iter()
                .map(|s| s.as_str().unwrap_or_default().to_string())
                .collect();
            v["purchaser_names"] = serde_json::json!(purchasers.join("; "));
            
            let orders: Vec<String> = v["order_nos"].as_array().unwrap()
                .iter()
                .map(|s| s.as_str().unwrap_or_default().to_string())
                .collect();
            v["order_nos"] = serde_json::json!(orders.join("; "));
            
            let remarks: Vec<String> = v["remarks"].as_array().unwrap()
                .iter()
                .map(|s| s.as_str().unwrap_or_default().to_string())
                .collect();
            v["remarks"] = serde_json::json!(remarks.join("; "));
            v
        })
        .collect();
    
    (StatusCode::OK, serde_json::to_string(&items).unwrap())
}

async fn api_sales_order_sort_items_excel() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT soi.id, soi.product_id, soi.product_name, soi.unit, soi.unit_price, soi.quantity, soi.amount, soi.remark,
                p.name as purchaser_name, so.order_no
         FROM sales_order_item soi 
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         WHERE so.status IN ('pending', 'sorting')
         ORDER BY soi.product_name"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let mut items_map: std::collections::HashMap<i64, serde_json::Value> = std::collections::HashMap::new();
    
    for r in &rows {
        let product_id = r.get::<i64, _>("product_id");
        let existing = items_map.entry(product_id).or_insert_with(|| {
            serde_json::json!({
                "id": r.get::<i64, _>("id"),
                "product_id": product_id,
                "product_name": r.get::<String, _>("product_name"),
                "unit": r.get::<Option<String>, _>("unit").unwrap_or_default(),
                "unit_price": r.get::<f64, _>("unit_price"),
                "total_quantity": 0.0,
                "total_amount": 0.0,
                "purchaser_names": Vec::new() as Vec<String>,
                "order_nos": Vec::new() as Vec<String>,
                "remarks": Vec::new() as Vec<String>,
            })
        });
        
        existing["total_quantity"] = serde_json::json!(existing["total_quantity"].as_f64().unwrap_or(0.0) + r.get::<f64, _>("quantity"));
        existing["total_amount"] = serde_json::json!(existing["total_amount"].as_f64().unwrap_or(0.0) + r.get::<f64, _>("amount"));
        
        let purchaser_name = r.get::<Option<String>, _>("purchaser_name").unwrap_or_default();
        let purchasers = existing["purchaser_names"].as_array_mut().unwrap();
        if !purchasers.contains(&serde_json::json!(purchaser_name)) {
            purchasers.push(serde_json::json!(purchaser_name));
        }
        
        let order_no = r.get::<Option<String>, _>("order_no").unwrap_or_default();
        let orders = existing["order_nos"].as_array_mut().unwrap();
        if !orders.contains(&serde_json::json!(order_no)) {
            orders.push(serde_json::json!(order_no));
        }
        
        let remark = r.get::<Option<String>, _>("remark").unwrap_or_default();
        if !remark.is_empty() {
            let remarks = existing["remarks"].as_array_mut().unwrap();
            if !remarks.contains(&serde_json::json!(remark)) {
                remarks.push(serde_json::json!(remark));
            }
        }
    }
    
    let items: Vec<serde_json::Value> = items_map.values()
        .map(|v| {
            let mut v = v.clone();
            let purchasers: Vec<String> = v["purchaser_names"].as_array().unwrap()
                .iter()
                .map(|s| s.as_str().unwrap_or_default().to_string())
                .collect();
            v["purchaser_names"] = serde_json::json!(purchasers.join("; "));
            
            let orders: Vec<String> = v["order_nos"].as_array().unwrap()
                .iter()
                .map(|s| s.as_str().unwrap_or_default().to_string())
                .collect();
            v["order_nos"] = serde_json::json!(orders.join("; "));
            
            let remarks: Vec<String> = v["remarks"].as_array().unwrap()
                .iter()
                .map(|s| s.as_str().unwrap_or_default().to_string())
                .collect();
            v["remarks"] = serde_json::json!(remarks.join("; "));
            v
        })
        .collect();

    let excel_result: Result<Vec<u8>, XlsxError> = (|| {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();

        worksheet.set_landscape();
        worksheet.set_margins(0.2, 0.2, 0.2, 0.2, 0.2, 0.2);
        worksheet.set_print_center_vertically(false);
        worksheet.set_print_center_horizontally(true);

        let title_format = Format::new()
            .set_bold()
            .set_font_size(14)
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
            .set_border(FormatBorder::Thin)
            .set_align(FormatAlign::VerticalCenter);

        let cell_left_format = Format::new()
            .set_font_size(10)
            .set_border(FormatBorder::Thin)
            .set_align(FormatAlign::Left)
            .set_align(FormatAlign::VerticalCenter);

        let price_format = Format::new()
            .set_font_size(10)
            .set_border(FormatBorder::Thin)
            .set_align(FormatAlign::Right)
            .set_num_format("0.00");

        worksheet.merge_range(0, 0, 0, 6, "采购分拣清单", &title_format)?;
        worksheet.set_row_height(0, 28)?;

        let headers = ["序号", "商品名称", "单位", "单价", "数量", "金额", "采购单位"];
        let mut current_row = 2;
        for (i, h) in headers.iter().enumerate() {
            worksheet.write_with_format(current_row, i as u16, *h, &header_format)?;
        }
        current_row += 1;

        let mut index = 1;
        for item in &items {
            worksheet.write_with_format(current_row, 0, index as f64, &cell_format)?;
            worksheet.write_with_format(current_row, 1, item["product_name"].as_str().unwrap_or_default(), &cell_left_format)?;
            worksheet.write_with_format(current_row, 2, item["unit"].as_str().unwrap_or_default(), &cell_format)?;
            worksheet.write_with_format(current_row, 3, item["unit_price"].as_f64().unwrap_or(0.0), &price_format)?;
            worksheet.write_with_format(current_row, 4, item["total_quantity"].as_f64().unwrap_or(0.0), &cell_format)?;
            worksheet.write_with_format(current_row, 5, item["total_amount"].as_f64().unwrap_or(0.0), &price_format)?;
            worksheet.write_with_format(current_row, 6, item["purchaser_names"].as_str().unwrap_or_default(), &cell_left_format)?;
            current_row += 1;
            index += 1;
        }

        worksheet.write_with_format(current_row, 0, "合计", &header_format)?;
        worksheet.merge_range(current_row, 0, current_row, 4, "", &header_format)?;
        let total_amount: f64 = items.iter().map(|item| item["total_amount"].as_f64().unwrap_or(0.0)).sum();
        worksheet.write_with_format(current_row, 5, total_amount, &price_format)?;

        worksheet.set_column_width(0, 10)?;
        worksheet.set_column_width(1, 30)?;
        worksheet.set_column_width(2, 12)?;
        worksheet.set_column_width(3, 12)?;
        worksheet.set_column_width(4, 12)?;
        worksheet.set_column_width(5, 12)?;
        worksheet.set_column_width(6, 30)?;

        let buf = workbook.save_to_buffer()?;
        Ok(buf)
    })();

    match excel_result {
        Ok(buf) => {
            let headers = [
                ("Content-Type", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
                ("Content-Disposition", "attachment; filename=\"采购分拣清单.xlsx\""),
            ];
            (StatusCode::OK, headers, buf).into_response()
        },
        Err(e) => {
            eprintln!("Excel export error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "导出失败").into_response()
        },
    }
}

async fn api_sales_order_sort_items_by_category() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT soi.id as item_id, soi.product_id, soi.product_name, soi.unit, soi.unit_price, soi.quantity, soi.amount, soi.remark,
                p.name as purchaser_name, so.order_no, c.name as category_name
         FROM sales_order_item soi 
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         LEFT JOIN product pr ON soi.product_id = pr.id
         LEFT JOIN category c ON pr.category_id = c.id
         WHERE so.status IN ('pending', 'sorting')
         ORDER BY c.name, p.name, soi.product_name"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let mut category_map: std::collections::HashMap<String, std::collections::HashMap<String, Vec<serde_json::Value>>> = std::collections::HashMap::new();
    
    for r in &rows {
        let category_name = r.get::<Option<String>, _>("category_name").unwrap_or_else(|| "未分类".to_string());
        let purchaser_name = r.get::<String, _>("purchaser_name");
        
        let purchaser_map = category_map.entry(category_name).or_insert_with(std::collections::HashMap::new);
        let purchaser_items = purchaser_map.entry(purchaser_name).or_insert_with(Vec::new);
        
        purchaser_items.push(serde_json::json!({
            "item_id": r.get::<i64, _>("item_id"),
            "product_id": r.get::<i64, _>("product_id"),
            "product_name": r.get::<String, _>("product_name"),
            "unit": r.get::<Option<String>, _>("unit").unwrap_or_default(),
            "unit_price": r.get::<f64, _>("unit_price"),
            "quantity": r.get::<f64, _>("quantity"),
            "amount": r.get::<f64, _>("amount"),
            "remark": r.get::<Option<String>, _>("remark").unwrap_or_default(),
            "order_no": r.get::<Option<String>, _>("order_no").unwrap_or_default(),
        }));
    }
    
    let mut result: Vec<serde_json::Value> = Vec::new();
    for (category_name, purchaser_map) in category_map {
        let mut purchasers: Vec<serde_json::Value> = Vec::new();
        for (purchaser_name, items) in purchaser_map {
            let total_qty: f64 = items.iter().map(|item| item["quantity"].as_f64().unwrap_or(0.0)).sum();
            purchasers.push(serde_json::json!({
                "purchaser_name": purchaser_name,
                "items": items,
                "total_quantity": total_qty,
            }));
        }
        purchasers.sort_by(|a, b| a["purchaser_name"].as_str().unwrap_or("").cmp(b["purchaser_name"].as_str().unwrap_or("")));
        
        let total_qty: f64 = purchasers.iter().map(|p| p["total_quantity"].as_f64().unwrap_or(0.0)).sum();
        result.push(serde_json::json!({
            "category_name": category_name,
            "purchasers": purchasers,
            "total_quantity": total_qty,
        }));
    }
    
    result.sort_by(|a, b| a["category_name"].as_str().unwrap_or("").cmp(b["category_name"].as_str().unwrap_or("")));
    
    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

async fn api_sales_order_sort_items_by_category_excel() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT soi.product_id, soi.product_name, soi.unit, soi.quantity, soi.remark,
                p.name as purchaser_name, so.order_no, c.name as category_name
         FROM sales_order_item soi 
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         LEFT JOIN product pr ON soi.product_id = pr.id
         LEFT JOIN category c ON pr.category_id = c.id
         WHERE so.status IN ('pending', 'sorting')
         ORDER BY c.name, p.name, soi.product_name"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let mut category_map: std::collections::HashMap<String, std::collections::HashMap<String, Vec<serde_json::Value>>> = std::collections::HashMap::new();
    
    for r in &rows {
        let category_name = r.get::<Option<String>, _>("category_name").unwrap_or_else(|| "未分类".to_string());
        let purchaser_name = r.get::<String, _>("purchaser_name");
        
        let purchaser_map = category_map.entry(category_name).or_insert_with(std::collections::HashMap::new);
        let purchaser_items = purchaser_map.entry(purchaser_name).or_insert_with(Vec::new);
        
        purchaser_items.push(serde_json::json!({
            "product_id": r.get::<i64, _>("product_id"),
            "product_name": r.get::<String, _>("product_name"),
            "unit": r.get::<Option<String>, _>("unit").unwrap_or_default(),
            "quantity": r.get::<f64, _>("quantity"),
            "remark": r.get::<Option<String>, _>("remark").unwrap_or_default(),
            "order_no": r.get::<Option<String>, _>("order_no").unwrap_or_default(),
        }));
    }
    
    let mut result: Vec<serde_json::Value> = Vec::new();
    for (category_name, purchaser_map) in category_map {
        let mut purchasers: Vec<serde_json::Value> = Vec::new();
        for (purchaser_name, items) in purchaser_map {
            let total_qty: f64 = items.iter().map(|item| item["quantity"].as_f64().unwrap_or(0.0)).sum();
            purchasers.push(serde_json::json!({
                "purchaser_name": purchaser_name,
                "items": items,
                "total_quantity": total_qty,
            }));
        }
        purchasers.sort_by(|a, b| a["purchaser_name"].as_str().unwrap_or("").cmp(b["purchaser_name"].as_str().unwrap_or("")));
        
        let total_qty: f64 = purchasers.iter().map(|p| p["total_quantity"].as_f64().unwrap_or(0.0)).sum();
        result.push(serde_json::json!({
            "category_name": category_name,
            "purchasers": purchasers,
            "total_quantity": total_qty,
        }));
    }
    
    result.sort_by(|a, b| a["category_name"].as_str().unwrap_or("").cmp(b["category_name"].as_str().unwrap_or("")));

    let excel_result: Result<Vec<u8>, XlsxError> = (|| {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();

        worksheet.set_landscape();
        worksheet.set_margins(0.2, 0.2, 0.2, 0.2, 0.2, 0.2);
        worksheet.set_print_center_vertically(false);
        worksheet.set_print_center_horizontally(true);

        let title_format = Format::new()
            .set_bold()
            .set_font_size(14)
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
            .set_border(FormatBorder::Thin);

        let cat_hunxian_format = Format::new()
            .set_bold()
            .set_font_size(12)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#DC2626")
            .set_font_color("#FFFFFF");

        let cat_xianshu_format = Format::new()
            .set_bold()
            .set_font_size(12)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#16A34A")
            .set_font_color("#FFFFFF");

        let cat_liangyou_format = Format::new()
            .set_bold()
            .set_font_size(12)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#1D4ED8")
            .set_font_color("#FFFFFF");

        let cat_douzhi_format = Format::new()
            .set_bold()
            .set_font_size(12)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#CA8A04")
            .set_font_color("#FFFFFF");

        let cat_fenmian_format = Format::new()
            .set_bold()
            .set_font_size(12)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#64748B")
            .set_font_color("#FFFFFF");

        let cat_shuiguo_format = Format::new()
            .set_bold()
            .set_font_size(12)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#EA580C")
            .set_font_color("#FFFFFF");

        let cat_other_format = Format::new()
            .set_bold()
            .set_font_size(12)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#6B7280")
            .set_font_color("#FFFFFF");

        let purchaser_format = Format::new()
            .set_bold()
            .set_font_size(11)
            .set_align(FormatAlign::Left)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#E5E7EB")
            .set_font_color("#374151");

        let col_widths = [6.0, 18.0, 8.0, 10.0, 20.0, 20.0];
        for (i, w) in col_widths.iter().enumerate() {
            worksheet.set_column_width(i as u16, *w)?;
        }

        let mut current_row = 0;
        worksheet.merge_range(current_row, 0, current_row, 5, "采购分拣清单（按分类）", &title_format)?;
        worksheet.set_row_height(current_row, 28)?;
        current_row += 2;

        let headers = ["序号", "商品名称", "单位", "数量", "备注", "采购单位"];
        for (i, header) in headers.iter().enumerate() {
            worksheet.write_with_format(current_row, i as u16, *header, &header_format)?;
        }
        current_row += 1;

        let mut seq = 1;
        for cat in &result {
            let cat_name = cat["category_name"].as_str().unwrap_or("未分类");
            
            let cat_format = match () {
                _ if cat_name.contains("荤鲜") => &cat_hunxian_format,
                _ if cat_name.contains("鲜蔬") => &cat_xianshu_format,
                _ if cat_name.contains("粮油") || cat_name.contains("干调") => &cat_liangyou_format,
                _ if cat_name.contains("豆制品") => &cat_douzhi_format,
                _ if cat_name.contains("粉面") => &cat_fenmian_format,
                _ if cat_name.contains("水果") => &cat_shuiguo_format,
                _ => &cat_other_format,
            };

            let cat_title = format!("【{}】", cat_name);
            worksheet.merge_range(current_row, 0, current_row, 5, cat_title.as_str(), cat_format)?;
            worksheet.set_row_height(current_row, 22)?;
            current_row += 1;

            if let Some(purchasers) = cat["purchasers"].as_array() {
                for purchaser in purchasers {
                    let purchaser_name = purchaser["purchaser_name"].as_str().unwrap_or("");
                    let purchaser_title = format!("├── {}", purchaser_name);
                    worksheet.merge_range(current_row, 0, current_row, 5, purchaser_title.as_str(), &purchaser_format)?;
                    worksheet.set_row_height(current_row, 20)?;
                    current_row += 1;

                    if let Some(items) = purchaser["items"].as_array() {
                        for item in items {
                            let product_name = item["product_name"].as_str().unwrap_or("");
                            let unit = item["unit"].as_str().unwrap_or("");
                            let quantity = item["quantity"].as_f64().unwrap_or(0.0);
                            let remark = item["remark"].as_str().unwrap_or("");

                            worksheet.write_with_format(current_row, 0, seq as f64, &cell_format)?;
                            worksheet.write_with_format(current_row, 1, product_name, &cell_left_format)?;
                            worksheet.write_with_format(current_row, 2, unit, &cell_format)?;
                            worksheet.write_with_format(current_row, 3, quantity, &cell_format)?;
                            worksheet.write_with_format(current_row, 4, remark, &cell_left_format)?;
                            worksheet.write_with_format(current_row, 5, purchaser_name, &cell_left_format)?;
                            current_row += 1;
                            seq += 1;
                        }
                    }
                }
            }
            
            current_row += 1;
        }

        let buf = workbook.save_to_buffer()?;
        Ok(buf)
    })();

    match excel_result {
        Ok(buf) => {
            let headers = [
                ("Content-Type", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
                ("Content-Disposition", "attachment; filename=\"采购分拣清单_按分类.xlsx\""),
            ];
            (StatusCode::OK, headers, buf).into_response()
        }
        Err(e) => {
            eprintln!("Excel export error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "导出失败").into_response()
        }
    }
}

async fn api_sales_order_sort_items_by_supplier() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT soi.id as item_id, soi.product_id, soi.product_name, soi.unit, soi.unit_price, soi.quantity, soi.amount, soi.remark,
                soi.supplier_id, s.name as supplier_name, p.name as purchaser_name, so.order_no
         FROM sales_order_item soi 
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         LEFT JOIN supplier s ON soi.supplier_id = s.id
         WHERE so.status IN ('pending', 'sorting')
         ORDER BY s.name, p.name, soi.product_name"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let mut supplier_map: std::collections::HashMap<String, std::collections::HashMap<String, Vec<serde_json::Value>>> = std::collections::HashMap::new();
    
    for r in &rows {
        let supplier_name = r.get::<Option<String>, _>("supplier_name").unwrap_or_else(|| {
            let supplier_id = r.get::<i64, _>("supplier_id");
            if supplier_id == 0 { "未分配供应商".to_string() } else { format!("供应商{}", supplier_id) }
        });
        let purchaser_name = r.get::<String, _>("purchaser_name");
        
        let purchaser_map = supplier_map.entry(supplier_name).or_insert_with(std::collections::HashMap::new);
        let purchaser_items = purchaser_map.entry(purchaser_name).or_insert_with(Vec::new);
        
        purchaser_items.push(serde_json::json!({
            "item_id": r.get::<i64, _>("item_id"),
            "product_id": r.get::<i64, _>("product_id"),
            "product_name": r.get::<String, _>("product_name"),
            "unit": r.get::<Option<String>, _>("unit").unwrap_or_default(),
            "unit_price": r.get::<f64, _>("unit_price"),
            "quantity": r.get::<f64, _>("quantity"),
            "amount": r.get::<f64, _>("amount"),
            "remark": r.get::<Option<String>, _>("remark").unwrap_or_default(),
            "order_no": r.get::<Option<String>, _>("order_no").unwrap_or_default(),
        }));
    }
    
    let mut result: Vec<serde_json::Value> = Vec::new();
    for (supplier_name, purchaser_map) in supplier_map {
        let mut purchasers: Vec<serde_json::Value> = Vec::new();
        for (purchaser_name, items) in purchaser_map {
            let total_qty: f64 = items.iter().map(|item| item["quantity"].as_f64().unwrap_or(0.0)).sum();
            purchasers.push(serde_json::json!({
                "purchaser_name": purchaser_name,
                "items": items,
                "total_quantity": total_qty,
            }));
        }
        purchasers.sort_by(|a, b| a["purchaser_name"].as_str().unwrap_or("").cmp(b["purchaser_name"].as_str().unwrap_or("")));
        
        let total_qty: f64 = purchasers.iter().map(|p| p["total_quantity"].as_f64().unwrap_or(0.0)).sum();
        result.push(serde_json::json!({
            "supplier_name": supplier_name,
            "purchasers": purchasers,
            "total_quantity": total_qty,
        }));
    }
    
    result.sort_by(|a, b| a["supplier_name"].as_str().unwrap_or("").cmp(b["supplier_name"].as_str().unwrap_or("")));
    
    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

async fn api_sales_order_sort_items_by_supplier_excel() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT soi.product_id, soi.product_name, soi.unit, soi.quantity, soi.remark,
                soi.supplier_id, s.name as supplier_name, p.name as purchaser_name, so.order_no
         FROM sales_order_item soi 
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         LEFT JOIN supplier s ON soi.supplier_id = s.id
         WHERE so.status IN ('pending', 'sorting')
         ORDER BY s.name, p.name, soi.product_name"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let mut supplier_map: std::collections::HashMap<String, std::collections::HashMap<String, Vec<serde_json::Value>>> = std::collections::HashMap::new();
    
    for r in &rows {
        let supplier_name = r.get::<Option<String>, _>("supplier_name").unwrap_or_else(|| {
            let supplier_id = r.get::<i64, _>("supplier_id");
            if supplier_id == 0 { "未分配供应商".to_string() } else { format!("供应商{}", supplier_id) }
        });
        let purchaser_name = r.get::<String, _>("purchaser_name");
        
        let purchaser_map = supplier_map.entry(supplier_name).or_insert_with(std::collections::HashMap::new);
        let purchaser_items = purchaser_map.entry(purchaser_name).or_insert_with(Vec::new);
        
        purchaser_items.push(serde_json::json!({
            "product_id": r.get::<i64, _>("product_id"),
            "product_name": r.get::<String, _>("product_name"),
            "unit": r.get::<Option<String>, _>("unit").unwrap_or_default(),
            "quantity": r.get::<f64, _>("quantity"),
            "remark": r.get::<Option<String>, _>("remark").unwrap_or_default(),
            "order_no": r.get::<Option<String>, _>("order_no").unwrap_or_default(),
        }));
    }
    
    let mut result: Vec<serde_json::Value> = Vec::new();
    for (supplier_name, purchaser_map) in supplier_map {
        let mut purchasers: Vec<serde_json::Value> = Vec::new();
        for (purchaser_name, items) in purchaser_map {
            let total_qty: f64 = items.iter().map(|item| item["quantity"].as_f64().unwrap_or(0.0)).sum();
            purchasers.push(serde_json::json!({
                "purchaser_name": purchaser_name,
                "items": items,
                "total_quantity": total_qty,
            }));
        }
        purchasers.sort_by(|a, b| a["purchaser_name"].as_str().unwrap_or("").cmp(b["purchaser_name"].as_str().unwrap_or("")));
        
        let total_qty: f64 = purchasers.iter().map(|p| p["total_quantity"].as_f64().unwrap_or(0.0)).sum();
        result.push(serde_json::json!({
            "supplier_name": supplier_name,
            "purchasers": purchasers,
            "total_quantity": total_qty,
        }));
    }
    
    result.sort_by(|a, b| a["supplier_name"].as_str().unwrap_or("").cmp(b["supplier_name"].as_str().unwrap_or("")));

    let excel_result: Result<Vec<u8>, XlsxError> = (|| {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();

        worksheet.set_landscape();
        worksheet.set_margins(0.2, 0.2, 0.2, 0.2, 0.2, 0.2);
        worksheet.set_print_center_vertically(false);
        worksheet.set_print_center_horizontally(true);

        let title_format = Format::new()
            .set_bold()
            .set_font_size(14)
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
            .set_border(FormatBorder::Thin);

        let supplier_format = Format::new()
            .set_bold()
            .set_font_size(12)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#10B981")
            .set_font_color("#FFFFFF");

        let purchaser_format = Format::new()
            .set_bold()
            .set_font_size(11)
            .set_align(FormatAlign::Left)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#E5E7EB")
            .set_font_color("#374151");

        let col_widths = [6.0, 18.0, 8.0, 10.0, 20.0, 20.0];
        for (i, w) in col_widths.iter().enumerate() {
            worksheet.set_column_width(i as u16, *w)?;
        }

        let mut current_row = 0;
        worksheet.merge_range(current_row, 0, current_row, 5, "采购分拣清单（按供应商）", &title_format)?;
        worksheet.set_row_height(current_row, 28)?;
        current_row += 2;

        let headers = ["序号", "商品名称", "单位", "数量", "备注", "采购单位"];
        for (i, header) in headers.iter().enumerate() {
            worksheet.write_with_format(current_row, i as u16, *header, &header_format)?;
        }
        current_row += 1;

        let mut seq = 1;
        for supplier in &result {
            let supplier_name = supplier["supplier_name"].as_str().unwrap_or("未分配供应商");
            
            let supplier_title = format!("【{}】", supplier_name);
            worksheet.merge_range(current_row, 0, current_row, 5, supplier_title.as_str(), &supplier_format)?;
            worksheet.set_row_height(current_row, 22)?;
            current_row += 1;

            if let Some(purchasers) = supplier["purchasers"].as_array() {
                for purchaser in purchasers {
                    let purchaser_name = purchaser["purchaser_name"].as_str().unwrap_or("");
                    let purchaser_title = format!("├── {}", purchaser_name);
                    worksheet.merge_range(current_row, 0, current_row, 5, purchaser_title.as_str(), &purchaser_format)?;
                    worksheet.set_row_height(current_row, 20)?;
                    current_row += 1;

                    if let Some(items) = purchaser["items"].as_array() {
                        for item in items {
                            let product_name = item["product_name"].as_str().unwrap_or("");
                            let unit = item["unit"].as_str().unwrap_or("");
                            let quantity = item["quantity"].as_f64().unwrap_or(0.0);
                            let remark = item["remark"].as_str().unwrap_or("");

                            worksheet.write_with_format(current_row, 0, seq as f64, &cell_format)?;
                            worksheet.write_with_format(current_row, 1, product_name, &cell_left_format)?;
                            worksheet.write_with_format(current_row, 2, unit, &cell_format)?;
                            worksheet.write_with_format(current_row, 3, quantity, &cell_format)?;
                            worksheet.write_with_format(current_row, 4, remark, &cell_left_format)?;
                            worksheet.write_with_format(current_row, 5, purchaser_name, &cell_left_format)?;
                            current_row += 1;
                            seq += 1;
                        }
                    }
                }
            }
            
            current_row += 1;
        }

        let buf = workbook.save_to_buffer()?;
        Ok(buf)
    })();

    match excel_result {
        Ok(buf) => {
            let headers = [
                ("Content-Type", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
                ("Content-Disposition", "attachment; filename=\"采购分拣清单_按供应商.xlsx\""),
            ];
            (StatusCode::OK, headers, buf).into_response()
        }
        Err(e) => {
            eprintln!("Excel export error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "导出失败").into_response()
        }
    }
}

async fn api_sales_order_sort_items_by_purchaser() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT soi.id, soi.product_id, soi.product_name, soi.unit, soi.unit_price, soi.quantity, soi.amount, soi.remark,
                p.id as purchaser_id, p.name as purchaser_name, so.order_no
         FROM sales_order_item soi 
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         WHERE so.status IN ('pending', 'sorting')
         ORDER BY p.name, soi.product_name"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let mut purchaser_map: std::collections::HashMap<i64, serde_json::Value> = std::collections::HashMap::new();
    
    for r in &rows {
        let purchaser_id = r.get::<i64, _>("purchaser_id");
        let purchaser_name = r.get::<Option<String>, _>("purchaser_name").unwrap_or_default();
        
        let purchaser = purchaser_map.entry(purchaser_id).or_insert_with(|| {
            serde_json::json!({
                "purchaser_id": purchaser_id,
                "purchaser_name": purchaser_name,
                "items": Vec::new() as Vec<serde_json::Value>,
                "total_amount": 0.0,
                "total_quantity": 0.0,
            })
        });
        
        purchaser["total_quantity"] = serde_json::json!(purchaser["total_quantity"].as_f64().unwrap_or(0.0) + r.get::<f64, _>("quantity"));
        purchaser["total_amount"] = serde_json::json!(purchaser["total_amount"].as_f64().unwrap_or(0.0) + r.get::<f64, _>("amount"));
        
        let items = purchaser["items"].as_array_mut().unwrap();
        items.push(serde_json::json!({
            "id": r.get::<i64, _>("id"),
            "product_id": r.get::<i64, _>("product_id"),
            "product_name": r.get::<String, _>("product_name"),
            "unit": r.get::<Option<String>, _>("unit").unwrap_or_default(),
            "unit_price": r.get::<f64, _>("unit_price"),
            "quantity": r.get::<f64, _>("quantity"),
            "amount": r.get::<f64, _>("amount"),
            "order_no": r.get::<Option<String>, _>("order_no").unwrap_or_default(),
            "remark": r.get::<Option<String>, _>("remark").unwrap_or_default(),
        }));
    }
    
    let purchasers: Vec<serde_json::Value> = purchaser_map.values().cloned().collect();
    
    (StatusCode::OK, serde_json::to_string(&purchasers).unwrap())
}

async fn api_sales_order_sort_items_by_purchaser_excel() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT soi.id, soi.product_id, soi.product_name, soi.unit, soi.unit_price, soi.quantity, soi.amount, soi.remark,
                p.id as purchaser_id, p.name as purchaser_name, so.order_no,
                pc.name as category_name, pc.parent_id, pc2.name as parent_name
         FROM sales_order_item soi 
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         LEFT JOIN product pr ON soi.product_id = pr.id
         LEFT JOIN category pc ON pr.category_id = pc.id
         LEFT JOIN category pc2 ON pc.parent_id = pc2.id
         WHERE so.status IN ('pending', 'sorting')
         ORDER BY p.name, soi.product_name"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let price_rows = sqlx::query(
        "SELECT poi.product_id, MAX(poi.unit_price) as max_price, MIN(poi.unit_price) as min_price,
                (SELECT unit_price FROM purchase_order_item WHERE product_id = poi.product_id ORDER BY id DESC LIMIT 1) as latest_price
         FROM purchase_order_item poi
         GROUP BY poi.product_id"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let mut price_map: std::collections::HashMap<i64, (f64, f64, f64)> = std::collections::HashMap::new();
    for r in &price_rows {
        let product_id = r.get::<i64, _>("product_id");
        let max_price = r.get::<Option<f64>, _>("max_price").unwrap_or(0.0);
        let min_price = r.get::<Option<f64>, _>("min_price").unwrap_or(0.0);
        let latest_price = r.get::<Option<f64>, _>("latest_price").unwrap_or(0.0);
        price_map.insert(product_id, (max_price, min_price, latest_price));
    }
    
    let mut purchaser_map: std::collections::HashMap<i64, serde_json::Value> = std::collections::HashMap::new();
    
    for r in &rows {
        let purchaser_id = r.get::<i64, _>("purchaser_id");
        let purchaser_name = r.get::<Option<String>, _>("purchaser_name").unwrap_or_default();
        let product_id = r.get::<i64, _>("product_id");
        
        let purchaser = purchaser_map.entry(purchaser_id).or_insert_with(|| {
            serde_json::json!({
                "purchaser_id": purchaser_id,
                "purchaser_name": purchaser_name,
                "items": Vec::new() as Vec<serde_json::Value>,
                "total_amount": 0.0,
                "total_quantity": 0.0,
            })
        });
        
        purchaser["total_quantity"] = serde_json::json!(purchaser["total_quantity"].as_f64().unwrap_or(0.0) + r.get::<f64, _>("quantity"));
        purchaser["total_amount"] = serde_json::json!(purchaser["total_amount"].as_f64().unwrap_or(0.0) + r.get::<f64, _>("amount"));
        
        let category_name = r.get::<Option<String>, _>("category_name").unwrap_or_default();
        let parent_name = r.get::<Option<String>, _>("parent_name").unwrap_or_default();
        let sort_key = get_category_sort_key(&category_name, &parent_name);
        
        let (max_price, min_price, latest_price) = price_map.get(&product_id).copied().unwrap_or((0.0, 0.0, 0.0));
        
        let items = purchaser["items"].as_array_mut().unwrap();
        items.push(serde_json::json!({
            "id": r.get::<i64, _>("id"),
            "product_id": product_id,
            "product_name": r.get::<String, _>("product_name"),
            "unit": r.get::<Option<String>, _>("unit").unwrap_or_default(),
            "unit_price": r.get::<f64, _>("unit_price"),
            "quantity": r.get::<f64, _>("quantity"),
            "amount": r.get::<f64, _>("amount"),
            "order_no": r.get::<Option<String>, _>("order_no").unwrap_or_default(),
            "remark": r.get::<Option<String>, _>("remark").unwrap_or_default(),
            "sort_key": sort_key,
            "max_price": max_price,
            "min_price": min_price,
            "latest_price": latest_price,
            "selling_price": r.get::<f64, _>("unit_price"),
        }));
    }
    
    let mut purchasers: Vec<serde_json::Value> = purchaser_map.values().cloned().collect();
    for p in purchasers.iter_mut() {
        let items = p["items"].as_array_mut().unwrap();
        items.sort_by(|a, b| a["sort_key"].as_i64().unwrap_or(999).cmp(&b["sort_key"].as_i64().unwrap_or(999)));
    }

    let excel_result: Result<Vec<u8>, XlsxError> = (|| {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();

        worksheet.set_landscape();
        worksheet.set_margins(0.2, 0.2, 0.2, 0.2, 0.2, 0.2);
        worksheet.set_print_center_vertically(false);
        worksheet.set_print_center_horizontally(true);

        let title_format = Format::new()
            .set_bold()
            .set_font_size(14)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter);

        let header_format = Format::new()
            .set_bold()
            .set_font_size(10)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin)
            .set_text_wrap();

        let section_title_format = Format::new()
            .set_bold()
            .set_font_size(12)
            .set_align(FormatAlign::Left)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#E5E7EB");

        let cell_format = Format::new()
            .set_font_size(10)
            .set_border(FormatBorder::Thin)
            .set_align(FormatAlign::VerticalCenter);

        let cell_left_format = Format::new()
            .set_font_size(10)
            .set_border(FormatBorder::Thin)
            .set_align(FormatAlign::Left)
            .set_align(FormatAlign::VerticalCenter);

        let price_format = Format::new()
            .set_font_size(10)
            .set_border(FormatBorder::Thin)
            .set_align(FormatAlign::Right)
            .set_num_format("0.00");

        worksheet.merge_range(0, 0, 0, 8, "按单位分拣清单", &title_format)?;
        worksheet.set_row_height(0, 28)?;

        let headers = ["序号", "商品名称", "单位", "数量", "备注", "历史最高", "历史最低", "历史最近", "售价"];
        let mut current_row = 2;

        for purchaser in &purchasers {
            current_row += 1;
            worksheet.merge_range(current_row, 0, current_row, 8, purchaser["purchaser_name"].as_str().unwrap_or_default(), &section_title_format)?;

            current_row += 1;
            for (i, h) in headers.iter().enumerate() {
                worksheet.write_with_format(current_row, i as u16, *h, &header_format)?;
            }

            let mut index = 1;
            let items = purchaser["items"].as_array().unwrap();
            for item in items {
                current_row += 1;
                worksheet.write_with_format(current_row, 0, index as f64, &cell_format)?;
                worksheet.write_with_format(current_row, 1, item["product_name"].as_str().unwrap_or_default(), &cell_left_format)?;
                worksheet.write_with_format(current_row, 2, item["unit"].as_str().unwrap_or_default(), &cell_format)?;
                worksheet.write_with_format(current_row, 3, item["quantity"].as_f64().unwrap_or(0.0), &cell_format)?;
                worksheet.write_with_format(current_row, 4, item["remark"].as_str().unwrap_or_default(), &cell_left_format)?;
                worksheet.write_with_format(current_row, 5, item["max_price"].as_f64().unwrap_or(0.0), &price_format)?;
                worksheet.write_with_format(current_row, 6, item["min_price"].as_f64().unwrap_or(0.0), &price_format)?;
                worksheet.write_with_format(current_row, 7, item["latest_price"].as_f64().unwrap_or(0.0), &price_format)?;
                worksheet.write_with_format(current_row, 8, item["selling_price"].as_f64().unwrap_or(0.0), &price_format)?;
                index += 1;
            }

            current_row += 2;
        }

        worksheet.set_column_width(0, 10)?;
        worksheet.set_column_width(1, 30)?;
        worksheet.set_column_width(2, 12)?;
        worksheet.set_column_width(3, 12)?;
        worksheet.set_column_width(4, 20)?;
        worksheet.set_column_width(5, 12)?;
        worksheet.set_column_width(6, 12)?;
        worksheet.set_column_width(7, 12)?;
        worksheet.set_column_width(8, 12)?;

        let buf = workbook.save_to_buffer()?;
        Ok(buf)
    })();

    match excel_result {
        Ok(buf) => {
            let headers = [
                ("Content-Type", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
                ("Content-Disposition", "attachment; filename=\"按单位分拣清单.xlsx\""),
            ];
            (StatusCode::OK, headers, buf).into_response()
        },
        Err(e) => {
            eprintln!("Excel export error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "导出失败").into_response()
        },
    }
}

async fn api_sales_order_sort_comprehensive() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT soi.id, soi.product_id, soi.product_name, soi.unit, soi.unit_price, soi.quantity, soi.amount, soi.remark,
                p.id as purchaser_id, p.name as purchaser_name, so.order_no,
                c.name as category_name
         FROM sales_order_item soi 
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         LEFT JOIN product pr ON soi.product_id = pr.id
         LEFT JOIN category c ON pr.category_id = c.id
         WHERE so.status IN ('pending', 'sorting')
         ORDER BY p.name, c.name, soi.product_name"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    #[derive(Debug, Clone)]
    struct CategoryData {
        name: String,
        items: Vec<serde_json::Value>,
    }
    
    #[derive(Debug, Clone)]
    struct PurchaserData {
        id: i64,
        name: String,
        categories: Vec<CategoryData>,
        total_amount: f64,
    }
    
    let mut purchaser_map: std::collections::HashMap<i64, PurchaserData> = std::collections::HashMap::new();
    
    for r in &rows {
        let purchaser_id = r.get::<i64, _>("purchaser_id");
        let purchaser_name = r.get::<Option<String>, _>("purchaser_name").unwrap_or_default();
        let category_name = r.get::<Option<String>, _>("category_name").unwrap_or_else(|| "未分类".to_string());
        
        let purchaser = purchaser_map.entry(purchaser_id).or_insert_with(|| PurchaserData {
            id: purchaser_id,
            name: purchaser_name,
            categories: Vec::new(),
            total_amount: 0.0,
        });
        
        purchaser.total_amount += r.get::<f64, _>("amount");
        
        let category = purchaser.categories.iter_mut()
            .find(|c| c.name == category_name);
        
        let category_items = match category {
            Some(c) => &mut c.items,
            None => {
                purchaser.categories.push(CategoryData {
                    name: category_name.clone(),
                    items: Vec::new(),
                });
                &mut purchaser.categories.last_mut().unwrap().items
            }
        };
        
        let existing_idx = category_items.iter().position(|item| item["product_id"].as_i64() == Some(r.get::<i64, _>("product_id")));
        
        if let Some(idx) = existing_idx {
            let item = &mut category_items[idx];
            let current_qty = item["quantity"].as_f64().unwrap_or(0.0);
            let current_amount = item["amount"].as_f64().unwrap_or(0.0);
            item["quantity"] = serde_json::json!(current_qty + r.get::<f64, _>("quantity"));
            item["amount"] = serde_json::json!(current_amount + r.get::<f64, _>("amount"));
            
            let order_nos: Vec<String> = item["order_nos"].as_array().unwrap_or(&vec![]).iter()
                .map(|v| v.as_str().unwrap_or("").to_string())
                .collect();
            let new_order = r.get::<Option<String>, _>("order_no").unwrap_or_default();
            if !order_nos.contains(&new_order) {
                let mut new_orders = order_nos;
                new_orders.push(new_order);
                item["order_nos"] = serde_json::json!(new_orders);
            }
            
            let remark = r.get::<Option<String>, _>("remark").unwrap_or_default();
            if !remark.is_empty() {
                let existing_remarks: Vec<String> = item["remarks"].as_array().unwrap_or(&vec![]).iter()
                    .map(|v| v.as_str().unwrap_or("").to_string())
                    .collect();
                if !existing_remarks.contains(&remark) {
                    let mut new_remarks = existing_remarks;
                    new_remarks.push(remark);
                    item["remarks"] = serde_json::json!(new_remarks);
                }
            }
        } else {
            let remark = r.get::<Option<String>, _>("remark").unwrap_or_default();
            category_items.push(serde_json::json!({
                "id": r.get::<i64, _>("id"),
                "product_id": r.get::<i64, _>("product_id"),
                "product_name": r.get::<String, _>("product_name"),
                "unit": r.get::<Option<String>, _>("unit").unwrap_or_default(),
                "unit_price": r.get::<f64, _>("unit_price"),
                "quantity": r.get::<f64, _>("quantity"),
                "amount": r.get::<f64, _>("amount"),
                "order_nos": vec![r.get::<Option<String>, _>("order_no").unwrap_or_default()],
                "remarks": if remark.is_empty() { vec![] } else { vec![remark] },
            }));
        }
    }
    
    let mut result: Vec<serde_json::Value> = Vec::new();
    for (_, purchaser) in purchaser_map {
        let mut categories_json: Vec<serde_json::Value> = Vec::new();
        for cat in purchaser.categories {
            categories_json.push(serde_json::json!({
                "category_name": cat.name,
                "items": cat.items,
            }));
        }
        categories_json.sort_by(|a, b| a["category_name"].as_str().unwrap_or("").cmp(b["category_name"].as_str().unwrap_or("")));
        
        result.push(serde_json::json!({
            "purchaser_id": purchaser.id,
            "purchaser_name": purchaser.name,
            "categories": categories_json,
            "total_amount": purchaser.total_amount,
        }));
    }
    
    result.sort_by(|a, b| a["purchaser_name"].as_str().unwrap_or("").cmp(b["purchaser_name"].as_str().unwrap_or("")));
    
    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

async fn api_sales_order_sort_comprehensive_excel() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT soi.id, soi.product_id, soi.product_name, soi.unit, soi.quantity, soi.remark,
                p.id as purchaser_id, p.name as purchaser_name, so.order_no,
                c.name as category_name
         FROM sales_order_item soi 
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         LEFT JOIN product pr ON soi.product_id = pr.id
         LEFT JOIN category c ON pr.category_id = c.id
         WHERE so.status IN ('pending', 'sorting')
         ORDER BY p.name, c.name, soi.product_name"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    #[derive(Debug, Clone)]
    struct CategoryData {
        name: String,
        items: Vec<serde_json::Value>,
    }
    
    #[derive(Debug, Clone)]
    struct PurchaserData {
        name: String,
        categories: Vec<CategoryData>,
    }
    
    let mut purchaser_map: std::collections::HashMap<i64, PurchaserData> = std::collections::HashMap::new();
    
    for r in &rows {
        let purchaser_id = r.get::<i64, _>("purchaser_id");
        let purchaser_name = r.get::<Option<String>, _>("purchaser_name").unwrap_or_default();
        let category_name = r.get::<Option<String>, _>("category_name").unwrap_or_else(|| "未分类".to_string());
        
        let purchaser = purchaser_map.entry(purchaser_id).or_insert_with(|| PurchaserData {
            name: purchaser_name,
            categories: Vec::new(),
        });
        
        let category = purchaser.categories.iter_mut()
            .find(|c| c.name == category_name);
        
        let category_items = match category {
            Some(c) => &mut c.items,
            None => {
                purchaser.categories.push(CategoryData {
                    name: category_name.clone(),
                    items: Vec::new(),
                });
                &mut purchaser.categories.last_mut().unwrap().items
            }
        };
        
        let existing_idx = category_items.iter().position(|item| item["product_id"].as_i64() == Some(r.get::<i64, _>("product_id")));
        
        if let Some(idx) = existing_idx {
            let item = &mut category_items[idx];
            let current_qty = item["quantity"].as_f64().unwrap_or(0.0);
            item["quantity"] = serde_json::json!(current_qty + r.get::<f64, _>("quantity"));
            
            let order_nos: Vec<String> = item["order_nos"].as_array().unwrap_or(&vec![]).iter()
                .map(|v| v.as_str().unwrap_or("").to_string())
                .collect();
            let new_order = r.get::<Option<String>, _>("order_no").unwrap_or_default();
            if !order_nos.contains(&new_order) {
                let mut new_orders = order_nos;
                new_orders.push(new_order);
                item["order_nos"] = serde_json::json!(new_orders);
            }
            
            let existing_remarks: Vec<String> = item["remarks"].as_array().unwrap_or(&vec![]).iter()
                .map(|v| v.as_str().unwrap_or("").to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let new_remark = r.get::<Option<String>, _>("remark").unwrap_or_default();
            if !new_remark.is_empty() && !existing_remarks.contains(&new_remark) {
                let mut new_remarks = existing_remarks;
                new_remarks.push(new_remark);
                item["remarks"] = serde_json::json!(new_remarks);
            }
        } else {
            let remark = r.get::<Option<String>, _>("remark").unwrap_or_default();
            category_items.push(serde_json::json!({
                "id": r.get::<i64, _>("id"),
                "product_id": r.get::<i64, _>("product_id"),
                "product_name": r.get::<String, _>("product_name"),
                "unit": r.get::<Option<String>, _>("unit").unwrap_or_default(),
                "quantity": r.get::<f64, _>("quantity"),
                "order_nos": vec![r.get::<Option<String>, _>("order_no").unwrap_or_default()],
                "remarks": if remark.is_empty() { vec![] } else { vec![remark] },
            }));
        }
    }
    
    let mut result: Vec<PurchaserData> = purchaser_map.into_values().collect();
    result.sort_by(|a, b| a.name.cmp(&b.name));
    for p in &mut result {
        p.categories.sort_by(|a, b| a.name.cmp(&b.name));
    }

    let excel_result: Result<Vec<u8>, XlsxError> = (|| {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();

        worksheet.set_landscape();
        worksheet.set_margins(0.2, 0.2, 0.2, 0.2, 0.2, 0.2);
        worksheet.set_print_center_vertically(false);
        worksheet.set_print_center_horizontally(true);

        let title_format = Format::new()
            .set_bold()
            .set_font_size(14)
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
            .set_border(FormatBorder::Thin);

        let purchaser_format = Format::new()
            .set_bold()
            .set_font_size(12)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#0EA5E9")
            .set_font_color("#FFFFFF");

        let cat_hunxian_format = Format::new()
            .set_bold()
            .set_font_size(11)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#DC2626")
            .set_font_color("#FFFFFF");

        let cat_xianshu_format = Format::new()
            .set_bold()
            .set_font_size(11)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#16A34A")
            .set_font_color("#FFFFFF");

        let cat_liangyou_format = Format::new()
            .set_bold()
            .set_font_size(11)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#1D4ED8")
            .set_font_color("#FFFFFF");

        let cat_douzhi_format = Format::new()
            .set_bold()
            .set_font_size(11)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#CA8A04")
            .set_font_color("#FFFFFF");

        let cat_fenmian_format = Format::new()
            .set_bold()
            .set_font_size(11)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#64748B")
            .set_font_color("#FFFFFF");

        let cat_shuiguo_format = Format::new()
            .set_bold()
            .set_font_size(11)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#EA580C")
            .set_font_color("#FFFFFF");

        let cat_other_format = Format::new()
            .set_bold()
            .set_font_size(11)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#6B7280")
            .set_font_color("#FFFFFF");

        let col_widths = [6.0, 20.0, 8.0, 10.0, 20.0, 20.0];
        for (i, w) in col_widths.iter().enumerate() {
            worksheet.set_column_width(i as u16, *w)?;
        }

        let mut current_row = 0;
        let title = format!("采购分拣清单（综合）");
        worksheet.merge_range(current_row, 0, current_row, 5, title.as_str(), &title_format)?;
        worksheet.set_row_height(current_row, 28)?;
        current_row += 2;

        let headers = ["序号", "商品名称", "单位", "数量", "备注", "订单号"];
        for (i, header) in headers.iter().enumerate() {
            worksheet.write_with_format(current_row, i as u16, *header, &header_format)?;
        }
        current_row += 1;

        let mut seq = 1;

        for purchaser in &result {
            let p_title = format!("【采购单位：{}】", purchaser.name);
            worksheet.merge_range(current_row, 0, current_row, 5, p_title.as_str(), &purchaser_format)?;
            worksheet.set_row_height(current_row, 22)?;
            current_row += 1;

            for cat in &purchaser.categories {
                let cat_format = match () {
                    _ if cat.name.contains("荤鲜") => &cat_hunxian_format,
                    _ if cat.name.contains("鲜蔬") => &cat_xianshu_format,
                    _ if cat.name.contains("粮油") || cat.name.contains("干调") => &cat_liangyou_format,
                    _ if cat.name.contains("豆制品") => &cat_douzhi_format,
                    _ if cat.name.contains("粉面") => &cat_fenmian_format,
                    _ if cat.name.contains("水果") => &cat_shuiguo_format,
                    _ => &cat_other_format,
                };

                let cat_title = format!("【{}】", cat.name);
                worksheet.merge_range(current_row, 0, current_row, 5, cat_title.as_str(), cat_format)?;
                worksheet.set_row_height(current_row, 18)?;
                current_row += 1;

                for item in &cat.items {
                    let product_name = item["product_name"].as_str().unwrap_or("");
                    let unit = item["unit"].as_str().unwrap_or("");
                    let quantity = item["quantity"].as_f64().unwrap_or(0.0);
                    
                    let order_nos: Vec<String> = item["order_nos"].as_array().unwrap_or(&vec![]).iter()
                        .map(|v| v.as_str().unwrap_or("").to_string())
                        .collect();
                    
                    let remarks: Vec<String> = item["remarks"].as_array().unwrap_or(&vec![]).iter()
                        .map(|v| v.as_str().unwrap_or("").to_string())
                        .collect();

                    worksheet.write_with_format(current_row, 0, seq as f64, &cell_format)?;
                    worksheet.write_with_format(current_row, 1, product_name, &cell_left_format)?;
                    worksheet.write_with_format(current_row, 2, unit, &cell_format)?;
                    worksheet.write_with_format(current_row, 3, quantity, &cell_format)?;
                    worksheet.write_with_format(current_row, 4, remarks.join(", "), &cell_left_format)?;
                    worksheet.write_with_format(current_row, 5, order_nos.join(", "), &cell_left_format)?;
                    current_row += 1;
                    seq += 1;
                }
            }
            
            current_row += 1;
        }

        let buf = workbook.save_to_buffer()?;
        Ok(buf)
    })();

    match excel_result {
        Ok(buf) => {
            let headers = [
                ("Content-Type", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
                ("Content-Disposition", "attachment; filename=\"采购分拣清单_综合.xlsx\""),
            ];
            (StatusCode::OK, headers, buf).into_response()
        }
        Err(e) => {
            eprintln!("Excel export error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "导出失败").into_response()
        }
    }
}

async fn api_sales_order_update_status(Json(req): Json<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let id = req.get("id").and_then(|s| s.parse::<i64>().ok());
    let new_status = req.get("status");
    
    if id.is_none() || new_status.is_none() {
        return (StatusCode::BAD_REQUEST, "缺少参数".to_string());
    }
    
    let id = id.unwrap();
    let new_status = new_status.unwrap();
    
    let valid_statuses = vec!["pending", "sorting", "sorted", "delivering", "delivered", "accepted", "settled"];
    if !valid_statuses.contains(&new_status.as_str()) {
        return (StatusCode::BAD_REQUEST, "无效状态".to_string());
    }
    
    let current_status: Option<String> = sqlx::query_scalar("SELECT status FROM sales_order WHERE id = ?")
        .bind(id)
        .fetch_one(pool())
        .await
        .ok();
    
    let current_status = current_status.unwrap_or_else(|| "pending".to_string());
    
    let allowed_transitions = match current_status.as_str() {
        "pending" => vec!["sorting"],
        "sorting" => vec!["pending", "sorted"],
        "sorted" => vec!["sorting", "delivering"],
        "delivering" => vec!["sorted", "delivered"],
        "delivered" => vec!["delivering", "accepted"],
        "accepted" => vec!["delivered", "settled"],
        "settled" => vec!["accepted"],
        _ => vec![],
    };
    
    if !allowed_transitions.contains(&new_status.as_str()) {
        return (StatusCode::BAD_REQUEST, format!("状态不允许从 {} 转换到 {}", current_status, new_status));
    }
    
    let result = sqlx::query("UPDATE sales_order SET status = ? WHERE id = ?")
        .bind(new_status)
        .bind(id)
        .execute(pool())
        .await;
    
    match result {
        Ok(_) => (StatusCode::OK, "状态更新成功".to_string()),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "状态更新失败".to_string()),
    }
}

async fn api_sales_order_correction(Json(data): Json<std::collections::HashMap<String, serde_json::Value>>) -> impl IntoResponse {
    let corrections = data.get("corrections");
    if corrections.is_none() {
        return (StatusCode::BAD_REQUEST, "缺少修正数据".to_string());
    }
    
    let corrections = corrections.unwrap().as_array().cloned().unwrap_or_default();
    let mut updated_count = 0;
    
    for item in corrections {
        let item_id = item.get("id").and_then(|v| v.as_i64());
        let product_id = item.get("product_id").and_then(|v| v.as_i64());
        let quantity = item.get("quantity").and_then(|v| v.as_f64());
        
        if quantity.is_none() {
            continue;
        }
        
        let quantity = quantity.unwrap();
        
        if let Some(item_id) = item_id {
            let result = sqlx::query(
                "UPDATE sales_order_item SET quantity = ?, amount = unit_price * ? WHERE id = ?"
            )
            .bind(quantity)
            .bind(quantity)
            .bind(item_id)
            .execute(pool())
            .await;
            
            if let Ok(r) = result {
                updated_count += r.rows_affected() as i64;
            }
        } else if let Some(product_id) = product_id {
            let result = sqlx::query(
                "UPDATE sales_order_item SET quantity = ?, amount = unit_price * ? WHERE product_id = ?"
            )
            .bind(quantity)
            .bind(quantity)
            .bind(product_id)
            .execute(pool())
            .await;
            
            if let Ok(r) = result {
                updated_count += r.rows_affected() as i64;
            }
        }
    }
    
    (StatusCode::OK, format!("成功修正 {} 条记录", updated_count))
}

async fn api_accept_create(Json(req): Json<AcceptReq>) -> impl IntoResponse {
    let result = sqlx::query(
        "INSERT INTO food_accept(supplier_id, purchaser_id, car_no, supply_time, total_price, discount_rate, final_price) VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(req.supplier_id)
    .bind(req.purchaser_id)
    .bind(&req.car_no)
    .bind(&req.supply_time)
    .bind(req.total_price)
    .bind(req.discount_rate)
    .bind(req.final_price)
    .execute(pool())
    .await;
    
    match result {
        Ok(res) => {
            let accept_id = res.last_insert_rowid();
            for item in req.items {
                sqlx::query(
                    "INSERT INTO food_item(accept_id, food_name, spec, unit_price, quantity, sub_total, produce_batch, shelf_life, has_veg_report, has_meat_quarantine, has_abnormal, pass_check, remark) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                )
                .bind(accept_id)
                .bind(&item.food_name)
                .bind(&item.spec)
                .bind(item.unit_price)
                .bind(item.quantity)
                .bind(item.sub_total)
                .bind(&item.produce_batch)
                .bind(&item.shelf_life)
                .bind(item.has_veg_report)
                .bind(item.has_meat_quarantine)
                .bind(item.has_abnormal)
                .bind(item.pass_check)
                .bind(&item.remark)
                .execute(pool())
                .await
                .ok();
            }
            StatusCode::OK
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn api_accept_list() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT fa.id, fa.supplier_id, fa.purchaser_id, fa.car_no, fa.supply_time, fa.total_price, fa.discount_rate, fa.final_price, fa.status,
                s.name as supplier_name, p.name as purchaser_name
         FROM food_accept fa 
         JOIN supplier s ON fa.supplier_id = s.id 
         JOIN purchaser p ON fa.purchaser_id = p.id 
         ORDER BY fa.id DESC"
    )
    .fetch_all(pool())
    .await
    .unwrap_or_default();
    
    let accepts: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| serde_json::json!({
            "id": row.get::<i64, _>("id"),
            "supplier_id": row.get::<i64, _>("supplier_id"),
            "purchaser_id": row.get::<i64, _>("purchaser_id"),
            "car_no": row.get::<Option<String>, _>("car_no"),
            "supply_time": row.get::<String, _>("supply_time"),
            "total_price": row.get::<f64, _>("total_price"),
            "discount_rate": row.get::<f64, _>("discount_rate"),
            "final_price": row.get::<f64, _>("final_price"),
            "status": row.get::<String, _>("status"),
            "supplier_name": row.get::<String, _>("supplier_name"),
            "purchaser_name": row.get::<String, _>("purchaser_name"),
        }))
        .collect();
    
    (StatusCode::OK, serde_json::to_string(&accepts).unwrap())
}

fn build_router() -> Router {
    Router::new()
        .route("/static/bootstrap.min.css", get(serve_bootstrap_css))
        .route("/static/bootstrap.bundle.min.js", get(serve_bootstrap_js))
        .route("/", get(page_index))
        .route("/supplier", get(page_supplier))
        .route("/purchaser", get(page_purchaser))
        .route("/product", get(page_product))
        .route("/warehouse", get(page_warehouse))
        .route("/inventory", get(page_inventory))
        .route("/purchase", get(page_purchase))
        .route("/sales", get(page_sales))
        .route("/query/purchase_order", get(page_query_purchase_order))
        .route("/query/purchase_price", get(page_query_purchase_price))
        .route("/query/purchase_summary", get(page_query_purchase_summary))
        .route("/query/supplier_balance", get(page_query_supplier_balance))
        .route("/query/sales_order", get(page_query_sales_order))
        .route("/query/sales_summary", get(page_query_sales_summary))
        .route("/query/sales_price", get(page_query_sales_price))
        .route("/query/purchaser_balance", get(page_query_purchaser_balance))
        .route("/query/product_rank", get(page_query_product_rank))
        .route("/query/stock_balance", get(page_query_stock_balance))
        .route("/query/stock_flow", get(page_query_stock_flow))
        .route("/query/stock_warning", get(page_query_stock_warning))
        .route("/query/slow_stock", get(page_query_slow_stock))
        .route("/query/income_expense", get(page_query_income_expense))
        .route("/query/profit_detail", get(page_query_profit_detail))
        .route("/query/overview", get(page_query_overview))
        .route("/query/category_stats", get(page_query_category_stats))
        .route("/query/document_summary", get(page_query_document_summary))
        .route("/user", get(page_user))
        .route("/system", get(page_system))
        .route("/backup", get(page_backup))
        .route("/restore", get(page_restore))
        .route("/api/system/config", post(api_system_config))
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
        .route("/api/supplier/export", get(api_supplier_export))
        .route("/api/supplier/import", post(api_supplier_import))
        .route("/api/purchaser/list", get(api_purchaser_list))
        .route("/api/purchaser/create", post(api_purchaser_create))
        .route("/api/purchaser/update", post(api_purchaser_update))
        .route("/api/purchaser/delete", post(api_purchaser_delete))
        .route("/api/purchaser/export", get(api_purchaser_export))
        .route("/api/purchaser/import", post(api_purchaser_import))
        .route("/api/product/list", get(api_product_list))
        .route("/api/product/check_name", get(api_product_check_name))
        .route("/api/product/search", get(api_product_search))
        .route("/api/product/by_id", get(api_product_by_id))
        .route("/api/product/create", post(api_product_create))
        .route("/api/product/update", post(api_product_update))
        .route("/api/product/delete", post(api_product_delete))
        .route("/api/product/toggle_status/{id}", post(api_product_toggle_status))
        .route("/api/product/export", get(api_product_export))
        .route("/api/product/import", post(api_product_import))
        .route("/api/product/upload_image", post(api_product_upload_image))
        .route("/api/product/delete_image", get(api_product_delete_image))
        .route("/api/product/image/{filename}", get(api_product_get_image))
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
        .route("/api/purchase_order/create", post(api_purchase_order_create))
        .route("/api/purchase_order/list", get(api_purchase_order_list))
        .route("/api/purchase_order/detail/{id}", get(api_purchase_order_detail))
        .route("/api/purchase_order/update", post(api_purchase_order_update))
        .route("/api/purchase_order/delete/{id}", delete(api_purchase_order_delete))
        .route("/api/purchase_order/export", get(api_purchase_order_export))
        .route("/api/purchase_order/import", post(api_purchase_order_import))
        .route("/api/sales_order/create", post(api_sales_order_create))
        .route("/api/sales_order/list", get(api_sales_order_list))
        .route("/api/sales_order/detail/{id}", get(api_sales_order_detail))
        .route("/api/sales_order/update", post(api_sales_order_update))
        .route("/api/sales_order/delete/{id}", delete(api_sales_order_delete))
        .route("/api/sales_order/export", get(api_sales_order_export))
        .route("/api/sales_order/import", post(api_sales_order_import))
        .route("/api/sales_order/accept/{id}", get(api_sales_order_accept))
        .route("/api/sales_order/accept_excel/{id}", get(api_sales_order_accept_excel))
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
        .route("/api/sales_order/sort_comprehensive", get(api_sales_order_sort_comprehensive))
        .route("/api/sales_order/sort_comprehensive_excel", get(api_sales_order_sort_comprehensive_excel))
        .route("/api/query/purchase_order", get(api_query_purchase_order))
        .route("/api/query/purchase_order/export", get(api_query_purchase_order_export))
        .route("/api/query/purchase_price", get(api_query_purchase_price))
        .route("/api/query/purchase_summary", get(api_query_purchase_summary))
        .route("/api/query/supplier_balance", get(api_query_supplier_balance))
        .route("/api/query/supplier_balance/export", get(api_query_supplier_balance_export))
        .route("/api/query/sales_order", get(api_query_sales_order))
        .route("/api/query/sales_order/export", get(api_query_sales_order_export))
        .route("/api/query/sales_price", get(api_query_sales_price))
        .route("/api/query/sales_summary", get(api_query_sales_summary))
        .route("/api/query/purchaser_balance", get(api_query_purchaser_balance))
        .route("/api/query/purchaser_balance/export", get(api_query_purchaser_balance_export))
        .route("/api/query/product_rank", get(api_query_product_rank))
        .route("/api/query/overview", get(api_query_overview))
        .route("/api/query/category_stats", get(api_query_category_stats))
        .route("/api/query/document_summary", get(api_query_document_summary))
        .route("/api/order/generate_no", get(api_order_generate_no))
        .route("/api/accept/create", post(api_accept_create))
        .route("/api/accept/list", get(api_accept_list))
        .route("/login", get(page_login))
        .route("/api/login", post(api_login))
        .route("/api/login/check", get(api_login_check))
        .route("/api/logout", get(api_logout))
}

fn make_app_icon() -> Icon {
    let size = 64u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let cx = 32.0f32;
    let cy = 32.0f32;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let idx = ((y * size + x) * 4) as usize;
            if dist <= 30.0 {
                rgba[idx] = 67;
                rgba[idx + 1] = 160;
                rgba[idx + 2] = 71;
                rgba[idx + 3] = 255;
                if dist <= 22.0 {
                    rgba[idx] = 255;
                    rgba[idx + 1] = 255;
                    rgba[idx + 2] = 255;
                    rgba[idx + 3] = 255;
                }
            }
        }
    }
    Icon::from_rgba(rgba, size, size).expect("生成图标失败")
}

fn open_browser() {
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", "http://127.0.0.1:3000"])
        .spawn();
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