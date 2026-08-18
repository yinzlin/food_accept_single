use axum::extract::{Json, Path, Query, Multipart};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use bytes::Bytes;
use calamine::{open_workbook_auto_from_rs, Data, Reader};
use chrono::Local;
use rust_xlsxwriter::{Format, FormatAlign, FormatBorder, Workbook, XlsxError};
use crate::utils::{xlsx_response, xlsx_header_format, parse_keyword_pattern, parse_csv, sanitize_filename_prefix, image_url_to_path, operation_action_label, build_category_tree_json, generate_order_no, round_to_allowed_last_digit};
use crate::models::*;
use crate::update_product_purchase_prices;
use crate::log_price_change;
use crate::recalc_base_price_by_markup;
use crate::build_purchase_order_export_workbook;
use crate::get_purchase_order_with_items;
use crate::PurchaseOrderPrintItem;
use crate::get_user_by_id;
use crate::compute_stock_summary;
use crate::compute_stock_summary_reimburse;
use crate::get_category_sort_key;
use crate::check_sales_order_access;
use crate::build_accept_excel;
use serde_json;
use sqlx::{AssertSqlSafe, Row};
pub async fn api_system_config(Json(data): Json<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    for (key, value) in data {
        sqlx::query("INSERT OR REPLACE INTO system_config (key, value) VALUES (?, ?)")
            .bind(&key)
            .bind(&value)
            .execute(crate::db::pool())
            .await
            .unwrap_or_default();
    }
    (StatusCode::OK, "设置保存成功".to_string())
}

pub async fn api_operation_log_list(
    headers: axum::http::HeaderMap,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/system/operation_log").await {
        Err(e) => return e,
        Ok(_) => {}
    }

    let page: i64 = params.get("page").and_then(|s| s.parse().ok()).unwrap_or(1);
    let page_size: i64 = params.get("page_size").and_then(|s| s.parse().ok()).unwrap_or(20).min(100);
    let offset = (page - 1) * page_size;

    let username = params.get("username").map(|s| s.as_str()).unwrap_or("").trim();
    let action = params.get("action").map(|s| s.as_str()).unwrap_or("").trim();
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or("");
    let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");

    let mut where_sql = String::from(" WHERE 1=1");
    let mut binds: Vec<String> = Vec::new();
    if !username.is_empty() {
        where_sql.push_str(" AND username LIKE ?");
        binds.push(format!("%{}%", username));
    }
    if !action.is_empty() {
        where_sql.push_str(" AND action LIKE ?");
        binds.push(format!("%{}%", action));
    }
    if !start_date.is_empty() {
        where_sql.push_str(" AND date(created_at) >= ?");
        binds.push(start_date.to_string());
    }
    if !end_date.is_empty() {
        where_sql.push_str(" AND date(created_at) <= ?");
        binds.push(end_date.to_string());
    }

    let count_sql = format!("SELECT COUNT(*) as count FROM operation_log{}", where_sql);
    let mut count_q = sqlx::query(AssertSqlSafe(count_sql.as_str()));
    for b in &binds {
        count_q = count_q.bind(b);
    }
    let total_rows = count_q.fetch_one(crate::db::pool()).await.unwrap();
    let total: i64 = total_rows.get("count");

    let data_sql = format!(
        "SELECT id, user_id, username, action, target_type, target_id, detail, created_at FROM operation_log{} ORDER BY id DESC LIMIT ? OFFSET ?",
        where_sql
    );
    let mut q = sqlx::query(AssertSqlSafe(data_sql.as_str()));
    for b in &binds {
        q = q.bind(b);
    }
    q = q.bind(page_size).bind(offset);
    let rows = q.fetch_all(crate::db::pool()).await.unwrap_or_default();

    let list: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            let action: String = r.get("action");
            serde_json::json!({
                "id": r.get::<i64, _>("id"),
                "user_id": r.get::<i64, _>("user_id"),
                "username": r.get::<String, _>("username"),
                "action": action,
                "action_label": operation_action_label(&action),
                "target_type": r.get::<String, _>("target_type"),
                "target_id": r.get::<String, _>("target_id"),
                "detail": r.get::<String, _>("detail"),
                "created_at": r.get::<String, _>("created_at"),
            })
        })
        .collect();

    let result = serde_json::json!({
        "data": list,
        "page": page,
        "page_size": page_size,
        "total": total,
        "total_pages": (total + page_size - 1) / page_size
    });
    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

pub async fn api_operation_log_export(
    headers: axum::http::HeaderMap,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/system/operation_log/export").await {
        Err(e) => return e.into_response(),
        Ok(_) => {}
    }

    let username = params.get("username").map(|s| s.as_str()).unwrap_or("").trim();
    let action = params.get("action").map(|s| s.as_str()).unwrap_or("").trim();
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or("");
    let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");

    let mut where_sql = String::from(" WHERE 1=1");
    let mut binds: Vec<String> = Vec::new();
    if !username.is_empty() {
        where_sql.push_str(" AND username LIKE ?");
        binds.push(format!("%{}%", username));
    }
    if !action.is_empty() {
        where_sql.push_str(" AND action LIKE ?");
        binds.push(format!("%{}%", action));
    }
    if !start_date.is_empty() {
        where_sql.push_str(" AND date(created_at) >= ?");
        binds.push(start_date.to_string());
    }
    if !end_date.is_empty() {
        where_sql.push_str(" AND date(created_at) <= ?");
        binds.push(end_date.to_string());
    }

    let sql = format!(
        "SELECT id, user_id, username, action, target_type, target_id, detail, created_at FROM operation_log{} ORDER BY id DESC",
        where_sql
    );
    let mut q = sqlx::query(AssertSqlSafe(sql.as_str()));
    for b in &binds {
        q = q.bind(b);
    }
    let rows = q.fetch_all(crate::db::pool()).await.unwrap_or_default();

    let mut workbook = rust_xlsxwriter::Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("操作日志").unwrap();

    let header_format = rust_xlsxwriter::Format::new()
        .set_bold()
        .set_background_color(rust_xlsxwriter::Color::RGB(0x2E75B6))
        .set_font_color(rust_xlsxwriter::Color::White)
        .set_align(rust_xlsxwriter::FormatAlign::Center)
        .set_border(rust_xlsxwriter::FormatBorder::Thin);

    let headers = ["ID", "操作时间", "操作人", "动作", "目标类型", "目标ID", "详情"];
    for (col, header) in headers.iter().enumerate() {
        worksheet.write_with_format(0, col as u16, *header, &header_format).unwrap();
    }

    for (i, row) in rows.iter().enumerate() {
        let r = (i + 1) as u32;
        let action: String = row.get("action");
        let target_type: String = row.get("target_type");
        let type_label = match target_type.as_str() {
            "purchase_order" => "采购单",
            "sales_order" => "销售单",
            "purchase_document" => "采购单据",
            _ => &target_type,
        };
        worksheet.write(r, 0, row.get::<i64, _>("id")).unwrap();
        worksheet.write(r, 1, row.get::<String, _>("created_at")).unwrap();
        worksheet.write(r, 2, row.get::<String, _>("username")).unwrap();
        worksheet.write(r, 3, operation_action_label(&action)).unwrap();
        worksheet.write(r, 4, type_label).unwrap();
        worksheet.write(r, 5, row.get::<String, _>("target_id")).unwrap();
        worksheet.write(r, 6, row.get::<String, _>("detail")).unwrap();
    }

    worksheet.set_column_width(0, 8).unwrap();
    worksheet.set_column_width(1, 20).unwrap();
    worksheet.set_column_width(2, 14).unwrap();
    worksheet.set_column_width(3, 18).unwrap();
    worksheet.set_column_width(4, 12).unwrap();
    worksheet.set_column_width(5, 10).unwrap();
    worksheet.set_column_width(6, 60).unwrap();

    let filename = format!("操作日志_{}.xlsx", chrono::Local::now().format("%Y%m%d_%H%M%S"));
    xlsx_response(workbook.save_to_buffer().unwrap(), &filename)
}

pub async fn api_user_list() -> impl IntoResponse {
    let rows = sqlx::query("SELECT id, username, nickname, phone, role, status, COALESCE(supplier_id,0) as supplier_id, COALESCE(purchaser_id,0) as purchaser_id FROM user_account WHERE status = 1 ORDER BY id")
        .fetch_all(crate::db::pool())
        .await
        .unwrap_or_default();
    let list: Vec<serde_json::Value> = rows.iter().map(|r| {
        serde_json::json!({
            "id": r.get::<i64, _>("id"),
            "username": r.get::<String, _>("username"),
            "nickname": r.get::<Option<String>, _>("nickname").unwrap_or_default(),
            "phone": r.get::<Option<String>, _>("phone").unwrap_or_default(),
            "role": r.get::<String, _>("role"),
            "supplier_id": r.get::<i64, _>("supplier_id"),
            "purchaser_id": r.get::<i64, _>("purchaser_id"),
        })
    }).collect();
    (StatusCode::OK, serde_json::to_string(&list).unwrap())
}

pub async fn api_user_get(Path(id): Path<i64>) -> impl IntoResponse {
    let rows = sqlx::query("SELECT id, username, nickname, role, status, COALESCE(supplier_id,0) as supplier_id, COALESCE(purchaser_id,0) as purchaser_id FROM user_account WHERE id = ?")
        .bind(id)
        .fetch_all(crate::db::pool())
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
            "status": row.get::<i32, _>("status"),
            "supplier_id": row.get::<i64, _>("supplier_id"),
            "purchaser_id": row.get::<i64, _>("purchaser_id")
        }
    })).unwrap())
}

// 获取当前登录用户最近保存的"联系方式"（销售订单主表单输入框的回填值）
pub async fn api_user_get_contact_phone(headers: axum::http::HeaderMap) -> impl IntoResponse {
    let ctx = crate::auth::get_user_ctx(&headers).await;
    let user_id = ctx.user_id;
    let row = sqlx::query("SELECT COALESCE(contact_phone, '') as contact_phone FROM user_account WHERE id = ?")
        .bind(user_id)
        .fetch_optional(crate::db::pool())
        .await
        .unwrap_or(None);
    let phone: String = match row {
        Some(r) => r.try_get("contact_phone").unwrap_or_default(),
        None => String::new(),
    };
    (StatusCode::OK, serde_json::to_string(&serde_json::json!({
        "success": true,
        "contact_phone": phone
    })).unwrap())
}

// 更新当前登录用户最近保存的"联系方式"。
// 销售订单保存时同步调用：把用户在主表单输入框填的联系方式持久化，
// 下次打开销售订单页 / 新建时由前端 GET 拉回作为默认值。
pub async fn api_user_set_contact_phone(headers: axum::http::HeaderMap, Json(data): Json<serde_json::Value>) -> impl IntoResponse {
    let ctx = crate::auth::get_user_ctx(&headers).await;
    let user_id = ctx.user_id;
    if user_id <= 0 {
        return (StatusCode::UNAUTHORIZED, serde_json::to_string(&serde_json::json!({
            "success": false,
            "message": "未登录"
        })).unwrap());
    }
    let phone = data["contact_phone"].as_str().unwrap_or("").trim().to_string();
    let _ = sqlx::query("UPDATE user_account SET contact_phone = ? WHERE id = ?")
        .bind(&phone)
        .bind(user_id)
        .execute(crate::db::pool())
        .await;
    (StatusCode::OK, serde_json::to_string(&serde_json::json!({
        "success": true,
        "contact_phone": phone
    })).unwrap())
}

pub async fn api_user_create(Json(data): Json<serde_json::Value>) -> impl IntoResponse {
    let username = data["username"].as_str().unwrap_or("");
    let password = data["password"].as_str().unwrap_or("");
    let nickname = data["nickname"].as_str().unwrap_or("");
    let role = data["role"].as_str().unwrap_or("user");
    let supplier_id = data["supplier_id"].as_i64().unwrap_or(0);
    let purchaser_id = data["purchaser_id"].as_i64().unwrap_or(0);
    
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
        .fetch_one(crate::db::pool())
        .await
        .unwrap_or(false);
    
    if exists {
        return (StatusCode::OK, serde_json::to_string(&serde_json::json!({
            "success": false,
            "message": "用户名已存在"
        })).unwrap());
    }
    
    let hashed_pwd = bcrypt::hash(password, bcrypt::DEFAULT_COST).unwrap();
    
    sqlx::query("INSERT INTO user_account (username, password, nickname, role, supplier_id, purchaser_id) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(username)
        .bind(hashed_pwd)
        .bind(nickname)
        .bind(role)
        .bind(supplier_id)
        .bind(purchaser_id)
        .execute(crate::db::pool())
        .await
        .ok();
    
    (StatusCode::OK, serde_json::to_string(&serde_json::json!({
        "success": true,
        "message": "用户创建成功"
    })).unwrap())
}

pub async fn api_user_update(Path(id): Path<i64>, Json(data): Json<serde_json::Value>) -> impl IntoResponse {
    let username = data["username"].as_str().unwrap_or("");
    let password = data["password"].as_str().unwrap_or("");
    let nickname = data["nickname"].as_str().unwrap_or("");
    let role = data["role"].as_str().unwrap_or("");
    let supplier_id = data["supplier_id"].as_i64().unwrap_or(0);
    let purchaser_id = data["purchaser_id"].as_i64().unwrap_or(0);
    
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
            .execute(crate::db::pool())
            .await
            .ok();
    }
    
    if role.is_empty() {
        sqlx::query("UPDATE user_account SET username = ?, nickname = ?, supplier_id = ?, purchaser_id = ?, update_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(username)
            .bind(nickname)
            .bind(supplier_id)
            .bind(purchaser_id)
            .bind(id)
            .execute(crate::db::pool())
            .await
            .ok();
    } else {
        sqlx::query("UPDATE user_account SET username = ?, nickname = ?, role = ?, supplier_id = ?, purchaser_id = ?, update_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(username)
            .bind(nickname)
            .bind(role)
            .bind(supplier_id)
            .bind(purchaser_id)
            .bind(id)
            .execute(crate::db::pool())
            .await
            .ok();
    }
    
    (StatusCode::OK, serde_json::to_string(&serde_json::json!({
        "success": true,
        "message": "用户更新成功"
    })).unwrap())
}

pub async fn api_user_delete(Path(id): Path<i64>) -> impl IntoResponse {
    sqlx::query("DELETE FROM user_account WHERE id = ?")
        .bind(id)
        .execute(crate::db::pool())
        .await
        .ok();
    
    (StatusCode::OK, serde_json::to_string(&serde_json::json!({
        "success": true,
        "message": "用户删除成功"
    })).unwrap())
}

pub async fn api_user_status(Path(id): Path<i64>, Json(data): Json<serde_json::Value>) -> impl IntoResponse {
    let status = data["status"].as_i64().unwrap_or(0);
    
    sqlx::query("UPDATE user_account SET status = ?, update_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(status)
        .bind(id)
        .execute(crate::db::pool())
        .await
        .ok();
    
    (StatusCode::OK, serde_json::to_string(&serde_json::json!({
        "success": true,
        "message": "状态更新成功"
    })).unwrap())
}

pub async fn api_backup() -> impl IntoResponse {
    use std::fs;
    use std::path::Path;

    let now = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let backup_dir = "backups";
    if !Path::new(backup_dir).exists() {
        fs::create_dir_all(backup_dir).unwrap_or_default();
    }

    let backup_file = format!("{}/backup_{}.db", backup_dir, now);
    
    let vacuum_sql = format!("VACUUM INTO '{}'", backup_file);
    match sqlx::query(AssertSqlSafe(vacuum_sql.as_str())).execute(crate::db::pool()).await {
        Ok(_) => {
            if let Ok(size) = fs::metadata(&backup_file) {
                sqlx::query("INSERT INTO backup_record (backup_time, file_name, size) VALUES (?, ?, ?)")
                    .bind(now)
                    .bind(&backup_file)
                    .bind(size.len() as i64)
                    .execute(crate::db::pool())
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

pub async fn api_backup_download(Path(id): Path<i64>) -> impl IntoResponse {
    let row = sqlx::query("SELECT file_name FROM backup_record WHERE id = ?")
        .bind(id)
        .fetch_optional(crate::db::pool())
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

pub async fn api_backup_delete(Path(id): Path<i64>) -> impl IntoResponse {
    let row = sqlx::query("SELECT file_name FROM backup_record WHERE id = ?")
        .bind(id)
        .fetch_optional(crate::db::pool())
        .await
        .unwrap_or_default();

    if let Some(row) = row {
        let file_name: String = row.get("file_name");
        std::fs::remove_file(&file_name).unwrap_or_default();
        sqlx::query("DELETE FROM backup_record WHERE id = ?")
            .bind(id)
            .execute(crate::db::pool())
            .await
            .unwrap_or_default();
        (StatusCode::OK, "删除成功".to_string())
    } else {
        (StatusCode::NOT_FOUND, "备份不存在".to_string())
    }
}

pub async fn api_restore(Path(id): Path<i64>) -> impl IntoResponse {
    let row = sqlx::query("SELECT file_name FROM backup_record WHERE id = ?")
        .bind(id)
        .fetch_optional(crate::db::pool())
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

pub async fn api_restore_file(mut multipart: Multipart) -> impl IntoResponse {
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

pub async fn api_inspect_corrupted_items(headers: axum::http::HeaderMap) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/inspect_corrupted_items").await {
        Err(e) => return e,
        Ok(_) => {}
    }

    let rows = sqlx::query(
        "SELECT id, order_id, product_id, product_name, unit, unit_price, quantity, amount FROM sales_order_item WHERE (unit_price = 0 OR quantity = 0 OR amount = 0) AND product_name != '' LIMIT 100"
    )
    .fetch_all(crate::db::pool())
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

pub async fn api_clean_corrupted_items(headers: axum::http::HeaderMap) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/clean_corrupted_items").await {
        Err(e) => return e,
        Ok(_) => {}
    }

    let corrupted_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM sales_order_item WHERE id >= 5527 AND id <= 5619"
    )
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();

    let count = corrupted_ids.len();

    for id in &corrupted_ids {
        sqlx::query("DELETE FROM sales_order_item WHERE id = ?")
            .bind(id)
            .execute(crate::db::pool())
            .await
            .ok();
    }

    let no_item_sales: Vec<i64> = sqlx::query_scalar(
        "SELECT so.id FROM sales_order so LEFT JOIN sales_order_item soi ON so.id = soi.order_id WHERE soi.id IS NULL"
    )
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();

    for id in &no_item_sales {
        sqlx::query("DELETE FROM sales_order WHERE id = ?")
            .bind(id)
            .execute(crate::db::pool())
            .await
            .ok();
    }

    let _ = sqlx::query("VACUUM").execute(crate::db::pool()).await;

    (StatusCode::OK, format!("清理完成，共删除 {} 条损坏的订单明细记录", count))
}

pub async fn api_clean_invalid_orders(headers: axum::http::HeaderMap) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/clean_invalid_orders").await {
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
    match sqlx::query(AssertSqlSafe(vacuum_sql.as_str())).execute(crate::db::pool()).await {
        Ok(_) => {}
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("清理前备份失败：{}", e)),
    }

    let mut deleted_count = 0;

    let no_item_sales: Vec<i64> = sqlx::query_scalar(
        "SELECT so.id FROM sales_order so LEFT JOIN sales_order_item soi ON so.id = soi.order_id WHERE soi.id IS NULL"
    )
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();

    for id in &no_item_sales {
        sqlx::query("DELETE FROM sales_order WHERE id = ?")
            .bind(id)
            .execute(crate::db::pool())
            .await
            .ok();
        deleted_count += 1;
    }

    let no_item_purchase: Vec<i64> = sqlx::query_scalar(
        "SELECT po.id FROM purchase_order po LEFT JOIN purchase_order_item poi ON po.id = poi.order_id WHERE poi.id IS NULL"
    )
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();

    for id in &no_item_purchase {
        sqlx::query("DELETE FROM purchase_order WHERE id = ?")
            .bind(id)
            .execute(crate::db::pool())
            .await
            .ok();
        deleted_count += 1;
    }

    let invalid_purchaser_sales: Vec<i64> = sqlx::query_scalar(
        "SELECT so.id FROM sales_order so LEFT JOIN purchaser p ON so.purchaser_id = p.id WHERE p.id IS NULL"
    )
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();

    for id in &invalid_purchaser_sales {
        sqlx::query("DELETE FROM sales_order_item WHERE order_id = ?")
            .bind(id)
            .execute(crate::db::pool())
            .await
            .ok();
        sqlx::query("DELETE FROM sales_order WHERE id = ?")
            .bind(id)
            .execute(crate::db::pool())
            .await
            .ok();
        deleted_count += 1;
    }

    let invalid_supplier_purchase: Vec<i64> = sqlx::query_scalar(
        "SELECT po.id FROM purchase_order po LEFT JOIN supplier s ON po.supplier_id = s.id WHERE s.id IS NULL"
    )
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();

    for id in &invalid_supplier_purchase {
        sqlx::query("DELETE FROM purchase_order_item WHERE order_id = ?")
            .bind(id)
            .execute(crate::db::pool())
            .await
            .ok();
        sqlx::query("DELETE FROM purchase_order WHERE id = ?")
            .bind(id)
            .execute(crate::db::pool())
            .await
            .ok();
        deleted_count += 1;
    }

    let invalid_product_sales_items: Vec<i64> = sqlx::query_scalar(
        "SELECT soi.id FROM sales_order_item soi LEFT JOIN product p ON soi.product_id = p.id WHERE p.id IS NULL"
    )
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();

    for id in &invalid_product_sales_items {
        sqlx::query("DELETE FROM sales_order_item WHERE id = ?")
            .bind(id)
            .execute(crate::db::pool())
            .await
            .ok();
    }

    let invalid_product_purchase_items: Vec<i64> = sqlx::query_scalar(
        "SELECT poi.id FROM purchase_order_item poi LEFT JOIN product p ON poi.product_id = p.id WHERE p.id IS NULL"
    )
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();

    for id in &invalid_product_purchase_items {
        sqlx::query("DELETE FROM purchase_order_item WHERE id = ?")
            .bind(id)
            .execute(crate::db::pool())
            .await
            .ok();
    }

    let no_item_after_clean_sales: Vec<i64> = sqlx::query_scalar(
        "SELECT so.id FROM sales_order so LEFT JOIN sales_order_item soi ON so.id = soi.order_id WHERE soi.id IS NULL"
    )
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();

    for id in &no_item_after_clean_sales {
        sqlx::query("DELETE FROM sales_order WHERE id = ?")
            .bind(id)
            .execute(crate::db::pool())
            .await
            .ok();
        deleted_count += 1;
    }

    let no_item_after_clean_purchase: Vec<i64> = sqlx::query_scalar(
        "SELECT po.id FROM purchase_order po LEFT JOIN purchase_order_item poi ON po.id = poi.order_id WHERE poi.id IS NULL"
    )
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();

    for id in &no_item_after_clean_purchase {
        sqlx::query("DELETE FROM purchase_order WHERE id = ?")
            .bind(id)
            .execute(crate::db::pool())
            .await
            .ok();
        deleted_count += 1;
    }

    let _ = sqlx::query("VACUUM").execute(crate::db::pool()).await;

    (StatusCode::OK, format!("清理完成，共删除 {} 条无效订单。清理前已备份到 {}", deleted_count, backup_file))
}

pub async fn api_supplier_export() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT s.id, s.name, s.contact, s.phone, s.address, s.business_scope, s.remark, c.name as category_name 
         FROM supplier s LEFT JOIN category c ON s.category_id = c.id ORDER BY s.id"
    )
    .fetch_all(crate::db::pool())
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

pub async fn api_supplier_import(content: Bytes) -> impl IntoResponse {
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
                .fetch_optional(crate::db::pool())
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
        .execute(crate::db::pool())
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

pub async fn api_login(Json(data): Json<LoginReq>) -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT id, username, password, nickname, role, status FROM user_account WHERE username = ?"
    )
    .bind(&data.username)
    .fetch_all(crate::db::pool())
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
        .execute(crate::db::pool())
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

pub async fn api_logout() -> impl IntoResponse {
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

pub async fn api_login_check(headers: axum::http::HeaderMap) -> impl IntoResponse {
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
    .fetch_all(crate::db::pool())
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

pub async fn api_supplier_list(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let category_id = params.get("category_id").and_then(|v| v.parse::<i64>().ok());
    let keyword_pattern = parse_keyword_pattern(&params);
    
    let rows = if let Some(cid) = category_id {
        sqlx::query(
            "SELECT s.id, s.name, s.contact, s.phone, s.address, s.business_scope, s.remark, s.category_id, s.audit_status, c.name as category_name 
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
        .fetch_all(crate::db::pool())
        .await
        .unwrap_or_default()
    } else {
        sqlx::query(
            "SELECT s.id, s.name, s.contact, s.phone, s.address, s.business_scope, s.remark, s.category_id, s.audit_status, c.name as category_name 
             FROM supplier s LEFT JOIN category c ON s.category_id = c.id
             WHERE s.name LIKE ?
             ORDER BY s.id DESC"
        )
        .bind(&keyword_pattern)
        .fetch_all(crate::db::pool())
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
            "audit_status": row.get::<Option<String>, _>("audit_status"),
        }))
        .collect();
    
    (StatusCode::OK, serde_json::to_string(&suppliers).unwrap())
}

pub async fn api_supplier_create(headers: axum::http::HeaderMap, Json(req): Json<SupplierReq>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/supplier/create").await {
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
    .execute(crate::db::pool())
    .await;
    
    match result {
        Ok(_) => (StatusCode::OK, "创建成功".to_string()),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "创建失败".to_string()),
    }
}

pub async fn api_supplier_update(headers: axum::http::HeaderMap, Json(req): Json<SupplierReq>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/supplier/update").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    if let Err((code, msg)) = check_basic_not_confirmed("supplier", req.id.unwrap_or(0)).await {
        return (code, msg);
    }
    let result = sqlx::query(
        "UPDATE supplier SET name=?, contact=?, phone=?, address=?, business_scope=?, remark=?, category_id=?, audit_status='pending' WHERE id=?"
    )
    .bind(&req.name)
    .bind(&req.contact)
    .bind(&req.phone)
    .bind(&req.address)
    .bind(&req.business_scope)
    .bind(&req.remark)
    .bind(&req.category_id)
    .bind(req.id.unwrap_or(0))
    .execute(crate::db::pool())
    .await;
    
    match result {
        Ok(_) => (StatusCode::OK, "更新成功".to_string()),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "更新失败".to_string()),
    }
}

pub async fn api_supplier_delete(headers: axum::http::HeaderMap, Json(req): Json<DeleteReq>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/supplier/delete").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    if let Err((code, msg)) = check_basic_not_confirmed("supplier", req.id).await {
        return (code, msg);
    }
    let result = sqlx::query("DELETE FROM supplier WHERE id=?")
        .bind(req.id)
        .execute(crate::db::pool())
        .await;
    
    match result {
        Ok(_) => (StatusCode::OK, "删除成功".to_string()),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "删除失败".to_string()),
    }
}

// ===== 基础数据审核状态管理 =====
// audit_status：pending=待审核，confirmed=已审核
// 审核/反审核仅超级管理员可执行（超级用户始终拥有最高权限），操作记入审计日志
async fn set_basic_audit_status(
    headers: &axum::http::HeaderMap,
    table: &str,
    id: i64,
    to_status: &str,
    action: &str,
) -> Result<(), (StatusCode, String)> {
    let ctx = crate::auth::get_user_ctx(headers).await;
    if ctx.role != "super_admin" {
        return Err((StatusCode::FORBIDDEN, "仅超级管理员可执行审核/反审核操作".to_string()));
    }
    let sql = format!("UPDATE {} SET audit_status = ? WHERE id = ?", table);
    let result = sqlx::query(AssertSqlSafe(sql.as_str()))
        .bind(to_status)
        .bind(id)
        .execute(crate::db::pool())
        .await;
    match result {
        Ok(r) if r.rows_affected() > 0 => {
            let status_text = if to_status == "confirmed" { "已审核" } else { "待审核" };
            crate::auth::log_operation(&ctx, action, table, &id.to_string(), &format!("{} ID={} 审核状态 → {}", table, id, status_text)).await;
            Ok(())
        }
        Ok(_) => Err((StatusCode::NOT_FOUND, "记录不存在".to_string())),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("操作失败: {}", e))),
    }
}

/// 已审核记录锁定校验：记录已审核（confirmed）时禁止修改/删除，需先反审核。
/// 用于基础数据及其子资源（商品单位/价格）的写操作。
async fn check_basic_not_confirmed(table: &str, id: i64) -> Result<(), (StatusCode, String)> {
    if id <= 0 {
        return Err((StatusCode::BAD_REQUEST, "缺少记录ID".to_string()));
    }
    let sql = format!("SELECT audit_status FROM {} WHERE id = ?", table);
    let status: Option<String> = sqlx::query_scalar(AssertSqlSafe(sql.as_str()))
        .bind(id)
        .fetch_optional(crate::db::pool())
        .await
        .unwrap_or(None);
    match status.as_deref() {
        Some("confirmed") => Err((StatusCode::BAD_REQUEST, "该记录已审核，如需修改/删除请先反审核".to_string())),
        Some(_) => Ok(()),
        None => Err((StatusCode::NOT_FOUND, "记录不存在".to_string())),
    }
}

/// 基础数据是否已审核（下单校验用；id<=0 视为通过，避免误伤无关联的零值）
async fn is_audit_confirmed(table: &str, id: i64) -> bool {
    if id <= 0 {
        return true;
    }
    let sql = format!("SELECT audit_status FROM {} WHERE id = ?", table);
    sqlx::query_scalar::<_, String>(AssertSqlSafe(sql.as_str()))
        .bind(id)
        .fetch_optional(crate::db::pool())
        .await
        .unwrap_or(None)
        .as_deref()
        == Some("confirmed")
}

pub async fn api_supplier_approve(headers: axum::http::HeaderMap, Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    let id = req["id"].as_i64().unwrap_or(0);
    match set_basic_audit_status(&headers, "supplier", id, "confirmed", "supplier.approve").await {
        Ok(()) => (StatusCode::OK, "审核成功".to_string()).into_response(),
        Err((code, msg)) => (code, msg).into_response(),
    }
}
pub async fn api_supplier_unapprove(headers: axum::http::HeaderMap, Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    let id = req["id"].as_i64().unwrap_or(0);
    match set_basic_audit_status(&headers, "supplier", id, "pending", "supplier.unapprove").await {
        Ok(()) => (StatusCode::OK, "反审核成功".to_string()).into_response(),
        Err((code, msg)) => (code, msg).into_response(),
    }
}
pub async fn api_purchaser_approve(headers: axum::http::HeaderMap, Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    let id = req["id"].as_i64().unwrap_or(0);
    match set_basic_audit_status(&headers, "purchaser", id, "confirmed", "purchaser.approve").await {
        Ok(()) => (StatusCode::OK, "审核成功".to_string()).into_response(),
        Err((code, msg)) => (code, msg).into_response(),
    }
}
pub async fn api_purchaser_unapprove(headers: axum::http::HeaderMap, Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    let id = req["id"].as_i64().unwrap_or(0);
    match set_basic_audit_status(&headers, "purchaser", id, "pending", "purchaser.unapprove").await {
        Ok(()) => (StatusCode::OK, "反审核成功".to_string()).into_response(),
        Err((code, msg)) => (code, msg).into_response(),
    }
}
pub async fn api_product_approve(headers: axum::http::HeaderMap, Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    let id = req["id"].as_i64().unwrap_or(0);
    match set_basic_audit_status(&headers, "product", id, "confirmed", "product.approve").await {
        Ok(()) => (StatusCode::OK, "审核成功".to_string()).into_response(),
        Err((code, msg)) => (code, msg).into_response(),
    }
}
pub async fn api_product_unapprove(headers: axum::http::HeaderMap, Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    let id = req["id"].as_i64().unwrap_or(0);
    match set_basic_audit_status(&headers, "product", id, "pending", "product.unapprove").await {
        Ok(()) => (StatusCode::OK, "反审核成功".to_string()).into_response(),
        Err((code, msg)) => (code, msg).into_response(),
    }
}
pub async fn api_warehouse_approve(headers: axum::http::HeaderMap, Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    let id = req["id"].as_i64().unwrap_or(0);
    match set_basic_audit_status(&headers, "warehouse", id, "confirmed", "warehouse.approve").await {
        Ok(()) => (StatusCode::OK, "审核成功".to_string()).into_response(),
        Err((code, msg)) => (code, msg).into_response(),
    }
}
pub async fn api_warehouse_unapprove(headers: axum::http::HeaderMap, Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    let id = req["id"].as_i64().unwrap_or(0);
    match set_basic_audit_status(&headers, "warehouse", id, "pending", "warehouse.unapprove").await {
        Ok(()) => (StatusCode::OK, "反审核成功".to_string()).into_response(),
        Err((code, msg)) => (code, msg).into_response(),
    }
}

pub async fn api_purchaser_export() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT p.id, p.name, p.contact, p.phone, p.address, p.business_scope, p.remark, c.name as category_name 
         FROM purchaser p LEFT JOIN category c ON p.category_id = c.id ORDER BY p.id"
    )
    .fetch_all(crate::db::pool())
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

pub async fn api_purchaser_import(content: Bytes) -> impl IntoResponse {
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
                .fetch_optional(crate::db::pool())
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
        .execute(crate::db::pool())
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

pub async fn api_purchaser_list(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let category_id = params.get("category_id").and_then(|v| v.parse::<i64>().ok());
    let keyword_pattern = parse_keyword_pattern(&params);
    
    let rows = if let Some(cid) = category_id {
        sqlx::query(
            "SELECT p.id, p.name, p.contact, p.phone, p.address, p.business_scope, p.remark, p.category_id, p.audit_status, c.name as category_name
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
        .fetch_all(crate::db::pool())
        .await
        .unwrap_or_default()
    } else {
        sqlx::query(
            "SELECT p.id, p.name, p.contact, p.phone, p.address, p.business_scope, p.remark, p.category_id, p.audit_status, c.name as category_name
             FROM purchaser p LEFT JOIN category c ON p.category_id = c.id
             WHERE p.name LIKE ?
             ORDER BY p.id DESC"
        )
        .bind(&keyword_pattern)
        .fetch_all(crate::db::pool())
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
            "audit_status": row.get::<Option<String>, _>("audit_status"),
        }))
        .collect();
    
    (StatusCode::OK, serde_json::to_string(&purchasers).unwrap())
}

pub async fn api_purchaser_create(headers: axum::http::HeaderMap, Json(req): Json<PurchaserReq>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/purchaser/create").await {
        Err(e) => return e,
        Ok(_) => {}
    }
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
    .execute(crate::db::pool())
    .await;
    
    match result {
        Ok(_) => (StatusCode::OK, "创建成功".to_string()),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "创建失败".to_string()),
    }
}

pub async fn api_purchaser_update(headers: axum::http::HeaderMap, Json(req): Json<PurchaserReq>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/purchaser/update").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    if let Err((code, msg)) = check_basic_not_confirmed("purchaser", req.id.unwrap_or(0)).await {
        return (code, msg);
    }
    let result = sqlx::query(
        "UPDATE purchaser SET name=?, contact=?, phone=?, address=?, business_scope=?, remark=?, category_id=?, audit_status='pending' WHERE id=?"
    )
    .bind(&req.name)
    .bind(&req.contact)
    .bind(&req.phone)
    .bind(&req.address)
    .bind(&req.business_scope)
    .bind(&req.remark)
    .bind(&req.category_id)
    .bind(req.id.unwrap_or(0))
    .execute(crate::db::pool())
    .await;
    
    match result {
        Ok(_) => (StatusCode::OK, "更新成功".to_string()),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "更新失败".to_string()),
    }
}

pub async fn api_purchaser_delete(headers: axum::http::HeaderMap, Json(req): Json<DeleteReq>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/purchaser/delete").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    if let Err((code, msg)) = check_basic_not_confirmed("purchaser", req.id).await {
        return (code, msg);
    }
    let result = sqlx::query("DELETE FROM purchaser WHERE id=?")
        .bind(req.id)
        .execute(crate::db::pool())
        .await;
    
    match result {
        Ok(_) => (StatusCode::OK, "删除成功".to_string()),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "删除失败".to_string()),
    }
}

pub async fn api_product_toggle_status(Path(id): Path<i64>) -> impl IntoResponse {
    if let Err((code, msg)) = check_basic_not_confirmed("product", id).await {
        return (code, msg);
    }
    let row = sqlx::query("SELECT status FROM product WHERE id = ?")
        .bind(id)
        .fetch_optional(crate::db::pool())
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
        .execute(crate::db::pool())
        .await;
    
    match result {
        Ok(_) => {
            let msg = if new_status == 1 { "商品已启用" } else { "商品已停用" };
            (StatusCode::OK, msg.to_string())
        },
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "操作失败".to_string()),
    }
}

pub async fn api_product_list(headers: axum::http::HeaderMap, axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    // 进价仅超级管理员可见：非 super_admin 返回的进价字段置 0
    let is_super_admin = crate::auth::get_user_ctx(&headers).await.role == "super_admin";
    let category_id = params.get("category_id").and_then(|v| v.parse::<i64>().ok());
    let product_id = params.get("id").and_then(|v| v.parse::<i64>().ok());
    let keyword_pattern = parse_keyword_pattern(&params);
    let page: i64 = params.get("page").and_then(|s| s.parse().ok()).unwrap_or(1);
    let page_size: i64 = params.get("page_size").and_then(|s| s.parse().ok()).unwrap_or(20);
    let offset = (page - 1) * page_size;
    // thumbs=1 返回 image_url 字段(默认);thumbs=0 精简返回,前端按需懒加载,适合远程大量商品场景
    let include_thumbs = params.get("thumbs").map(|s| s != "0").unwrap_or(true);

    // 构造 WHERE 条件、绑定参数和 COUNT 基础 SQL
    #[derive(Debug)]
    struct QueryParts {
        where_sql: String,
        binds: Vec<String>,
        category_bind: Option<i64>,
        id_bind: Option<i64>,
    }

    let query_parts = if let Some(pid) = product_id {
        // 按 id 精确查询(用于前端按需加载单条商品图片)
        QueryParts {
            where_sql: "WHERE p.id = ?".to_string(),
            binds: vec![],
            category_bind: None,
            id_bind: Some(pid),
        }
    } else if let Some(cid) = category_id {
        QueryParts {
            where_sql: format!(
                "WHERE p.category_id IN (
                    WITH RECURSIVE cat_tree(id) AS (
                        SELECT id FROM category WHERE id = ?
                        UNION ALL
                        SELECT c.id FROM category c 
                        JOIN cat_tree ct ON c.parent_id = ct.id
                    )
                    SELECT id FROM cat_tree
                )
                AND (p.name LIKE ? OR p.alias1 LIKE ? OR p.alias2 LIKE ?)"
            ),
            binds: vec![keyword_pattern.clone(), keyword_pattern.clone(), keyword_pattern.clone()],
            category_bind: Some(cid),
            id_bind: None,
        }
    } else {
        QueryParts {
            where_sql: "WHERE p.name LIKE ? OR p.alias1 LIKE ? OR p.alias2 LIKE ?".to_string(),
            binds: vec![keyword_pattern.clone(), keyword_pattern.clone(), keyword_pattern.clone()],
            category_bind: None,
            id_bind: None,
        }
    };

    // 总数查询
    let count_sql = format!(
        "SELECT COUNT(*) as count FROM product p LEFT JOIN category c ON p.category_id = c.id {}",
        query_parts.where_sql
    );
    let mut count_q = sqlx::query(AssertSqlSafe(count_sql.as_str()));
    if let Some(pid) = query_parts.id_bind {
        count_q = count_q.bind(pid);
    }
    if let Some(cid) = query_parts.category_bind {
        count_q = count_q.bind(cid);
    }
    for b in &query_parts.binds {
        count_q = count_q.bind(b);
    }
    let total_row = count_q.fetch_one(crate::db::pool()).await.unwrap();
    let total: i64 = total_row.get("count");

    // 分页数据查询
    let select_cols = if include_thumbs {
        "p.id, p.name, p.spec, p.alias1, p.alias2, p.unit, p.base_unit, p.base_price, p.purchase_price, p.max_purchase_price, p.min_purchase_price, p.markup_rate, p.auto_update_price, p.image_url, p.category_id, p.status, p.audit_status, c.name as category_name"
    } else {
        // thumbs=0 模式: 省 image_url 列,流量更小;前端按需单独请求 ?thumbs=1
        "p.id, p.name, p.spec, p.alias1, p.alias2, p.unit, p.base_unit, p.base_price, p.purchase_price, p.max_purchase_price, p.min_purchase_price, p.markup_rate, p.auto_update_price, p.category_id, p.status, p.audit_status, c.name as category_name"
    };
    let data_sql = format!(
        "SELECT {} 
         FROM product p LEFT JOIN category c ON p.category_id = c.id
         {}
         ORDER BY p.id DESC LIMIT ? OFFSET ?",
        select_cols,
        query_parts.where_sql
    );
    let mut data_q = sqlx::query(AssertSqlSafe(data_sql.as_str()));
    if let Some(pid) = query_parts.id_bind {
        data_q = data_q.bind(pid);
    }
    if let Some(cid) = query_parts.category_bind {
        data_q = data_q.bind(cid);
    }
    for b in &query_parts.binds {
        data_q = data_q.bind(b);
    }
    data_q = data_q.bind(page_size).bind(offset);
    let rows = data_q.fetch_all(crate::db::pool()).await.unwrap_or_default();
    
    let mut products: Vec<serde_json::Value> = Vec::new();
    for row in rows {
        let product_id: i64 = row.get("id");
        let unit_rows = sqlx::query(
            "SELECT id, unit_name, ratio, unit_price, purchase_price, sort_order FROM product_unit 
             WHERE product_id = ? ORDER BY sort_order, id"
        )
        .bind(product_id)
        .fetch_all(crate::db::pool())
        .await
        .unwrap_or_default();
        
        let units: Vec<serde_json::Value> = unit_rows
            .iter()
            .map(|ur| serde_json::json!({
                "id": ur.get::<i64, _>("id"),
                "unit_name": ur.get::<String, _>("unit_name"),
                "ratio": ur.get::<f64, _>("ratio"),
                "unit_price": ur.get::<f64, _>("unit_price"),
                "purchase_price": if is_super_admin { ur.get::<f64, _>("purchase_price") } else { 0.0 },
                "sort_order": ur.get::<i32, _>("sort_order"),
            }))
            .collect();
        
        let price_rows = sqlx::query(
            "SELECT price_type, price FROM product_price WHERE product_id = ?"
        )
        .bind(product_id)
        .fetch_all(crate::db::pool())
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
            "purchase_price": if is_super_admin { row.get::<f64, _>("purchase_price") } else { 0.0 },
            "max_purchase_price": if is_super_admin { row.get::<f64, _>("max_purchase_price") } else { 0.0 },
            "min_purchase_price": if is_super_admin { row.get::<f64, _>("min_purchase_price") } else { 0.0 },
            "markup_rate": row.get::<f64, _>("markup_rate"),
            "auto_update_price": row.get::<i64, _>("auto_update_price"),
            "image_url": if include_thumbs { row.get::<Option<String>, _>("image_url") } else { None },
            "category_id": row.get::<Option<i64>, _>("category_id"),
            "status": row.get::<i64, _>("status"),
            "audit_status": row.get::<Option<String>, _>("audit_status"),
            "category_name": row.get::<Option<String>, _>("category_name"),
            "units": units,
            "prices": prices,
            "selling_price": selling_price,
        }));
    }
    
    let total_pages = if page_size > 0 { (total + page_size - 1) / page_size } else { 0 };
    let result = serde_json::json!({
        "data": products,
        "page": page,
        "page_size": page_size,
        "total": total,
        "total_pages": total_pages
    });

    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

pub async fn api_product_create(headers: axum::http::HeaderMap, Json(req): Json<ProductReq>) -> impl IntoResponse {
    // 进价仅超级管理员可设置：非 super_admin 创建商品时进价强制为 0
    let role = crate::auth::get_user_ctx(&headers).await.role;
    let base_unit = req.base_unit.clone().unwrap_or_else(|| req.unit.clone().unwrap_or_else(|| "个".to_string()));
    let unit = req.unit.clone().unwrap_or_else(|| "个".to_string());
    let base_price = req.base_price.unwrap_or(0.0);
    
    let purchase_price = if role == "super_admin" { req.purchase_price.unwrap_or(0.0) } else { 0.0 };
    
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
    .execute(crate::db::pool())
    .await;

    match result {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            eprintln!("创建商品失败: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        },
    }
}

pub async fn api_product_check_name(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
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
    .fetch_all(crate::db::pool())
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

pub async fn api_product_by_id(headers: axum::http::HeaderMap, axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let is_super_admin = crate::auth::get_user_ctx(&headers).await.role == "super_admin";
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
    .fetch_one(crate::db::pool())
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
                "purchase_price": if is_super_admin { r.get::<f64, _>("purchase_price") } else { 0.0 },
                "selling_price": r.get::<f64, _>("selling_price"),
                "category_name": r.get::<Option<String>, _>("category_name").unwrap_or_default(),
            });
            (StatusCode::OK, serde_json::to_string(&product).unwrap())
        },
        Err(_) => (StatusCode::OK, serde_json::to_string(&serde_json::json!({})).unwrap()),
    }
}

pub async fn api_product_search(headers: axum::http::HeaderMap, axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let is_super_admin = crate::auth::get_user_ctx(&headers).await.role == "super_admin";
    let keyword = params.get("keyword").filter(|s| !s.is_empty());
    if keyword.is_none() {
        return (StatusCode::OK, serde_json::to_string(&Vec::<serde_json::Value>::new()).unwrap());
    }
    // audited=1：仅返回已审核商品（开单商品选择用）
    let only_audited = params.get("audited").map(|s| s == "1").unwrap_or(false);
    
    let pattern = format!("%{}%", keyword.unwrap());
    
    let where_clause = if only_audited {
        "WHERE p.status = 1 AND p.audit_status = 'confirmed' AND (p.name LIKE ? OR p.alias1 LIKE ? OR p.alias2 LIKE ?)"
    } else {
        "WHERE p.status = 1 AND (p.name LIKE ? OR p.alias1 LIKE ? OR p.alias2 LIKE ?)"
    };
    let sql = format!(
        "SELECT p.id, p.name, p.alias1, p.alias2, p.spec, p.unit, p.base_unit, p.base_price, p.purchase_price,
                COALESCE(NULLIF((SELECT price FROM product_price WHERE product_id = p.id AND price_type = 'gov_procurement'), 0),
                         (SELECT MAX(price) FROM product_price WHERE product_id = p.id AND price_type LIKE 'supermarket_%'),
                         p.base_price) as selling_price,
                c.name as category_name
         FROM product p LEFT JOIN category c ON p.category_id = c.id
         {}
         ORDER BY p.name",
        where_clause
    );
    let rows = sqlx::query(AssertSqlSafe(sql.as_str()))
        .bind(&pattern)
        .bind(&pattern)
        .bind(&pattern)
        .fetch_all(crate::db::pool())
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
            "purchase_price": if is_super_admin { row.get::<f64, _>("purchase_price") } else { 0.0 },
            "selling_price": row.get::<f64, _>("selling_price"),
            "category_name": row.get::<Option<String>, _>("category_name"),
        }))
        .collect();
    
    (StatusCode::OK, serde_json::to_string(&products).unwrap())
}

pub async fn api_product_update(headers: axum::http::HeaderMap, Json(mut req): Json<ProductUpdateReq>) -> impl IntoResponse {
    let role = match crate::auth::check_api_permission(&headers, "/api/product/update").await {
        Err(e) => return e,
        Ok(role) => role,
    };
    if let Err((code, msg)) = check_basic_not_confirmed("product", req.id).await {
        return (code, msg);
    }
    // 读取旧值用于日志
    let old_row = sqlx::query(
        "SELECT base_price, purchase_price, markup_rate, auto_update_price FROM product WHERE id = ?"
    )
    .bind(req.id)
    .fetch_optional(crate::db::pool())
    .await
    .ok()
    .flatten();
    let (old_base, old_purchase, old_markup, old_auto): (f64, f64, f64, i64) = if let Some(r) = &old_row {
        (
            r.get::<f64, _>("base_price"),
            r.get::<f64, _>("purchase_price"),
            r.get::<f64, _>("markup_rate"),
            r.get::<i64, _>("auto_update_price"),
        )
    } else {
        (0.0, 0.0, 0.5, 0)
    };
    // 进价仅超级管理员可修改：非 super_admin 忽略前端传入的进价，保持原值
    if role != "super_admin" {
        req.purchase_price = Some(old_purchase);
    }

    let result = sqlx::query(
        "UPDATE product SET name = ?, spec = ?, alias1 = ?, alias2 = ?, unit = ?, base_unit = ?, base_price = ?, purchase_price = ?, image_url = ?, category_id = ?, markup_rate = ?, auto_update_price = ?, audit_status='pending' WHERE id = ?"
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
    .bind(&req.markup_rate)
    .bind(&req.auto_update_price)
    .bind(req.id)
    .execute(crate::db::pool())
    .await;

    match result {
        Ok(_) => {
            let new_purchase = req.purchase_price.unwrap_or(old_purchase);
            let new_base = req.base_price.unwrap_or(old_base);
            // 记录进价变更
            if (old_purchase - new_purchase).abs() >= 0.001 {
                log_price_change(
                    req.id,
                    "purchase_price",
                    old_purchase,
                    new_purchase,
                    "product_update",
                    None,
                    Some("商品编辑修改进价"),
                ).await;
            }
            // 记录售价变更
            if (old_base - new_base).abs() >= 0.001 {
                log_price_change(
                    req.id,
                    "base_price",
                    old_base,
                    new_base,
                    "product_update",
                    None,
                    Some("商品编辑修改售价"),
                ).await;
            }
            // 若加成率或 auto_update_price 改变，触发重算
            let new_markup = req.markup_rate.unwrap_or(old_markup);
            let new_auto = req.auto_update_price.unwrap_or(old_auto);
            if (old_markup - new_markup).abs() >= 0.001 || old_auto != new_auto {
                eprintln!(
                    "[商品编辑] 商品ID={} 加成率/开关变更 旧加成率={:.4} 新加成率={:.4} 旧开关={} 新开关={} 触发售价重算",
                    req.id, old_markup, new_markup, old_auto, new_auto
                );
                recalc_base_price_by_markup(req.id, "product_update", None).await;
            }
            (StatusCode::OK, "更新成功".to_string())
        }
        Err(e) => {
            eprintln!("更新商品失败: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "更新失败".to_string())
        }
    }
}

pub async fn api_product_delete(headers: axum::http::HeaderMap, Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/product/delete").await {
        Err(e) => return e,
        Ok(_) => {}
    }

    let id = req["id"].as_i64().unwrap_or(0);
    if let Err((code, msg)) = check_basic_not_confirmed("product", id).await {
        return (code, msg);
    }

    let check_tables = vec![
        ("库存", "SELECT COUNT(*) FROM inventory WHERE product_id = ?"),
        ("采购订单明细", "SELECT COUNT(*) FROM purchase_order_item WHERE product_id = ?"),
        ("销售订单明细", "SELECT COUNT(*) FROM sales_order_item WHERE product_id = ?"),
        ("食品项", "SELECT COUNT(*) FROM food_item WHERE product_id = ?"),
    ];

    for (name, sql) in check_tables {
        let count: i64 = sqlx::query(sql)
            .bind(id)
            .fetch_one(crate::db::pool())
            .await
            .map(|r| r.get(0))
            .unwrap_or(0);
        if count > 0 {
            return (StatusCode::BAD_REQUEST, format!("该商品存在{}记录（{}条），无法删除，请先处理关联数据", name, count));
        }
    }

    let result = sqlx::query("DELETE FROM product WHERE id = ?")
        .bind(id)
        .execute(crate::db::pool())
        .await;
    match result {
        Ok(r) => {
            if r.rows_affected() > 0 {
                sqlx::query("DELETE FROM product_unit WHERE product_id = ?")
                    .bind(id)
                    .execute(crate::db::pool())
                    .await
                    .ok();
                sqlx::query("DELETE FROM product_price WHERE product_id = ?")
                    .bind(id)
                    .execute(crate::db::pool())
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

pub async fn api_product_upload_image(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let product_id = params.get("product_id").and_then(|s| s.parse::<i64>().ok());
    if product_id.is_none() {
        return (StatusCode::BAD_REQUEST, "缺少 product_id 参数".to_string());
    }
    let product_id = product_id.unwrap();

    // 获取商品名称作为文件名前缀
    let product_name: String = sqlx::query_scalar("SELECT name FROM product WHERE id = ?")
        .bind(product_id)
        .fetch_optional(crate::db::pool())
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "product".to_string());
    let name_prefix = sanitize_filename_prefix(&product_name);

    tokio::fs::create_dir_all("uploads/products").await.ok();

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
        let new_filename = format!("{}_{}_{}.{}", name_prefix, timestamp, random, ext);
        let path = format!("uploads/products/{}", new_filename);

        let bytes = field.bytes().await.unwrap_or_default();
        if bytes.len() > 5 * 1024 * 1024 {
            return (StatusCode::BAD_REQUEST, "图片大小不能超过5MB".to_string());
        }

        if tokio::fs::write(&path, bytes).await.is_err() {
            return (StatusCode::INTERNAL_SERVER_ERROR, "保存图片失败".to_string());
        }

        file_path = format!("/api/uploads/products/{}", new_filename);
    }

    if !has_file {
        return (StatusCode::BAD_REQUEST, "请选择要上传的图片".to_string());
    }

    let _ = sqlx::query("UPDATE product SET image_url = ? WHERE id = ?")
        .bind(&file_path)
        .bind(product_id)
        .execute(crate::db::pool())
        .await;

    (StatusCode::OK, serde_json::json!({ "url": file_path }).to_string())
}

pub async fn api_product_delete_image(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> (StatusCode, String) {
    let product_id = params.get("product_id").and_then(|s| s.parse::<i64>().ok());
    if product_id.is_none() {
        return (StatusCode::BAD_REQUEST, "缺少 product_id 参数".to_string());
    }
    let product_id = product_id.unwrap();

    let row = sqlx::query("SELECT image_url FROM product WHERE id = ?")
        .bind(product_id)
        .fetch_optional(crate::db::pool())
        .await
        .unwrap_or(None);

    if let Some(row) = row {
        let image_url: Option<String> = row.get("image_url");
        if let Some(url) = image_url {
            if let Some(path) = image_url_to_path(&url) {
                let _ = tokio::fs::remove_file(&path).await;
            }
        }
    }

    let _ = sqlx::query("UPDATE product SET image_url = NULL WHERE id = ?")
        .bind(product_id)
        .execute(crate::db::pool())
        .await;

    (StatusCode::OK, "删除成功".to_string())
}

pub async fn api_sales_order_upload_image(
    headers: axum::http::HeaderMap,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let ctx = crate::auth::get_user_ctx(&headers).await;

    let order_id = params.get("order_id").and_then(|s| s.parse::<i64>().ok());
    if order_id.is_none() {
        return (StatusCode::BAD_REQUEST, "缺少 order_id 参数".to_string());
    }
    let order_id = order_id.unwrap();

    // 行级数据权限：仅可为自己采购单位的销售单上传图片
    let order_purchaser_id: i64 = sqlx::query_scalar("SELECT purchaser_id FROM sales_order WHERE id = ?")
        .bind(order_id)
        .fetch_one(crate::db::pool())
        .await
        .unwrap_or(-1);
    if !crate::auth::can_access_sales_order(&ctx, order_purchaser_id) {
        return (StatusCode::FORBIDDEN, "您没有权限操作此订单".to_string());
    }

    let image_type = params.get("type").map(|s| s.as_str()).unwrap_or("");
    let (folder, prefix, column) = match image_type {
        "customer" => ("customer_orders", "客户订单", "customer_order_image"),
        "signed" => ("signed_orders", "签字验收单", "signed_order_image"),
        _ => return (StatusCode::BAD_REQUEST, "无效的图片类型".to_string()),
    };

    // 获取销售单位（采购方）名称和订单日期作为前缀
    let row = sqlx::query(
        "SELECT p.name, so.order_date FROM sales_order so LEFT JOIN purchaser p ON so.purchaser_id = p.id WHERE so.id = ?"
    )
    .bind(order_id)
    .fetch_optional(crate::db::pool())
    .await
    .unwrap_or(None);

    let (purchaser_name, order_date) = if let Some(r) = row {
        let name: Option<String> = r.get("name");
        let date: Option<String> = r.get("order_date");
        (name.unwrap_or_else(|| "purchaser".to_string()), date.unwrap_or_else(|| "nodate".to_string()))
    } else {
        ("purchaser".to_string(), "nodate".to_string())
    };

    let name_prefix = format!("{}_{}_{}", sanitize_filename_prefix(&purchaser_name), order_date, prefix);
    let full_folder = format!("uploads/{}", folder);
    tokio::fs::create_dir_all(full_folder).await.ok();

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
        let new_filename = format!("{}_{}_{}.{}", name_prefix, timestamp, random, ext);
        let path = format!("uploads/{}/{}", folder, new_filename);

        let bytes = field.bytes().await.unwrap_or_default();
        if bytes.len() > 5 * 1024 * 1024 {
            return (StatusCode::BAD_REQUEST, "图片大小不能超过5MB".to_string());
        }

        if tokio::fs::write(&path, bytes).await.is_err() {
            return (StatusCode::INTERNAL_SERVER_ERROR, "保存图片失败".to_string());
        }

        file_path = format!("/api/uploads/{}/{}", folder, new_filename);
    }

    if !has_file {
        return (StatusCode::BAD_REQUEST, "请选择要上传的图片".to_string());
    }

    let update_sql = format!("UPDATE sales_order SET {} = ? WHERE id = ?", column);
    let _ = sqlx::query(AssertSqlSafe(update_sql.as_str()))
        .bind(&file_path)
        .bind(order_id)
        .execute(crate::db::pool())
        .await;

    crate::auth::log_operation(&ctx, "sales_order.upload_image", "sales_order", &order_id.to_string(),
        &format!("上传{}图片：{}", if image_type == "customer" { "客户订单" } else { "签字验收单" }, file_path)).await;

    (StatusCode::OK, serde_json::json!({ "url": file_path }).to_string())
}

pub async fn api_sales_order_delete_image(
    headers: axum::http::HeaderMap,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> (StatusCode, String) {
    let ctx = crate::auth::get_user_ctx(&headers).await;

    let order_id = params.get("order_id").and_then(|s| s.parse::<i64>().ok());
    if order_id.is_none() {
        return (StatusCode::BAD_REQUEST, "缺少 order_id 参数".to_string());
    }
    let order_id = order_id.unwrap();

    // 行级数据权限：仅可操作归属自己的销售单
    let order_purchaser_id: i64 = sqlx::query_scalar("SELECT purchaser_id FROM sales_order WHERE id = ?")
        .bind(order_id)
        .fetch_one(crate::db::pool())
        .await
        .unwrap_or(-1);
    if !crate::auth::can_access_sales_order(&ctx, order_purchaser_id) {
        return (StatusCode::FORBIDDEN, "您没有权限操作此订单".to_string());
    }

    let image_type = params.get("type").map(|s| s.as_str()).unwrap_or("");
    let column = match image_type {
        "customer" => "customer_order_image",
        "signed" => "signed_order_image",
        _ => return (StatusCode::BAD_REQUEST, "无效的图片类型".to_string()),
    };

    let select_sql = format!("SELECT {} as img FROM sales_order WHERE id = ?", column);
    let row = sqlx::query(AssertSqlSafe(select_sql.as_str()))
        .bind(order_id)
        .fetch_optional(crate::db::pool())
        .await
        .unwrap_or(None);

    if let Some(row) = row {
        let image_url: Option<String> = row.get("img");
        if let Some(url) = image_url {
            if let Some(path) = image_url_to_path(&url) {
                let _ = tokio::fs::remove_file(&path).await;
            }
        }
    }

    let update_sql = format!("UPDATE sales_order SET {} = NULL WHERE id = ?", column);
    let _ = sqlx::query(AssertSqlSafe(update_sql.as_str()))
        .bind(order_id)
        .execute(crate::db::pool())
        .await;

    crate::auth::log_operation(&ctx, "sales_order.delete_image", "sales_order", &order_id.to_string(),
        &format!("删除{}图片", if image_type == "customer" { "客户订单" } else { "签字验收单" })).await;

    (StatusCode::OK, "删除成功".to_string())
}

pub async fn api_purchase_document_list(
    headers: axum::http::HeaderMap,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    // 行级数据权限：supplier 角色只能看自己绑定的供应商单据
    let ctx = crate::auth::get_user_ctx(&headers).await;
    let supplier_id: Option<i64> = if ctx.role == "supplier" {
        if ctx.supplier_id > 0 { Some(ctx.supplier_id) } else { Some(-1) }
    } else {
        params.get("supplier_id").and_then(|s| s.parse::<i64>().ok())
    };
    let document_date = params.get("document_date").map(|s| s.as_str()).unwrap_or("");

    let mut sql = "SELECT id, supplier_id, supplier_name, document_date, image_url, remark, create_at FROM purchase_document WHERE 1=1".to_string();
    let rows = match (supplier_id, document_date.is_empty()) {
        (Some(sid), false) => {
            sql.push_str(" AND supplier_id = ? AND document_date = ?");
            sql.push_str(" ORDER BY create_at DESC");
            sqlx::query(AssertSqlSafe(sql.as_str()))
                .bind(sid)
                .bind(document_date)
                .fetch_all(crate::db::pool())
                .await
                .unwrap_or_default()
        },
        (Some(sid), true) => {
            sql.push_str(" AND supplier_id = ? ORDER BY create_at DESC");
            sqlx::query(AssertSqlSafe(sql.as_str()))
                .bind(sid)
                .fetch_all(crate::db::pool())
                .await
                .unwrap_or_default()
        },
        (None, false) => {
            sql.push_str(" AND document_date = ? ORDER BY create_at DESC");
            sqlx::query(AssertSqlSafe(sql.as_str()))
                .bind(document_date)
                .fetch_all(crate::db::pool())
                .await
                .unwrap_or_default()
        },
        (None, true) => {
            sql.push_str(" ORDER BY create_at DESC");
            sqlx::query(AssertSqlSafe(sql.as_str()))
                .fetch_all(crate::db::pool())
                .await
                .unwrap_or_default()
        },
    };

    let result: Vec<serde_json::Value> = rows.iter().map(|row| serde_json::json!({
        "id": row.get::<i64, _>("id"),
        "supplier_id": row.get::<i64, _>("supplier_id"),
        "supplier_name": row.get::<String, _>("supplier_name"),
        "document_date": row.get::<String, _>("document_date"),
        "image_url": row.get::<String, _>("image_url"),
        "remark": row.get::<Option<String>, _>("remark"),
        "create_at": row.get::<String, _>("create_at"),
    })).collect();

    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

pub async fn api_purchase_document_upload(headers: axum::http::HeaderMap, mut multipart: Multipart) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/purchase_document/upload").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let ctx = crate::auth::get_user_ctx(&headers).await;

    let mut supplier_id: Option<i64> = None;
    let mut supplier_name: Option<String> = None;
    let mut document_date: Option<String> = None;
    let mut remark: Option<String> = None;
    // 先缓存文件字节与扩展名，待所有字段解析完再按 供应商+日期 前缀写盘
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut file_ext: String = "jpg".to_string();

    while let Some(field) = multipart.next_field().await.unwrap() {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            let filename = field.file_name().unwrap_or_else(|| "unknown.jpg");
            let ext = std::path::Path::new(filename)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("jpg")
                .to_lowercase();
            if !["jpg", "jpeg", "png", "gif", "webp"].contains(&ext.as_str()) {
                continue;
            }
            let bytes = field.bytes().await.unwrap_or_default();
            if bytes.len() > 10 * 1024 * 1024 {
                continue;
            }
            file_ext = ext;
            file_bytes = Some(bytes.to_vec());
        } else if name == "supplier_id" {
            if let Ok(v) = field.text().await {
                supplier_id = v.parse::<i64>().ok();
            }
        } else if name == "supplier_name" {
            supplier_name = field.text().await.ok();
        } else if name == "document_date" {
            document_date = field.text().await.ok();
        } else if name == "remark" {
            remark = field.text().await.ok();
        }
    }

    if supplier_id.is_none() || supplier_name.is_none() || document_date.is_none() || file_bytes.is_none() {
        return (StatusCode::BAD_REQUEST, "缺少必填参数".to_string());
    }

    // 行级数据权限：supplier 角色只能上传自己绑定的供应商单据
    if ctx.role == "supplier" {
        if ctx.supplier_id == 0 || supplier_id != Some(ctx.supplier_id) {
            return (StatusCode::FORBIDDEN, "供应商账号只能为自己上传单据".to_string());
        }
    }

    let sname = supplier_name.unwrap();
    let ddate = document_date.unwrap();
    let name_prefix = format!("{}_{}", sanitize_filename_prefix(&sname), ddate);

    tokio::fs::create_dir_all("uploads/purchase_documents").await.ok();
    let timestamp = chrono::Utc::now().timestamp_millis();
    let random: u32 = rand::random();
    let new_filename = format!("{}_{}_{}.{}", name_prefix, timestamp, random, file_ext);
    let path = format!("uploads/purchase_documents/{}", new_filename);

    if tokio::fs::write(&path, file_bytes.unwrap()).await.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, "保存图片失败".to_string());
    }
    let saved_url = format!("/api/uploads/purchase_documents/{}", new_filename);

    let result = sqlx::query(
        "INSERT INTO purchase_document(supplier_id, supplier_name, document_date, image_url, remark) VALUES (?, ?, ?, ?, ?)"
    )
    .bind(supplier_id.unwrap())
    .bind(&sname)
    .bind(&ddate)
    .bind(&saved_url)
    .bind(remark)
    .execute(crate::db::pool())
    .await;

    match result {
        Ok(r) => {
            let id = r.last_insert_rowid();
            crate::auth::log_operation(&ctx, "purchase_document.upload", "purchase_document", &id.to_string(),
                &format!("上传采购单据（供应商ID={}，日期={}）：{}", supplier_id.unwrap_or(0), ddate, saved_url)).await;
            (StatusCode::OK, serde_json::json!({ "id": id, "url": saved_url }).to_string())
        },
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "保存失败".to_string()),
    }
}

pub async fn api_purchase_document_delete(headers: axum::http::HeaderMap, Path(id): Path<i64>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/purchase_document/delete").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let ctx = crate::auth::get_user_ctx(&headers).await;

    let row = sqlx::query("SELECT image_url, supplier_id FROM purchase_document WHERE id = ?")
        .bind(id)
        .fetch_optional(crate::db::pool())
        .await
        .unwrap_or(None);

    if let Some(row) = row {
        // 行级数据权限：supplier 角色只能删除自己供应商的单据
        if ctx.role == "supplier" && row.get::<i64, _>("supplier_id") != ctx.supplier_id {
            return (StatusCode::FORBIDDEN, "您没有权限删除此单据".to_string());
        }
        let url: String = row.get("image_url");
        if let Some(path) = image_url_to_path(&url) {
            let _ = tokio::fs::remove_file(&path).await;
        }
    } else {
        return (StatusCode::NOT_FOUND, "单据不存在".to_string());
    }

    let result = sqlx::query("DELETE FROM purchase_document WHERE id = ?")
        .bind(id)
        .execute(crate::db::pool())
        .await;

    match result {
        Ok(_) => {
            crate::auth::log_operation(&ctx, "purchase_document.delete", "purchase_document", &id.to_string(),
                &format!("删除采购单据 ID={}", id)).await;
            (StatusCode::OK, "删除成功".to_string())
        },
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "删除失败".to_string()),
    }
}

pub async fn api_product_get_image(
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

pub async fn api_get_uploaded_image(
    Path((folder, filename)): Path<(String, String)>,
) -> impl IntoResponse {
    // 仅允许已知子目录，防止路径穿越
    let allowed = ["products", "customer_orders", "signed_orders", "purchase_documents"];
    if !allowed.contains(&folder.as_str()) || filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain")],
            "图片不存在".as_bytes().to_vec(),
        );
    }
    let path = format!("uploads/{}/{}", folder, filename);
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

pub async fn api_product_export() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT p.id, p.name, p.alias1, p.alias2, p.spec, p.unit, p.base_unit, p.base_price, p.purchase_price, c.name as category_name 
         FROM product p LEFT JOIN category c ON p.category_id = c.id ORDER BY p.id"
    )
    .fetch_all(crate::db::pool())
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

pub async fn api_product_import(content: Bytes) -> impl IntoResponse {
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
                .fetch_optional(crate::db::pool())
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
        .execute(crate::db::pool())
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

pub async fn api_product_unit_create(Json(req): Json<ProductUnitReq>) -> impl IntoResponse {
    if let Err((code, msg)) = check_basic_not_confirmed("product", req.product_id).await {
        return (code, msg);
    }
    let result = sqlx::query(
        "INSERT INTO product_unit(product_id, unit_name, ratio, unit_price, purchase_price, sort_order) VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(req.product_id)
    .bind(&req.unit_name)
    .bind(req.ratio)
    .bind(req.unit_price.unwrap_or(0.0))
    .bind(req.purchase_price.unwrap_or(0.0))
    .bind(req.sort_order.unwrap_or(0))
    .execute(crate::db::pool())
    .await;

    match result {
        Ok(_) => (StatusCode::OK, "创建成功".to_string()),
        Err(e) => {
            eprintln!("创建单位失败: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("创建单位失败: {:?}", e))
        }
    }
}

pub async fn api_product_unit_update(Json(req): Json<ProductUnitReq>) -> (StatusCode, String) {
    let result = sqlx::query(
        "UPDATE product_unit SET unit_name = ?, ratio = ?, unit_price = ?, purchase_price = ?, sort_order = ? WHERE id = ?"
    )
    .bind(&req.unit_name)
    .bind(req.ratio)
    .bind(req.unit_price.unwrap_or(0.0))
    .bind(req.purchase_price.unwrap_or(0.0))
    .bind(req.sort_order.unwrap_or(0))
    .bind(req.product_id)
    .execute(crate::db::pool())
    .await;

    match result {
        Ok(_) => (StatusCode::OK, "更新成功".to_string()),
        Err(e) => {
            eprintln!("更新单位失败: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "更新失败".to_string())
        }
    }
}

pub async fn api_product_unit_delete(Json(req): Json<DeleteReq>) -> (StatusCode, String) {
    let result = sqlx::query("DELETE FROM product_unit WHERE id = ?")
        .bind(req.id)
        .execute(crate::db::pool())
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

pub async fn api_product_unit_delete_by_product(Json(req): Json<serde_json::Value>) -> (StatusCode, String) {
    let product_id = req["product_id"].as_i64().unwrap_or(0);
    if let Err((code, msg)) = check_basic_not_confirmed("product", product_id).await {
        return (code, msg);
    }
    let result = sqlx::query("DELETE FROM product_unit WHERE product_id = ?")
        .bind(product_id)
        .execute(crate::db::pool())
        .await;
    match result {
        Ok(_) => (StatusCode::OK, "删除成功".to_string()),
        Err(e) => {
            eprintln!("删除单位失败: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "删除失败".to_string())
        }
    }
}

pub async fn api_product_unit_list(headers: axum::http::HeaderMap, axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let is_super_admin = crate::auth::get_user_ctx(&headers).await.role == "super_admin";
    let product_id = params.get("product_id").and_then(|s| s.parse::<i64>().ok());
    if product_id.is_none() {
        return (StatusCode::OK, serde_json::to_string(&Vec::<serde_json::Value>::new()).unwrap());
    }
    
    let rows = sqlx::query(
        "SELECT unit_name, ratio, unit_price, purchase_price FROM product_unit WHERE product_id = ? ORDER BY sort_order, id"
    )
    .bind(product_id.unwrap())
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();
    
    let units: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| serde_json::json!({
            "name": row.get::<String, _>("unit_name"),
            "ratio": row.get::<f64, _>("ratio"),
            "unit_price": row.get::<f64, _>("unit_price"),
            "purchase_price": if is_super_admin { row.get::<f64, _>("purchase_price") } else { 0.0 },
        }))
        .collect();
    
    (StatusCode::OK, serde_json::to_string(&units).unwrap())
}

pub async fn api_product_price_upsert(Json(req): Json<ProductPriceReq>) -> impl IntoResponse {
    if let Err((code, msg)) = check_basic_not_confirmed("product", req.product_id).await {
        return (code, msg).into_response();
    }
    let result = sqlx::query(
        "INSERT OR REPLACE INTO product_price(product_id, price_type, price, collected_at, source) VALUES (?, ?, ?, ?, ?)"
    )
    .bind(req.product_id)
    .bind(&req.price_type)
    .bind(req.price.unwrap_or(0.0))
    .bind(&req.collected_at)
    .bind(&req.source)
    .execute(crate::db::pool())
    .await;

    match result {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => {
            eprintln!("保存价格失败: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn api_product_price_list(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let product_id = match params.get("product_id") {
        Some(v) => v.parse::<i64>().unwrap_or(0),
        None => 0,
    };
    
    let rows = sqlx::query("SELECT id, product_id, price_type, price, collected_at, source FROM product_price WHERE product_id = ?")
        .bind(product_id)
        .fetch_all(crate::db::pool())
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

pub async fn api_product_price_delete(Json(req): Json<DeleteReq>) -> (StatusCode, String) {
    let result = sqlx::query("DELETE FROM product_price WHERE id = ?")
        .bind(req.id)
        .execute(crate::db::pool())
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

pub async fn api_product_price_delete_by_product(Json(req): Json<std::collections::HashMap<String, i64>>) -> impl IntoResponse {
    let product_id = match req.get("product_id") {
        Some(&id) => id,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };
    if let Err((code, msg)) = check_basic_not_confirmed("product", product_id).await {
        return (code, msg).into_response();
    }
    sqlx::query("DELETE FROM product_price WHERE product_id = ?")
        .bind(product_id)
        .execute(crate::db::pool())
        .await
        .ok();
    StatusCode::OK.into_response()
}

pub async fn api_product_sync_base_price(Json(req): Json<std::collections::HashMap<String, i64>>) -> impl IntoResponse {
    let product_id = match req.get("product_id") {
        Some(&id) => id,
        None => return StatusCode::BAD_REQUEST,
    };

    let gov_row = sqlx::query("SELECT price FROM product_price WHERE product_id = ? AND price_type = 'gov_procurement'")
        .bind(product_id)
        .fetch_optional(crate::db::pool())
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
                .fetch_optional(crate::db::pool())
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
            .fetch_optional(crate::db::pool())
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
        // 读取旧值用于日志
        let old_row = sqlx::query("SELECT base_price FROM product WHERE id = ?")
            .bind(product_id)
            .fetch_optional(crate::db::pool())
            .await
            .ok()
            .flatten();
        let old_base_price: f64 = old_row.as_ref().map(|r| r.get::<f64, _>("base_price")).unwrap_or(0.0);

        // 应用统一尾数规则
        let normalized = round_to_allowed_last_digit(selling_price);
        eprintln!(
            "[售价同步] 商品ID={} 同步原始售价={:.4} 尾数处理后={:.4} 旧售价={:.4}",
            product_id, selling_price, normalized, old_base_price
        );

        let _ = sqlx::query("UPDATE product SET base_price = ? WHERE id = ?")
            .bind(normalized)
            .bind(product_id)
            .execute(crate::db::pool())
            .await;

        // 若开启了自动售价更新，则该同步值会立即被加成率重算覆盖，这里仅记录日志
        log_price_change(
            product_id,
            "base_price",
            old_base_price,
            normalized,
            "sync_base_price",
            None,
            Some("按政府指导价/超市价同步"),
        ).await;

        // 若开启自动更新售价，则按加成率重算 base_price
        recalc_base_price_by_markup(product_id, "sync_base_price", None).await;
    }

    StatusCode::OK
}

pub async fn api_product_price_log_list(
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let product_id: Option<i64> = params.get("product_id").and_then(|s| s.parse().ok());
    let limit: i64 = params.get("limit").and_then(|s| s.parse().ok()).unwrap_or(100);

    let rows = if let Some(pid) = product_id {
        sqlx::query(
            "SELECT ppl.id, ppl.product_id, p.name as product_name, ppl.price_type, ppl.old_price, ppl.new_price, ppl.source, ppl.ref_id, ppl.remark, ppl.changed_at
             FROM product_price_log ppl LEFT JOIN product p ON ppl.product_id = p.id
             WHERE ppl.product_id = ? ORDER BY ppl.changed_at DESC LIMIT ?"
        )
        .bind(pid)
        .bind(limit)
        .fetch_all(crate::db::pool())
        .await
        .unwrap_or_default()
    } else {
        sqlx::query(
            "SELECT ppl.id, ppl.product_id, p.name as product_name, ppl.price_type, ppl.old_price, ppl.new_price, ppl.source, ppl.ref_id, ppl.remark, ppl.changed_at
             FROM product_price_log ppl LEFT JOIN product p ON ppl.product_id = p.id
             ORDER BY ppl.changed_at DESC LIMIT ?"
        )
        .bind(limit)
        .fetch_all(crate::db::pool())
        .await
        .unwrap_or_default()
    };

    let logs: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| serde_json::json!({
            "id": r.get::<i64, _>("id"),
            "product_id": r.get::<i64, _>("product_id"),
            "product_name": r.get::<Option<String>, _>("product_name"),
            "price_type": r.get::<String, _>("price_type"),
            "old_price": r.get::<f64, _>("old_price"),
            "new_price": r.get::<f64, _>("new_price"),
            "source": r.get::<Option<String>, _>("source"),
            "ref_id": r.get::<Option<i64>, _>("ref_id"),
            "remark": r.get::<Option<String>, _>("remark"),
            "changed_at": r.get::<Option<String>, _>("changed_at"),
        }))
        .collect();

    (StatusCode::OK, serde_json::to_string(&logs).unwrap())
}

pub async fn api_category_list() -> impl IntoResponse {
    let rows = sqlx::query("SELECT id, name, parent_id, entity_type, sort_order FROM category ORDER BY entity_type, sort_order, id")
        .fetch_all(crate::db::pool())
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

pub async fn api_category_create(Json(req): Json<CategoryReq>) -> impl IntoResponse {
    let result = sqlx::query(
        "INSERT INTO category(name, parent_id, entity_type, sort_order) VALUES (?, ?, ?, ?)"
    )
    .bind(&req.name)
    .bind(&req.parent_id)
    .bind(&req.entity_type)
    .bind(&req.sort_order)
    .execute(crate::db::pool())
    .await;
    
    match result {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            eprintln!("创建分类失败: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

pub async fn api_category_delete(Json(req): Json<serde_json::Value>) -> (StatusCode, String) {
    let id = req["id"].as_i64().unwrap_or(0);
    // 先检查是否有子分类
    let child_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM category WHERE parent_id = ?")
        .bind(id)
        .fetch_one(crate::db::pool())
        .await
        .unwrap_or(0);
    if child_count > 0 {
        return (StatusCode::BAD_REQUEST, "该分类下有子分类，无法删除".to_string());
    }
    // 检查是否有实体引用
    for table in &["supplier", "purchaser", "product"] {
        let count = match *table {
            "supplier" => sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM supplier WHERE category_id = ?").bind(id).fetch_one(crate::db::pool()).await.unwrap_or(0),
            "purchaser" => sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM purchaser WHERE category_id = ?").bind(id).fetch_one(crate::db::pool()).await.unwrap_or(0),
            "product" => sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM product WHERE category_id = ?").bind(id).fetch_one(crate::db::pool()).await.unwrap_or(0),
            _ => 0,
        };
        if count > 0 {
            return (StatusCode::BAD_REQUEST, format!("该分类已被{}引用，无法删除", table));
        }
    }
    let result = sqlx::query("DELETE FROM category WHERE id = ?").bind(id).execute(crate::db::pool()).await;
    match result {
        Ok(_) => (StatusCode::OK, "删除成功".to_string()),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "删除失败".to_string()),
    }
}

pub async fn api_category_rename(Json(req): Json<CategoryRenameReq>) -> (StatusCode, String) {
    let result = sqlx::query("UPDATE category SET name = ? WHERE id = ?")
        .bind(&req.name)
        .bind(req.id)
        .execute(crate::db::pool())
        .await;
    match result {
        Ok(_) => (StatusCode::OK, "重命名成功".to_string()),
        Err(e) => {
            eprintln!("重命名分类失败: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "重命名失败".to_string())
        }
    }
}

pub async fn api_category_tree(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let entity_type = params.get("entity_type").cloned().unwrap_or_else(|| "product".to_string());
    let rows = sqlx::query("SELECT id, name, parent_id, entity_type, sort_order FROM category ORDER BY sort_order, id")
        .fetch_all(crate::db::pool())
        .await
        .unwrap_or_default();
    
    let tree = build_category_tree_json(&rows, None, &entity_type);
    (StatusCode::OK, serde_json::to_string(&tree).unwrap())
}

pub async fn api_inventory_list() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT i.id, i.product_id, i.warehouse_id, p.name, p.spec, i.quantity, i.min_stock, i.max_stock, w.name as warehouse_name
         FROM inventory i JOIN product p ON i.product_id = p.id LEFT JOIN warehouse w ON i.warehouse_id = w.id"
    )
    .fetch_all(crate::db::pool())
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

pub async fn api_warehouse_list() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT id, name, code, address, contact, phone, status, sort_order, audit_status, create_at, update_at FROM warehouse ORDER BY sort_order, id"
    )
    .fetch_all(crate::db::pool())
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
            "audit_status": row.get::<Option<String>, _>("audit_status"),
            "create_at": row.get::<Option<String>, _>("create_at"),
            "update_at": row.get::<Option<String>, _>("update_at"),
        }))
        .collect();
    
    (StatusCode::OK, serde_json::to_string(&warehouses).unwrap())
}

pub async fn api_warehouse_create(Json(req): Json<WarehouseCreateReq>) -> (StatusCode, String) {
    let result = sqlx::query(
        "INSERT INTO warehouse (name, code, address, contact, phone, sort_order) VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(&req.name)
    .bind(req.code)
    .bind(req.address)
    .bind(req.contact)
    .bind(req.phone)
    .bind(req.sort_order.unwrap_or(0))
    .execute(crate::db::pool())
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

pub async fn api_warehouse_update(Json(req): Json<WarehouseUpdateReq>) -> (StatusCode, String) {
    if let Err((code, msg)) = check_basic_not_confirmed("warehouse", req.id).await {
        return (code, msg);
    }
    let result = sqlx::query(
        "UPDATE warehouse SET name = ?, code = ?, address = ?, contact = ?, phone = ?, status = ?, sort_order = ?, audit_status='pending', update_at = CURRENT_TIMESTAMP WHERE id = ?"
    )
    .bind(&req.name)
    .bind(req.code)
    .bind(req.address)
    .bind(req.contact)
    .bind(req.phone)
    .bind(req.status.unwrap_or(1))
    .bind(req.sort_order.unwrap_or(0))
    .bind(req.id)
    .execute(crate::db::pool())
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

pub async fn api_warehouse_delete(Json(req): Json<std::collections::HashMap<String, i64>>) -> (StatusCode, String) {
    let id = req.get("id").copied().unwrap_or(0);
    if id == 1 {
        return (StatusCode::BAD_REQUEST, "默认仓库无法删除".to_string());
    }
    if let Err((code, msg)) = check_basic_not_confirmed("warehouse", id).await {
        return (code, msg);
    }
    
    let count: i64 = sqlx::query("SELECT COUNT(*) FROM inventory WHERE warehouse_id = ?")
        .bind(id)
        .fetch_one(crate::db::pool())
        .await
        .map(|r| r.get(0))
        .unwrap_or(0);
    
    if count > 0 {
        return (StatusCode::BAD_REQUEST, "该仓库存在库存记录，无法删除".to_string());
    }
    
    let result = sqlx::query("DELETE FROM warehouse WHERE id = ?")
        .bind(id)
        .execute(crate::db::pool())
        .await;
    
    match result {
        Ok(_) => (StatusCode::OK, "删除成功".to_string()),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "删除失败".to_string()),
    }
}

pub async fn api_order_generate_no(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let default_type = "purchase".to_string();
    let order_type = params.get("type").unwrap_or(&default_type);
    let default_date = Local::now().format("%Y-%m-%d").to_string();
    let order_date = params.get("date").unwrap_or(&default_date);
    
    let order_no = generate_order_no(order_type, order_date).await;
    
    (StatusCode::OK, serde_json::to_string(&serde_json::json!({ "order_no": order_no })).unwrap())
}

pub async fn api_product_set_auto_update_price(
    Json(req): Json<std::collections::HashMap<String, i64>>,
) -> impl IntoResponse {
    let product_id = match req.get("product_id") {
        Some(&id) => id,
        None => return (StatusCode::BAD_REQUEST, serde_json::json!({"error": "missing product_id"}).to_string()).into_response(),
    };
    let auto = req.get("auto_update_price").copied().unwrap_or(1);

    let _ = sqlx::query("UPDATE product SET auto_update_price = ? WHERE id = ?")
        .bind(auto)
        .bind(product_id)
        .execute(crate::db::pool())
        .await;

    recalc_base_price_by_markup(product_id, "set_auto_update_price", None).await;

    (StatusCode::OK, serde_json::json!({"ok": true}).to_string()).into_response()
}

pub async fn api_product_batch_set_auto_update_price(
    Json(req): Json<std::collections::HashMap<String, i64>>,
) -> impl IntoResponse {
    let auto = req.get("auto_update_price").copied().unwrap_or(1);
    let _ = sqlx::query("UPDATE product SET auto_update_price = ?")
        .bind(auto)
        .execute(crate::db::pool())
        .await;

    if auto == 1 {
        eprintln!("[批量售价自动更新] 开始处理所有商品（auto=开启）");
        // 开启时，对所有进价>0 的商品按加成率重算售价
        let rows = sqlx::query("SELECT id, purchase_price, base_price, markup_rate FROM product WHERE purchase_price > 0")
            .fetch_all(crate::db::pool())
            .await
            .unwrap_or_default();
        eprintln!("[批量售价自动更新] 共 {} 个商品需要重算", rows.len());
        for r in rows {
            let pid: i64 = r.get("id");
            let old_base: f64 = r.get("base_price");
            let purchase: f64 = r.get("purchase_price");
            let markup: f64 = r.get("markup_rate");
            let raw = purchase * (1.0 + markup);
            let new_base = round_to_allowed_last_digit(raw);
            eprintln!(
                "[批量售价自动更新] 商品ID={} 进价={:.4} 加成率={:.4} 原始售价={:.6} 取整后={:.4} 旧售价={:.4}",
                pid, purchase, markup, raw, new_base, old_base
            );
            if (old_base - new_base).abs() >= 0.001 {
                let _ = sqlx::query("UPDATE product SET base_price = ? WHERE id = ?")
                    .bind(new_base)
                    .bind(pid)
                    .execute(crate::db::pool())
                    .await;
                log_price_change(
                    pid,
                    "base_price",
                    old_base,
                    new_base,
                    "batch_set_auto_update_price",
                    None,
                    Some("批量开启自动更新售价"),
                ).await;
            }
        }
    }

    (StatusCode::OK, serde_json::json!({"ok": true}).to_string()).into_response()
}

pub async fn api_product_last_purchase_price(
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let product_id: i64 = match params.get("product_id").and_then(|s| s.parse().ok()) {
        Some(v) => v,
        None => return (StatusCode::BAD_REQUEST, serde_json::json!({"error": "missing product_id"}).to_string()).into_response(),
    };

    // 优先用商品表里维护的 purchase_price（每次采购后已同步）
    let row = sqlx::query(
        "SELECT purchase_price, base_unit FROM product WHERE id = ?"
    )
    .bind(product_id)
    .fetch_optional(crate::db::pool())
    .await
    .ok()
    .flatten();

    if let Some(r) = row {
        let purchase_price: f64 = r.get("purchase_price");
        let base_unit: String = r.get("base_unit");
        // 同时取出最近一次采购的原始下单单价及单位，便于前端核对是否同基础单位
        let last_row = sqlx::query(
            "SELECT poi.unit_price, poi.unit, p.base_unit
             FROM purchase_order_item poi
             JOIN purchase_order po ON poi.order_id = po.id
             JOIN product p ON poi.product_id = p.id
             WHERE poi.product_id = ?
             ORDER BY po.order_date DESC, po.id DESC
             LIMIT 1"
        )
        .bind(product_id)
        .fetch_optional(crate::db::pool())
        .await
        .ok()
        .flatten();

        let (last_unit_price, last_unit) = if let Some(lr) = last_row {
            (
                lr.get::<f64, _>("unit_price"),
                lr.get::<String, _>("unit"),
            )
        } else {
            (0.0, base_unit.clone())
        };

        let payload = serde_json::json!({
            "purchase_price": purchase_price,
            "base_unit": base_unit,
            "last_unit_price": last_unit_price,
            "last_unit": last_unit,
        });
        return (StatusCode::OK, serde_json::to_string(&payload).unwrap()).into_response();
    }

    (StatusCode::NOT_FOUND, serde_json::json!({"error": "product not found"}).to_string()).into_response()
}

pub async fn api_purchase_order_create(headers: axum::http::HeaderMap, Json(req): Json<PurchaseOrderReq>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/purchase_order/create").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let ctx = crate::auth::get_user_ctx(&headers).await;

    // 仅已审核的供应商允许下单
    if !is_audit_confirmed("supplier", req.supplier_id).await {
        return (StatusCode::BAD_REQUEST, "该供应商尚未审核通过，暂不能下单".to_string());
    }

    // 行级数据权限：supplier 只能为自己绑定的供应商创建采购单
    if ctx.role == "supplier" {
        let effective_supplier_id = if req.supplier_id != 0 { req.supplier_id } else { ctx.supplier_id };
        if ctx.supplier_id == 0 || effective_supplier_id != ctx.supplier_id {
            return (StatusCode::FORBIDDEN, "供应商账号只能为自己创建采购单".to_string());
        }
    }

    // 主表仓库按明细汇总：各行全部同一仓库则记该仓库；否则仓库名去重后以"、"连接
    let mut wh_id_set: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut wh_names: Vec<String> = Vec::new();
    for it in &req.items {
        let wid = it.warehouse_id.unwrap_or(0);
        let wname = it.warehouse_name.clone().unwrap_or_default();
        if wid > 0 { wh_id_set.insert(wid); }
        if !wname.trim().is_empty() && !wh_names.contains(&wname) { wh_names.push(wname); }
    }
    let main_wh_id = if wh_id_set.len() == 1 { *wh_id_set.iter().next().unwrap() } else { 0 };
    let main_wh_name = wh_names.join("、");

    let result = sqlx::query(
        "INSERT INTO purchase_order(supplier_id, order_no, order_date, total_amount, discount_rate, amount_reduction, final_amount, warehouse_id, warehouse_name, user_id, handler_phone, remark, is_settled) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(req.supplier_id)
    .bind(&req.order_no)
    .bind(&req.order_date)
    .bind(req.total_amount)
    .bind(req.discount_rate)
    .bind(req.amount_reduction)
    .bind(req.final_amount)
    .bind(main_wh_id)
    .bind(&main_wh_name)
    .bind(req.user_id.unwrap_or(0))
    .bind(&req.handler_phone.clone().unwrap_or_default())
    .bind(&req.remark)
    .bind(req.is_settled.unwrap_or(0))
    .execute(crate::db::pool())
    .await;
    
    match result {
        Ok(res) => {
            let order_id = res.last_insert_rowid();
            if !req.items.is_empty() {
                let placeholders: Vec<String> = req.items.iter()
                    .map(|_| "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)".to_string())
                    .collect();
                let sql = format!(
                    "INSERT INTO purchase_order_item(order_id, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, base_quantity, amount, ordered_quantity, remark, warehouse_id, warehouse_name) VALUES {}",
                    placeholders.join(", ")
                );
                
                let mut query = sqlx::query(AssertSqlSafe(sql.as_str()));
                for item in &req.items {
                    query = query
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
                        .bind(item.ordered_quantity.unwrap_or(0.0))
                        .bind(&item.remark)
                        .bind(item.warehouse_id.unwrap_or(0))
                        .bind(&item.warehouse_name.clone().unwrap_or_default());
                }
                let _ = query.execute(crate::db::pool()).await;
                // 采购入库后更新商品进价（当前/最高/最低）
                update_product_purchase_prices(&req.items).await;
            }
            crate::auth::log_operation(&ctx, "purchase_order.create", "purchase_order", &order_id.to_string(),
                &format!("创建采购单 {}（供应商ID={}）", req.order_no, req.supplier_id)).await;
            (StatusCode::OK, "创建成功".to_string())
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "创建失败".to_string()),
    }
}

pub async fn api_purchase_order_list(headers: axum::http::HeaderMap, axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let keyword_pattern = parse_keyword_pattern(&params);
    
    let page: i64 = params.get("page").and_then(|s| s.parse().ok()).unwrap_or(1);
    let page_size: i64 = params.get("page_size").and_then(|s| s.parse().ok()).unwrap_or(20);
    let offset = (page - 1) * page_size;
    
    // 行级数据权限：supplier 角色强制只看自己绑定的供应商
    let ctx = crate::auth::get_user_ctx(&headers).await;
    let supplier_id: Option<i64> = if ctx.role == "supplier" {
        if ctx.supplier_id > 0 { Some(ctx.supplier_id) } else { Some(-1) /* 未绑定则查不到任何数据 */ }
    } else {
        params.get("supplier_id").and_then(|s| s.parse().ok())
    };
    
    // 排序处理
    let sort_field = params.get("sort_field").map(|s| s.as_str()).unwrap_or("id");
    let sort_order = params.get("sort_order").map(|s| s.as_str()).unwrap_or("desc");
    let order_clause = match sort_field {
        "order_no" => format!("po.order_no {}", sort_order),
        "order_date" => format!("po.order_date {}", sort_order),
        "unit_name" => format!("s.name {}", sort_order),
        "status" => format!("po.status {}", sort_order),
        _ => format!("po.id {}", sort_order),
    };
    
    let is_settled_filter: Option<String> = match params.get("is_settled").map(|s| s.as_str()) {
        Some("0") | Some("1") => params.get("is_settled").cloned(),
        _ => None,
    };

    let (total_sql, total_params) = match (supplier_id, is_settled_filter.as_ref()) {
        (Some(sid), Some(isf)) => (
            "SELECT COUNT(*) as count FROM purchase_order po JOIN supplier s ON po.supplier_id = s.id
             WHERE po.supplier_id = ? AND po.is_settled = ? AND (po.order_no LIKE ? OR s.name LIKE ? OR po.order_date LIKE ?)",
            vec![sid.to_string(), isf.clone(), keyword_pattern.clone(), keyword_pattern.clone(), keyword_pattern.clone()]
        ),
        (Some(sid), None) => (
            "SELECT COUNT(*) as count FROM purchase_order po JOIN supplier s ON po.supplier_id = s.id
             WHERE po.supplier_id = ? AND (po.order_no LIKE ? OR s.name LIKE ? OR po.order_date LIKE ?)",
            vec![sid.to_string(), keyword_pattern.clone(), keyword_pattern.clone(), keyword_pattern.clone()]
        ),
        (None, Some(isf)) => (
            "SELECT COUNT(*) as count FROM purchase_order po JOIN supplier s ON po.supplier_id = s.id
             WHERE po.is_settled = ? AND (po.order_no LIKE ? OR s.name LIKE ? OR po.order_date LIKE ?)",
            vec![isf.clone(), keyword_pattern.clone(), keyword_pattern.clone(), keyword_pattern.clone()]
        ),
        (None, None) => (
            "SELECT COUNT(*) as count FROM purchase_order po JOIN supplier s ON po.supplier_id = s.id
             WHERE po.order_no LIKE ? OR s.name LIKE ? OR po.order_date LIKE ?",
            vec![keyword_pattern.clone(), keyword_pattern.clone(), keyword_pattern.clone()]
        ),
    };

    let mut total_query = sqlx::query(total_sql);
    for p in &total_params {
        total_query = total_query.bind(p);
    }
    let total_rows = total_query.fetch_one(crate::db::pool()).await.unwrap();
    let total: i64 = total_rows.get("count");

    let (sql, query_params) = match (supplier_id, is_settled_filter.as_ref()) {
        (Some(sid), Some(isf)) => (
            format!(
                "SELECT po.id, po.order_no, po.order_date, po.total_amount, po.discount_rate, po.amount_reduction, po.final_amount, po.status, po.remark, po.warehouse_id, po.warehouse_name, po.is_settled, s.name as supplier_name,
                        (SELECT GROUP_CONCAT(DISTINCT NULLIF(TRIM(poi.warehouse_name), ''))
                         FROM purchase_order_item poi
                         WHERE poi.order_id = po.id) as item_warehouse_names,
                        (SELECT COUNT(DISTINCT NULLIF(TRIM(poi.warehouse_name), ''))
                         FROM purchase_order_item poi
                         WHERE poi.order_id = po.id) as item_warehouse_count
                 FROM purchase_order po JOIN supplier s ON po.supplier_id = s.id
                 WHERE po.supplier_id = ? AND po.is_settled = ? AND (po.order_no LIKE ? OR s.name LIKE ? OR po.order_date LIKE ?)
                 ORDER BY {} LIMIT ? OFFSET ?",
                order_clause
            ),
            vec![sid.to_string(), isf.clone(), keyword_pattern.clone(), keyword_pattern.clone(), keyword_pattern.clone()]
        ),
        (Some(sid), None) => (
            format!(
                "SELECT po.id, po.order_no, po.order_date, po.total_amount, po.discount_rate, po.amount_reduction, po.final_amount, po.status, po.remark, po.warehouse_id, po.warehouse_name, po.is_settled, s.name as supplier_name,
                        (SELECT GROUP_CONCAT(DISTINCT NULLIF(TRIM(poi.warehouse_name), ''))
                         FROM purchase_order_item poi
                         WHERE poi.order_id = po.id) as item_warehouse_names,
                        (SELECT COUNT(DISTINCT NULLIF(TRIM(poi.warehouse_name), ''))
                         FROM purchase_order_item poi
                         WHERE poi.order_id = po.id) as item_warehouse_count
                 FROM purchase_order po JOIN supplier s ON po.supplier_id = s.id
                 WHERE po.supplier_id = ? AND (po.order_no LIKE ? OR s.name LIKE ? OR po.order_date LIKE ?)
                 ORDER BY {} LIMIT ? OFFSET ?",
                order_clause
            ),
            vec![sid.to_string(), keyword_pattern.clone(), keyword_pattern.clone(), keyword_pattern.clone()]
        ),
        (None, Some(isf)) => (
            format!(
                "SELECT po.id, po.order_no, po.order_date, po.total_amount, po.discount_rate, po.amount_reduction, po.final_amount, po.status, po.remark, po.warehouse_id, po.warehouse_name, po.is_settled, s.name as supplier_name,
                        (SELECT GROUP_CONCAT(DISTINCT NULLIF(TRIM(poi.warehouse_name), ''))
                         FROM purchase_order_item poi
                         WHERE poi.order_id = po.id) as item_warehouse_names,
                        (SELECT COUNT(DISTINCT NULLIF(TRIM(poi.warehouse_name), ''))
                         FROM purchase_order_item poi
                         WHERE poi.order_id = po.id) as item_warehouse_count
                 FROM purchase_order po JOIN supplier s ON po.supplier_id = s.id
                 WHERE po.is_settled = ? AND (po.order_no LIKE ? OR s.name LIKE ? OR po.order_date LIKE ?)
                 ORDER BY {} LIMIT ? OFFSET ?",
                order_clause
            ),
            vec![isf.clone(), keyword_pattern.clone(), keyword_pattern.clone(), keyword_pattern.clone()]
        ),
        (None, None) => (
            format!(
                "SELECT po.id, po.order_no, po.order_date, po.total_amount, po.discount_rate, po.amount_reduction, po.final_amount, po.status, po.remark, po.warehouse_id, po.warehouse_name, po.is_settled, s.name as supplier_name,
                        (SELECT GROUP_CONCAT(DISTINCT NULLIF(TRIM(poi.warehouse_name), ''))
                         FROM purchase_order_item poi
                         WHERE poi.order_id = po.id) as item_warehouse_names,
                        (SELECT COUNT(DISTINCT NULLIF(TRIM(poi.warehouse_name), ''))
                         FROM purchase_order_item poi
                         WHERE poi.order_id = po.id) as item_warehouse_count
                 FROM purchase_order po JOIN supplier s ON po.supplier_id = s.id
                 WHERE po.order_no LIKE ? OR s.name LIKE ? OR po.order_date LIKE ?
                 ORDER BY {} LIMIT ? OFFSET ?",
                order_clause
            ),
            vec![keyword_pattern.clone(), keyword_pattern.clone(), keyword_pattern.clone()]
        ),
    };

    let mut query = sqlx::query(AssertSqlSafe(sql.as_str()));
    for p in &query_params {
        query = query.bind(p);
    }
    query = query.bind(page_size).bind(offset);
    let rows = query.fetch_all(crate::db::pool()).await.unwrap_or_default();

    let orders: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let item_wh_count: i64 = row.get::<i64, _>("item_warehouse_count");
            let item_wh_names: Option<String> = row.try_get::<Option<String>, _>("item_warehouse_names").unwrap_or(None);
            let main_wh_name: Option<String> = row.try_get::<Option<String>, _>("warehouse_name").unwrap_or(None);

            // 仓库展示规则：
            // 1) 明细里无任何仓库名 → 沿用主表仓库名
            // 2) 明细里只有一个去重仓库名 → 显示该仓库名
            // 3) 明细里有 ≥2 个去重仓库名 → 显示"综合"
            let display_warehouse = if item_wh_count == 0 {
                main_wh_name.unwrap_or_default()
            } else if item_wh_count == 1 {
                item_wh_names.unwrap_or_default()
            } else {
                "综合".to_string()
            };

            serde_json::json!({
                "id": row.get::<i64, _>("id"),
                "order_no": row.get::<String, _>("order_no"),
                "order_date": row.get::<String, _>("order_date"),
                "total_amount": row.get::<f64, _>("total_amount"),
                "discount_rate": row.get::<f64, _>("discount_rate"),
                "amount_reduction": row.get::<f64, _>("amount_reduction"),
                "final_amount": row.get::<f64, _>("final_amount"),
                "warehouse_id": row.get::<i64, _>("warehouse_id"),
                "warehouse_name": display_warehouse,
                "status": row.get::<String, _>("status"),
                "remark": row.get::<Option<String>, _>("remark"),
                "supplier_name": row.get::<String, _>("supplier_name"),
                "is_settled": row.get::<i64, _>("is_settled"),
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

pub async fn api_purchase_order_detail(headers: axum::http::HeaderMap, Path(id): Path<i64>) -> impl IntoResponse {
    let ctx = crate::auth::get_user_ctx(&headers).await;
    let order_row = sqlx::query(
        "SELECT po.id, po.supplier_id, po.order_no, po.order_date, po.total_amount, po.discount_rate, po.amount_reduction, po.final_amount, po.status, po.remark, po.warehouse_id, po.warehouse_name, po.user_id, po.version, po.is_settled, s.name as supplier_name
         FROM purchase_order po JOIN supplier s ON po.supplier_id = s.id WHERE po.id = ?"
    )
    .bind(id)
    .fetch_optional(crate::db::pool())
    .await
    .unwrap_or(None);
    
    if order_row.is_none() {
        return (StatusCode::NOT_FOUND, "订单不存在".to_string());
    }
    
    let row = order_row.unwrap();
    // 行级数据权限：supplier 只能看自己的
    let order_supplier_id: i64 = row.get("supplier_id");
    if !crate::auth::can_access_purchase_order(&ctx, order_supplier_id) {
        return (StatusCode::FORBIDDEN, "您没有权限查看此订单".to_string());
    }
    
    let item_rows = sqlx::query(
        "SELECT id, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, base_quantity, amount, ordered_quantity, remark, warehouse_id, warehouse_name FROM purchase_order_item WHERE order_id = ?"
    )
    .bind(id)
    .fetch_all(crate::db::pool())
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
            "ordered_quantity": r.get::<Option<f64>, _>("ordered_quantity"),
            "remark": r.get::<Option<String>, _>("remark"),
            "warehouse_id": r.get::<i64, _>("warehouse_id"),
            "warehouse_name": r.get::<Option<String>, _>("warehouse_name"),
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
        "version": row.get::<i64, _>("version"),
        "is_settled": row.get::<i64, _>("is_settled"),
        "remark": row.get::<Option<String>, _>("remark"),
        "supplier_name": row.get::<String, _>("supplier_name"),
        "items": items,
    });
    
    (StatusCode::OK, serde_json::to_string(&order).unwrap())
}

/// 采购单明细单条 INSERT（更新流程中新增明细/兜底插入共用）：
/// source_sales_order_id 用主表 source 兜底，保证新增明细归属到生成它的销售单。
async fn insert_purchase_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    order_id: i64,
    item: &crate::models::PurchaseOrderItemReq,
    fallback_source: Option<i64>,
) {
    let _ = sqlx::query(
        "INSERT INTO purchase_order_item(order_id, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, base_quantity, amount, ordered_quantity, remark, warehouse_id, warehouse_name, sales_unit, source_sales_order_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
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
    .bind(item.ordered_quantity.unwrap_or(0.0))
    .bind(&item.remark)
    .bind(item.warehouse_id.unwrap_or(0))
    .bind(&item.warehouse_name.clone().unwrap_or_default())
    .bind(&item.unit.clone().unwrap_or_default())
    .bind(fallback_source)
    .execute(&mut **tx)
    .await
    .ok();
}

pub async fn api_purchase_order_update(headers: axum::http::HeaderMap, Json(req): Json<PurchaseOrderReq>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/purchase_order/update").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let ctx = crate::auth::get_user_ctx(&headers).await;

    // 行级数据权限：先查订单所属供应商
    let order = sqlx::query("SELECT supplier_id, status FROM purchase_order WHERE id = ?")
        .bind(req.id)
        .fetch_optional(crate::db::pool())
        .await
        .ok()
        .flatten();
    let order = match order {
        Some(o) => o,
        None => return (StatusCode::NOT_FOUND, "订单不存在".to_string()),
    };
    let order_supplier_id: i64 = order.get("supplier_id");
    let order_status: String = order.get("status");
    if !crate::auth::can_access_purchase_order(&ctx, order_supplier_id) {
        return (StatusCode::FORBIDDEN, "您没有权限操作此订单".to_string());
    }
    // 仅待审核（pending）状态的订单允许修改；已审核/已流转/已作废的订单必须反审核后才能修改（防篡改）
    if order_status != "pending" {
        return (StatusCode::BAD_REQUEST, format!("当前订单状态为「{}」，仅待审核状态的订单允许修改；已审核订单需管理员反审核后才能修改", order_status));
    }
    // 强制乐观锁：req.version 必须传入且 > 0。
    // 前端一定传 version（从 loadOrderDetail 取得），如果缺失说明调用方不是前端合法路径，直接拒绝。
    let ver = match req.version {
        Some(v) if v > 0 => v,
        _ => return (StatusCode::BAD_REQUEST, "保存失败：缺少订单版本号，请刷新页面后重试".to_string()),
    };
    // 安全护栏：更新时明细不允许为空（防止并发覆盖导致明细被清空）
    if req.items.is_empty() {
        return (StatusCode::BAD_REQUEST, "保存失败：订单明细不能为空".to_string());
    }

    // 主表仓库按明细汇总（与创建逻辑一致）
    let mut wh_id_set: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut wh_names: Vec<String> = Vec::new();
    for it in &req.items {
        let wid = it.warehouse_id.unwrap_or(0);
        let wname = it.warehouse_name.clone().unwrap_or_default();
        if wid > 0 { wh_id_set.insert(wid); }
        if !wname.trim().is_empty() && !wh_names.contains(&wname) { wh_names.push(wname); }
    }
    let main_wh_id = if wh_id_set.len() == 1 { *wh_id_set.iter().next().unwrap() } else { 0 };
    let main_wh_name = wh_names.join("、");

    // 用 BEGIN IMMEDIATE 启动写事务，立即获得 SQLite 写锁（RESERVED）。
    // 作用：把"主表 UPDATE + 明细同步"整体串行化，
    // 杜绝连点两次保存时两个事务都读到同一 version、都通过乐观锁、都把主表 version 自增、
    // 然后后提交的事务用全量 items 把先提交的事务的明细覆盖/丢失。
    let mut tx = match crate::db::pool().begin_with("BEGIN IMMEDIATE").await {
        Ok(t) => t,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "更新失败：事务启动失败".to_string()),
    };

    // 事务内做"主表 UPDATE + version 自增"：WHERE 同时校验 id 和 version。
    // 如果行不存在或 version 已被他人递增，rows_affected = 0 → 回滚事务 + 返回 409。
    let upd = sqlx::query(
        "UPDATE purchase_order SET supplier_id = ?, order_no = ?, order_date = ?, total_amount = ?, discount_rate = ?, amount_reduction = ?, final_amount = ?, warehouse_id = ?, warehouse_name = ?, user_id = ?, handler_phone = ?, remark = ?, is_settled = ?, version = version + 1 WHERE id = ? AND version = ?"
    )
    .bind(req.supplier_id)
    .bind(&req.order_no)
    .bind(&req.order_date)
    .bind(req.total_amount)
    .bind(req.discount_rate)
    .bind(req.amount_reduction)
    .bind(req.final_amount)
    .bind(main_wh_id)
    .bind(&main_wh_name)
    .bind(req.user_id.unwrap_or(0))
    .bind(&req.handler_phone.clone().unwrap_or_default())
    .bind(&req.remark)
    .bind(req.is_settled.unwrap_or(0))
    .bind(req.id)
    .bind(ver)
    .execute(&mut *tx)
    .await;

    let upd_res = match upd {
        Ok(r) => r,
        Err(e) => {
            // 出现异常，让事务 drop 时自动回滚
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("更新失败: {}", e));
        }
    };
    if upd_res.rows_affected() == 0 {
        // version 已被他人递增（极端并发）→ 整个事务回滚（其实只 UPDATE 了一行 ROLLBACK 也无害）
        return (StatusCode::CONFLICT, format!("订单已被其他用户修改（版本 {} 已被覆盖），请刷新后重试", ver));
    }
    let new_version = ver + 1;
    let old_order_no = req.order_no.clone();
    let old_total = req.total_amount;
    let old_final = req.final_amount;

    // 按 id 精确同步明细（保留每条明细的 source_sales_order_id 归属）
    let po_main_source: Option<i64> = sqlx::query("SELECT source_sales_order_id FROM purchase_order WHERE id = ?")
        .bind(req.id)
        .fetch_optional(&mut *tx)
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get("source_sales_order_id").ok().flatten());

    let existing_by_id: std::collections::HashMap<i64, Option<i64>> = sqlx::query(
        "SELECT id, source_sales_order_id FROM purchase_order_item WHERE order_id = ?"
    )
    .bind(req.id)
    .fetch_all(&mut *tx)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|r| {
        let id: i64 = r.get("id");
        let src: Option<i64> = r.try_get("source_sales_order_id").ok().flatten();
        (id, src)
    })
    .collect();

    let mut req_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for item in &req.items {
        if let Some(iid) = item.id {
            if iid > 0 {
                req_ids.insert(iid);
            }
        }
    }
    for (db_id, _src) in &existing_by_id {
        if !req_ids.contains(db_id) {
            let _ = sqlx::query("DELETE FROM purchase_order_item WHERE id = ? AND order_id = ?")
                .bind(db_id)
                .bind(req.id)
                .execute(&mut *tx)
                .await;
        }
    }

    for item in &req.items {
        if let Some(iid) = item.id {
            if iid > 0 && existing_by_id.contains_key(&iid) {
                let _ = sqlx::query(
                    "UPDATE purchase_order_item SET product_id = ?, product_name = ?, alias1 = ?, alias2 = ?, spec = ?, unit = ?, unit_price = ?, quantity = ?, base_quantity = ?, amount = ?, ordered_quantity = ?, remark = ?, warehouse_id = ?, warehouse_name = ? WHERE id = ? AND order_id = ?"
                )
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
                .bind(item.ordered_quantity.unwrap_or(0.0))
                .bind(&item.remark)
                .bind(item.warehouse_id.unwrap_or(0))
                .bind(&item.warehouse_name.clone().unwrap_or_default())
                .bind(iid)
                .bind(req.id)
                .execute(&mut *tx)
                .await
                .ok();
            } else {
                insert_purchase_item(&mut tx, req.id.unwrap_or(0), item, po_main_source).await;
            }
        } else {
            insert_purchase_item(&mut tx, req.id.unwrap_or(0), item, po_main_source).await;
        }
    }
    if let Err(_) = tx.commit().await {
        return (StatusCode::INTERNAL_SERVER_ERROR, "更新失败：事务提交失败".to_string());
    }
    if !req.items.is_empty() {
        // 采购单更新后同步商品进价（当前/最高/最低）
        update_product_purchase_prices(&req.items).await;
    }
    crate::auth::log_operation(&ctx, "purchase_order.update", "purchase_order", &req.id.unwrap_or(0).to_string(),
        &format!("更新采购单 {}（原单号 {}）：金额 {:.2}→{:.2}，下浮后合计 {:.2}→{:.2}，版本 {}→{}",
            req.order_no, old_order_no, old_total, req.total_amount, old_final, req.final_amount, ver, new_version)).await;
    (StatusCode::OK, "更新成功".to_string())
}

pub async fn api_purchase_order_delete(headers: axum::http::HeaderMap, Path(id): Path<i64>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/purchase_order/delete").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let ctx = crate::auth::get_user_ctx(&headers).await;

    // 行级数据权限 + 状态约束
    let order = sqlx::query("SELECT supplier_id, status FROM purchase_order WHERE id = ?")
        .bind(id)
        .fetch_optional(crate::db::pool())
        .await
        .ok()
        .flatten();
    let order = match order {
        Some(o) => o,
        None => return (StatusCode::NOT_FOUND, "订单不存在".to_string()),
    };
    let order_supplier_id: i64 = order.get("supplier_id");
    let order_status: String = order.get("status");
    if !crate::auth::can_access_purchase_order(&ctx, order_supplier_id) {
        return (StatusCode::FORBIDDEN, "您没有权限操作此订单".to_string());
    }
    if order_status != "pending" {
        return (StatusCode::BAD_REQUEST, format!("当前订单状态为「{}」，仅待审核状态的订单允许删除；已审核订单需管理员反审核后才能删除", order_status));
    }

    // 删除前收集本单涉及的商品，便于删除后从剩余采购历史重算最近/最高/最低进价
    let mut affected_products: std::collections::HashSet<i64> = std::collections::HashSet::new();
    if let Ok(rows) = sqlx::query("SELECT DISTINCT product_id FROM purchase_order_item WHERE order_id = ? AND product_id > 0")
        .bind(id)
        .fetch_all(crate::db::pool())
        .await
    {
        for r in rows {
            if let Ok(pid) = r.try_get::<i64, _>("product_id") {
                affected_products.insert(pid);
            }
        }
    }

    sqlx::query("DELETE FROM purchase_order_item WHERE order_id = ?")
        .bind(id)
        .execute(crate::db::pool())
        .await
        .ok();
    
    let result = sqlx::query("DELETE FROM purchase_order WHERE id = ?")
        .bind(id)
        .execute(crate::db::pool())
        .await;
    
    match result {
        Ok(_) => {
            // 删除成功后，对涉及的商品从剩余采购历史重算进价（回滚/刷新最高/最低/最近价）
            for pid in affected_products {
                crate::recalc_product_purchase_prices_from_history(pid).await;
            }
            crate::auth::log_operation(&ctx, "purchase_order.delete", "purchase_order", &id.to_string(),
                &format!("删除采购单 ID={}", id)).await;
            (StatusCode::OK, "删除成功".to_string())
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "删除失败".to_string()),
    }
}

/// 设置采购订单结算状态（0=未结 1=已结），无需版本号，操作列直接调用
pub async fn api_purchase_order_settle(headers: axum::http::HeaderMap, Path(id): Path<i64>, Json(data): Json<serde_json::Value>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/purchase_order/settle").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let ctx = crate::auth::get_user_ctx(&headers).await;

    let order = sqlx::query("SELECT supplier_id FROM purchase_order WHERE id = ?")
        .bind(id)
        .fetch_optional(crate::db::pool())
        .await
        .ok()
        .flatten();
    let order = match order {
        Some(o) => o,
        None => return (StatusCode::NOT_FOUND, "订单不存在".to_string()),
    };
    let order_supplier_id: i64 = order.get("supplier_id");
    if !crate::auth::can_access_purchase_order(&ctx, order_supplier_id) {
        return (StatusCode::FORBIDDEN, "您没有权限操作此订单".to_string());
    }

    let is_settled: i64 = data["is_settled"].as_i64().unwrap_or(0);
    if is_settled != 0 && is_settled != 1 {
        return (StatusCode::BAD_REQUEST, "参数无效".to_string());
    }

    let result = sqlx::query("UPDATE purchase_order SET is_settled = ? WHERE id = ?")
        .bind(is_settled)
        .bind(id)
        .execute(crate::db::pool())
        .await;

    match result {
        Ok(res) if res.rows_affected() > 0 => {
            let label = if is_settled == 1 { "已结" } else { "未结" };
            crate::auth::log_operation(&ctx, "purchase_order.settle", "purchase_order", &id.to_string(),
                &format!("设置采购单结算状态为「{}」", label)).await;
            (StatusCode::OK, "操作成功".to_string())
        }
        _ => (StatusCode::CONFLICT, "订单不存在或状态已变化".to_string()),
    }
}

pub async fn api_purchase_order_approve(headers: axum::http::HeaderMap, Path(id): Path<i64>, Json(data): Json<serde_json::Value>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/purchase_order/approve").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let ctx = crate::auth::get_user_ctx(&headers).await;

    let order = sqlx::query("SELECT supplier_id, status FROM purchase_order WHERE id = ?")
        .bind(id)
        .fetch_optional(crate::db::pool())
        .await
        .ok()
        .flatten();
    let order = match order {
        Some(o) => o,
        None => return (StatusCode::NOT_FOUND, "订单不存在".to_string()),
    };
    let order_supplier_id: i64 = order.get("supplier_id");
    let order_status: String = order.get("status");
    if !crate::auth::can_access_purchase_order(&ctx, order_supplier_id) {
        return (StatusCode::FORBIDDEN, "您没有权限操作此订单".to_string());
    }
    if order_status == "confirmed" {
        return (StatusCode::BAD_REQUEST, "订单已审核，请勿重复操作".to_string());
    }
    if order_status != "pending" {
        return (StatusCode::BAD_REQUEST, format!("当前订单状态为「{}」，仅待审核状态的订单允许审核", order_status));
    }

    let reason = data["reason"].as_str().unwrap_or("").trim().to_string();
    let result = sqlx::query("UPDATE purchase_order SET status = 'confirmed', is_settled = 1, version = version + 1 WHERE id = ? AND status = 'pending'")
        .bind(id)
        .execute(crate::db::pool())
        .await;

    match result {
        Ok(res) if res.rows_affected() > 0 => {
            crate::auth::log_operation(&ctx, "purchase_order.approve", "purchase_order", &id.to_string(),
                &format!("审核通过采购单 ID={}（{}）", id, if reason.is_empty() { "无备注" } else { &reason })).await;
            (StatusCode::OK, "审核成功，订单已锁定".to_string())
        }
        _ => (StatusCode::CONFLICT, "订单状态已变化，请刷新后重试".to_string()),
    }
}

pub async fn api_purchase_order_unapprove(headers: axum::http::HeaderMap, Path(id): Path<i64>, Json(data): Json<serde_json::Value>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/purchase_order/unapprove").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let ctx = crate::auth::get_user_ctx(&headers).await;

    let reason = data["reason"].as_str().unwrap_or("").trim().to_string();
    if reason.is_empty() {
        return (StatusCode::BAD_REQUEST, "反审核必须填写原因".to_string());
    }

    let order = sqlx::query("SELECT supplier_id, status FROM purchase_order WHERE id = ?")
        .bind(id)
        .fetch_optional(crate::db::pool())
        .await
        .ok()
        .flatten();
    let order = match order {
        Some(o) => o,
        None => return (StatusCode::NOT_FOUND, "订单不存在".to_string()),
    };
    let order_supplier_id: i64 = order.get("supplier_id");
    let order_status: String = order.get("status");
    if !crate::auth::can_access_purchase_order(&ctx, order_supplier_id) {
        return (StatusCode::FORBIDDEN, "您没有权限操作此订单".to_string());
    }
    // 超级管理员拥有任何时刻的反审核权限；其他角色仅允许对已审核（confirmed）订单反审核
    let is_super_admin = ctx.role == "super_admin";
    if !is_super_admin && order_status != "confirmed" {
        return (StatusCode::BAD_REQUEST, format!("当前订单状态为「{}」，仅已审核状态的订单允许反审核", order_status));
    }

    let update_sql = if is_super_admin {
        "UPDATE purchase_order SET status = 'pending', is_settled = 0, version = version + 1 WHERE id = ?"
    } else {
        "UPDATE purchase_order SET status = 'pending', is_settled = 0, version = version + 1 WHERE id = ? AND status = 'confirmed'"
    };
    let result = sqlx::query(update_sql)
        .bind(id)
        .execute(crate::db::pool())
        .await;

    match result {
        Ok(res) if res.rows_affected() > 0 => {
            crate::auth::log_operation(&ctx, "purchase_order.unapprove", "purchase_order", &id.to_string(),
                &format!("反审核采购单 ID={}，原因：{}", id, reason)).await;
            (StatusCode::OK, "反审核成功，订单已解锁".to_string())
        }
        _ => (StatusCode::CONFLICT, "订单状态已变化，请刷新后重试".to_string()),
    }
}

pub async fn api_purchase_order_export(
    headers: axum::http::HeaderMap,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    // 行级数据权限：supplier 只能导出自己的
    let ctx = crate::auth::get_user_ctx(&headers).await;
    let keyword_pattern = parse_keyword_pattern(&params);

    // 行级数据权限 + 筛选：supplier 强制只看自己，其他角色可按 supplier_id 过滤
    let supplier_id: Option<i64> = if ctx.role == "supplier" {
        if ctx.supplier_id > 0 { Some(ctx.supplier_id) } else { Some(-1) }
    } else {
        params.get("supplier_id").and_then(|s| s.parse().ok())
    };

    // 导出按订单ID与明细ID顺序排列，便于阅读核对
    let base_sql = "SELECT po.id, po.order_no, po.order_date, po.total_amount, po.discount_rate, po.final_amount, po.status, po.remark, po.warehouse_id, po.warehouse_name, s.name as supplier_name,
                           (SELECT GROUP_CONCAT(DISTINCT NULLIF(TRIM(poi2.warehouse_name), ''))
                            FROM purchase_order_item poi2
                            WHERE poi2.order_id = po.id) as item_warehouse_names,
                           (SELECT COUNT(DISTINCT NULLIF(TRIM(poi2.warehouse_name), ''))
                            FROM purchase_order_item poi2
                            WHERE poi2.order_id = po.id) as item_warehouse_count,
                           poi.product_name, poi.alias1, poi.alias2, poi.spec, poi.unit, poi.ordered_quantity, poi.quantity, poi.unit_price, poi.base_quantity, poi.amount, poi.remark as item_remark, poi.warehouse_name as item_warehouse_name
                    FROM purchase_order po
                    JOIN supplier s ON po.supplier_id = s.id
                    LEFT JOIN purchase_order_item poi ON po.id = poi.order_id
                    WHERE (po.order_no LIKE ? OR s.name LIKE ? OR po.order_date LIKE ?)";
    let (sql, rows): (String, Vec<sqlx::sqlite::SqliteRow>) = if let Some(sid) = supplier_id {
        let sql = format!("{} AND po.supplier_id = ? ORDER BY po.id, poi.id", base_sql);
        let q = sqlx::query(AssertSqlSafe(sql.as_str()))
            .bind(&keyword_pattern).bind(&keyword_pattern).bind(&keyword_pattern)
            .bind(sid);
        (sql, q.fetch_all(crate::db::pool()).await.unwrap_or_default())
    } else {
        let sql = format!("{} ORDER BY po.id, poi.id", base_sql);
        let q = sqlx::query(AssertSqlSafe(sql.as_str()))
            .bind(&keyword_pattern).bind(&keyword_pattern).bind(&keyword_pattern);
        (sql, q.fetch_all(crate::db::pool()).await.unwrap_or_default())
    };
    let _ = sql; // 当前未使用，保留以备后续排查
    build_purchase_order_export_workbook(rows)
}

pub async fn api_purchase_order_print_excel(
    Path(id): Path<i64>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let (order, items) = match get_purchase_order_with_items(id).await {
        Some(v) => v,
        None => return (StatusCode::NOT_FOUND, "采购订单不存在".to_string()).into_response(),
    };

    // 如果传入了 user_id 则优先使用参数里的；否则使用订单里存的
    let (mut handler_name, mut handler_phone) = (
        order.user_name.clone().unwrap_or_default(),
        order.handler_phone.clone().unwrap_or_default(),
    );
    if let Some(uid_str) = params.get("user_id").or(params.get("userId")) {
        if let Ok(uid) = uid_str.parse::<i64>() {
            if uid > 0 {
                if let Some(u) = get_user_by_id(uid).await {
                    handler_name = u.nickname;
                    handler_phone = u.phone;
                }
            }
        }
    }
    let export_filename = format!("采购单_{}.xlsx", order.order_no);

    let title_format = Format::new()
        .set_bold()
        .set_font_size(16)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter);
    let header_format = Format::new()
        .set_bold()
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin);
    let cell_center = Format::new()
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin);
    // 合并单元格左对齐格式（默认不加自动换行）
    let info_left = Format::new()
        .set_align(FormatAlign::Left)
        .set_align(FormatAlign::VerticalCenter);
    // 合并单元格左对齐格式（自动换行，仅用于 E2:F3 地址/长文本）
    let info_left_wrap = Format::new()
        .set_align(FormatAlign::Left)
        .set_align(FormatAlign::VerticalCenter)
        .set_text_wrap();
    let sum_left_noline = Format::new()
        .set_bold()
        .set_align(FormatAlign::Left)
        .set_align(FormatAlign::VerticalCenter);
    // 最后一行的"最终合计"靠右布局
    let sum_right_noline = Format::new()
        .set_bold()
        .set_align(FormatAlign::Right)
        .set_align(FormatAlign::VerticalCenter);
    // 打印导出：分组标题（仓库）格式，灰底加粗
    let print_group_title = Format::new()
        .set_bold()
        .set_align(FormatAlign::Left)
        .set_align(FormatAlign::VerticalCenter)
        .set_background_color("#E5E7EB")
        .set_font_color("#374151");
    // 打印导出：分组小计 A:D（保留 A 列左外框、右边框去掉，与 E:F 合并之间无竖线）
    let print_summary_format = Format::new()
        .set_bold()
        .set_align(FormatAlign::Left)
        .set_align(FormatAlign::VerticalCenter)
        .set_border_left(FormatBorder::Thin)
        .set_border_right(FormatBorder::None)
        .set_border_top(FormatBorder::Thin)
        .set_border_bottom(FormatBorder::Thin);
    // 打印导出：小计 E:F 合并（左边框去掉，与小计文本之间无竖线；F 列右外框保留）
    let print_summary_right = Format::new()
        .set_bold()
        .set_align(FormatAlign::Right)
        .set_align(FormatAlign::VerticalCenter)
        .set_border_left(FormatBorder::None)
        .set_border_right(FormatBorder::Thin)
        .set_border_top(FormatBorder::Thin)
        .set_border_bottom(FormatBorder::Thin)
        .set_num_format("¥#,##0.00");
    // 打印导出：总计 A:D 灰底（A 列左外框保留、右边框去掉）
    let print_grand_total_label = Format::new()
        .set_bold()
        .set_align(FormatAlign::Left)
        .set_align(FormatAlign::VerticalCenter)
        .set_background_color("#E5E7EB")
        .set_font_color("#374151")
        .set_border_left(FormatBorder::Thin)
        .set_border_right(FormatBorder::None)
        .set_border_top(FormatBorder::Thin)
        .set_border_bottom(FormatBorder::Thin);
    // 打印导出：总计 E:F 合并 灰底（左边框去掉；F 列右外框保留）
    let print_grand_total_amount = Format::new()
        .set_bold()
        .set_align(FormatAlign::Right)
        .set_align(FormatAlign::VerticalCenter)
        .set_background_color("#E5E7EB")
        .set_font_color("#374151")
        .set_border_left(FormatBorder::None)
        .set_border_right(FormatBorder::Thin)
        .set_border_top(FormatBorder::Thin)
        .set_border_bottom(FormatBorder::Thin)
        .set_num_format("¥#,##0.00");
    // 货币格式（¥ 前缀，右对齐）
    let currency_right = Format::new()
        .set_num_format("¥#,##0.00")
        .set_align(FormatAlign::Right)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin);
    let cell_left = Format::new()
        .set_align(FormatAlign::Left)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin);
    let cell_right = Format::new()
        .set_align(FormatAlign::Right)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin);

    let result: Result<Vec<u8>, XlsxError> = (move || {
        let mut wb = Workbook::new();
        let ws = wb.add_worksheet();
        ws.set_name("采购单")?;
        // 页面设置：241-2S 两层两等份，0 左右边距，水平居中，横向
        ws.set_landscape();
        ws.set_margins(0.0, 0.0, 0.0, 0.4, 0.0, 0.0);
        ws.set_print_center_horizontally(true);

        // 列宽：A(28:品名规格/标签+值) B(8)+C(10)+D(12)=30 E(12)+F(18)=30
        ws.set_column_width(0, 20)?;
        ws.set_column_width(1, 5)?;
        ws.set_column_width(2, 6)?;
        ws.set_column_width(3, 8)?;
        ws.set_column_width(4, 10)?;
        ws.set_column_width(5, 18)?;

        // 行 0: 标题（采购单），合并 A-F
        ws.merge_range(0, 0, 0, 5, "采购单", &title_format)?;
        ws.set_row_height(0, 28)?;

        // 准备主表数据用于页眉
        let order_no = order.order_no.clone();
        let order_date = order.order_date.clone();
        let supplier_name = order.supplier_name.clone().unwrap_or_default();
        let supplier_phone = order.supplier_phone.clone().unwrap_or_default();
        let supplier_addr = order.supplier_address.clone().unwrap_or_default();
        let remark_val = order.remark.clone().unwrap_or_default();

        // ---- 排版：A+B / C+D / E+F 三段两等份 ----
        // 每行 6 列：A(20)+B(5)=25, C(6)+D(8)=14, E(10)+F(18)=28
        // 参考截图：订单号+日期+地址（地址跨两行）、供应商+联系、经手人+联系+备注
        // -------------------------------------------------------------------

        // 行 1: 间隔
        ws.set_row_height(1, 6)?;

        // 行 2: A+B="订单号：xxx", C+D="日期：xxx"
        let cell_ab2 = format!("订单号：{}", order_no);
        let cell_cd2 = format!("日期：{}", order_date);
        ws.merge_range(2, 0, 2, 1, cell_ab2.as_str(), &info_left)?;
        ws.merge_range(2, 2, 2, 3, cell_cd2.as_str(), &info_left)?;

        // 行 3: A+B="供应商：xxx", C+D="联系：xxx"
        let cell_ab3 = format!("供应商：{}", supplier_name);
        let cell_cd3 = format!("联系：{}", supplier_phone);
        ws.merge_range(3, 0, 3, 1, cell_ab3.as_str(), &info_left)?;
        ws.merge_range(3, 2, 3, 3, cell_cd3.as_str(), &info_left)?;

        // 行 2~3: E+F 合并（跨两行）写"地址：xxx"，靠左自动换行
        let cell_ef23 = format!("地址：{}", supplier_addr);
        ws.merge_range(2, 4, 3, 5, cell_ef23.as_str(), &info_left_wrap)?;

        // 行 4: A+B="经手人：xxx", C+D="联系：xxx", E+F="备注：xxx"
        let cell_ab4 = format!("经手人：{}", handler_name);
        let cell_cd4 = format!("联系：{}", handler_phone);
        let cell_ef4 = format!("备注：{}", remark_val);
        ws.merge_range(4, 0, 4, 1, cell_ab4.as_str(), &info_left)?;
        ws.merge_range(4, 2, 4, 3, cell_cd4.as_str(), &info_left)?;
        ws.merge_range(4, 4, 4, 5, cell_ef4.as_str(), &info_left)?;

        // 行 5: 间隔
        ws.set_row_height(5, 6)?;

        // 行 6: 表头
        let header_row = 6u32;
        let headers = ["品名规格", "单位", "数量", "单价", "金额", "备注"];
        for (i, h) in headers.iter().enumerate() {
            ws.write_with_format(header_row, i as u16, *h, &header_format)?;
        }
        ws.set_row_height(header_row, 22)?;

        // ---- 打印分页设置 ----
        // 页头（每一页都显示标题+主表+间隔+表头）：重复行 0~6
        let _ = ws.set_repeat_rows(0, 6);
        // 页脚：居中"第 X 页，共 Y 页"
        ws.set_footer("&C第 &P 页，共 &N 页");

        // ---- 明细按仓库分组：分组标题 + 明细 + 小计 + 总计 ----
        // 预估行数：每组 = 1（标题） + N（明细） + 1（小计），加最后 1（总计）
        let mut cur_row: u32 = 7; // 紧接表头行 6
        let mut grand_item_count: i64 = 0;
        let mut grand_amount: f64 = 0.0;

        // 按仓库分组（保持 SQL 排序顺序：warehouse_name, id）
        let mut last_wh: Option<String> = None;
        let mut group_buf: Vec<&PurchaseOrderPrintItem> = Vec::new();
        let mut finished_groups: Vec<(String, Vec<&PurchaseOrderPrintItem>)> = Vec::new();

        // 先把所有分组聚合好，便于最后按顺序统一输出（避免在循环内同时写小计/总计造成行索引管理复杂）
        for it in &items {
            let wh = it.warehouse_name.trim().to_string();
            if last_wh.as_deref() != Some(wh.as_str()) {
                if let Some(prev) = last_wh.take() {
                    let prev_display = if prev.is_empty() { "未指定".to_string() } else { prev.clone() };
                    finished_groups.push((prev_display, std::mem::take(&mut group_buf)));
                }
                last_wh = Some(wh);
                group_buf.push(it);
            } else {
                group_buf.push(it);
            }
        }
        if let Some(prev) = last_wh.take() {
            let prev_display = if prev.is_empty() { "未指定".to_string() } else { prev.clone() };
            finished_groups.push((prev_display, group_buf));
        }

        for (wh_display, group_items) in &finished_groups {
            // 分组标题
            let title = format!("├── {}", wh_display);
            ws.merge_range(cur_row, 0, cur_row, 5, title.as_str(), &print_group_title)?;
            ws.set_row_height(cur_row, 20)?;
            cur_row += 1;
            for it in group_items {
                let name_spec = if let Some(spec) = it.spec.clone() {
                    if spec.trim().is_empty() { it.product_name.clone() } else { format!("{} {}", it.product_name, spec) }
                } else { it.product_name.clone() };
                ws.write_with_format(cur_row, 0, name_spec, &cell_left)?;
                ws.write_with_format(cur_row, 1, it.unit.clone().unwrap_or_default(), &cell_center)?;
                ws.write_with_format(cur_row, 2, it.quantity, &cell_right)?;
                ws.write_with_format(cur_row, 3, it.unit_price, &currency_right)?;
                ws.write_with_format(cur_row, 4, it.amount, &currency_right)?;
                ws.write_with_format(cur_row, 5, it.remark.clone().unwrap_or_default(), &cell_left)?;
                ws.set_row_height(cur_row, 22)?;
                cur_row += 1;
                grand_item_count += 1;
                grand_amount += it.amount;
            }
            // 分组小计：A:D 合并写文本，E:F 合并写金额（合并单元格的左竖线去除）
            let group_amount: f64 = group_items.iter().map(|x| x.amount).sum();
            let subtotal = format!("小计: 包装数量 {}", group_items.len());
            ws.merge_range(cur_row, 0, cur_row, 3, subtotal.as_str(), &print_summary_format)?;
            ws.merge_range(cur_row, 4, cur_row, 5, "", &print_summary_right)?;
            ws.write_with_format(cur_row, 4, group_amount, &print_summary_right)?;
            ws.set_row_height(cur_row, 20)?;
            cur_row += 1;
        }

        // 总计：A:D 合并写文本，E:F 合并写金额（合并单元格的左竖线去除，整行灰底）
        let grand_total = format!("总计: 包装数量 {}", grand_item_count);
        ws.merge_range(cur_row, 0, cur_row, 3, grand_total.as_str(), &print_grand_total_label)?;
        ws.merge_range(cur_row, 4, cur_row, 5, "", &print_grand_total_amount)?;
        ws.write_with_format(cur_row, 4, grand_amount, &print_grand_total_amount)?;
        ws.set_row_height(cur_row, 22)?;
        cur_row += 1;

        let sum_row = cur_row;

        // ---- 合计排版（A+B | C+D 靠左；E+F 最终合计靠右）----
        let cell_sum = format!("合计金额: ¥{:.2}", order.total_amount);
        let cell_discount = format!("折减金额: ¥{:.2}", order.amount_reduction);
        let cell_final = format!("最终合计: ¥{:.2}", order.final_amount);
        ws.merge_range(sum_row, 0, sum_row, 1, cell_sum.as_str(), &sum_left_noline)?;
        ws.merge_range(sum_row, 2, sum_row, 3, cell_discount.as_str(), &sum_left_noline)?;
        ws.merge_range(sum_row, 4, sum_row, 5, cell_final.as_str(), &sum_right_noline)?;

        wb.save_to_buffer()
    })();

    match result {
        Ok(data) => xlsx_response(data, export_filename.as_str()),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("导出失败: {}", e)).into_response(),
    }
}

pub async fn api_purchase_order_import(headers: axum::http::HeaderMap, content: Bytes) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/purchase_order/import").await {
        Err(e) => return e.into_response(),
        Ok(_) => {}
    }
    let ctx = crate::auth::get_user_ctx(&headers).await;

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
                .fetch_optional(crate::db::pool())
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
        .execute(crate::db::pool())
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
                            .fetch_optional(crate::db::pool())
                            .await
                            .ok()
                            .flatten()
                            .map(|r| r.get::<i64, _>("id"))
                            .unwrap_or(0);
                        
                        sqlx::query(
                            "INSERT INTO purchase_order_item(order_id, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, base_quantity, amount, ordered_quantity, remark) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
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
                        .bind(0.0f64)
                        .bind(item_remark)
                        .execute(crate::db::pool())
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
    
    crate::auth::log_operation(&ctx, "purchase_order.import", "purchase_order", "",
        &format!("导入采购单：成功 {} 条，失败 {} 条", success, failed)).await;

    (StatusCode::OK, format!("导入完成：成功 {} 条，失败 {} 条", success, failed)).into_response()
}

pub async fn api_sales_order_detail(headers: axum::http::HeaderMap, Path(id): Path<i64>) -> impl IntoResponse {
    let ctx = crate::auth::get_user_ctx(&headers).await;

    // 行级数据权限：先确认订单存在并校验归属
    let exists_row: Option<(i64,)> = sqlx::query_as("SELECT purchaser_id FROM sales_order WHERE id = ?")
        .bind(id)
        .fetch_optional(crate::db::pool())
        .await
        .unwrap_or(None);

    let exists = match exists_row {
        Some((pid,)) => {
            if !crate::auth::can_access_sales_order(&ctx, pid) {
                return (StatusCode::FORBIDDEN, "您没有权限查看此订单".to_string());
            }
            true
        }
        None => false,
    };

    if !exists {
        return (StatusCode::NOT_FOUND, format!("订单不存在 (ID: {})", id).to_string());
    }

    let order_row = sqlx::query(
        "SELECT so.id, so.purchaser_id, so.order_no, so.order_date, so.total_amount, so.discount_rate, so.amount_reduction, so.final_amount, so.status, so.version, so.remark, so.warehouse_id, so.warehouse_name, so.customer_order_image, so.signed_order_image, so.supplier_company, so.truck_plate, so.is_settled, COALESCE(p.name, '') as purchaser_name
         FROM sales_order so LEFT JOIN purchaser p ON so.purchaser_id = p.id WHERE so.id = ?"
    )
    .bind(id)
    .fetch_optional(crate::db::pool())
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
        "SELECT id, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, base_quantity, amount, pre_sale_quantity, supplier_id, supplier_name, remark FROM sales_order_item WHERE order_id = ?"
    )
    .bind(id)
    .fetch_all(crate::db::pool())
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
            "pre_sale_quantity": r.get::<Option<f64>, _>("pre_sale_quantity"),
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
        "version": row.get::<i64, _>("version"),
        "remark": row.get::<Option<String>, _>("remark"),
        "customer_order_image": row.get::<Option<String>, _>("customer_order_image"),
        "signed_order_image": row.get::<Option<String>, _>("signed_order_image"),
        "supplier_company": row.get::<Option<String>, _>("supplier_company"),
        "truck_plate": row.get::<Option<String>, _>("truck_plate"),
        "is_settled": row.get::<i64, _>("is_settled"),
        "purchaser_name": row.get::<String, _>("purchaser_name"),
        "items": items,
    });
    
    match serde_json::to_string(&order) {
        Ok(json_str) => (StatusCode::OK, json_str),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("序列化订单JSON失败：{}", e).to_string()),
    }
}

pub async fn api_sales_order_update(headers: axum::http::HeaderMap, Json(req): Json<SalesOrderReq>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/sales_order/update").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let ctx = crate::auth::get_user_ctx(&headers).await;

    // 行级数据权限：先查订单所属采购单位
    let order = sqlx::query("SELECT purchaser_id, status FROM sales_order WHERE id = ?")
        .bind(req.id)
        .fetch_optional(crate::db::pool())
        .await
        .ok()
        .flatten();
    let order = match order {
        Some(o) => o,
        None => return (StatusCode::NOT_FOUND, "订单不存在".to_string()),
    };
    let order_purchaser_id: i64 = order.get("purchaser_id");
    let order_status: String = order.get("status");
    if !crate::auth::can_access_sales_order(&ctx, order_purchaser_id) {
        return (StatusCode::FORBIDDEN, "您没有权限操作此订单".to_string());
    }
    // 仅待审核（pending）状态允许修改；已审核/已流转的订单必须反审核后才能修改（防篡改）
    if order_status != "pending" {
        return (StatusCode::BAD_REQUEST, format!("当前订单状态为「{}」，仅待审核状态的订单允许修改；已审核订单需管理员反审核后才能修改", order_status));
    }
    // 强制乐观锁：req.version 必须传入且 > 0
    let ver = match req.version {
        Some(v) if v > 0 => v,
        _ => return (StatusCode::BAD_REQUEST, "保存失败：缺少订单版本号，请刷新页面后重试".to_string()),
    };
    // 安全护栏：更新时明细不允许为空（防止并发覆盖导致明细被清空）
    if req.items.is_empty() {
        return (StatusCode::BAD_REQUEST, "保存失败：订单明细不能为空".to_string());
    }

    // 用 BEGIN IMMEDIATE 启动写事务，立即获得 SQLite 写锁（RESERVED）。
    // 作用：把"主表 UPDATE + 明细全量重写"整体串行化，
    // 杜绝连点两次保存时两个事务都读到同一 version、都通过乐观锁、然后后提交的事务用全量 items
    // 把先提交的事务的明细覆盖/丢失。
    let mut tx = match crate::db::pool().begin_with("BEGIN IMMEDIATE").await {
        Ok(t) => t,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "更新失败：事务启动失败".to_string()),
    };

    // 事务内做"主表 UPDATE + version 自增"：WHERE 同时校验 id 和 version。
    let upd = sqlx::query(
        "UPDATE sales_order SET purchaser_id = ?, order_no = ?, order_date = ?, total_amount = ?, discount_rate = ?, amount_reduction = ?, final_amount = ?, warehouse_id = ?, warehouse_name = ?, remark = ?, supplier_company = ?, truck_plate = ?, is_settled = ?, version = version + 1 WHERE id = ? AND version = ?"
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
    .bind(req.supplier_company.as_deref().unwrap_or(""))
    .bind(req.truck_plate.as_deref().unwrap_or(""))
    .bind(req.is_settled.unwrap_or(0))
    .bind(req.id)
    .bind(ver)
    .execute(&mut *tx)
    .await;

    let upd_res = match upd {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("更新失败: {}", e));
        }
    };
    if upd_res.rows_affected() == 0 {
        return (StatusCode::CONFLICT, format!("订单已被其他用户修改（版本 {} 已被覆盖），请刷新后重试", ver));
    }
    let new_version = ver + 1;
    let old_order_no = req.order_no.clone();
    let old_total = req.total_amount;
    let old_final = req.final_amount;

    sqlx::query("DELETE FROM sales_order_item WHERE order_id = ?")
        .bind(req.id)
        .execute(&mut *tx)
        .await
        .ok();

    if !req.items.is_empty() {
        let placeholders: Vec<String> = req.items.iter()
            .map(|_| "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)".to_string())
            .collect();
        let sql = format!(
            "INSERT INTO sales_order_item(order_id, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, base_quantity, amount, pre_sale_quantity, supplier_id, supplier_name, remark) VALUES {}",
            placeholders.join(", ")
        );

        let order_id = req.id;
        let mut query = sqlx::query(AssertSqlSafe(sql.as_str()));
        for mut item in req.items {
            // 数量同步：保存时若销售数量为 0 而预售数量 > 0，
            // 用预售数量兜底，避免后续生成采购时数量为 0 的空明细
            if item.quantity <= 0.0 && item.pre_sale_quantity.unwrap_or(0.0) > 0.0 {
                item.quantity = item.pre_sale_quantity.unwrap();
            }
            query = query
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
                .bind(item.pre_sale_quantity.unwrap_or(0.0))
                .bind(item.supplier_id)
                .bind(&item.supplier_name)
                .bind(&item.remark);
        }
        let _ = query.execute(&mut *tx).await;
    }
    if let Err(_) = tx.commit().await {
        return (StatusCode::INTERNAL_SERVER_ERROR, "更新失败：事务提交失败".to_string());
    }
    crate::auth::log_operation(&ctx, "sales_order.update", "sales_order", &req.id.unwrap_or(0).to_string(),
        &format!("更新销售单 {}（原单号 {}）：金额 {:.2}→{:.2}，下浮后合计 {:.2}→{:.2}，版本 {}→{}",
            req.order_no, old_order_no, old_total, req.total_amount, old_final, req.final_amount, ver, new_version)).await;
    (StatusCode::OK, "更新成功".to_string())
}

pub async fn api_sales_order_update_prices(headers: axum::http::HeaderMap, Path(id): Path<i64>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/sales_order/update_prices").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let ctx = crate::auth::get_user_ctx(&headers).await;

    // 状态约束 + 行级数据权限：调价仅限待配单且归属自己的订单
    let order = sqlx::query("SELECT purchaser_id, status FROM sales_order WHERE id = ?")
        .bind(id)
        .fetch_optional(crate::db::pool())
        .await
        .ok()
        .flatten();
    let order = match order {
        Some(o) => o,
        None => return (StatusCode::NOT_FOUND, "订单不存在".to_string()),
    };
    let order_purchaser_id: i64 = order.get("purchaser_id");
    let order_status: String = order.get("status");
    if !crate::auth::can_access_sales_order(&ctx, order_purchaser_id) {
        return (StatusCode::FORBIDDEN, "您没有权限操作此订单".to_string());
    }
    if order_status != "pending" {
        return (StatusCode::BAD_REQUEST, format!("当前订单状态为「{}」，仅待配单状态的订单允许调整价格", order_status));
    }

    let items = sqlx::query(
        "SELECT id, product_id, quantity, base_quantity FROM sales_order_item WHERE order_id = ?"
    )
    .bind(id)
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();

    if items.is_empty() {
        return (StatusCode::NOT_FOUND, "订单不存在或无明细".to_string());
    }

    let mut result_items = Vec::new();
    let mut errors = Vec::new();
    let mut new_total = 0.0;

    for item in &items {
        let item_id: i64 = item.get("id");
        let product_id: i64 = item.get("product_id");
        let quantity: f64 = item.get("quantity");

        let price_row: Option<(f64,)> = sqlx::query_as(
            "SELECT COALESCE(base_price, 0) FROM product WHERE id = ?"
        )
        .bind(product_id)
        .fetch_optional(crate::db::pool())
        .await
        .unwrap_or(None);

        match price_row {
            Some((new_price,)) if new_price > 0.0 => {
                let new_amount = new_price * quantity;
                new_total += new_amount;
                result_items.push(serde_json::json!({
                    "item_id": item_id,
                    "product_id": product_id,
                    "unit_price": new_price,
                    "amount": new_amount,
                }));
            }
            Some((_,)) => {
                errors.push(format!("商品ID {} 售价为0，跳过", product_id));
            }
            None => {
                errors.push(format!("商品ID {} 未找到", product_id));
            }
        }
    }

    let discount_rate: f64 = sqlx::query_scalar(
        "SELECT COALESCE(discount_rate, 0) FROM sales_order WHERE id = ?"
    )
    .bind(id)
    .fetch_one(crate::db::pool())
    .await
    .unwrap_or(0.0);

    let amount_reduction: f64 = sqlx::query_scalar(
        "SELECT COALESCE(amount_reduction, 0) FROM sales_order WHERE id = ?"
    )
    .bind(id)
    .fetch_one(crate::db::pool())
    .await
    .unwrap_or(0.0);

    let discount_amount = new_total * (1.0 - discount_rate / 100.0);
    let new_final = discount_amount - amount_reduction;

    let resp = serde_json::json!({
        "items": result_items,
        "total_amount": new_total,
        "final_amount": new_final.max(0.0),
        "errors": errors,
    });
    crate::auth::log_operation(&ctx, "sales_order.adjust_price", "sales_order", &id.to_string(),
        &format!("一键获取销售单 {} 最新售价，新合计={}，错误数={}", id, new_total, errors.len())).await;
    (StatusCode::OK, serde_json::to_string(&resp).unwrap())
}

pub async fn api_sales_order_delete(headers: axum::http::HeaderMap, Path(id): Path<i64>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/sales_order/delete").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let ctx = crate::auth::get_user_ctx(&headers).await;

    // 行级数据权限 + 状态约束
    let order = sqlx::query("SELECT purchaser_id, status FROM sales_order WHERE id = ?")
        .bind(id)
        .fetch_optional(crate::db::pool())
        .await
        .ok()
        .flatten();
    let order = match order {
        Some(o) => o,
        None => return (StatusCode::NOT_FOUND, "订单不存在".to_string()),
    };
    let order_purchaser_id: i64 = order.get("purchaser_id");
    let order_status: String = order.get("status");
    if !crate::auth::can_access_sales_order(&ctx, order_purchaser_id) {
        return (StatusCode::FORBIDDEN, "您没有权限操作此订单".to_string());
    }
    if order_status != "pending" {
        return (StatusCode::BAD_REQUEST, format!("当前订单状态为「{}」，仅待配单状态的订单允许删除", order_status));
    }

    let delete_items_result = sqlx::query("DELETE FROM sales_order_item WHERE order_id = ?")
        .bind(id)
        .execute(crate::db::pool())
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
        .execute(crate::db::pool())
        .await;
    
    match result {
        Ok(_) => {
            crate::auth::log_operation(&ctx, "sales_order.delete", "sales_order", &id.to_string(),
                &format!("删除销售单 ID={}", id)).await;
            (StatusCode::OK, "删除成功".to_string())
        }
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

pub async fn api_sales_order_export(
    headers: axum::http::HeaderMap,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    // 行级数据权限：purchaser 只能导出自己采购单位的销售单
    let ctx = crate::auth::get_user_ctx(&headers).await;
    let keyword_pattern = parse_keyword_pattern(&params);

    // 行级数据权限 + 筛选：purchaser 强制只看自己，其他角色可按 purchaser_id 过滤
    let purchaser_id: Option<i64> = if ctx.role == "purchaser" {
        if ctx.purchaser_id > 0 { Some(ctx.purchaser_id) } else { Some(-1) }
    } else {
        params.get("purchaser_id").and_then(|s| s.parse().ok())
    };

    let base_sql = "SELECT so.id, so.order_no, so.order_date, so.total_amount, so.discount_rate, so.final_amount, so.status, so.remark, p.name as purchaser_name,
                           soi.product_name, soi.alias1, soi.alias2, soi.spec, soi.unit, soi.pre_sale_quantity, soi.quantity, soi.unit_price, soi.base_quantity, soi.amount, soi.remark as item_remark
                    FROM sales_order so
                    JOIN purchaser p ON so.purchaser_id = p.id
                    LEFT JOIN sales_order_item soi ON so.id = soi.order_id
                    WHERE (so.order_no LIKE ? OR p.name LIKE ? OR so.order_date LIKE ?)";
    let rows: Vec<sqlx::sqlite::SqliteRow> = if let Some(pid) = purchaser_id {
        let sql = format!("{} AND so.purchaser_id = ? ORDER BY so.id, soi.id", base_sql);
        let q = sqlx::query(AssertSqlSafe(sql.as_str()))
            .bind(&keyword_pattern).bind(&keyword_pattern).bind(&keyword_pattern)
            .bind(pid);
        q.fetch_all(crate::db::pool()).await.unwrap_or_default()
    } else {
        let sql = format!("{} ORDER BY so.id, soi.id", base_sql);
        let q = sqlx::query(AssertSqlSafe(sql.as_str()))
            .bind(&keyword_pattern).bind(&keyword_pattern).bind(&keyword_pattern);
        q.fetch_all(crate::db::pool()).await.unwrap_or_default()
    };

    let result: Result<Vec<u8>, XlsxError> = (|| {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();

        let header_format = Format::new()
            .set_bold()
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter);

        let headers = ["订单ID", "订单号", "订单日期", "采购单位", "总金额", "下浮率(%)", "下浮后合计", "状态", "备注", "商品名称", "下订名称(别称1)", "配单名称(别称2)", "规格", "单位", "预售数量", "数量", "单价", "基本数量", "金额", "商品备注"];
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
            worksheet.write(row_idx, 14, row.get::<Option<f64>, _>("pre_sale_quantity").unwrap_or(0.0))?;
            worksheet.write(row_idx, 15, row.get::<Option<f64>, _>("quantity").unwrap_or(0.0))?;
            worksheet.write(row_idx, 16, row.get::<Option<f64>, _>("unit_price").unwrap_or(0.0))?;
            worksheet.write(row_idx, 17, row.get::<Option<f64>, _>("base_quantity").unwrap_or(0.0))?;
            worksheet.write(row_idx, 18, row.get::<Option<f64>, _>("amount").unwrap_or(0.0))?;
            worksheet.write(row_idx, 19, row.get::<Option<String>, _>("item_remark").unwrap_or_default())?;
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
                ("Content-Disposition", "attachment; filename=\"sales_orders.xlsx\""),
            ],
            data,
        ).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("导出失败: {}", e)).into_response(),
    }
}

pub async fn api_sales_order_import(headers: axum::http::HeaderMap, content: Bytes) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/sales_order/import").await {
        Err(e) => return e.into_response(),
        Ok(_) => {}
    }
    let ctx = crate::auth::get_user_ctx(&headers).await;

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
                .fetch_optional(crate::db::pool())
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
        .execute(crate::db::pool())
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
                            .fetch_optional(crate::db::pool())
                            .await
                            .ok()
                            .flatten()
                            .map(|r| r.get::<i64, _>("id"))
                            .unwrap_or(0);
                        
                        sqlx::query(
                            "INSERT INTO sales_order_item(order_id, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, base_quantity, amount, pre_sale_quantity, remark) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
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
                        .bind(0.0f64)
                        .bind(item_remark)
                        .execute(crate::db::pool())
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
    
    crate::auth::log_operation(&ctx, "sales_order.import", "sales_order", "",
        &format!("导入销售单：成功 {} 条，失败 {} 条", success, failed)).await;

    (StatusCode::OK, format!("导入完成：成功 {} 条，失败 {} 条", success, failed)).into_response()
}

pub async fn api_query_purchase_order(headers: axum::http::HeaderMap, axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    // 行级数据权限：supplier 角色强制只看自己绑定的供应商
    let ctx = crate::auth::get_user_ctx(&headers).await;
    let supplier_id = if ctx.role == "supplier" {
        if ctx.supplier_id > 0 { ctx.supplier_id.to_string() } else { "-1".to_string() }
    } else {
        params.get("supplier_id").map(|s| s.as_str()).unwrap_or("").to_string()
    };
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
    let total_rows = count_query.fetch_one(crate::db::pool()).await.unwrap();
    let total: i64 = total_rows.get("count");
    
    let mut query = sqlx::query(AssertSqlSafe(sql.as_str()));
    for b in &binds {
        query = query.bind(b);
    }
    query = query.bind(page_size).bind(offset);
    
    let rows = query.fetch_all(crate::db::pool()).await.unwrap_or_default();
    
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

pub async fn api_query_purchase_order_export(headers: axum::http::HeaderMap, axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    // 行级数据权限：supplier 角色只能导出自己供应商的数据
    let ctx = crate::auth::get_user_ctx(&headers).await;
    let supplier_id = if ctx.role == "supplier" {
        if ctx.supplier_id > 0 { ctx.supplier_id.to_string() } else { "-1".to_string() }
    } else {
        params.get("supplier_id").map(|s| s.as_str()).unwrap_or("").to_string()
    };
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
    
    let rows = query.fetch_all(crate::db::pool()).await.unwrap_or_default();
    
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

pub async fn api_query_purchase_price(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
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
    let total_rows = count_q.fetch_one(crate::db::pool()).await.unwrap();
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
    
    let rows = query.fetch_all(crate::db::pool()).await.unwrap_or_default();
    
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

pub async fn api_query_purchase_summary(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
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
    let supplier_rows = query1.fetch_all(crate::db::pool()).await.unwrap_or_default();
    
    let mut query2 = sqlx::query(AssertSqlSafe(product_sql.as_str()));
    for b in &binds2 {
        query2 = query2.bind(b);
    }
    let product_rows = query2.fetch_all(crate::db::pool()).await.unwrap_or_default();
    
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

pub async fn api_query_supplier_balance(headers: axum::http::HeaderMap) -> impl IntoResponse {
    // 行级数据权限：supplier 角色只能看自己绑定的供应商往来
    let ctx = crate::auth::get_user_ctx(&headers).await;
    let mut sql = String::from(
        "SELECT s.id, s.name, 
                COALESCE(SUM(po.final_amount), 0.0) as purchase_total,
                COALESCE(SUM(po.final_amount), 0.0) as unpaid
         FROM supplier s 
         LEFT JOIN purchase_order po ON po.supplier_id = s.id"
    );
    let mut binds: Vec<i64> = Vec::new();
    if ctx.role == "supplier" {
        sql.push_str(" WHERE s.id = ?");
        binds.push(ctx.supplier_id);
    }
    sql.push_str(" GROUP BY s.id, s.name ORDER BY purchase_total DESC");

    let mut q = sqlx::query(AssertSqlSafe(sql.as_str()));
    for b in &binds {
        q = q.bind(b);
    }
    let rows = q.fetch_all(crate::db::pool()).await.unwrap_or_default();
    
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

pub async fn api_query_supplier_balance_export(headers: axum::http::HeaderMap) -> impl IntoResponse {
    // 行级数据权限：supplier 角色只能导出自己绑定的供应商往来
    let ctx = crate::auth::get_user_ctx(&headers).await;
    let mut sql = String::from(
        "SELECT s.id, s.name, 
                COALESCE(SUM(po.final_amount), 0.0) as purchase_total,
                COALESCE(SUM(po.final_amount), 0.0) as unpaid
         FROM supplier s 
         LEFT JOIN purchase_order po ON po.supplier_id = s.id"
    );
    let mut binds: Vec<i64> = Vec::new();
    if ctx.role == "supplier" {
        sql.push_str(" WHERE s.id = ?");
        binds.push(ctx.supplier_id);
    }
    sql.push_str(" GROUP BY s.id, s.name ORDER BY purchase_total DESC");

    let mut q = sqlx::query(AssertSqlSafe(sql.as_str()));
    for b in &binds {
        q = q.bind(b);
    }
    let rows = q.fetch_all(crate::db::pool()).await.unwrap_or_default();
    
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

pub async fn api_query_sales_order(headers: axum::http::HeaderMap, axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    // 行级数据权限：purchaser 角色强制只看自己绑定的采购单位
    let ctx = crate::auth::get_user_ctx(&headers).await;
    let purchaser_id = if ctx.role == "purchaser" {
        if ctx.purchaser_id > 0 { ctx.purchaser_id.to_string() } else { "-1".to_string() }
    } else {
        params.get("purchaser_id").map(|s| s.as_str()).unwrap_or("").to_string()
    };
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or("");
    let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    let status = params.get("status").map(|s| s.as_str()).unwrap_or("");
    
    let page: i64 = params.get("page").and_then(|s| s.parse().ok()).unwrap_or(1);
    let page_size: i64 = params.get("page_size").and_then(|s| s.parse().ok()).unwrap_or(20);
    let offset = (page - 1) * page_size;

    // 排序处理：支持订单号、采购单位、日期、金额、状态列点击排序
    let sort_field = params.get("sort_field").map(|s| s.as_str()).unwrap_or("id");
    let sort_order_raw = params.get("sort_order").map(|s| s.as_str()).unwrap_or("desc");
    // 仅允许 asc/desc，防止任意 SQL 片段注入
    let sort_order = if sort_order_raw.eq_ignore_ascii_case("asc") { "asc" } else { "desc" };
    let order_clause = match sort_field {
        "order_no" => format!("so.order_no {}", sort_order),
        "unit_name" => format!("p.name {}", sort_order),
        "order_date" => format!("so.order_date {}", sort_order),
        "total_amount" => format!("so.total_amount {}", sort_order),
        "status" => format!("so.status {}", sort_order),
        _ => format!("so.id {}", sort_order),
    };

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
    let total_rows = count_q.fetch_one(crate::db::pool()).await.unwrap();
    let total: i64 = total_rows.get("COUNT(*)");

    let data_sql = format!(
        "SELECT so.id, so.order_no, so.order_date, so.total_amount, so.discount_rate, so.final_amount, so.status, so.remark, p.name as purchaser_name {} ORDER BY {} LIMIT ? OFFSET ?",
        base_sql, order_clause
    );
    let mut query = sqlx::query(AssertSqlSafe(data_sql.as_str()));
    for b in &binds {
        query = query.bind(b);
    }
    query = query.bind(page_size).bind(offset);
    
    let rows = query.fetch_all(crate::db::pool()).await.unwrap_or_default();
    
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

pub async fn api_query_sales_order_export(headers: axum::http::HeaderMap, axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    // 行级数据权限：purchaser 角色只能导出自己采购单位的数据
    let ctx = crate::auth::get_user_ctx(&headers).await;
    let purchaser_id = if ctx.role == "purchaser" {
        if ctx.purchaser_id > 0 { ctx.purchaser_id.to_string() } else { "-1".to_string() }
    } else {
        params.get("purchaser_id").map(|s| s.as_str()).unwrap_or("").to_string()
    };
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
    
    let rows = query.fetch_all(crate::db::pool()).await.unwrap_or_default();
    
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

pub async fn api_query_product_price_trend(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let product_id: i64 = match params.get("product_id").and_then(|s| s.parse().ok()) {
        Some(v) => v,
        None => return (StatusCode::BAD_REQUEST, serde_json::json!({"error": "missing product_id"}).to_string()).into_response(),
    };
    let start_date = params.get("start_date").cloned().unwrap_or_default();
    let end_date = params.get("end_date").cloned().unwrap_or_default();

    // 1) 取商品信息（基础单位）
    let product_row = sqlx::query("SELECT id, name, spec, base_unit FROM product WHERE id = ?")
        .bind(product_id)
        .fetch_optional(crate::db::pool())
        .await
        .ok()
        .flatten();
    if product_row.is_none() {
        return (StatusCode::NOT_FOUND, serde_json::json!({"error": "product not found"}).to_string()).into_response();
    }
    let product = product_row.unwrap();
    let product_name: String = product.get("name");
    let base_unit: String = product.get("base_unit");

    // 2) 同一商品可能因别名/规格存在多个 product_id，需要一并查询（仅当同 base_unit 时聚合）
    // 简化处理：只查主 product_id；如 spec 不同则按名称+基础单位匹配
    let same_base_products: Vec<i64> = {
        let rows = sqlx::query("SELECT id FROM product WHERE name = ? AND base_unit = ?")
            .bind(&product_name)
            .bind(&base_unit)
            .fetch_all(crate::db::pool())
            .await
            .unwrap_or_default();
        rows.iter().map(|r| r.get::<i64, _>("id")).collect()
    };
    if same_base_products.is_empty() {
        return (StatusCode::OK, serde_json::to_string(&serde_json::json!({
            "product_id": product_id,
            "product_name": product_name,
            "base_unit": base_unit,
            "purchase_points": [],
            "selling_points": [],
        })).unwrap()).into_response();
    }

    // 构造 IN 列表
    let placeholders = same_base_products.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    // 备注：早期版本曾尝试在 SQL 内做单位换算匹配，因实现复杂改为在应用层按 base_unit 过滤
    let purchase_sql = format!(
        "SELECT po.order_date as date, poi.unit_price as price, poi.unit as unit, poi.base_quantity as bq, poi.quantity as qty, poi.amount as amount
         FROM purchase_order_item poi
         JOIN purchase_order po ON poi.order_id = po.id
         WHERE poi.product_id IN ({})
         {} {}
         ORDER BY po.order_date ASC",
        placeholders,
        if start_date.is_empty() { String::new() } else { format!("AND po.order_date >= '{}'", start_date.replace('\'', "''")) },
        if end_date.is_empty() { String::new() } else { format!("AND po.order_date <= '{}'", end_date.replace('\'', "''")) },
    );
    let mut purchase_q = sqlx::query(AssertSqlSafe(purchase_sql));
    for pid in &same_base_products {
        purchase_q = purchase_q.bind(pid);
    }
    let purchase_rows = purchase_q.fetch_all(crate::db::pool()).await.unwrap_or_default();

    // 换算为基础单位单价：基础单位单价 = amount / base_quantity（若 base_quantity>0）；否则 unit_price（仅在 unit == base_unit 时）
    let mut purchase_points: Vec<serde_json::Value> = Vec::new();
    for r in &purchase_rows {
        let date: String = r.get("date");
        let unit: String = r.get("unit");
        let unit_price: f64 = r.get("price");
        let bq: Option<f64> = r.get("bq");
        let qty: Option<f64> = r.get("qty");
        let amount: Option<f64> = r.get("amount");
        let base_unit_price = if unit == base_unit {
            unit_price
        } else if let (Some(bqv), Some(_qv), Some(av)) = (bq, qty, amount) {
            if bqv > 0.0 { av / bqv } else { unit_price }
        } else {
            // 没有 base_quantity 字段时跳过（避免与不同单位数据混淆）
            continue;
        };
        purchase_points.push(serde_json::json!({
            "date": date,
            "price": base_unit_price,
        }));
    }

    // 3) 售价点：来自销售订单明细的实际成交价（sales_order_item），与查询列表数据源一致
    //    换算为基础单位单价，与进价同口径比较
    let selling_sql = format!(
        "SELECT so.order_date as date, soi.unit_price as price, soi.unit as unit, soi.base_quantity as bq, soi.quantity as qty, soi.amount as amount
         FROM sales_order_item soi
         JOIN sales_order so ON soi.order_id = so.id
         WHERE soi.product_id IN ({})
         {} {}
         ORDER BY so.order_date ASC",
        placeholders,
        if start_date.is_empty() { String::new() } else { format!("AND so.order_date >= '{}'", start_date.replace('\'', "''")) },
        if end_date.is_empty() { String::new() } else { format!("AND so.order_date <= '{}'", end_date.replace('\'', "''")) },
    );
    let mut selling_q = sqlx::query(AssertSqlSafe(selling_sql));
    for pid in &same_base_products {
        selling_q = selling_q.bind(pid);
    }
    let selling_rows = selling_q.fetch_all(crate::db::pool()).await.unwrap_or_default();
    // 换算为基础单位单价：同进价口径
    let mut selling_points: Vec<serde_json::Value> = Vec::new();
    for r in &selling_rows {
        let date: String = r.get("date");
        let unit: String = r.get("unit");
        let unit_price: f64 = r.get("price");
        let bq: Option<f64> = r.get("bq");
        let qty: Option<f64> = r.get("qty");
        let amount: Option<f64> = r.get("amount");
        let base_unit_price = if unit == base_unit {
            unit_price
        } else if let (Some(bqv), Some(_qv), Some(av)) = (bq, qty, amount) {
            if bqv > 0.0 { av / bqv } else { unit_price }
        } else {
            continue;
        };
        selling_points.push(serde_json::json!({
            "date": date,
            "price": base_unit_price,
        }));
    }

    (StatusCode::OK, serde_json::to_string(&serde_json::json!({
        "product_id": product_id,
        "product_name": product_name,
        "base_unit": base_unit,
        "purchase_points": purchase_points,
        "selling_points": selling_points,
    })).unwrap()).into_response()
}

pub async fn api_query_sales_price(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let product_name = params.get("product_name").map(|s| s.as_str()).unwrap_or("");
    let product_id = params.get("product_id").map(|s| s.as_str()).unwrap_or("");
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
    if !product_id.is_empty() {
        base_sql.push_str(" AND soi.product_id = ?");
        binds.push(product_id.to_string());
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
    let total_rows = count_q.fetch_one(crate::db::pool()).await.unwrap();
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
    
    let rows = query.fetch_all(crate::db::pool()).await.unwrap_or_default();
    
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

pub async fn api_query_sales_summary(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
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
    let purchaser_rows = query1.fetch_all(crate::db::pool()).await.unwrap_or_default();
    
    let mut query2 = sqlx::query(AssertSqlSafe(product_sql.as_str()));
    for b in &binds2 {
        query2 = query2.bind(b);
    }
    let product_rows = query2.fetch_all(crate::db::pool()).await.unwrap_or_default();
    
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

pub async fn api_query_reimburse_summary(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or("");
    let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");

    // 作为分摊来源的订单 id（排除，不计入报销口径）
    let source_ids: Vec<i64> = sqlx::query_scalar::<_, i64>(
        "SELECT DISTINCT source_order_id FROM consumable_allocation"
    )
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();
    let source_set: std::collections::HashSet<i64> = source_ids.into_iter().collect();

    let mut date_cond = String::from("");
    let mut binds: Vec<String> = Vec::new();
    if !start_date.is_empty() {
        date_cond.push_str(" AND so.order_date >= ?");
        binds.push(start_date.to_string());
    }
    if !end_date.is_empty() {
        date_cond.push_str(" AND so.order_date <= ?");
        binds.push(end_date.to_string());
    }

    // 1) 真实明细：按采购单位聚合（真实账套口径，含全部订单，不排除来源耗材单）
    let real_purchaser_sql = format!(
        "SELECT so.purchaser_id, p.name, SUM(soi.amount) as amount
         FROM sales_order_item soi
         JOIN sales_order so ON soi.order_id = so.id
         JOIN purchaser p ON so.purchaser_id = p.id
         WHERE 1=1 {}
         GROUP BY so.purchaser_id", date_cond
    );
    let mut q = sqlx::query(AssertSqlSafe(real_purchaser_sql.as_str()));
    for b in &binds { q = q.bind(b); }
    let real_rows = q.fetch_all(crate::db::pool()).await.unwrap_or_default();

    use std::collections::HashMap;
    // 按采购单位累加：real_amount
    let mut purchaser_map: HashMap<i64, (String, f64, f64)> = HashMap::new(); // id -> (name, real, supp)
    for r in &real_rows {
        let pid = r.get::<i64, _>("purchaser_id");
        let name = r.get::<String, _>("name");
        let amount = r.get::<f64, _>("amount");
        let entry = purchaser_map.entry(pid).or_insert((name, 0.0, 0.0));
        entry.1 += amount;
    }

    // 2a) 分摊增项：作为「目标单」收到的增项（正），按目标单采购单位聚合
    let supp_target_sql = format!(
        "SELECT so.purchaser_id, p.name, SUM(osi.amount) as supp_amount
         FROM order_supplement_item osi
         JOIN sales_order so ON osi.target_order_id = so.id
         JOIN purchaser p ON so.purchaser_id = p.id
         WHERE 1=1 {}
         GROUP BY so.purchaser_id", date_cond
    );
    let mut q2 = sqlx::query(AssertSqlSafe(supp_target_sql.as_str()));
    for b in &binds { q2 = q2.bind(b); }
    let supp_target_rows = q2.fetch_all(crate::db::pool()).await.unwrap_or_default();
    for r in &supp_target_rows {
        let pid = r.get::<i64, _>("purchaser_id");
        let name = r.get::<String, _>("name");
        let supp_amount = r.get::<f64, _>("supp_amount");
        let entry = purchaser_map.entry(pid).or_insert((name, 0.0, 0.0));
        entry.2 += supp_amount;
    }

    // 2b) 分摊减项：来源耗材单在报销口径里不报销，扣除其「真实明细金额」（非已分摊金额）
    // 净额 = 目标单收到的分摊增项(+) − 来源单真实金额(−)，正好等于分摊尾差（超分为正、少分为负）
    // 仅针对实际作为分摊来源的订单
    if !source_set.is_empty() {
        let source_real_sql = format!(
            "SELECT so.purchaser_id, p.name, so.id as order_id, SUM(soi.amount) as amount
             FROM sales_order_item soi
             JOIN sales_order so ON soi.order_id = so.id
             JOIN purchaser p ON so.purchaser_id = p.id
             WHERE 1=1 {}
             GROUP BY so.id", date_cond
        );
        let mut q2b = sqlx::query(AssertSqlSafe(source_real_sql.as_str()));
        for b in &binds { q2b = q2b.bind(b); }
        let source_real_rows = q2b.fetch_all(crate::db::pool()).await.unwrap_or_default();
        for r in &source_real_rows {
            let order_id = r.get::<i64, _>("order_id");
            if !source_set.contains(&order_id) { continue; } // 只扣除来源单
            let pid = r.get::<i64, _>("purchaser_id");
            let name = r.get::<String, _>("name");
            let amount = r.get::<f64, _>("amount");
            let entry = purchaser_map.entry(pid).or_insert((name, 0.0, 0.0));
            entry.2 -= amount;
        }
    }

    let mut by_purchaser: Vec<serde_json::Value> = purchaser_map.values().map(|(name, real, supp)| {
        serde_json::json!({
            "name": name,
            "real_amount": real,
            "supplement_amount": supp,
            "reimburse_amount": real + supp,
        })
    }).collect();
    by_purchaser.sort_by(|a, b| b["reimburse_amount"].as_f64().unwrap_or(0.0).partial_cmp(&a["reimburse_amount"].as_f64().unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal));

    // 3) 按商品汇总（报销口径）：真实明细（排除来源单）+ 分摊增项
    let mut product_map: HashMap<(String, String), (f64, f64)> = HashMap::new(); // (name,spec) -> (qty, amount)
    let prod_real_sql = format!(
        "SELECT soi.product_name, COALESCE(soi.spec,'') as spec, soi.order_id, soi.quantity, soi.amount
         FROM sales_order_item soi
         JOIN sales_order so ON soi.order_id = so.id
         WHERE 1=1 {}", date_cond
    );
    let mut q3 = sqlx::query(AssertSqlSafe(prod_real_sql.as_str()));
    for b in &binds { q3 = q3.bind(b); }
    let prod_real_rows = q3.fetch_all(crate::db::pool()).await.unwrap_or_default();
    for r in &prod_real_rows {
        let order_id = r.get::<i64, _>("order_id");
        if source_set.contains(&order_id) { continue; }
        let name = r.get::<String, _>("product_name");
        let spec = r.get::<String, _>("spec");
        let qty = r.get::<f64, _>("quantity");
        let amount = r.get::<f64, _>("amount");
        let entry = product_map.entry((name, spec)).or_insert((0.0, 0.0));
        entry.0 += qty;
        entry.1 += amount;
    }
    let prod_supp_sql = format!(
        "SELECT osi.product_name, COALESCE(osi.spec,'') as spec, osi.quantity, osi.amount
         FROM order_supplement_item osi
         JOIN sales_order so ON osi.target_order_id = so.id
         WHERE 1=1 {}", date_cond
    );
    let mut q4 = sqlx::query(AssertSqlSafe(prod_supp_sql.as_str()));
    for b in &binds { q4 = q4.bind(b); }
    let prod_supp_rows = q4.fetch_all(crate::db::pool()).await.unwrap_or_default();
    for r in &prod_supp_rows {
        let name = r.get::<String, _>("product_name");
        let spec = r.get::<String, _>("spec");
        let qty = r.get::<f64, _>("quantity");
        let amount = r.get::<f64, _>("amount");
        let entry = product_map.entry((name, spec)).or_insert((0.0, 0.0));
        entry.0 += qty;
        entry.1 += amount;
    }
    let mut by_product: Vec<serde_json::Value> = product_map.iter().map(|((name, spec), (qty, amount))| {
        serde_json::json!({
            "product_name": name,
            "spec": spec,
            "quantity": qty,
            "reimburse_amount": amount,
        })
    }).collect();
    by_product.sort_by(|a, b| b["reimburse_amount"].as_f64().unwrap_or(0.0).partial_cmp(&a["reimburse_amount"].as_f64().unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal));

    let result = serde_json::json!({
        "by_purchaser": by_purchaser,
        "by_product": by_product,
    });
    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

pub async fn api_query_allocation_source(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or("");
    let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");

    let mut sql = String::from(
        "SELECT ca.source_order_id, ca.total_amount, ca.allocated_amount, ca.remaining_balance, ca.status,
                so.order_no, so.order_date
         FROM consumable_allocation ca
         JOIN sales_order so ON ca.source_order_id = so.id
         WHERE 1=1"
    );
    let mut binds: Vec<String> = Vec::new();
    if !start_date.is_empty() {
        sql.push_str(" AND so.order_date >= ?");
        binds.push(start_date.to_string());
    }
    if !end_date.is_empty() {
        sql.push_str(" AND so.order_date <= ?");
        binds.push(end_date.to_string());
    }
    sql.push_str(" ORDER BY so.order_date DESC");

    let mut q = sqlx::query(AssertSqlSafe(sql.as_str()));
    for b in &binds { q = q.bind(b); }
    let rows = q.fetch_all(crate::db::pool()).await.unwrap_or_default();

    let mut result: Vec<serde_json::Value> = Vec::new();
    for r in &rows {
        let source_order_id = r.get::<i64, _>("source_order_id");
        // 查询该来源单分摊到的各目标单及金额
        let targets = sqlx::query(
            "SELECT so.order_no, SUM(osi.amount) as amount
             FROM order_supplement_item osi
             JOIN sales_order so ON osi.target_order_id = so.id
             WHERE osi.source_order_id = ?
             GROUP BY osi.target_order_id
             HAVING SUM(osi.amount) <> 0
             ORDER BY amount DESC"
        )
        .bind(source_order_id)
        .fetch_all(crate::db::pool())
        .await
        .unwrap_or_default();

        let target_list: Vec<serde_json::Value> = targets.iter().map(|t| {
            serde_json::json!({
                "order_no": t.get::<String, _>("order_no"),
                "amount": t.get::<f64, _>("amount"),
            })
        }).collect();

        result.push(serde_json::json!({
            "source_order_id": source_order_id,
            "order_no": r.get::<String, _>("order_no"),
            "order_date": r.get::<String, _>("order_date"),
            "total_amount": r.get::<f64, _>("total_amount"),
            "allocated_amount": r.get::<f64, _>("allocated_amount"),
            "remaining_balance": r.get::<f64, _>("remaining_balance"),
            "status": r.get::<i64, _>("status"),
            "targets": target_list,
        }));
    }

    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

pub async fn api_query_purchaser_balance(headers: axum::http::HeaderMap) -> impl IntoResponse {
    // 行级数据权限：purchaser 角色只能看自己绑定的采购单位往来
    let ctx = crate::auth::get_user_ctx(&headers).await;
    let mut sql = String::from(
        "SELECT p.id, p.name, 
                COALESCE(SUM(so.final_amount), 0) as sales_total,
                COALESCE(SUM(so.final_amount), 0) as unreceived
         FROM purchaser p 
         LEFT JOIN sales_order so ON so.purchaser_id = p.id"
    );
    let mut binds: Vec<i64> = Vec::new();
    if ctx.role == "purchaser" {
        sql.push_str(" WHERE p.id = ?");
        binds.push(ctx.purchaser_id);
    }
    sql.push_str(" GROUP BY p.id, p.name ORDER BY sales_total DESC");

    let mut q = sqlx::query(AssertSqlSafe(sql.as_str()));
    for b in &binds {
        q = q.bind(b);
    }
    let rows = q.fetch_all(crate::db::pool()).await.unwrap_or_default();
    
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

pub async fn api_query_purchaser_balance_export(headers: axum::http::HeaderMap) -> impl IntoResponse {
    // 行级数据权限：purchaser 角色只能导出自己绑定的采购单位往来
    let ctx = crate::auth::get_user_ctx(&headers).await;
    let mut sql = String::from(
        "SELECT p.id, p.name, 
                COALESCE(SUM(so.final_amount), 0) as sales_total,
                COALESCE(SUM(so.final_amount), 0) as unreceived
         FROM purchaser p 
         LEFT JOIN sales_order so ON so.purchaser_id = p.id"
    );
    let mut binds: Vec<i64> = Vec::new();
    if ctx.role == "purchaser" {
        sql.push_str(" WHERE p.id = ?");
        binds.push(ctx.purchaser_id);
    }
    sql.push_str(" GROUP BY p.id, p.name ORDER BY sales_total DESC");

    let mut q = sqlx::query(AssertSqlSafe(sql.as_str()));
    for b in &binds {
        q = q.bind(b);
    }
    let rows = q.fetch_all(crate::db::pool()).await.unwrap_or_default();
    
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

pub async fn api_sales_order_generate_purchase(
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/sales_order/generate_purchase").await {
        Err(e) => return e.into_response(),
        Ok(_) => {}
    }
    let ctx = crate::auth::get_user_ctx(&headers).await;

    let force = params.get("force").map(|s| s == "1").unwrap_or(false);

    let order_row = sqlx::query(
        "SELECT so.id, so.order_date, so.warehouse_id, so.warehouse_name, so.purchaser_id
         FROM sales_order so WHERE so.id = ?"
    )
    .bind(id)
    .fetch_optional(crate::db::pool())
    .await
    .unwrap_or(None);

    if order_row.is_none() {
        return (StatusCode::NOT_FOUND, serde_json::json!({ "message": "销售订单不存在" }).to_string()).into_response();
    }

    let row = order_row.unwrap();
    // 行级数据权限：仅可为自己采购单位的销售单生成采购订单
    let row_purchaser_id: i64 = row.get("purchaser_id");
    if !crate::auth::can_access_sales_order(&ctx, row_purchaser_id) {
        return (StatusCode::FORBIDDEN, "您没有权限操作此订单".to_string()).into_response();
    }
    let order_date = row.get::<String, _>("order_date");
    let so_warehouse_id = row.get::<i64, _>("warehouse_id");
    let so_warehouse_name = row.get::<Option<String>, _>("warehouse_name").unwrap_or_default();

    // 重复生成检查：是否已存在本销售单关联的采购订单
    // 已处理（非 pending）的采购订单受保护，不允许覆盖；
    // pending 状态在 force=true 时会被删除并重新合并，避免用户重复点击产生重复明细
    //
    // 检测范围（必须全部命中才算"已存在"）：
    //   1) 主表 source_sales_order_id = id（首次创建时的销售单）
    //   2) 明细中 source_sales_order_id = id（严格限定本销售单，避免误伤其他销售单/手动添加的明细）
    //   3) 当天同供应商已存在的非取消 PO（合并场景：首次创建来自其他销售单）
    // 三路任一命中即视为本销售单已生成过
    let mut existed: Vec<(i64, String, String, String)> = sqlx::query(
        "SELECT po.id, po.order_no, po.status, COALESCE(s.name, '未知供应商') as supplier_name
         FROM purchase_order po LEFT JOIN supplier s ON po.supplier_id = s.id
         WHERE po.source_sales_order_id = ?
            OR EXISTS (SELECT 1 FROM purchase_order_item poi
                       WHERE poi.order_id = po.id AND poi.source_sales_order_id = ?)
         ORDER BY po.id"
    )
    .bind(id)
    .bind(id)
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|r| (
        r.get::<i64, _>("id"),
        r.get::<String, _>("order_no"),
        r.get::<String, _>("status"),
        r.get::<String, _>("supplier_name"),
    ))
    .collect();

    // 兜底：当天同供应商已存在非取消 PO（按"供应商+日期"合并到他人 PO 的场景）
    if existed.is_empty() {
        let involved_suppliers: Vec<(i64,)> = sqlx::query(
            "SELECT DISTINCT supplier_id FROM sales_order_item WHERE order_id = ? AND supplier_id > 0"
        )
        .bind(id)
        .fetch_all(crate::db::pool())
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| (r.get::<i64, _>("supplier_id"),))
        .collect();
        if !involved_suppliers.is_empty() {
            // 拼 IN (?, ?, ?)
            let placeholders = std::iter::repeat("?")
                .take(involved_suppliers.len())
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "SELECT po.id, po.order_no, po.status, COALESCE(s.name, '未知供应商') as supplier_name
                 FROM purchase_order po LEFT JOIN supplier s ON po.supplier_id = s.id
                 WHERE po.order_date = ? AND po.status != 'cancelled'
                   AND po.supplier_id IN ({})
                 ORDER BY po.id",
                placeholders
            );
            let mut q = sqlx::query(AssertSqlSafe(sql.as_str())).bind(&order_date);
            for (sid,) in &involved_suppliers {
                q = q.bind(sid);
            }
            existed = q
                .fetch_all(crate::db::pool())
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|r| (
                    r.get::<i64, _>("id"),
                    r.get::<String, _>("order_no"),
                    r.get::<String, _>("status"),
                    r.get::<String, _>("supplier_name"),
                ))
                .collect();
        }
    }

    // force 路径需要的旧明细（按 po 收集），仅当 force=true 时填充
    // supplier_items 循环读取以做 UPDATE/INSERT/DELETE
    let mut old_items_by_po: std::collections::HashMap<i64, Vec<(i64, i64, String, f64, f64)>> = std::collections::HashMap::new();

    // force 路径下受影响的 PO 集合（仅在 force=true 时填充并使用）
    let mut affected_po_ids: std::collections::HashSet<i64>;

    // 把"销售单 (P,U) -> PO 明细 id"索引提到外层作用域，
    // supplier_items 循环需要用它做"按主键 UPDATE"判定
    let mut snapshot_by_pu: std::collections::HashMap<(i64, String, i64), i64> = std::collections::HashMap::new();

    if !existed.is_empty() {
        let status_text = |s: &str| match s {
            "pending" => "待分拣",
            "sorting" => "分拣中",
            "sorted" => "已分拣",
            "delivering" => "配送中",
            "delivered" => "已送达",
            "accepted" => "已验收",
            "settled" => "已结算",
            _ => "未知",
        };
        let pending: Vec<&(i64, String, String, String)> = existed.iter().filter(|x| x.2 == "pending").collect();
        let processed: Vec<&(i64, String, String, String)> = existed.iter().filter(|x| x.2 != "pending").collect();

        if !processed.is_empty() {
            let detail = processed.iter()
                .map(|x| format!("{}（{}，状态：{}）", x.1, x.3, status_text(&x.2)))
                .collect::<Vec<_>>().join("\n");
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
                serde_json::json!({
                    "error": true,
                    "message": format!("该销售订单已生成过采购订单，其中 {} 张已处理（{}），不能重新生成。\n如需调整，请到采购订单页面手动处理对应单据。", processed.len(), detail)
                }).to_string(),
            ).into_response();
        }

        if !force {
            let detail = pending.iter()
                .map(|x| format!("{}（{}）", x.1, x.3))
                .collect::<Vec<_>>().join("\n");
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
                serde_json::json!({
                    "warning": true,
                    "count": pending.len(),
                    "message": format!("该销售订单已生成过 {} 张采购订单（均为待分拣状态）：{}\n继续操作将对存在的采购订单对应的明细进行增删改（按新销售单明细），是否继续？", pending.len(), detail)
                }).to_string(),
            ).into_response();
        }

        // force=true 准备：
        // 不再"先删后插"，改为"对现存明细做 CRUD"。
        // 1) 收集本销售单所贡献的现存 PO
        // 2) 这些 PO 中本销售单已有的明细(source=id OR 0)暂存为 old_items_by_po
        // 3) supplier_items 循环里据此做 UPDATE/INSERT/DELETE
        // 注：已删除的 PO 不在范围内，无须处理

        // 收集本销售单所贡献的 PO（按主表 source_sales_order_id 命中）
        affected_po_ids = std::collections::HashSet::new();
        for x in &pending {
            affected_po_ids.insert(x.0);
        }

        // 兜底：按 supplier_id + order_date 找出当天所有非取消 PO，让重算也能覆盖到
        // （例如本销售单明细合并到他人创建的 PO 时）
        let po_pool: Vec<(i64, i64, String)> = sqlx::query(
            "SELECT po.id, po.supplier_id, po.status FROM purchase_order po WHERE po.order_date = ? AND po.status != 'cancelled'"
        )
        .bind(&order_date)
        .fetch_all(crate::db::pool())
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| (r.get::<i64, _>("id"), r.get::<i64, _>("supplier_id"), r.get::<String, _>("status")))
        .collect();

        // 计算该销售单涉及的 supplier_id 集合（来自明细），用于过滤受影响的 PO
        let mut involved_supplier_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();
        let preview_items: Vec<(i64,)> = sqlx::query(
            "SELECT supplier_id FROM sales_order_item WHERE order_id = ?"
        )
        .bind(id)
        .fetch_all(crate::db::pool())
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| (r.get::<i64, _>("supplier_id"),))
        .collect();
        for (sid,) in &preview_items {
            if *sid > 0 {
                involved_supplier_ids.insert(*sid);
            }
        }
        for (_po_id, sid, _status) in &po_pool {
            if involved_supplier_ids.contains(sid) {
                affected_po_ids.insert(*_po_id);
            }
        }

        // 拉取每个 affected PO 中**本销售单**已有的明细（严格按 source_sales_order_id = id）
        // 严格匹配是为了不误伤其他销售单（other_so）贡献的明细：
        //   - other_so 贡献的明细保持其 source=other_so，不参与本轮"补充"逻辑
        //   - api_purchase_order_update 重插时已用主表 source 兜底回填了归属，
        //     所以本销售单贡献的明细其 source=id 是稳定可识别的
        // 匹配键用 COALESCE(sales_unit, unit, '') 而非 sales_unit：purchase_order_update
        // 重插时 sales_unit 字段丢失（前端表单不传），但 unit 仍保留，匹配用 unit 才能识别。
        // 暂存为 po_id -> Vec<(id, product_id, unit_key, quantity, amount)>
        for po_id in &affected_po_ids {
            let rows: Vec<(i64, i64, String, f64, f64)> = sqlx::query(
                "SELECT id, product_id, COALESCE(NULLIF(TRIM(COALESCE(sales_unit, '')), ''), unit, '') as unit_key, quantity, COALESCE(amount, 0) as amount
                 FROM purchase_order_item
                 WHERE order_id = ? AND source_sales_order_id = ?"
            )
            .bind(po_id)
            .bind(id)
            .fetch_all(crate::db::pool())
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|r| (
                r.get::<i64, _>("id"),
                r.get::<i64, _>("product_id"),
                r.get::<String, _>("unit_key"),
                r.get::<f64, _>("quantity"),
                r.get::<f64, _>("amount"),
            ))
            .collect();
            if !rows.is_empty() {
                old_items_by_po.insert(*po_id, rows);
            }
        }

        // 读取本销售单"曾生成过的 PO 明细 id 快照"（用于跨调用追踪）：
        // 解决 force=true 路径下"用户从 PO 中删了明细 → to_consume 池因 source=id 过滤
        // 而为 0 → 重新 INSERT 全部 3 条"造成的重复 BUG。
        // 快照是一组 PO 明细主键 id。每次 force 结束时，会用"本次实际 UPDATE/INSERT
        // 的 PO 明细 id 集合"覆盖写回，所以它始终代表"本销售单最新一次生成时
        // 写入/更新的 PO 明细"。
        // 匹配策略：
        //   - 销售单 (P,U) 在快照中：按主键 UPDATE（id 在 DB 中消失时退化为 INSERT）
        //   - 销售单 (P,U) 不在快照中：INSERT 新条目（source=本单）
        //   - 快照中存在但销售单 (P,U) 不再出现：从快照中清掉（PO 中对应行保留，不删）
        //   - PO 中 source=本单 但不在新快照的明细：保留不动（兜底防误删）
        let snapshot_str: Option<String> = sqlx::query(
            "SELECT generated_purchase_item_ids FROM sales_order WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(crate::db::pool())
        .await
        .unwrap_or(None)
        .and_then(|r| r.try_get::<Option<String>, _>("generated_purchase_item_ids").ok().flatten());
        let snapshot_ids: std::collections::HashSet<i64> = snapshot_str
            .as_deref()
            .and_then(|s| serde_json::from_str::<Vec<i64>>(s).ok())
            .map(|v| v.into_iter().collect())
            .unwrap_or_default();

        // 把快照 id 翻译成 (P,U) -> po_item_id 索引（仅 source=本单的 id 纳入）
        // 同一 (P,U) 在快照中只可能 1 条（销售单明细与 PO 明细 1:1 关系）
        // snapshot_by_pu 已在外层定义
        if !snapshot_ids.is_empty() {
            let placeholders = std::iter::repeat("?")
                .take(snapshot_ids.len())
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "SELECT id, product_id, COALESCE(NULLIF(TRIM(COALESCE(sales_unit, '')), ''), unit, '') as unit_key, warehouse_id
                 FROM purchase_order_item
                 WHERE source_sales_order_id = ? AND id IN ({})",
                placeholders
            );
            let mut q = sqlx::query(AssertSqlSafe(sql.as_str())).bind(id);
            for iid in &snapshot_ids {
                q = q.bind(iid);
            }
            let snap_rows: Vec<(i64, i64, String, i64)> = q
                .fetch_all(crate::db::pool())
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|r| (
                    r.get::<i64, _>("id"),
                    r.get::<i64, _>("product_id"),
                    r.get::<String, _>("unit_key"),
                    r.get::<i64, _>("warehouse_id"),
                ))
                .collect();
            // 索引用 (P, U, warehouse_id) 三元组：相同 (P,U) 在不同 warehouse 的明细
            // 是不同行，跨仓库复用会破坏"不同仓库出库同商品同时保留"的业务规则。
            for (po_id, pid, ukey, wh_id) in snap_rows {
                snapshot_by_pu.insert((pid, ukey, wh_id), po_id);
            }
        }
    }

    // 本轮 force 实际写入/更新的 PO 明细 id（循环结束后回写 sales_order 快照）
    // 提到外层作用域以供 supplier_items 循环后的快照回写使用
    let mut new_snapshot_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();

    // 取销售订单明细（含销售单位 unit、备注）
    let item_rows = sqlx::query(
        "SELECT soi.product_id, soi.product_name, soi.alias1, soi.alias2, soi.spec, soi.unit, soi.quantity, soi.pre_sale_quantity, soi.supplier_id, soi.supplier_name, soi.remark, p.purchase_price, p.base_unit, p.base_price
         FROM sales_order_item soi LEFT JOIN product p ON soi.product_id = p.id
         WHERE soi.order_id = ?"
    )
    .bind(id)
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();

    // 按供应商分组明细；每个 (supplier_id, product_id, warehouse_id, sales_unit) 唯一
    // 单条销售明细条目的结构：(product_id, product_name, alias1, alias2, spec, unit, quantity, pre_sale_quantity, unit_price, base_unit, base_price, remark)
    let mut supplier_items: std::collections::HashMap<i64, Vec<(i64, String, String, String, String, String, f64, f64, f64, String, f64, String)>> = std::collections::HashMap::new();

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
        // ordered_quantity 由销售订单的预售数量生成：两者需保持同步
        let ordered_quantity = r.get::<Option<f64>, _>("pre_sale_quantity").unwrap_or(quantity);
        let purchase_price = r.get::<f64, _>("purchase_price");
        let base_unit = r.get::<Option<String>, _>("base_unit").unwrap_or_default();
        let base_price = r.get::<f64, _>("base_price");
        let remark = r.get::<Option<String>, _>("remark").unwrap_or_default();

        let unit_price = if purchase_price > 0.0 { purchase_price } else { base_price };

        supplier_items.entry(supplier_id).or_insert_with(Vec::new).push(
            (product_id, product_name, alias1, alias2, spec, unit, quantity, ordered_quantity, unit_price, base_unit, base_price, remark)
        );
    }

    if supplier_items.is_empty() {
        return (StatusCode::BAD_REQUEST, serde_json::json!({ "message": "销售订单中没有供应商信息，无法生成采购订单" }).to_string()).into_response();
    }

    let mut created_count = 0;
    let mut merged_count = 0;
    // 跟踪已处理的 PO ID（force 清理时用于计算差集）
    let mut processed_po_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();

    for (supplier_id, items) in supplier_items {
        // 计算该供应商所有明细的合计（用于新建 PO 时初始化主表金额）
        let mut total_amount: f64 = 0.0;
        let wh_id_set: std::collections::HashSet<i64> = std::collections::HashSet::new();
        let mut wh_names: Vec<String> = Vec::new();
        for (product_id, product_name, alias1, alias2, spec, unit, quantity, ordered_quantity, unit_price, _base_unit, _base_price, remark) in &items {
            total_amount += quantity * unit_price;
            if !wh_names.contains(unit) && !unit.trim().is_empty() {
                wh_names.push(unit.clone());
            }
            let _ = (product_id, product_name, alias1, alias2, spec, ordered_quantity, remark);
        }
        // 主表仓库从销售订单的 warehouse 取（销售单的仓库为唯一入口仓库）
        let main_wh_id = so_warehouse_id;
        let main_wh_name = so_warehouse_name.clone();
        let _ = (wh_id_set, wh_names, total_amount);

        // 查找当天同供应商是否已有非取消的采购订单
        let existing_po: Option<i64> = sqlx::query(
            "SELECT id FROM purchase_order WHERE supplier_id = ? AND order_date = ? AND status != 'cancelled' ORDER BY id LIMIT 1"
        )
        .bind(supplier_id)
        .bind(&order_date)
        .fetch_optional(crate::db::pool())
        .await
        .unwrap_or(None)
        .map(|r| r.get::<i64, _>("id"));

        let po_id: i64 = if let Some(existing_id) = existing_po {
            existing_id
        } else {
            let supplier_name_result = sqlx::query("SELECT name FROM supplier WHERE id = ?")
                .bind(supplier_id)
                .fetch_optional(crate::db::pool())
                .await
                .unwrap_or(None);
            let _supplier_name = match supplier_name_result {
                Some(sr) => sr.get::<String, _>("name"),
                None => "未知供应商".to_string(),
            };

            let order_no = generate_order_no("purchase", &order_date).await;

            let insert_res = sqlx::query(
                "INSERT INTO purchase_order(supplier_id, order_no, order_date, total_amount, discount_rate, amount_reduction, final_amount, warehouse_id, warehouse_name, remark, source_sales_order_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(supplier_id)
            .bind(&order_no)
            .bind(&order_date)
            .bind(0.0)
            .bind(0.0)
            .bind(0.0)
            .bind(0.0)
            .bind(main_wh_id)
            .bind(&main_wh_name)
            .bind(None::<String>)
            .bind(id)
            .execute(crate::db::pool())
            .await;

            match insert_res {
                Ok(res) => {
                    created_count += 1;
                    res.last_insert_rowid()
                }
                Err(e) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "message": format!("创建采购订单失败: {}", e) }).to_string()).into_response();
                }
            }
        };
        processed_po_ids.insert(po_id);

        // 拉取 PO 中本销售单已存在的明细（按 product_id+sales_unit 去重，仅本销售单贡献的）
        // 这样其他销售单贡献的明细不会被错误合并
        // 注意：force=true 路径下，old_items_by_po 已包含待 diff 的旧明细，此处复用之
        let existing_items: Vec<(i64, f64, String)> = if let Some(old_rows) = old_items_by_po.get(&po_id) {
            old_rows.iter().map(|(_id, pid, sunit, qty, _amt)| (*pid, *qty, sunit.clone())).collect()
        } else {
            sqlx::query(
                "SELECT product_id, quantity, COALESCE(sales_unit, '') as sales_unit
                 FROM purchase_order_item WHERE order_id = ? AND source_sales_order_id = ?"
            )
            .bind(po_id)
            .bind(id)
            .fetch_all(crate::db::pool())
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|r| (
                r.get::<i64, _>("product_id"),
                r.get::<f64, _>("quantity"),
                r.get::<String, _>("sales_unit"),
            ))
            .collect()
        };

        // "补充式"语义（force=true 路径），按 sales_order.generated_purchase_item_ids 快照驱动：
        //   1) 销售单 (P,U) 在快照中（且 id 仍在 PO） → 按主键 UPDATE 同步
        //   2) 销售单 (P,U) 在快照中（但 id 已从 PO 消失/被删） → 退化为 INSERT 补回
        //   3) 销售单 (P,U) 不在快照中，但 PO 中有同 (P,U) 的"孤儿"（其他 source，可能是老代码
        //      INSERT fallback 留下的 source=主表兜底） → UPDATE 复用并把 source 改成本销售单
        //      （关键：避免与孤儿并存形成 (P,U) 重复）
        //   4) 销售单 (P,U) 不在快照中且 PO 中无同 (P,U) → INSERT 新条目（source=本单）
        //   5) 快照中存在但销售单 (P,U) 不再出现 → 从快照中清掉（PO 中对应行保留，不删）
        //   6) PO 中 source=本单 但不在新快照的明细：保留不动（兜底防误删）
        // 这样既保证"用户从 PO 删的明细能补回"（快照覆盖到被删的 id），
        // 又不会因老代码 INSERT fallback 留下的 source=主表兜底孤儿"被新代码当成本销售单的",
        // 进而形成 (P,U) 重复插入。
        let mut updated_lines: i64 = 0;
        let mut added_lines: i64 = 0;
        let mut delta_amount: f64 = 0.0; // UPDATE 同步：金额增量 = 新 - 旧

        for (product_id, product_name, alias1, alias2, spec, unit, quantity, ordered_quantity, unit_price, _base_unit, _base_price, remark) in items {
            let amount = quantity * unit_price;
            let base_quantity = quantity;
            let sales_unit = unit.clone();
            // 1) 快照中找 (P,U, warehouse) → 尝试按主键 UPDATE
            // key 加 warehouse_id：跨仓库同 (P,U) 是不同行，必须区分
            let mut updated_via_snapshot = false;
            let key = (product_id, sales_unit.trim().to_string(), main_wh_id);
            if let Some(snap_po_id) = snapshot_by_pu.get(&key).copied() {
                snapshot_by_pu.remove(&key);
                let new_amount = quantity * unit_price;
                // 读取旧 amount 以计算差量（保持主表金额正确）
                let old_amount: f64 = sqlx::query(
                    "SELECT COALESCE(amount, 0) as amount FROM purchase_order_item WHERE id = ? AND order_id = ?"
                )
                .bind(snap_po_id)
                .bind(po_id)
                .fetch_optional(crate::db::pool())
                .await
                .ok()
                .flatten()
                .map(|r| r.get::<f64, _>("amount"))
                .unwrap_or(0.0);
                let upd_res = sqlx::query(
                    "UPDATE purchase_order_item SET product_id = ?, product_name = ?, alias1 = ?, alias2 = ?, spec = ?, unit = ?, unit_price = ?, quantity = ?, base_quantity = ?, amount = ?, ordered_quantity = ?, remark = ?, warehouse_id = ?, warehouse_name = ?, sales_unit = ? WHERE order_id = ? AND id = ?"
                )
                .bind(product_id)
                .bind(&product_name)
                .bind(&alias1)
                .bind(&alias2)
                .bind(&spec)
                .bind(&unit)
                .bind(unit_price)
                .bind(quantity)
                .bind(base_quantity)
                .bind(new_amount)
                .bind(ordered_quantity)
                .bind(&remark)
                .bind(main_wh_id)
                .bind(&main_wh_name)
                .bind(&sales_unit)
                .bind(po_id)
                .bind(snap_po_id)
                .execute(crate::db::pool())
                .await
                .ok();
                let affected = upd_res.map(|r| r.rows_affected()).unwrap_or(0);
                if affected > 0 {
                    // 真正命中 DB 中存在的明细 → UPDATE 成功
                    new_snapshot_ids.insert(snap_po_id);
                    updated_via_snapshot = true;
                    delta_amount += new_amount - old_amount; // 差量计入主表
                    updated_lines += 1;
                }
                // affected == 0 → 快照 id 在 DB 中已被删，退化到下面的"孤儿复用"或 INSERT 分支
            }

            if !updated_via_snapshot {
                // 2) 快照命中但 id 已被删 或 3) 快照无此 (P,U) →
                //    先在 PO 中找同 (P,U,**warehouse**) 的"孤儿"（任意 source；通常 source=主表兜底），
                //    有就 UPDATE 复用并把 source 改成本销售单；没有才 INSERT。
                // 这是修复"老代码 INSERT fallback 留下 source=主表兜底孤儿"的二次重复 BUG 的关键。
                //
                // 关键：匹配键必须包含 warehouse_id！否则"不同仓库出库同 (P,U) 商品"场景下，
                // SO Y force 生成时会错误地 UPDATE SO X 留下的 wh=X 的孤儿，把 SO Y 的
                // 明细（数量/仓库等）覆盖到 SO X 的行上，导致不同仓库的明细被吞并成最后一条。
                // 业务上要求"不同仓库出库的相同商品在同一个 PO 中要同时保留"，
                // 所以仓库是行级区分维度，不能跨仓库复用。
                let new_amount = quantity * unit_price;
                // 孤儿匹配：同 (P,U,warehouse_id) 任意 source；优先选 source=本单的（极端兜底），
                // 否则按 id 最小的（最早插入的）复用。
                let orphan_id: Option<i64> = sqlx::query(
                    "SELECT id FROM purchase_order_item
                     WHERE order_id = ? AND product_id = ?
                       AND COALESCE(NULLIF(TRIM(COALESCE(sales_unit, '')), ''), unit, '') = ?
                       AND warehouse_id = ?
                     ORDER BY (CASE WHEN source_sales_order_id = ? THEN 0 ELSE 1 END), id
                     LIMIT 1"
                )
                .bind(po_id)
                .bind(product_id)
                .bind(sales_unit.trim())
                .bind(main_wh_id)
                .bind(id)
                .fetch_optional(crate::db::pool())
                .await
                .ok()
                .flatten()
                .map(|r| r.get::<i64, _>("id"));

                if let Some(orphan_po_id) = orphan_id {
                    // 读取旧 amount 以计算差量
                    let old_amount: f64 = sqlx::query(
                        "SELECT COALESCE(amount, 0) as amount FROM purchase_order_item WHERE id = ? AND order_id = ?"
                    )
                    .bind(orphan_po_id)
                    .bind(po_id)
                    .fetch_optional(crate::db::pool())
                    .await
                    .ok()
                    .flatten()
                    .map(|r| r.get::<f64, _>("amount"))
                    .unwrap_or(0.0);
                    // UPDATE 复用孤儿：同步字段 + 把 source 改成本销售单
                    let upd_res = sqlx::query(
                        "UPDATE purchase_order_item SET product_id = ?, product_name = ?, alias1 = ?, alias2 = ?, spec = ?, unit = ?, unit_price = ?, quantity = ?, base_quantity = ?, amount = ?, ordered_quantity = ?, remark = ?, warehouse_id = ?, warehouse_name = ?, sales_unit = ?, source_sales_order_id = ? WHERE order_id = ? AND id = ?"
                    )
                    .bind(product_id)
                    .bind(&product_name)
                    .bind(&alias1)
                    .bind(&alias2)
                    .bind(&spec)
                    .bind(&unit)
                    .bind(unit_price)
                    .bind(quantity)
                    .bind(base_quantity)
                    .bind(new_amount)
                    .bind(ordered_quantity)
                    .bind(&remark)
                    .bind(main_wh_id)
                    .bind(&main_wh_name)
                    .bind(&sales_unit)
                    .bind(id)
                    .bind(po_id)
                    .bind(orphan_po_id)
                    .execute(crate::db::pool())
                    .await
                    .ok();
                    if let Some(res) = upd_res {
                        if res.rows_affected() > 0 {
                            new_snapshot_ids.insert(orphan_po_id);
                            delta_amount += new_amount - old_amount;
                            updated_lines += 1;
                            // 不加 added_lines：这是复用，不是新增
                            continue; // 处理下一个销售单明细
                        }
                    }
                    // UPDATE 0 行（极端情况：行已被并发删除）→ 回落到 INSERT
                }

                // 4) 快照无此 (P,U) 且 PO 中无孤儿 → INSERT 新条目（source=本单）
                let ins_res = sqlx::query(
                    "INSERT INTO purchase_order_item(order_id, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, base_quantity, amount, ordered_quantity, remark, warehouse_id, warehouse_name, sales_unit, source_sales_order_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
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
                .bind(ordered_quantity)
                .bind(&remark)
                .bind(main_wh_id)
                .bind(&main_wh_name)
                .bind(&sales_unit)
                .bind(id)
                .execute(crate::db::pool())
                .await
                .ok();
                if let Some(res) = ins_res {
                    let new_po_id = res.last_insert_rowid();
                    if new_po_id > 0 {
                        new_snapshot_ids.insert(new_po_id);
                    }
                }
                delta_amount += amount;
                added_lines += 1;
            }
        }
        // 注意：snapshot_by_pu 中剩余的项（快照有但销售单 (P,U) 不再出现）
        // → 不写入 new_snapshot_ids（让它们从快照中清掉，PO 中对应行保留，不删）

        // 同步更新主表总金额：仅按"补充式"的差量调整（新增/UPDATE 金额变化）
        if delta_amount.abs() > 0.0001 {
            let _ = sqlx::query(
                "UPDATE purchase_order SET total_amount = COALESCE(total_amount, 0) + ?, final_amount = COALESCE(final_amount, 0) + ? WHERE id = ?"
            )
            .bind(delta_amount)
            .bind(delta_amount)
            .bind(po_id)
            .execute(crate::db::pool())
            .await
            .ok();
        }

        // "补充式"语义：不删主表（即使主表对本单没有可见贡献，也要保留 PO 整体给其他销售单）

        merged_count += updated_lines + added_lines;
        let _ = existing_items; // 兼容旧变量（CRUD 路径下不再使用）
    }

    // 把本次 force 实际写入/更新的 PO 明细 id 写回 sales_order 快照列
    // （只有 force=true 路径填充了 new_snapshot_ids；非 force 走警告返回，不会到这里）
    if !new_snapshot_ids.is_empty() {
        let mut ids: Vec<i64> = new_snapshot_ids.into_iter().collect();
        ids.sort();
        let json = serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string());
        let _ = sqlx::query("UPDATE sales_order SET generated_purchase_item_ids = ? WHERE id = ?")
            .bind(&json)
            .bind(id)
            .execute(crate::db::pool())
            .await;
    }

    // "补充式"语义：不再强制清理/删除其他销售单主导的 PO。
    // 即使用户在当前销售单上移除了某个供应商的全部明细，该供应商若还有其他销售单贡献的明细，
    // 也不应被本流程删掉——PO 的归属以"明细来源"为准，不以"主表创建者"为准。

    crate::auth::log_operation(&ctx, "sales_order.generate_purchase", "sales_order", &id.to_string(),
        &format!("由销售单生成采购订单，新建 {} 张，合并明细 {} 条", created_count, merged_count)).await;

    let msg = if created_count > 0 && merged_count > 0 {
        format!("成功生成 {} 张采购订单，并合并 {} 条相同明细", created_count, merged_count)
    } else if created_count > 0 {
        format!("成功生成 {} 张采购订单", created_count)
    } else {
        format!("已将销售明细合并到已有采购订单（{} 条合并）", merged_count)
    };
    (StatusCode::OK, serde_json::json!({
        "count": created_count,
        "merged": merged_count,
        "message": msg
    }).to_string()).into_response()
}

pub async fn api_query_product_rank(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
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
    let top_rows = query1.fetch_all(crate::db::pool()).await.unwrap_or_default();
    
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
    let slow_rows = query2.fetch_all(crate::db::pool()).await.unwrap_or_default();
    
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

pub async fn api_query_overview(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let month = params.get("month").map(|s| s.as_str()).unwrap_or("");
    
    let purchase_total: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(po.total_amount), 0) FROM purchase_order po WHERE strftime('%Y-%m', po.order_date) = ?"
    )
    .bind(month)
    .fetch_one(crate::db::pool())
    .await
    .unwrap_or(0.0);
    
    let sales_total: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(so.total_amount), 0) FROM sales_order so WHERE strftime('%Y-%m', so.order_date) = ?"
    )
    .bind(month)
    .fetch_one(crate::db::pool())
    .await
    .unwrap_or(0.0);
    
    let stock_total: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(i.quantity * pr.selling_price), 0) FROM inventory i JOIN product pr ON i.product_id = pr.id"
    )
    .fetch_one(crate::db::pool())
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
    .fetch_all(crate::db::pool())
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
    .fetch_all(crate::db::pool())
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

pub async fn api_query_category_stats(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
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
    .fetch_all(crate::db::pool())
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

pub async fn api_query_document_summary(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
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
    .fetch_all(crate::db::pool())
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

pub async fn api_query_stock_balance(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let product_name = params.get("product_name").map(|s| s.as_str()).unwrap_or("");
    let category_id = params.get("category_id").map(|s| s.as_str()).unwrap_or("");
    
    let sql = if category_id.is_empty() {
        format!(
            "SELECT i.id, i.product_id, i.warehouse_id, i.quantity, i.min_stock, i.max_stock,
                    p.name as product_name, p.spec, p.unit, p.base_price,
                    (i.quantity * p.base_price) as amount
             FROM inventory i JOIN product p ON i.product_id = p.id
             WHERE p.name LIKE ? ORDER BY p.name"
        )
    } else {
        format!(
            "SELECT i.id, i.product_id, i.warehouse_id, i.quantity, i.min_stock, i.max_stock,
                    p.name as product_name, p.spec, p.unit, p.base_price,
                    (i.quantity * p.base_price) as amount
             FROM inventory i JOIN product p ON i.product_id = p.id
             WHERE p.name LIKE ? AND p.category_id = ? ORDER BY p.name"
        )
    };
    
    let pattern = format!("%{}%", product_name);
    let rows = if category_id.is_empty() {
        sqlx::query(AssertSqlSafe(sql.as_str()))
            .bind(&pattern)
            .fetch_all(crate::db::pool())
            .await
            .unwrap_or_default()
    } else {
        let cat_id: i64 = category_id.parse().unwrap_or(0);
        sqlx::query(AssertSqlSafe(sql.as_str()))
            .bind(&pattern)
            .bind(cat_id)
            .fetch_all(crate::db::pool())
            .await
            .unwrap_or_default()
    };
    
    let items: Vec<serde_json::Value> = rows.iter().map(|row| {
        let qty = row.try_get::<f64, _>("quantity").unwrap_or(0.0);
        let amt = row.try_get::<f64, _>("amount").unwrap_or(0.0);
        serde_json::json!({
            "product_id": row.get::<i64, _>("product_id"),
            "product_name": row.get::<String, _>("product_name"),
            "spec": row.get::<Option<String>, _>("spec"),
            "unit": row.get::<Option<String>, _>("unit"),
            "quantity": qty,
            "amount": amt,
            "min_stock": row.try_get::<f64, _>("min_stock").unwrap_or(0.0),
            "max_stock": row.try_get::<f64, _>("max_stock").unwrap_or(0.0),
        })
    }).collect();
    
    (StatusCode::OK, serde_json::to_string(&items).unwrap())
}

pub async fn api_query_stock_flow(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let product_name = params.get("product_name").map(|s| s.as_str()).unwrap_or("");
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or("");
    let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    let product_id = params.get("product_id").and_then(|s| s.parse::<i64>().ok());
    
    let pattern = format!("%{}%", product_name);
    
    let mut where_clause = String::from("WHERE 1=1");
    let mut sales_where_clause = String::from("WHERE 1=1");
    if let Some(pid) = product_id {
        where_clause.push_str(&format!(" AND p.id = {}", pid));
        sales_where_clause.push_str(&format!(" AND p.id = {}", pid));
    } else if !product_name.is_empty() {
        where_clause.push_str(" AND p.name LIKE ?");
        sales_where_clause.push_str(" AND p.name LIKE ?");
    }
    if !start_date.is_empty() {
        where_clause.push_str(&format!(" AND po.order_date >= '{}'", start_date));
        sales_where_clause.push_str(&format!(" AND so.order_date >= '{}'", start_date));
    }
    if !end_date.is_empty() {
        where_clause.push_str(&format!(" AND po.order_date <= '{}'", end_date));
        sales_where_clause.push_str(&format!(" AND so.order_date <= '{}'", end_date));
    }
    
    // 采购入库 + 销售出库
    let purchase_sql = format!(
        "SELECT po.order_date as create_time, '采购入库' as type, p.name as product_name, p.spec,
                poi.quantity as in_quantity, 0 as out_quantity, poi.remark
         FROM purchase_order_item poi
         JOIN purchase_order po ON poi.order_id = po.id
         JOIN product p ON poi.product_id = p.id
         {}
         UNION ALL
         SELECT so.order_date as create_time, '销售出库' as type, p.name as product_name, p.spec,
                0 as in_quantity, soi.quantity as out_quantity, soi.remark
         FROM sales_order_item soi
         JOIN sales_order so ON soi.order_id = so.id
         JOIN product p ON soi.product_id = p.id
         {}
         ORDER BY create_time",
        where_clause, sales_where_clause
    );
    
    let rows = if product_id.is_some() || product_name.is_empty() {
        sqlx::query(AssertSqlSafe(purchase_sql.as_str()))
            .fetch_all(crate::db::pool())
            .await
            .unwrap_or_default()
    } else {
        sqlx::query(AssertSqlSafe(purchase_sql.as_str()))
            .bind(&pattern)
            .bind(&pattern)
            .fetch_all(crate::db::pool())
            .await
            .unwrap_or_default()
    };
    
    let items: Vec<serde_json::Value> = rows.iter().map(|row| {
        let in_qty = row.try_get::<f64, _>("in_quantity").unwrap_or(0.0);
        let out_qty = row.try_get::<f64, _>("out_quantity").unwrap_or(0.0);
        serde_json::json!({
            "create_time": row.get::<String, _>("create_time"),
            "type": row.get::<String, _>("type"),
            "product_name": row.get::<String, _>("product_name"),
            "spec": row.get::<Option<String>, _>("spec"),
            "in_quantity": in_qty,
            "out_quantity": out_qty,
            "remark": row.get::<Option<String>, _>("remark"),
        })
    }).collect();
    
    (StatusCode::OK, serde_json::to_string(&items).unwrap())
}

pub async fn api_query_stock_summary(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or("");
    let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    let (rows, total_in, total_out, total_discounted_out) = compute_stock_summary(start_date, end_date).await;

    // 收集所有仓库列表（保持原有兼容性，虽然导出不再需要）
    use std::collections::BTreeMap;
    let mut warehouse_names: BTreeMap<i64, String> = BTreeMap::new();
    for r in &rows {
        if r.warehouse_id >= 0 {
            warehouse_names.insert(r.warehouse_id, r.warehouse_name.clone());
        }
    }
    let warehouse_list: Vec<serde_json::Value> = warehouse_names
        .iter()
        .map(|(id, name)| serde_json::json!({"id": id, "name": name}))
        .collect();

    let items: Vec<serde_json::Value> = rows.iter().map(|r| {
        serde_json::json!({
            "day": r.day,
            "warehouse_id": r.warehouse_id,
            "warehouse_name": r.warehouse_name,
            "is_summary": r.is_summary,
            "in_amount": r.in_amount,
            "in_item_count": r.in_item_count,
            "in_order_count": r.in_order_count,
            "out_amount": r.out_amount,
            "out_item_count": r.out_item_count,
            "out_order_count": r.out_order_count,
            "discounted_out_amount": r.discounted_out_amount,
            "gross_profit": r.gross_profit,
        })
    }).collect();

    let result = serde_json::json!({
        "rows": items,
        "warehouses": warehouse_list,
        "total_in_amount": total_in,
        "total_out_amount": total_out,
        "total_discounted_out_amount": total_discounted_out,
        "total_gross_profit": total_discounted_out - total_in,
    });

    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

pub async fn api_query_stock_summary_export(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or("");
    let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    let (rows, total_in, total_out, total_discounted_out) = compute_stock_summary(start_date, end_date).await;

    let mut workbook = rust_xlsxwriter::Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("出入库统计").unwrap();

    let header_format = rust_xlsxwriter::Format::new()
        .set_bold()
        .set_background_color(rust_xlsxwriter::Color::RGB(0x2E75B6))
        .set_font_color(rust_xlsxwriter::Color::White)
        .set_align(rust_xlsxwriter::FormatAlign::Center)
        .set_border(rust_xlsxwriter::FormatBorder::Thin);

    let summary_format = rust_xlsxwriter::Format::new()
        .set_bold()
        .set_background_color(rust_xlsxwriter::Color::RGB(0xFFF2CC))
        .set_border(rust_xlsxwriter::FormatBorder::Thin);

    let num_format = rust_xlsxwriter::Format::new()
        .set_num_format("#,##0.00")
        .set_align(rust_xlsxwriter::FormatAlign::Right);

    let num_format_sum = rust_xlsxwriter::Format::new()
        .set_bold()
        .set_num_format("#,##0.00")
        .set_background_color(rust_xlsxwriter::Color::RGB(0xFFF2CC))
        .set_align(rust_xlsxwriter::FormatAlign::Right);

    // 列顺序：日期、仓库、入库金额、入库单数、入库条数、出库金额、下浮后出库金额、出库单数、出库条数、毛利
    let headers = ["日期", "仓库", "入库金额", "入库单数", "入库条数", "出库金额", "下浮后出库金额", "出库单数", "出库条数", "毛利"];
    for (col, header) in headers.iter().enumerate() {
        worksheet.write_with_format(0, col as u16, *header, &header_format).unwrap();
    }

    // 设置列宽
    worksheet.set_column_width(0, 14).unwrap(); // 日期
    worksheet.set_column_width(1, 16).unwrap(); // 仓库
    worksheet.set_column_width_pixels(2, 90).unwrap();  // 入库金额
    worksheet.set_column_width_pixels(3, 70).unwrap();  // 入库单数
    worksheet.set_column_width_pixels(4, 70).unwrap();  // 入库条数
    worksheet.set_column_width_pixels(5, 90).unwrap();  // 出库金额
    worksheet.set_column_width_pixels(6, 90).unwrap();  // 下浮后出库金额
    worksheet.set_column_width_pixels(7, 70).unwrap();  // 出库单数
    worksheet.set_column_width_pixels(8, 70).unwrap();  // 出库条数
    worksheet.set_column_width_pixels(9, 90).unwrap();  // 毛利

    let mut prev_day = String::new();
    for (row_idx, row) in rows.iter().enumerate() {
        let sheet_row = (row_idx + 1) as u32;
        // 日期只在第一次出现时填写
        let day_display = if row.day != prev_day { row.day.as_str() } else { "" };
        prev_day = row.day.clone();

        if row.is_summary {
            worksheet.write_with_format(sheet_row, 0, day_display, &summary_format).unwrap();
            worksheet.write_with_format(sheet_row, 1, &row.warehouse_name, &summary_format).unwrap();
            worksheet.write_with_format(sheet_row, 2, row.in_amount, &num_format_sum).unwrap();
            worksheet.write_with_format(sheet_row, 3, row.in_order_count, &summary_format).unwrap();
            worksheet.write_with_format(sheet_row, 4, row.in_item_count, &summary_format).unwrap();
            worksheet.write_with_format(sheet_row, 5, row.out_amount, &num_format_sum).unwrap();
            worksheet.write_with_format(sheet_row, 6, row.discounted_out_amount, &num_format_sum).unwrap();
            worksheet.write_with_format(sheet_row, 7, row.out_order_count, &summary_format).unwrap();
            worksheet.write_with_format(sheet_row, 8, row.out_item_count, &summary_format).unwrap();
            worksheet.write_with_format(sheet_row, 9, row.gross_profit, &num_format_sum).unwrap();
        } else {
            worksheet.write(sheet_row, 0, day_display).unwrap();
            worksheet.write(sheet_row, 1, &row.warehouse_name).unwrap();
            worksheet.write_with_format(sheet_row, 2, row.in_amount, &num_format).unwrap();
            worksheet.write(sheet_row, 3, row.in_order_count).unwrap();
            worksheet.write(sheet_row, 4, row.in_item_count).unwrap();
            worksheet.write_with_format(sheet_row, 5, row.out_amount, &num_format).unwrap();
            worksheet.write_with_format(sheet_row, 6, row.discounted_out_amount, &num_format).unwrap();
            worksheet.write(sheet_row, 7, row.out_order_count).unwrap();
            worksheet.write(sheet_row, 8, row.out_item_count).unwrap();
            worksheet.write_with_format(sheet_row, 9, row.gross_profit, &num_format).unwrap();
        }
    }

    // 最后附加一个总计行
    let total_row = (rows.len() + 2) as u32;
    worksheet.write_with_format(total_row, 0, "合计", &summary_format).unwrap();
    worksheet.write_with_format(total_row, 2, total_in, &num_format_sum).unwrap();
    worksheet.write_with_format(total_row, 5, total_out, &num_format_sum).unwrap();
    worksheet.write_with_format(total_row, 6, total_discounted_out, &num_format_sum).unwrap();
    worksheet.write_with_format(total_row, 9, total_discounted_out - total_in, &num_format_sum).unwrap();

    let buf = workbook.save_to_buffer().unwrap();
    (
        [
            (header::CONTENT_TYPE, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"stock_summary.xlsx\""),
        ],
        buf,
    ).into_response()
}

pub async fn api_query_stock_summary_reimburse(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or("");
    let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    let (rows, total_in, total_out, total_discounted_out) = compute_stock_summary_reimburse(start_date, end_date).await;

    let items: Vec<serde_json::Value> = rows.iter().map(|r| {
        serde_json::json!({
            "day": r.day,
            "warehouse_id": r.warehouse_id,
            "warehouse_name": r.warehouse_name,
            "is_summary": r.is_summary,
            "in_amount": r.in_amount,
            "in_item_count": r.in_item_count,
            "in_order_count": r.in_order_count,
            "out_amount": r.out_amount,
            "out_item_count": r.out_item_count,
            "out_order_count": r.out_order_count,
            "discounted_out_amount": r.discounted_out_amount,
            "gross_profit": r.gross_profit,
        })
    }).collect();

    let result = serde_json::json!({
        "rows": items,
        "total_in_amount": total_in,
        "total_out_amount": total_out,
        "total_discounted_out_amount": total_discounted_out,
        "total_gross_profit": total_discounted_out - total_in,
    });

    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

pub async fn api_query_stock_summary_reimburse_export(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or("");
    let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    let (rows, total_in, total_out, total_discounted_out) = compute_stock_summary_reimburse(start_date, end_date).await;

    let mut workbook = rust_xlsxwriter::Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("报销出入库统计").unwrap();

    let header_format = rust_xlsxwriter::Format::new()
        .set_bold()
        .set_background_color(rust_xlsxwriter::Color::RGB(0x2E75B6))
        .set_font_color(rust_xlsxwriter::Color::White)
        .set_align(rust_xlsxwriter::FormatAlign::Center)
        .set_border(rust_xlsxwriter::FormatBorder::Thin);
    let summary_format = rust_xlsxwriter::Format::new()
        .set_bold()
        .set_background_color(rust_xlsxwriter::Color::RGB(0xFFF2CC))
        .set_border(rust_xlsxwriter::FormatBorder::Thin);
    let num_format = rust_xlsxwriter::Format::new()
        .set_num_format("#,##0.00")
        .set_align(rust_xlsxwriter::FormatAlign::Right);
    let num_format_sum = rust_xlsxwriter::Format::new()
        .set_bold()
        .set_num_format("#,##0.00")
        .set_background_color(rust_xlsxwriter::Color::RGB(0xFFF2CC))
        .set_align(rust_xlsxwriter::FormatAlign::Right);

    // 列顺序：日期、仓库、入库金额、入库单数、入库条数、出库金额、下浮后出库金额、出库单数、出库条数、毛利
    let headers = ["日期", "仓库", "入库金额", "入库单数", "入库条数", "出库金额", "下浮后出库金额", "出库单数", "出库条数", "毛利"];
    for (col, header) in headers.iter().enumerate() {
        worksheet.write_with_format(0, col as u16, *header, &header_format).unwrap();
    }
    worksheet.set_column_width(0, 14).unwrap();
    worksheet.set_column_width(1, 16).unwrap();
    worksheet.set_column_width_pixels(2, 90).unwrap();
    worksheet.set_column_width_pixels(3, 70).unwrap();
    worksheet.set_column_width_pixels(4, 70).unwrap();
    worksheet.set_column_width_pixels(5, 90).unwrap();
    worksheet.set_column_width_pixels(6, 90).unwrap();
    worksheet.set_column_width_pixels(7, 70).unwrap();
    worksheet.set_column_width_pixels(8, 70).unwrap();
    worksheet.set_column_width_pixels(9, 90).unwrap();

    let mut prev_day = String::new();
    for (row_idx, row) in rows.iter().enumerate() {
        let sheet_row = (row_idx + 1) as u32;
        let day_display = if row.day != prev_day { row.day.as_str() } else { "" };
        prev_day = row.day.clone();
        if row.is_summary {
            worksheet.write_with_format(sheet_row, 0, day_display, &summary_format).unwrap();
            worksheet.write_with_format(sheet_row, 1, &row.warehouse_name, &summary_format).unwrap();
            worksheet.write_with_format(sheet_row, 2, row.in_amount, &num_format_sum).unwrap();
            worksheet.write_with_format(sheet_row, 3, row.in_order_count, &summary_format).unwrap();
            worksheet.write_with_format(sheet_row, 4, row.in_item_count, &summary_format).unwrap();
            worksheet.write_with_format(sheet_row, 5, row.out_amount, &num_format_sum).unwrap();
            worksheet.write_with_format(sheet_row, 6, row.discounted_out_amount, &num_format_sum).unwrap();
            worksheet.write_with_format(sheet_row, 7, row.out_order_count, &summary_format).unwrap();
            worksheet.write_with_format(sheet_row, 8, row.out_item_count, &summary_format).unwrap();
            worksheet.write_with_format(sheet_row, 9, row.gross_profit, &num_format_sum).unwrap();
        } else {
            worksheet.write(sheet_row, 0, day_display).unwrap();
            worksheet.write(sheet_row, 1, &row.warehouse_name).unwrap();
            worksheet.write_with_format(sheet_row, 2, row.in_amount, &num_format).unwrap();
            worksheet.write(sheet_row, 3, row.in_order_count).unwrap();
            worksheet.write(sheet_row, 4, row.in_item_count).unwrap();
            worksheet.write_with_format(sheet_row, 5, row.out_amount, &num_format).unwrap();
            worksheet.write_with_format(sheet_row, 6, row.discounted_out_amount, &num_format).unwrap();
            worksheet.write(sheet_row, 7, row.out_order_count).unwrap();
            worksheet.write(sheet_row, 8, row.out_item_count).unwrap();
            worksheet.write_with_format(sheet_row, 9, row.gross_profit, &num_format).unwrap();
        }
    }

    let total_row = (rows.len() + 2) as u32;
    worksheet.write_with_format(total_row, 0, "合计", &summary_format).unwrap();
    worksheet.write_with_format(total_row, 2, total_in, &num_format_sum).unwrap();
    worksheet.write_with_format(total_row, 5, total_out, &num_format_sum).unwrap();
    worksheet.write_with_format(total_row, 6, total_discounted_out, &num_format_sum).unwrap();
    worksheet.write_with_format(total_row, 9, total_discounted_out - total_in, &num_format_sum).unwrap();

    let buf = workbook.save_to_buffer().unwrap();
    (
        [
            (header::CONTENT_TYPE, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"stock_summary_reimburse.xlsx\""),
        ],
        buf,
    ).into_response()
}

pub async fn api_query_stock_warning() -> impl IntoResponse {
    let low_rows = sqlx::query(
        "SELECT p.name as product_name, p.spec, p.unit, i.quantity as current_stock, i.min_stock
         FROM inventory i JOIN product p ON i.product_id = p.id
         WHERE i.quantity < i.min_stock ORDER BY (i.min_stock - i.quantity) DESC"
    )
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();
    
    let high_rows = sqlx::query(
        "SELECT p.name as product_name, p.spec, p.unit, i.quantity as current_stock, i.max_stock
         FROM inventory i JOIN product p ON i.product_id = p.id
         WHERE i.quantity > i.max_stock ORDER BY (i.quantity - i.max_stock) DESC"
    )
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();
    
    let low_stock: Vec<serde_json::Value> = low_rows.iter().map(|row| {
        serde_json::json!({
            "product_name": row.get::<String, _>("product_name"),
            "spec": row.get::<Option<String>, _>("spec"),
            "unit": row.get::<Option<String>, _>("unit"),
            "current_stock": row.try_get::<f64, _>("current_stock").unwrap_or(0.0),
            "min_stock": row.try_get::<f64, _>("min_stock").unwrap_or(0.0),
        })
    }).collect();
    
    let high_stock: Vec<serde_json::Value> = high_rows.iter().map(|row| {
        serde_json::json!({
            "product_name": row.get::<String, _>("product_name"),
            "spec": row.get::<Option<String>, _>("spec"),
            "unit": row.get::<Option<String>, _>("unit"),
            "current_stock": row.try_get::<f64, _>("current_stock").unwrap_or(0.0),
            "max_stock": row.try_get::<f64, _>("max_stock").unwrap_or(0.0),
        })
    }).collect();
    
    (StatusCode::OK, serde_json::to_string(&serde_json::json!({
        "low_stock": low_stock,
        "high_stock": high_stock,
    })).unwrap())
}

pub async fn api_query_slow_stock(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let days: i64 = params.get("days").and_then(|s| s.parse().ok()).unwrap_or(30);
    
    let rows = sqlx::query(
        "SELECT p.id, p.name as product_name, p.spec, p.unit, i.quantity as current_stock,
                COALESCE(soi.last_sale_date, '无销售记录') as last_sale_date
         FROM inventory i
         JOIN product p ON i.product_id = p.id
         LEFT JOIN (
             SELECT soi.product_id, MAX(so.order_date) as last_sale_date
             FROM sales_order_item soi
             JOIN sales_order so ON soi.order_id = so.id
             GROUP BY soi.product_id
         ) soi ON i.product_id = soi.product_id
         WHERE soi.last_sale_date IS NULL
            OR julianday('now') - julianday(soi.last_sale_date) > ?
         ORDER BY soi.last_sale_date ASC NULLS FIRST"
    )
    .bind(days)
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();
    
    let items: Vec<serde_json::Value> = rows.iter().map(|row| {
        let curr_stock = row.try_get::<f64, _>("current_stock").unwrap_or(0.0);
        serde_json::json!({
            "product_id": row.get::<i64, _>("id"),
            "product_name": row.get::<String, _>("product_name"),
            "spec": row.get::<Option<String>, _>("spec"),
            "unit": row.get::<Option<String>, _>("unit"),
            "current_stock": curr_stock,
            "last_sale_date": row.get::<String, _>("last_sale_date"),
        })
    }).collect();
    
    (StatusCode::OK, serde_json::to_string(&items).unwrap())
}

pub async fn api_query_income_expense(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or("");
    let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    
    let mut date_filter = String::new();
    if !start_date.is_empty() {
        date_filter.push_str(&format!(" AND order_date >= '{}'", start_date));
    }
    if !end_date.is_empty() {
        date_filter.push_str(&format!(" AND order_date <= '{}'", end_date));
    }
    
    let sql = format!(
        "SELECT order_date, '销售订单' as type, CAST(total_amount AS REAL) as total_amount, CAST(final_amount AS REAL) as final_amount, '收入' as direction
         FROM sales_order WHERE status != 'cancelled' {}
         UNION ALL
         SELECT order_date, '采购订单' as type, CAST(total_amount AS REAL) as total_amount, CAST(final_amount AS REAL) as final_amount, '支出' as direction
         FROM purchase_order WHERE status != 'cancelled' {}
         ORDER BY order_date",
        date_filter, date_filter
    );
    
    let rows = sqlx::query(AssertSqlSafe(sql.as_str()))
        .fetch_all(crate::db::pool())
        .await
        .unwrap_or_default();
    
    let items: Vec<serde_json::Value> = rows.iter().map(|row| {
        let total_amt = row.try_get::<f64, _>("total_amount").unwrap_or(0.0);
        let final_amt = row.try_get::<f64, _>("final_amount").unwrap_or(0.0);
        serde_json::json!({
            "order_date": row.get::<String, _>("order_date"),
            "type": row.get::<String, _>("type"),
            "total_amount": total_amt,
            "final_amount": final_amt,
            "direction": row.get::<String, _>("direction"),
        })
    }).collect();
    
    (StatusCode::OK, serde_json::to_string(&items).unwrap())
}

pub async fn api_query_profit_detail(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or("");
    let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    
    let mut date_filter = String::new();
    if !start_date.is_empty() {
        date_filter.push_str(&format!(" AND so.order_date >= '{}'", start_date));
    }
    if !end_date.is_empty() {
        date_filter.push_str(&format!(" AND so.order_date <= '{}'", end_date));
    }
    
    let sql = format!(
        "SELECT so.order_no, so.order_date, soi.product_name, CAST(soi.quantity AS REAL) as quantity, CAST(soi.unit_price AS REAL) as sale_price,
                COALESCE(CAST(p.purchase_price AS REAL), 0) as purchase_price,
                (CAST(soi.unit_price AS REAL) - COALESCE(CAST(p.purchase_price AS REAL), 0)) * CAST(soi.quantity AS REAL) as profit,
                CAST(soi.amount AS REAL) as sale_amount,
                COALESCE(CAST(p.purchase_price AS REAL), 0) * CAST(soi.quantity AS REAL) as cost_amount
         FROM sales_order_item soi
         JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN product p ON soi.product_id = p.id
         WHERE so.status != 'cancelled' {}
         ORDER BY so.order_date, so.order_no",
        date_filter
    );
    
    let rows = sqlx::query(AssertSqlSafe(sql.as_str()))
        .fetch_all(crate::db::pool())
        .await
        .unwrap_or_default();
    
    let items: Vec<serde_json::Value> = rows.iter().map(|row| {
        let qty = row.try_get::<f64, _>("quantity").unwrap_or(0.0);
        let sale_price = row.try_get::<f64, _>("sale_price").unwrap_or(0.0);
        let purchase_price = row.try_get::<f64, _>("purchase_price").unwrap_or(0.0);
        let profit = row.try_get::<f64, _>("profit").unwrap_or(0.0);
        let sale_amount = row.try_get::<f64, _>("sale_amount").unwrap_or(0.0);
        let cost_amount = row.try_get::<f64, _>("cost_amount").unwrap_or(0.0);
        serde_json::json!({
            "order_no": row.get::<String, _>("order_no"),
            "order_date": row.get::<String, _>("order_date"),
            "product_name": row.get::<String, _>("product_name"),
            "quantity": qty,
            "sale_price": sale_price,
            "purchase_price": purchase_price,
            "profit": profit,
            "sale_amount": sale_amount,
            "cost_amount": cost_amount,
        })
    }).collect();
    
    (StatusCode::OK, serde_json::to_string(&items).unwrap())
}

pub async fn api_query_purchase_price_export(headers: axum::http::HeaderMap, axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    // 采购单价为进价信息，仅超级管理员可见：非 super_admin 导出时该列置空
    let is_super_admin = crate::auth::get_user_ctx(&headers).await.role == "super_admin";
    let product_name = params.get("product_name").map(|s| s.as_str()).unwrap_or(""); let supplier_id = params.get("supplier_id").map(|s| s.as_str()).unwrap_or("");
    let mut base_sql = String::from(" FROM purchase_order_item poi JOIN purchase_order po ON poi.order_id = po.id JOIN supplier s ON po.supplier_id = s.id WHERE 1=1");
    let mut binds: Vec<String> = Vec::new();
    if !product_name.is_empty() { base_sql.push_str(" AND poi.product_name LIKE ?"); binds.push(format!("%{}%", product_name)); }
    if !supplier_id.is_empty() { base_sql.push_str(" AND po.supplier_id = ?"); binds.push(supplier_id.to_string()); }
    let data_sql = format!("SELECT poi.product_name, poi.spec, poi.unit, poi.unit_price, poi.quantity, po.order_date, s.name as supplier_name {} ORDER BY po.order_date DESC", base_sql);
    let mut query = sqlx::query(AssertSqlSafe(data_sql.as_str())); for b in &binds { query = query.bind(b); }
    let rows = query.fetch_all(crate::db::pool()).await.unwrap_or_default();
    let mut workbook = Workbook::new(); let ws = workbook.add_worksheet(); ws.set_name("采购价格").unwrap();
    let hf = xlsx_header_format(0x4472C4);
    for (col, h) in ["商品名称", "规格", "供应商", "采购单价", "采购日期", "采购数量"].iter().enumerate() { ws.write_with_format(0, col as u16, if *h == "采购单价" && !is_super_admin { "" } else { *h }, &hf).unwrap(); }
    ws.set_column_width(0, 20).unwrap(); ws.set_column_width(1, 14).unwrap(); ws.set_column_width(2, 16).unwrap(); ws.set_column_width(3, 14).unwrap(); ws.set_column_width(4, 14).unwrap(); ws.set_column_width(5, 14).unwrap();
    for (i, row) in rows.iter().enumerate() { let r = (i + 1) as u32; ws.write(r, 0, row.get::<String, _>("product_name")).unwrap(); ws.write(r, 1, row.get::<Option<String>, _>("spec").unwrap_or_default()).unwrap(); ws.write(r, 2, row.get::<String, _>("supplier_name")).unwrap(); if is_super_admin { ws.write(r, 3, row.get::<f64, _>("unit_price")).unwrap(); } else { ws.write(r, 3, "").unwrap(); } ws.write(r, 4, row.get::<String, _>("order_date")).unwrap(); ws.write(r, 5, row.get::<f64, _>("quantity")).unwrap(); }
    xlsx_response(workbook.save_to_buffer().unwrap(), "采购价格查询.xlsx")
}

pub async fn api_query_purchase_summary_export(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or(""); let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    let mut supplier_sql = String::from("SELECT s.name, SUM(poi.quantity) as quantity, SUM(poi.amount) as amount FROM purchase_order_item poi JOIN purchase_order po ON poi.order_id = po.id JOIN supplier s ON po.supplier_id = s.id WHERE 1=1");
    let mut product_sql = String::from("SELECT poi.product_name, poi.spec, SUM(poi.quantity) as quantity, SUM(poi.amount) as amount FROM purchase_order_item poi JOIN purchase_order po ON poi.order_id = po.id WHERE 1=1");
    let mut binds: Vec<String> = Vec::new(); if !start_date.is_empty() { supplier_sql.push_str(" AND po.order_date >= ?"); product_sql.push_str(" AND po.order_date >= ?"); binds.push(start_date.to_string()); }
    let mut binds2 = binds.clone(); if !end_date.is_empty() { supplier_sql.push_str(" AND po.order_date <= ?"); product_sql.push_str(" AND po.order_date <= ?"); binds.push(end_date.to_string()); binds2.push(end_date.to_string()); }
    supplier_sql.push_str(" GROUP BY s.id ORDER BY amount DESC"); product_sql.push_str(" GROUP BY poi.product_name, poi.spec ORDER BY amount DESC");
    let mut q1 = sqlx::query(AssertSqlSafe(supplier_sql.as_str())); for b in &binds { q1 = q1.bind(b); } let supplier_rows = q1.fetch_all(crate::db::pool()).await.unwrap_or_default();
    let mut q2 = sqlx::query(AssertSqlSafe(product_sql.as_str())); for b in &binds2 { q2 = q2.bind(b); } let product_rows = q2.fetch_all(crate::db::pool()).await.unwrap_or_default();
    let mut workbook = Workbook::new(); let hf = xlsx_header_format(0x4472C4);
    let ws1 = workbook.add_worksheet(); ws1.set_name("按供应商汇总").unwrap();
    for (col, h) in ["供应商", "采购数量", "采购金额", "平均成本"].iter().enumerate() { ws1.write_with_format(0, col as u16, *h, &hf).unwrap(); }
    ws1.set_column_width(0, 18).unwrap(); ws1.set_column_width(1, 14).unwrap(); ws1.set_column_width(2, 14).unwrap(); ws1.set_column_width(3, 14).unwrap();
    for (i, row) in supplier_rows.iter().enumerate() { let r = (i + 1) as u32; let qty: f64 = row.get("quantity"); let amt: f64 = row.get("amount"); ws1.write(r, 0, row.get::<String, _>("name")).unwrap(); ws1.write(r, 1, qty).unwrap(); ws1.write(r, 2, amt).unwrap(); ws1.write(r, 3, if qty > 0.0 { amt / qty } else { 0.0 }).unwrap(); }
    let ws2 = workbook.add_worksheet(); ws2.set_name("按商品汇总").unwrap();
    for (col, h) in ["商品名称", "规格", "采购数量", "采购金额", "平均单价"].iter().enumerate() { ws2.write_with_format(0, col as u16, *h, &hf).unwrap(); }
    ws2.set_column_width(0, 20).unwrap(); ws2.set_column_width(1, 14).unwrap(); ws2.set_column_width(2, 14).unwrap(); ws2.set_column_width(3, 14).unwrap(); ws2.set_column_width(4, 14).unwrap();
    for (i, row) in product_rows.iter().enumerate() { let r = (i + 1) as u32; let qty: f64 = row.get("quantity"); let amt: f64 = row.get("amount"); ws2.write(r, 0, row.get::<String, _>("product_name")).unwrap(); ws2.write(r, 1, row.get::<Option<String>, _>("spec").unwrap_or_default()).unwrap(); ws2.write(r, 2, qty).unwrap(); ws2.write(r, 3, amt).unwrap(); ws2.write(r, 4, if qty > 0.0 { amt / qty } else { 0.0 }).unwrap(); }
    xlsx_response(workbook.save_to_buffer().unwrap(), "采购汇总统计.xlsx")
}

pub async fn api_query_sales_price_export(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let product_name = params.get("product_name").map(|s| s.as_str()).unwrap_or(""); let purchaser_id = params.get("purchaser_id").map(|s| s.as_str()).unwrap_or("");
    let mut base_sql = String::from(" FROM sales_order_item soi JOIN sales_order so ON soi.order_id = so.id JOIN purchaser p ON so.purchaser_id = p.id WHERE 1=1");
    let mut binds: Vec<String> = Vec::new();
    if !product_name.is_empty() { base_sql.push_str(" AND soi.product_name LIKE ?"); binds.push(format!("%{}%", product_name)); }
    if !purchaser_id.is_empty() { base_sql.push_str(" AND so.purchaser_id = ?"); binds.push(purchaser_id.to_string()); }
    let data_sql = format!("SELECT soi.product_name, soi.spec, soi.unit, soi.unit_price, soi.quantity, so.order_date, p.name as purchaser_name {} ORDER BY so.order_date DESC", base_sql);
    let mut query = sqlx::query(AssertSqlSafe(data_sql.as_str())); for b in &binds { query = query.bind(b); } let rows = query.fetch_all(crate::db::pool()).await.unwrap_or_default();
    let mut workbook = Workbook::new(); let ws = workbook.add_worksheet(); ws.set_name("销售价格").unwrap(); let hf = xlsx_header_format(0x70AD47);
    for (col, h) in ["商品名称", "规格", "采购单位", "销售单价", "销售日期", "销售数量"].iter().enumerate() { ws.write_with_format(0, col as u16, *h, &hf).unwrap(); }
    ws.set_column_width(0, 20).unwrap(); ws.set_column_width(1, 14).unwrap(); ws.set_column_width(2, 16).unwrap(); ws.set_column_width(3, 14).unwrap(); ws.set_column_width(4, 14).unwrap(); ws.set_column_width(5, 14).unwrap();
    for (i, row) in rows.iter().enumerate() { let r = (i + 1) as u32; ws.write(r, 0, row.get::<String, _>("product_name")).unwrap(); ws.write(r, 1, row.get::<Option<String>, _>("spec").unwrap_or_default()).unwrap(); ws.write(r, 2, row.get::<String, _>("purchaser_name")).unwrap(); ws.write(r, 3, row.get::<f64, _>("unit_price")).unwrap(); ws.write(r, 4, row.get::<String, _>("order_date")).unwrap(); ws.write(r, 5, row.get::<f64, _>("quantity")).unwrap(); }
    xlsx_response(workbook.save_to_buffer().unwrap(), "销售价格查询.xlsx")
}

pub async fn api_query_sales_summary_export(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or(""); let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    let mut purchaser_sql = String::from("SELECT p.name, SUM(soi.quantity) as quantity, SUM(soi.amount) as sales_amount FROM sales_order_item soi JOIN sales_order so ON soi.order_id = so.id JOIN purchaser p ON so.purchaser_id = p.id WHERE 1=1");
    let mut product_sql = String::from("SELECT soi.product_name, soi.spec, SUM(soi.quantity) as quantity, SUM(soi.amount) as sales_amount FROM sales_order_item soi JOIN sales_order so ON soi.order_id = so.id WHERE 1=1");
    let mut binds: Vec<String> = Vec::new(); if !start_date.is_empty() { purchaser_sql.push_str(" AND so.order_date >= ?"); product_sql.push_str(" AND so.order_date >= ?"); binds.push(start_date.to_string()); }
    let mut binds2 = binds.clone(); if !end_date.is_empty() { purchaser_sql.push_str(" AND so.order_date <= ?"); product_sql.push_str(" AND so.order_date <= ?"); binds.push(end_date.to_string()); binds2.push(end_date.to_string()); }
    purchaser_sql.push_str(" GROUP BY p.id ORDER BY sales_amount DESC"); product_sql.push_str(" GROUP BY soi.product_name, soi.spec ORDER BY sales_amount DESC");
    let mut q1 = sqlx::query(AssertSqlSafe(purchaser_sql.as_str())); for b in &binds { q1 = q1.bind(b); } let purchaser_rows = q1.fetch_all(crate::db::pool()).await.unwrap_or_default();
    let mut q2 = sqlx::query(AssertSqlSafe(product_sql.as_str())); for b in &binds2 { q2 = q2.bind(b); } let product_rows = q2.fetch_all(crate::db::pool()).await.unwrap_or_default();
    let mut workbook = Workbook::new(); let hf = xlsx_header_format(0x70AD47);
    let ws1 = workbook.add_worksheet(); ws1.set_name("按采购单位汇总").unwrap();
    for (col, h) in ["采购单位", "销售数量", "销售金额", "成本", "毛利", "毛利率"].iter().enumerate() { ws1.write_with_format(0, col as u16, *h, &hf).unwrap(); }
    ws1.set_column_width(0, 18).unwrap(); ws1.set_column_width(1, 14).unwrap(); ws1.set_column_width(2, 14).unwrap(); ws1.set_column_width(3, 14).unwrap(); ws1.set_column_width(4, 14).unwrap(); ws1.set_column_width(5, 14).unwrap();
    for (i, row) in purchaser_rows.iter().enumerate() { let r = (i + 1) as u32; let qty: f64 = row.get("quantity"); let sales: f64 = row.get("sales_amount"); let cost = 0.0; let profit = sales - cost; let margin = if sales > 0.0 { profit / sales * 100.0 } else { 0.0 }; ws1.write(r, 0, row.get::<String, _>("name")).unwrap(); ws1.write(r, 1, qty).unwrap(); ws1.write(r, 2, sales).unwrap(); ws1.write(r, 3, cost).unwrap(); ws1.write(r, 4, profit).unwrap(); ws1.write(r, 5, margin).unwrap(); }
    let ws2 = workbook.add_worksheet(); ws2.set_name("按商品汇总").unwrap();
    for (col, h) in ["商品名称", "规格", "销售数量", "销售金额", "成本", "毛利"].iter().enumerate() { ws2.write_with_format(0, col as u16, *h, &hf).unwrap(); }
    ws2.set_column_width(0, 20).unwrap(); ws2.set_column_width(1, 14).unwrap(); ws2.set_column_width(2, 14).unwrap(); ws2.set_column_width(3, 14).unwrap(); ws2.set_column_width(4, 14).unwrap(); ws2.set_column_width(5, 14).unwrap();
    for (i, row) in product_rows.iter().enumerate() { let r = (i + 1) as u32; let qty: f64 = row.get("quantity"); let sales: f64 = row.get("sales_amount"); let cost = 0.0; let profit = sales - cost; ws2.write(r, 0, row.get::<String, _>("product_name")).unwrap(); ws2.write(r, 1, row.get::<Option<String>, _>("spec").unwrap_or_default()).unwrap(); ws2.write(r, 2, qty).unwrap(); ws2.write(r, 3, sales).unwrap(); ws2.write(r, 4, cost).unwrap(); ws2.write(r, 5, profit).unwrap(); }
    xlsx_response(workbook.save_to_buffer().unwrap(), "销售汇总报表.xlsx")
}

pub async fn api_query_product_rank_export(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or(""); let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    let mut top_sql = String::from("SELECT soi.product_name, soi.spec, SUM(soi.quantity) as quantity, SUM(soi.amount) as amount FROM sales_order_item soi JOIN sales_order so ON soi.order_id = so.id WHERE 1=1");
    let mut binds: Vec<String> = Vec::new(); if !start_date.is_empty() { top_sql.push_str(" AND so.order_date >= ?"); binds.push(start_date.to_string()); } if !end_date.is_empty() { top_sql.push_str(" AND so.order_date <= ?"); binds.push(end_date.to_string()); }
    top_sql.push_str(" GROUP BY soi.product_name, soi.spec ORDER BY quantity DESC LIMIT 10");
    let mut query = sqlx::query(AssertSqlSafe(top_sql.as_str())); for b in &binds { query = query.bind(b); } let rows = query.fetch_all(crate::db::pool()).await.unwrap_or_default();
    let mut workbook = Workbook::new(); let ws = workbook.add_worksheet(); ws.set_name("畅销商品TOP10").unwrap(); let hf = xlsx_header_format(0x70AD47);
    for (col, h) in ["排名", "商品名称", "规格", "销售数量", "销售金额"].iter().enumerate() { ws.write_with_format(0, col as u16, *h, &hf).unwrap(); }
    ws.set_column_width(0, 8).unwrap(); ws.set_column_width(1, 20).unwrap(); ws.set_column_width(2, 14).unwrap(); ws.set_column_width(3, 14).unwrap(); ws.set_column_width(4, 14).unwrap();
    for (i, row) in rows.iter().enumerate() { let r = (i + 1) as u32; ws.write(r, 0, (i + 1) as i64).unwrap(); ws.write(r, 1, row.get::<String, _>("product_name")).unwrap(); ws.write(r, 2, row.get::<Option<String>, _>("spec").unwrap_or_default()).unwrap(); ws.write(r, 3, row.get::<f64, _>("quantity")).unwrap(); ws.write(r, 4, row.get::<f64, _>("amount")).unwrap(); }
    xlsx_response(workbook.save_to_buffer().unwrap(), "畅销商品排名.xlsx")
}

pub async fn api_query_reimburse_summary_export(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or(""); let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    let source_ids: Vec<i64> = sqlx::query_scalar::<_, i64>("SELECT DISTINCT source_order_id FROM consumable_allocation").fetch_all(crate::db::pool()).await.unwrap_or_default();
    let source_set: std::collections::HashSet<i64> = source_ids.into_iter().collect();
    let mut date_cond = String::from(""); let mut binds: Vec<String> = Vec::new();
    if !start_date.is_empty() { date_cond.push_str(" AND so.order_date >= ?"); binds.push(start_date.to_string()); } if !end_date.is_empty() { date_cond.push_str(" AND so.order_date <= ?"); binds.push(end_date.to_string()); }
    let real_sql = format!("SELECT so.purchaser_id, p.name, SUM(soi.amount) as amount FROM sales_order_item soi JOIN sales_order so ON soi.order_id = so.id JOIN purchaser p ON so.purchaser_id = p.id WHERE 1=1 {} GROUP BY so.purchaser_id", date_cond);
    let mut q = sqlx::query(AssertSqlSafe(real_sql.as_str())); for b in &binds { q = q.bind(b); } let real_rows = q.fetch_all(crate::db::pool()).await.unwrap_or_default();
    use std::collections::HashMap; let mut pmap: HashMap<i64, (String, f64, f64)> = HashMap::new();
    for r in &real_rows { let pid = r.get::<i64,_>("purchaser_id"); let name = r.get::<String,_>("name"); let a = r.get::<f64,_>("amount"); pmap.entry(pid).or_insert((name,0.0,0.0)).1 += a; }
    let supp_sql = format!("SELECT so.purchaser_id, p.name, SUM(osi.amount) as supp_amount FROM order_supplement_item osi JOIN sales_order so ON osi.target_order_id = so.id JOIN purchaser p ON so.purchaser_id = p.id WHERE 1=1 {} GROUP BY so.purchaser_id", date_cond);
    let mut q2 = sqlx::query(AssertSqlSafe(supp_sql.as_str())); for b in &binds { q2 = q2.bind(b); }
    for r in q2.fetch_all(crate::db::pool()).await.unwrap_or_default() { let pid = r.get::<i64,_>("purchaser_id"); let name = r.get::<String,_>("name"); let sa = r.get::<f64,_>("supp_amount"); pmap.entry(pid).or_insert((name,0.0,0.0)).2 += sa; }
    if !source_set.is_empty() {
        let src_sql = format!("SELECT so.purchaser_id, p.name, so.id as oid, SUM(soi.amount) as amount FROM sales_order_item soi JOIN sales_order so ON soi.order_id = so.id JOIN purchaser p ON so.purchaser_id = p.id WHERE 1=1 {} GROUP BY so.id", date_cond);
        let mut q3 = sqlx::query(AssertSqlSafe(src_sql.as_str())); for b in &binds { q3 = q3.bind(b); }
        for r in q3.fetch_all(crate::db::pool()).await.unwrap_or_default() { let oid = r.get::<i64,_>("oid"); if !source_set.contains(&oid) { continue; } let pid = r.get::<i64,_>("purchaser_id"); let name = r.get::<String,_>("name"); let a = r.get::<f64,_>("amount"); pmap.entry(pid).or_insert((name,0.0,0.0)).2 -= a; }
    }
    let mut by_purchaser: Vec<serde_json::Value> = pmap.values().map(|(n,r,s)| serde_json::json!({"name":n,"real_amount":r,"supplement_amount":s,"reimburse_amount":r+s})).collect();
    by_purchaser.sort_by(|a,b| b["reimburse_amount"].as_f64().unwrap_or(0.0).partial_cmp(&a["reimburse_amount"].as_f64().unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal));
    let mut prod_map: HashMap<(String, String), (f64, f64)> = HashMap::new();
    let pr = format!("SELECT soi.product_name, COALESCE(soi.spec,'') as spec, soi.order_id, soi.quantity, soi.amount FROM sales_order_item soi JOIN sales_order so ON soi.order_id = so.id WHERE 1=1 {}", date_cond);
    let mut q4 = sqlx::query(AssertSqlSafe(pr.as_str())); for b in &binds { q4 = q4.bind(b); }
    for r in q4.fetch_all(crate::db::pool()).await.unwrap_or_default() { let oid = r.get::<i64,_>("order_id"); if source_set.contains(&oid) { continue; } let nm = r.get::<String,_>("product_name"); let sp = r.get::<String,_>("spec"); let qty = r.get::<f64,_>("quantity"); let amt = r.get::<f64,_>("amount"); let e = prod_map.entry((nm,sp)).or_insert((0.0,0.0)); e.0 += qty; e.1 += amt; }
    let ps = format!("SELECT osi.product_name, COALESCE(osi.spec,'') as spec, osi.quantity, osi.amount FROM order_supplement_item osi JOIN sales_order so ON osi.target_order_id = so.id WHERE 1=1 {}", date_cond);
    let mut q5 = sqlx::query(AssertSqlSafe(ps.as_str())); for b in &binds { q5 = q5.bind(b); }
    for r in q5.fetch_all(crate::db::pool()).await.unwrap_or_default() { let nm = r.get::<String,_>("product_name"); let sp = r.get::<String,_>("spec"); let qty = r.get::<f64,_>("quantity"); let amt = r.get::<f64,_>("amount"); let e = prod_map.entry((nm,sp)).or_insert((0.0,0.0)); e.0 += qty; e.1 += amt; }
    let mut by_product: Vec<serde_json::Value> = prod_map.iter().map(|((n,s),(q,a))| serde_json::json!({"product_name":n,"spec":s,"quantity":q,"reimburse_amount":a})).collect();
    by_product.sort_by(|a,b| b["reimburse_amount"].as_f64().unwrap_or(0.0).partial_cmp(&a["reimburse_amount"].as_f64().unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal));
    let mut workbook = Workbook::new(); let hf = xlsx_header_format(0x70AD47);
    let ws1 = workbook.add_worksheet(); ws1.set_name("按采购单位汇总").unwrap();
    for (c,h) in ["采购单位","真实金额","分摊增项净额","报销金额"].iter().enumerate() { ws1.write_with_format(0,c as u16,*h,&hf).unwrap(); }
    ws1.set_column_width(0,18).unwrap(); ws1.set_column_width(1,14).unwrap(); ws1.set_column_width(2,16).unwrap(); ws1.set_column_width(3,14).unwrap();
    for (i,item) in by_purchaser.iter().enumerate() { let r=(i+1)as u32; ws1.write(r,0,item.get("name").and_then(|v|v.as_str()).unwrap_or("")).unwrap(); ws1.write(r,1,item.get("real_amount").and_then(|v|v.as_f64()).unwrap_or(0.0)).unwrap(); ws1.write(r,2,item.get("supplement_amount").and_then(|v|v.as_f64()).unwrap_or(0.0)).unwrap(); ws1.write(r,3,item.get("reimburse_amount").and_then(|v|v.as_f64()).unwrap_or(0.0)).unwrap(); }
    let ws2 = workbook.add_worksheet(); ws2.set_name("按商品汇总").unwrap();
    for (c,h) in ["商品名称","规格","数量","报销金额"].iter().enumerate() { ws2.write_with_format(0,c as u16,*h,&hf).unwrap(); }
    ws2.set_column_width(0,20).unwrap(); ws2.set_column_width(1,14).unwrap(); ws2.set_column_width(2,14).unwrap(); ws2.set_column_width(3,14).unwrap();
    for (i,item) in by_product.iter().enumerate() { let r=(i+1)as u32; ws2.write(r,0,item.get("product_name").and_then(|v|v.as_str()).unwrap_or("")).unwrap(); ws2.write(r,1,item.get("spec").and_then(|v|v.as_str()).unwrap_or_default()).unwrap(); ws2.write(r,2,item.get("quantity").and_then(|v|v.as_f64()).unwrap_or(0.0)).unwrap(); ws2.write(r,3,item.get("reimburse_amount").and_then(|v|v.as_f64()).unwrap_or(0.0)).unwrap(); }
    xlsx_response(workbook.save_to_buffer().unwrap(), "报销口径汇总.xlsx")
}

pub async fn api_query_allocation_source_export(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or(""); let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    let mut sql = String::from("SELECT ca.source_order_id, ca.total_amount, ca.allocated_amount, ca.remaining_balance, ca.status, so.order_no, so.order_date FROM consumable_allocation ca JOIN sales_order so ON ca.source_order_id = so.id WHERE 1=1");
    let mut binds: Vec<String> = Vec::new(); if !start_date.is_empty() { sql.push_str(" AND so.order_date >= ?"); binds.push(start_date.to_string()); } if !end_date.is_empty() { sql.push_str(" AND so.order_date <= ?"); binds.push(end_date.to_string()); }
    sql.push_str(" ORDER BY so.order_date DESC"); let mut q = sqlx::query(AssertSqlSafe(sql.as_str())); for b in &binds { q = q.bind(b); } let rows = q.fetch_all(crate::db::pool()).await.unwrap_or_default();
    let mut workbook = Workbook::new(); let ws = workbook.add_worksheet(); ws.set_name("分摊来源").unwrap(); let hf = xlsx_header_format(0x70AD47);
    for (c,h) in ["来源订单","日期","来源金额","已分摊","剩余","状态","分摊去向"].iter().enumerate() { ws.write_with_format(0,c as u16,*h,&hf).unwrap(); }
    ws.set_column_width(0,18).unwrap(); ws.set_column_width(1,14).unwrap(); ws.set_column_width(2,14).unwrap(); ws.set_column_width(3,14).unwrap(); ws.set_column_width(4,14).unwrap(); ws.set_column_width(5,10).unwrap(); ws.set_column_width(6,30).unwrap();
    let sm = ["未分摊","分摊中","已完成","已终止"];
    for (i,row) in rows.iter().enumerate() { let r=(i+1)as u32; let src_id=row.get::<i64,_>("source_order_id"); let st:i64=row.get("status");
        let tgts=sqlx::query("SELECT so.order_no, SUM(osi.amount) as amount FROM order_supplement_item osi JOIN sales_order so ON osi.target_order_id=so.id WHERE osi.source_order_id=? GROUP BY osi.target_order_id HAVING SUM(osi.amount)<>0 ORDER BY amount DESC").bind(src_id).fetch_all(crate::db::pool()).await.unwrap_or_default();
        let tstr:String=tgts.iter().map(|t| format!("{}(¥{:.2})",t.get::<String,_>("order_no"),t.get::<f64,_>("amount"))).collect::<Vec<_>>().join("、");
        let ss=if(st as usize)<sm.len(){sm[st as usize]}else{"未知"}; ws.write(r,0,row.get::<String,_>("order_no")).unwrap(); ws.write(r,1,row.get::<String,_>("order_date")).unwrap(); ws.write(r,2,row.get::<f64,_>("total_amount")).unwrap(); ws.write(r,3,row.get::<f64,_>("allocated_amount")).unwrap(); ws.write(r,4,row.get::<f64,_>("remaining_balance")).unwrap(); ws.write(r,5,ss).unwrap(); ws.write(r,6,&tstr).unwrap(); }
    xlsx_response(workbook.save_to_buffer().unwrap(), "分摊来源统计.xlsx")
}

pub async fn api_query_overview_export(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let month = params.get("month").map(|s| s.as_str()).unwrap_or("");
    let purchase_total: f64 = sqlx::query_scalar("SELECT COALESCE(SUM(po.total_amount),0) FROM purchase_order po WHERE strftime('%Y-%m',po.order_date)=?").bind(month).fetch_one(crate::db::pool()).await.unwrap_or(0.0);
    let sales_total: f64 = sqlx::query_scalar("SELECT COALESCE(SUM(so.total_amount),0) FROM sales_order so WHERE strftime('%Y-%m',so.order_date)=?").bind(month).fetch_one(crate::db::pool()).await.unwrap_or(0.0);
    let stock_total: f64 = sqlx::query_scalar("SELECT COALESCE(SUM(i.quantity*pr.selling_price),0) FROM inventory i JOIN product pr ON i.product_id=pr.id").fetch_one(crate::db::pool()).await.unwrap_or(0.0);
    let profit_total = sales_total - purchase_total;
    let pur_rows = sqlx::query("SELECT s.name,COALESCE(SUM(poi.amount),0) as amount,COALESCE(SUM(poi.quantity),0) as quantity FROM purchase_order_item poi JOIN purchase_order po ON poi.order_id=po.id JOIN supplier s ON po.supplier_id=s.id WHERE strftime('%Y-%m',po.order_date)=? GROUP BY s.id,s.name ORDER BY amount DESC").bind(month).fetch_all(crate::db::pool()).await.unwrap_or_default();
    let sal_rows = sqlx::query("SELECT p.name,COALESCE(SUM(soi.amount),0) as amount,COALESCE(SUM(soi.quantity),0) as quantity FROM sales_order_item soi JOIN sales_order so ON soi.order_id=so.id JOIN purchaser p ON so.purchaser_id=p.id WHERE strftime('%Y-%m',so.order_date)=? GROUP BY p.id,p.name ORDER BY amount DESC").bind(month).fetch_all(crate::db::pool()).await.unwrap_or_default();
    let mut workbook=Workbook::new(); let hf=xlsx_header_format(0x2E75B6);
    let ws1=workbook.add_worksheet(); ws1.set_name("汇总").unwrap();
    for (c,h) in ["指标","金额"].iter().enumerate(){ws1.write_with_format(0,c as u16,*h,&hf).unwrap();} ws1.set_column_width(0,16).unwrap(); ws1.set_column_width(1,16).unwrap();
    ws1.write(1,0,"总进货").unwrap(); ws1.write(1,1,purchase_total).unwrap(); ws1.write(2,0,"总销售").unwrap(); ws1.write(2,1,sales_total).unwrap(); ws1.write(3,0,"库存").unwrap(); ws1.write(3,1,stock_total).unwrap(); ws1.write(4,0,"毛利").unwrap(); ws1.write(4,1,profit_total).unwrap();
    let ws2=workbook.add_worksheet(); ws2.set_name("采购汇总").unwrap();
    for (c,h) in ["供应商","采购金额","数量"].iter().enumerate(){ws2.write_with_format(0,c as u16,*h,&hf).unwrap();} ws2.set_column_width(0,18).unwrap(); ws2.set_column_width(1,14).unwrap(); ws2.set_column_width(2,14).unwrap();
    for (i,row) in pur_rows.iter().enumerate(){let r=(i+1)as u32;ws2.write(r,0,row.get::<String,_>("name")).unwrap();ws2.write(r,1,row.get::<f64,_>("amount")).unwrap();ws2.write(r,2,row.get::<f64,_>("quantity")).unwrap();}
    let ws3=workbook.add_worksheet(); ws3.set_name("销售汇总").unwrap();
    for (c,h) in ["采购单位","销售金额","数量"].iter().enumerate(){ws3.write_with_format(0,c as u16,*h,&hf).unwrap();} ws3.set_column_width(0,18).unwrap(); ws3.set_column_width(1,14).unwrap(); ws3.set_column_width(2,14).unwrap();
    for (i,row) in sal_rows.iter().enumerate(){let r=(i+1)as u32;ws3.write(r,0,row.get::<String,_>("name")).unwrap();ws3.write(r,1,row.get::<f64,_>("amount")).unwrap();ws3.write(r,2,row.get::<f64,_>("quantity")).unwrap();}
    xlsx_response(workbook.save_to_buffer().unwrap(), "进销存汇总报表.xlsx")
}

pub async fn api_query_stock_balance_export(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let product_name=params.get("product_name").map(|s|s.as_str()).unwrap_or("");let category_id=params.get("category_id").map(|s|s.as_str()).unwrap_or("");
    let sql=if category_id.is_empty(){"SELECT i.id,i.product_id,i.quantity,i.min_stock,i.max_stock,p.name as product_name,p.spec,p.unit,p.base_price,(i.quantity*p.base_price) as amount FROM inventory i JOIN product p ON i.product_id=p.id WHERE p.name LIKE ? ORDER BY p.name".to_string()}
    else{format!("SELECT i.id,i.product_id,i.quantity,i.min_stock,i.max_stock,p.name as product_name,p.spec,p.unit,p.base_price,(i.quantity*p.base_price) as amount FROM inventory i JOIN product p ON i.product_id=p.id WHERE p.name LIKE ? AND p.category_id={} ORDER BY p.name",category_id)};
    let pattern=format!("%{}%",product_name);
    let rows=if category_id.is_empty(){sqlx::query(AssertSqlSafe(sql.as_str())).bind(&pattern).fetch_all(crate::db::pool()).await.unwrap_or_default()}else{let cid:i64=category_id.parse().unwrap_or(0);sqlx::query(AssertSqlSafe(sql.as_str())).bind(&pattern).bind(cid).fetch_all(crate::db::pool()).await.unwrap_or_default()};
    let mut workbook=Workbook::new();let ws=workbook.add_worksheet();ws.set_name("库存余额").unwrap();let hf=xlsx_header_format(0x2E75B6);
    for(c,h)in["商品名称","规格","单位","库存数量","库存金额","最低库存","最高库存"].iter().enumerate(){ws.write_with_format(0,c as u16,*h,&hf).unwrap();}
    ws.set_column_width(0,20).unwrap();ws.set_column_width(1,14).unwrap();ws.set_column_width(2,10).unwrap();ws.set_column_width(3,14).unwrap();ws.set_column_width(4,14).unwrap();ws.set_column_width(5,14).unwrap();ws.set_column_width(6,14).unwrap();
    for(i,row)in rows.iter().enumerate(){let r=(i+1)as u32;ws.write(r,0,row.get::<String,_>("product_name")).unwrap();ws.write(r,1,row.get::<Option<String>,_>("spec").unwrap_or_default()).unwrap();ws.write(r,2,row.get::<Option<String>,_>("unit").unwrap_or_default()).unwrap();ws.write(r,3,row.try_get::<f64,_>("quantity").unwrap_or(0.0)).unwrap();ws.write(r,4,row.try_get::<f64,_>("amount").unwrap_or(0.0)).unwrap();ws.write(r,5,row.try_get::<f64,_>("min_stock").unwrap_or(0.0)).unwrap();ws.write(r,6,row.try_get::<f64,_>("max_stock").unwrap_or(0.0)).unwrap();}
    xlsx_response(workbook.save_to_buffer().unwrap(), "库存余额查询.xlsx")
}

pub async fn api_query_stock_flow_export(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let product_name=params.get("product_name").map(|s|s.as_str()).unwrap_or("");let start_date=params.get("start_date").map(|s|s.as_str()).unwrap_or("");let end_date=params.get("end_date").map(|s|s.as_str()).unwrap_or("");let product_id=params.get("product_id").and_then(|s|s.parse::<i64>().ok());
    let pattern=format!("%{}%",product_name);let mut wc=String::from("WHERE 1=1");let mut swc=String::from("WHERE 1=1");
    if let Some(pid)=product_id{wc.push_str(&format!(" AND p.id={}",pid));swc.push_str(&format!(" AND p.id={}",pid));}else if!product_name.is_empty(){wc.push_str(" AND p.name LIKE ?");swc.push_str(" AND p.name LIKE ?");}
    if!start_date.is_empty(){wc.push_str(&format!(" AND po.order_date>='{}'",start_date));swc.push_str(&format!(" AND so.order_date>='{}'",start_date));}if!end_date.is_empty(){wc.push_str(&format!(" AND po.order_date<='{}'",end_date));swc.push_str(&format!(" AND so.order_date<='{}'",end_date));}
    let sql=format!("SELECT po.order_date as create_time,'采购入库' as type,p.name as product_name,p.spec,poi.quantity as in_quantity,0 as out_quantity,poi.remark FROM purchase_order_item poi JOIN purchase_order po ON poi.order_id=po.id JOIN product p ON poi.product_id=p.id {} UNION ALL SELECT so.order_date as create_time,'销售出库' as type,p.name as product_name,p.spec,0 as in_quantity,soi.quantity as out_quantity,soi.remark FROM sales_order_item soi JOIN sales_order so ON soi.order_id=so.id JOIN product p ON soi.product_id=p.id {} ORDER BY create_time",wc,swc);
    let rows=if product_id.is_some()||product_name.is_empty(){sqlx::query(AssertSqlSafe(sql.as_str())).fetch_all(crate::db::pool()).await.unwrap_or_default()}else{sqlx::query(AssertSqlSafe(sql.as_str())).bind(&pattern).bind(&pattern).fetch_all(crate::db::pool()).await.unwrap_or_default()};
    let mut workbook=Workbook::new();let ws=workbook.add_worksheet();ws.set_name("库存流水").unwrap();let hf=xlsx_header_format(0x2E75B6);
    for(c,h)in["日期","类型","商品名称","规格","入库数量","出库数量","备注"].iter().enumerate(){ws.write_with_format(0,c as u16,*h,&hf).unwrap();}
    ws.set_column_width(0,14).unwrap();ws.set_column_width(1,12).unwrap();ws.set_column_width(2,20).unwrap();ws.set_column_width(3,14).unwrap();ws.set_column_width(4,14).unwrap();ws.set_column_width(5,14).unwrap();ws.set_column_width(6,20).unwrap();
    for(i,row)in rows.iter().enumerate(){let r=(i+1)as u32;ws.write(r,0,row.get::<String,_>("create_time")).unwrap();ws.write(r,1,row.get::<String,_>("type")).unwrap();ws.write(r,2,row.get::<String,_>("product_name")).unwrap();ws.write(r,3,row.get::<Option<String>,_>("spec").unwrap_or_default()).unwrap();ws.write(r,4,row.try_get::<f64,_>("in_quantity").unwrap_or(0.0)).unwrap();ws.write(r,5,row.try_get::<f64,_>("out_quantity").unwrap_or(0.0)).unwrap();ws.write(r,6,row.get::<Option<String>,_>("remark").unwrap_or_default()).unwrap();}
    xlsx_response(workbook.save_to_buffer().unwrap(), "库存流水查询.xlsx")
}

pub async fn api_query_stock_warning_export() -> impl IntoResponse {
    let low_rows=sqlx::query("SELECT p.name as product_name,p.spec,p.unit,i.quantity as current_stock,i.min_stock FROM inventory i JOIN product p ON i.product_id=p.id WHERE i.quantity<i.min_stock ORDER BY (i.min_stock-i.quantity) DESC").fetch_all(crate::db::pool()).await.unwrap_or_default();
    let high_rows=sqlx::query("SELECT p.name as product_name,p.spec,p.unit,i.quantity as current_stock,i.max_stock FROM inventory i JOIN product p ON i.product_id=p.id WHERE i.quantity>i.max_stock ORDER BY (i.quantity-i.max_stock) DESC").fetch_all(crate::db::pool()).await.unwrap_or_default();
    let mut workbook=Workbook::new();let hf=xlsx_header_format(0x2E75B6);
    let ws1=workbook.add_worksheet();ws1.set_name("低于最低库存").unwrap();
    for(c,h)in["商品名称","规格","单位","当前库存","最低库存","缺货数量"].iter().enumerate(){ws1.write_with_format(0,c as u16,*h,&hf).unwrap();}
    ws1.set_column_width(0,20).unwrap();ws1.set_column_width(1,14).unwrap();ws1.set_column_width(2,10).unwrap();ws1.set_column_width(3,14).unwrap();ws1.set_column_width(4,14).unwrap();ws1.set_column_width(5,14).unwrap();
    for(i,row)in low_rows.iter().enumerate(){let r=(i+1)as u32;let cur=row.try_get::<f64,_>("current_stock").unwrap_or(0.0);let min=row.try_get::<f64,_>("min_stock").unwrap_or(0.0);ws1.write(r,0,row.get::<String,_>("product_name")).unwrap();ws1.write(r,1,row.get::<Option<String>,_>("spec").unwrap_or_default()).unwrap();ws1.write(r,2,row.get::<Option<String>,_>("unit").unwrap_or_default()).unwrap();ws1.write(r,3,cur).unwrap();ws1.write(r,4,min).unwrap();ws1.write(r,5,(min-cur).max(0.0)).unwrap();}
    let ws2=workbook.add_worksheet();ws2.set_name("高于最高库存").unwrap();
    for(c,h)in["商品名称","规格","单位","当前库存","最高库存","积压数量"].iter().enumerate(){ws2.write_with_format(0,c as u16,*h,&hf).unwrap();}
    ws2.set_column_width(0,20).unwrap();ws2.set_column_width(1,14).unwrap();ws2.set_column_width(2,10).unwrap();ws2.set_column_width(3,14).unwrap();ws2.set_column_width(4,14).unwrap();ws2.set_column_width(5,14).unwrap();
    for(i,row)in high_rows.iter().enumerate(){let r=(i+1)as u32;let cur=row.try_get::<f64,_>("current_stock").unwrap_or(0.0);let max=row.try_get::<f64,_>("max_stock").unwrap_or(0.0);ws2.write(r,0,row.get::<String,_>("product_name")).unwrap();ws2.write(r,1,row.get::<Option<String>,_>("spec").unwrap_or_default()).unwrap();ws2.write(r,2,row.get::<Option<String>,_>("unit").unwrap_or_default()).unwrap();ws2.write(r,3,cur).unwrap();ws2.write(r,4,max).unwrap();ws2.write(r,5,(cur-max).max(0.0)).unwrap();}
    xlsx_response(workbook.save_to_buffer().unwrap(), "库存预警.xlsx")
}

pub async fn api_query_slow_stock_export(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let days:i64=params.get("days").and_then(|s|s.parse().ok()).unwrap_or(30);
    let rows=sqlx::query("SELECT p.id,p.name as product_name,p.spec,p.unit,i.quantity as current_stock,i.quantity*p.base_price as amount,COALESCE(soi.last_sale_date,'无') as last_sale_date FROM inventory i JOIN product p ON i.product_id=p.id LEFT JOIN (SELECT soi.product_id,MAX(so.order_date) as last_sale_date FROM sales_order_item soi JOIN sales_order so ON soi.order_id=so.id GROUP BY soi.product_id) soi ON i.product_id=soi.product_id WHERE soi.last_sale_date IS NULL OR julianday('now')-julianday(soi.last_sale_date)>? ORDER BY soi.last_sale_date ASC NULLS FIRST").bind(days).fetch_all(crate::db::pool()).await.unwrap_or_default();
    let mut workbook=Workbook::new();let ws=workbook.add_worksheet();ws.set_name("呆滞库存").unwrap();let hf=xlsx_header_format(0x2E75B6);
    for(c,h)in["商品名称","规格","单位","当前库存","库存金额","最后出库日期","呆滞天数"].iter().enumerate(){ws.write_with_format(0,c as u16,*h,&hf).unwrap();}
    ws.set_column_width(0,20).unwrap();ws.set_column_width(1,14).unwrap();ws.set_column_width(2,10).unwrap();ws.set_column_width(3,14).unwrap();ws.set_column_width(4,14).unwrap();ws.set_column_width(5,16).unwrap();ws.set_column_width(6,12).unwrap();
    for(i,row)in rows.iter().enumerate(){let r=(i+1)as u32;let cur=row.try_get::<f64,_>("current_stock").unwrap_or(0.0);let amt=row.try_get::<f64,_>("amount").unwrap_or(0.0);ws.write(r,0,row.get::<String,_>("product_name")).unwrap();ws.write(r,1,row.get::<Option<String>,_>("spec").unwrap_or_default()).unwrap();ws.write(r,2,row.get::<Option<String>,_>("unit").unwrap_or_default()).unwrap();ws.write(r,3,cur).unwrap();ws.write(r,4,amt).unwrap();ws.write(r,5,row.get::<String,_>("last_sale_date")).unwrap();ws.write(r,6,days).unwrap();}
    xlsx_response(workbook.save_to_buffer().unwrap(), "呆滞库存查询.xlsx")
}

pub async fn api_query_income_expense_export(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let start_date=params.get("start_date").map(|s|s.as_str()).unwrap_or("");let end_date=params.get("end_date").map(|s|s.as_str()).unwrap_or("");
    let mut df=String::new();if!start_date.is_empty(){df.push_str(&format!(" AND order_date>='{}'",start_date));}if!end_date.is_empty(){df.push_str(&format!(" AND order_date<='{}'",end_date));}
    let sql=format!("SELECT order_date,'销售订单' as type,CAST(total_amount AS REAL) as total_amount,CAST(final_amount AS REAL) as final_amount,'收入' as direction FROM sales_order WHERE status!='cancelled'{} UNION ALL SELECT order_date,'采购订单' as type,CAST(total_amount AS REAL) as total_amount,CAST(final_amount AS REAL) as final_amount,'支出' as direction FROM purchase_order WHERE status!='cancelled'{} ORDER BY order_date",df,df);
    let rows=sqlx::query(AssertSqlSafe(sql.as_str())).fetch_all(crate::db::pool()).await.unwrap_or_default();
    let mut workbook=Workbook::new();let ws=workbook.add_worksheet();ws.set_name("收支流水").unwrap();let hf=xlsx_header_format(0x2E75B6);
    for(c,h)in["日期","类型","方向","订单金额","实付金额"].iter().enumerate(){ws.write_with_format(0,c as u16,*h,&hf).unwrap();}
    ws.set_column_width(0,14).unwrap();ws.set_column_width(1,12).unwrap();ws.set_column_width(2,10).unwrap();ws.set_column_width(3,14).unwrap();ws.set_column_width(4,14).unwrap();
    for(i,row)in rows.iter().enumerate(){let r=(i+1)as u32;ws.write(r,0,row.get::<String,_>("order_date")).unwrap();ws.write(r,1,row.get::<String,_>("type")).unwrap();ws.write(r,2,row.get::<String,_>("direction")).unwrap();ws.write(r,3,row.try_get::<f64,_>("total_amount").unwrap_or(0.0)).unwrap();ws.write(r,4,row.try_get::<f64,_>("final_amount").unwrap_or(0.0)).unwrap();}
    xlsx_response(workbook.save_to_buffer().unwrap(), "收支流水查询.xlsx")
}

pub async fn api_query_profit_detail_export(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let start_date=params.get("start_date").map(|s|s.as_str()).unwrap_or("");let end_date=params.get("end_date").map(|s|s.as_str()).unwrap_or("");
    let mut df=String::new();if!start_date.is_empty(){df.push_str(&format!(" AND so.order_date>='{}'",start_date));}if!end_date.is_empty(){df.push_str(&format!(" AND so.order_date<='{}'",end_date));}
    let sql=format!("SELECT so.order_no,so.order_date,soi.product_name,CAST(soi.quantity AS REAL) as quantity,CAST(soi.unit_price AS REAL) as sale_price,COALESCE(CAST(p.purchase_price AS REAL),0) as purchase_price,(CAST(soi.unit_price AS REAL)-COALESCE(CAST(p.purchase_price AS REAL),0))*CAST(soi.quantity AS REAL) as profit,CAST(soi.amount AS REAL) as sale_amount,COALESCE(CAST(p.purchase_price AS REAL),0)*CAST(soi.quantity AS REAL) as cost_amount FROM sales_order_item soi JOIN sales_order so ON soi.order_id=so.id LEFT JOIN product p ON soi.product_id=p.id WHERE so.status!='cancelled'{} ORDER BY so.order_date,so.order_no",df);
    let rows=sqlx::query(AssertSqlSafe(sql.as_str())).fetch_all(crate::db::pool()).await.unwrap_or_default();
    let mut workbook=Workbook::new();let ws=workbook.add_worksheet();ws.set_name("毛利明细").unwrap();let hf=xlsx_header_format(0x2E75B6);
    for(c,h)in["订单号","日期","商品名称","数量","销售单价","进货价","销售金额","成本金额","毛利","毛利率"].iter().enumerate(){ws.write_with_format(0,c as u16,*h,&hf).unwrap();}
    ws.set_column_width(0,20).unwrap();ws.set_column_width(1,14).unwrap();ws.set_column_width(2,20).unwrap();ws.set_column_width(3,10).unwrap();ws.set_column_width(4,12).unwrap();ws.set_column_width(5,12).unwrap();ws.set_column_width(6,14).unwrap();ws.set_column_width(7,14).unwrap();ws.set_column_width(8,14).unwrap();ws.set_column_width(9,10).unwrap();
    for(i,row)in rows.iter().enumerate(){let r=(i+1)as u32;let qty=row.try_get::<f64,_>("quantity").unwrap_or(0.0);let sp=row.try_get::<f64,_>("sale_price").unwrap_or(0.0);let pp=row.try_get::<f64,_>("purchase_price").unwrap_or(0.0);let sa=row.try_get::<f64,_>("sale_amount").unwrap_or(0.0);let ca=row.try_get::<f64,_>("cost_amount").unwrap_or(0.0);let pf=row.try_get::<f64,_>("profit").unwrap_or(0.0);let mg=if sa>0.0{pf/sa*100.0}else{0.0};ws.write(r,0,row.get::<String,_>("order_no")).unwrap();ws.write(r,1,row.get::<String,_>("order_date")).unwrap();ws.write(r,2,row.get::<String,_>("product_name")).unwrap();ws.write(r,3,qty).unwrap();ws.write(r,4,sp).unwrap();ws.write(r,5,pp).unwrap();ws.write(r,6,sa).unwrap();ws.write(r,7,ca).unwrap();ws.write(r,8,pf).unwrap();ws.write(r,9,mg).unwrap();}
    xlsx_response(workbook.save_to_buffer().unwrap(), "毛利明细查询.xlsx")
}

/// 财务结算查询 API：合并采购/销售订单，返回应付/已付/未付/下浮/扣减统计
pub async fn api_query_finance_settlement(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let start_date = params.get("start_date").map(|s| s.as_str()).unwrap_or("");
    let end_date = params.get("end_date").map(|s| s.as_str()).unwrap_or("");
    let keyword = params.get("keyword").map(|s| s.as_str()).unwrap_or("");
    let order_type = params.get("type").map(|s| s.as_str()).unwrap_or(""); // ""=全部, purchase=采购, sales=销售
    let is_settled = params.get("is_settled").map(|s| s.as_str()).unwrap_or(""); // ""=全部, 0=未结, 1=已结
    let page: i64 = params.get("page").and_then(|s| s.parse().ok()).unwrap_or(1);
    let page_size: i64 = params.get("page_size").and_then(|s| s.parse().ok()).unwrap_or(20);
    let offset = (page - 1) * page_size;
    let kw = format!("%{}%", keyword);

    // 用 UNION ALL 合并采购/销售订单，统一字段名
    // 应付 = total_amount, 下浮 = total_amount * discount_rate / 100, 扣减 = amount_reduction
    let mut where_clauses = Vec::new();

    // 日期筛选
    if !start_date.is_empty() {
        where_clauses.push(format!("order_date >= '{}'", start_date.replace('\'', "''")));
    }
    if !end_date.is_empty() {
        where_clauses.push(format!("order_date <= '{}'", end_date.replace('\'', "''")));
    }
    // 关键字筛选
    if !keyword.is_empty() {
        where_clauses.push(format!("(order_no LIKE '{}' OR party_name LIKE '{}')", kw.replace('\'', "''"), kw.replace('\'', "''")));
    }
    // 结算状态筛选
    if is_settled == "0" || is_settled == "1" {
        where_clauses.push(format!("is_settled = {}", is_settled));
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    // 类型筛选：当 where_sql 为空时加 WHERE，否则用 AND
    let type_filter = if order_type == "purchase" {
        if where_sql.is_empty() { "WHERE order_type = 'purchase'".to_string() } else { "AND order_type = 'purchase'".to_string() }
    } else if order_type == "sales" {
        if where_sql.is_empty() { "WHERE order_type = 'sales'".to_string() } else { "AND order_type = 'sales'".to_string() }
    } else {
        String::new()
    };

    // 统计总数
    let mut count_query = String::from(
        "SELECT COUNT(*) as cnt FROM (
            SELECT 'purchase' as order_type, po.order_no, po.order_date, po.total_amount, po.discount_rate, po.amount_reduction, po.final_amount, po.is_settled, s.name as party_name
            FROM purchase_order po JOIN supplier s ON po.supplier_id = s.id
            UNION ALL
            SELECT 'sales' as order_type, so.order_no, so.order_date, so.total_amount, so.discount_rate, so.amount_reduction, so.final_amount, so.is_settled, p.name as party_name
            FROM sales_order so JOIN purchaser p ON so.purchaser_id = p.id
        ) combined"
    );
    if !where_sql.is_empty() {
        count_query.push(' ');
        count_query.push_str(&where_sql);
    }
    if !type_filter.is_empty() {
        count_query.push(' ');
        count_query.push_str(&type_filter);
    }
    let total_rows: i64 = sqlx::query_scalar(AssertSqlSafe(count_query.as_str()))
        .fetch_one(crate::db::pool())
        .await
        .unwrap_or(0);

    // 查询数据
    let mut data_query = String::from(
        "SELECT * FROM (
            SELECT 'purchase' as order_type, po.id as source_id, po.order_no, po.order_date, po.total_amount, po.discount_rate, po.amount_reduction, po.final_amount, po.is_settled, s.name as party_name, s.id as party_id
            FROM purchase_order po JOIN supplier s ON po.supplier_id = s.id
            UNION ALL
            SELECT 'sales' as order_type, so.id as source_id, so.order_no, so.order_date, so.total_amount, so.discount_rate, so.amount_reduction, so.final_amount, so.is_settled, p.name as party_name, p.id as party_id
            FROM sales_order so JOIN purchaser p ON so.purchaser_id = p.id
        ) combined"
    );
    if !where_sql.is_empty() {
        data_query.push(' ');
        data_query.push_str(&where_sql);
    }
    if !type_filter.is_empty() {
        data_query.push(' ');
        data_query.push_str(&type_filter);
    }
    data_query.push_str(&format!(" ORDER BY order_date DESC, order_no DESC LIMIT {} OFFSET {}", page_size, offset));
    let rows = sqlx::query(AssertSqlSafe(data_query.as_str()))
        .fetch_all(crate::db::pool())
        .await
        .unwrap_or_default();

    let mut orders: Vec<serde_json::Value> = Vec::new();
    let mut sum_total = 0.0f64;
    let mut sum_discount = 0.0f64;
    let mut sum_reduction = 0.0f64;
    let mut sum_paid = 0.0f64;
    let mut sum_unpaid = 0.0f64;

    for row in &rows {
        let total_amount: f64 = row.get("total_amount");
        let discount_rate: f64 = row.get("discount_rate");
        let amount_reduction: f64 = row.get("amount_reduction");
        let final_amount: f64 = row.get("final_amount");
        let settled: i64 = row.get("is_settled");
        let discount_amount = total_amount * discount_rate / 100.0;

        let paid = if settled == 1 { final_amount } else { 0.0 };
        let unpaid = if settled == 0 { final_amount } else { 0.0 };

        sum_total += total_amount;
        sum_discount += discount_amount;
        sum_reduction += amount_reduction;
        sum_paid += paid;
        sum_unpaid += unpaid;

        orders.push(serde_json::json!({
            "order_type": row.get::<String, _>("order_type"),
            "source_id": row.get::<i64, _>("source_id"),
            "order_no": row.get::<String, _>("order_no"),
            "order_date": row.get::<String, _>("order_date"),
            "party_name": row.get::<String, _>("party_name"),
            "total_amount": total_amount,
            "discount_rate": discount_rate,
            "discount_amount": (discount_amount * 100.0).round() / 100.0,
            "amount_reduction": amount_reduction,
            "final_amount": final_amount,
            "is_settled": settled,
            "paid_amount": (paid * 100.0).round() / 100.0,
            "unpaid_amount": (unpaid * 100.0).round() / 100.0,
        }));
    }

    let result = serde_json::json!({
        "data": orders,
        "page": page,
        "page_size": page_size,
        "total": total_rows,
        "total_pages": (total_rows + page_size - 1) / page_size,
        "summary": {
            "total_amount": (sum_total * 100.0).round() / 100.0,
            "discount_amount": (sum_discount * 100.0).round() / 100.0,
            "reduction_amount": (sum_reduction * 100.0).round() / 100.0,
            "paid_amount": (sum_paid * 100.0).round() / 100.0,
            "unpaid_amount": (sum_unpaid * 100.0).round() / 100.0,
        }
    });
    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

pub async fn api_query_category_stats_export(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let start_date=params.get("start_date").map(|s|s.as_str()).unwrap_or("");let end_date=params.get("end_date").map(|s|s.as_str()).unwrap_or("");
    let rows=sqlx::query("SELECT pc.id,pc.name as category_name,COALESCE((SELECT SUM(poi.quantity) FROM purchase_order_item poi JOIN purchase_order po ON poi.order_id=po.id JOIN product pr ON poi.product_id=pr.id WHERE pr.category_id=pc.id AND po.order_date>=? AND po.order_date<=?),0) as purchase_quantity,COALESCE((SELECT SUM(poi.amount) FROM purchase_order_item poi JOIN purchase_order po ON poi.order_id=po.id JOIN product pr ON poi.product_id=pr.id WHERE pr.category_id=pc.id AND po.order_date>=? AND po.order_date<=?),0) as purchase_amount,COALESCE((SELECT SUM(soi.quantity) FROM sales_order_item soi JOIN sales_order so ON soi.order_id=so.id JOIN product pr ON soi.product_id=pr.id WHERE pr.category_id=pc.id AND so.order_date>=? AND so.order_date<=?),0) as sales_quantity,COALESCE((SELECT SUM(soi.amount) FROM sales_order_item soi JOIN sales_order so ON soi.order_id=so.id JOIN product pr ON soi.product_id=pr.id WHERE pr.category_id=pc.id AND so.order_date>=? AND so.order_date<=?),0) as sales_amount,COALESCE((SELECT SUM(i.quantity) FROM inventory i JOIN product pr ON i.product_id=pr.id WHERE pr.category_id=pc.id),0) as stock_quantity,COALESCE((SELECT SUM(i.quantity*pr.selling_price) FROM inventory i JOIN product pr ON i.product_id=pr.id WHERE pr.category_id=pc.id),0) as stock_amount FROM category pc WHERE pc.entity_type='product' AND pc.parent_id IS NULL ORDER BY pc.id")
    .bind(start_date).bind(end_date).bind(start_date).bind(end_date).bind(start_date).bind(end_date).bind(start_date).bind(end_date).fetch_all(crate::db::pool()).await.unwrap_or_default();
    let mut workbook=Workbook::new();let ws=workbook.add_worksheet();ws.set_name("品类统计").unwrap();let hf=xlsx_header_format(0x2E75B6);
    for(c,h)in["品类名称","采购数量","采购金额","销售数量","销售金额","库存数量","库存金额","毛利"].iter().enumerate(){ws.write_with_format(0,c as u16,*h,&hf).unwrap();}
    ws.set_column_width(0,16).unwrap();ws.set_column_width(1,12).unwrap();ws.set_column_width(2,12).unwrap();ws.set_column_width(3,12).unwrap();ws.set_column_width(4,12).unwrap();ws.set_column_width(5,12).unwrap();ws.set_column_width(6,12).unwrap();ws.set_column_width(7,12).unwrap();
    for(i,row)in rows.iter().enumerate(){let r=(i+1)as u32;let pa:f64=row.get("purchase_amount");let sa:f64=row.get("sales_amount");let mg=sa-pa;ws.write(r,0,row.get::<String,_>("category_name")).unwrap();ws.write(r,1,row.get::<f64,_>("purchase_quantity")).unwrap();ws.write(r,2,pa).unwrap();ws.write(r,3,row.get::<f64,_>("sales_quantity")).unwrap();ws.write(r,4,sa).unwrap();ws.write(r,5,row.get::<f64,_>("stock_quantity")).unwrap();ws.write(r,6,row.get::<f64,_>("stock_amount")).unwrap();ws.write(r,7,mg).unwrap();}
    xlsx_response(workbook.save_to_buffer().unwrap(), "品类进销存统计.xlsx")
}

pub async fn api_query_document_summary_export(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let month=params.get("month").map(|s|s.as_str()).unwrap_or("");
    let rows=sqlx::query("SELECT strftime('%Y-%m',po.order_date) as month,COUNT(DISTINCT po.id) as purchase_count,COALESCE(SUM(po.total_amount),0) as purchase_amount,COALESCE((SELECT COUNT(DISTINCT so.id) FROM sales_order so WHERE strftime('%Y-%m',so.order_date)=strftime('%Y-%m',po.order_date)),0) as sales_count,COALESCE((SELECT SUM(so.total_amount) FROM sales_order so WHERE strftime('%Y-%m',so.order_date)=strftime('%Y-%m',po.order_date)),0) as sales_amount FROM purchase_order po WHERE strftime('%Y-%m',po.order_date)=? GROUP BY strftime('%Y-%m',po.order_date) UNION ALL SELECT strftime('%Y-%m',so.order_date) as month,COALESCE((SELECT COUNT(DISTINCT po.id) FROM purchase_order po WHERE strftime('%Y-%m',po.order_date)=strftime('%Y-%m',so.order_date)),0) as purchase_count,COALESCE((SELECT SUM(po.total_amount) FROM purchase_order po WHERE strftime('%Y-%m',po.order_date)=strftime('%Y-%m',so.order_date)),0) as purchase_amount,COUNT(DISTINCT so.id) as sales_count,COALESCE(SUM(so.total_amount),0) as sales_amount FROM sales_order so WHERE strftime('%Y-%m',so.order_date)=? GROUP BY strftime('%Y-%m',so.order_date)").bind(month).bind(month).fetch_all(crate::db::pool()).await.unwrap_or_default();
    let mut mm:std::collections::HashMap<String,serde_json::Value>=std::collections::HashMap::new();
    for row in &rows{let m=row.get::<String,_>("month");let pc:i64=row.get("purchase_count");let pa:f64=row.get("purchase_amount");let sc:i64=row.get("sales_count");let sa:f64=row.get("sales_amount");if let Some(e)=mm.get_mut(&m){e["purchase_count"]=serde_json::json!(std::cmp::max(e["purchase_count"].as_i64().unwrap_or(0),pc));e["purchase_amount"]=serde_json::json!(e["purchase_amount"].as_f64().unwrap_or(0.0).max(pa));e["sales_count"]=serde_json::json!(std::cmp::max(e["sales_count"].as_i64().unwrap_or(0),sc));e["sales_amount"]=serde_json::json!(e["sales_amount"].as_f64().unwrap_or(0.0).max(sa));}else{let mj=m.clone();mm.insert(mj,serde_json::json!({"month":m,"purchase_count":pc,"purchase_amount":pa,"sales_count":sc,"sales_amount":sa}));}}
    let mut result:Vec<serde_json::Value>=mm.values().cloned().collect();result.sort_by(|a,b|a["month"].as_str().unwrap_or("").cmp(b["month"].as_str().unwrap_or("")));
    let mut workbook=Workbook::new();let ws=workbook.add_worksheet();ws.set_name("单据汇总").unwrap();let hf=xlsx_header_format(0x2E75B6);
    for(c,h)in["月份","采购订单数","销售订单数","采购金额","销售金额"].iter().enumerate(){ws.write_with_format(0,c as u16,*h,&hf).unwrap();}
    ws.set_column_width(0,12).unwrap();ws.set_column_width(1,14).unwrap();ws.set_column_width(2,14).unwrap();ws.set_column_width(3,14).unwrap();ws.set_column_width(4,14).unwrap();
    for(i,item)in result.iter().enumerate(){let r=(i+1)as u32;ws.write(r,0,item.get("month").and_then(|v|v.as_str()).unwrap_or("")).unwrap();ws.write(r,1,item.get("purchase_count").and_then(|v|v.as_i64()).unwrap_or(0)).unwrap();ws.write(r,2,item.get("sales_count").and_then(|v|v.as_i64()).unwrap_or(0)).unwrap();ws.write(r,3,item.get("purchase_amount").and_then(|v|v.as_f64()).unwrap_or(0.0)).unwrap();ws.write(r,4,item.get("sales_amount").and_then(|v|v.as_f64()).unwrap_or(0.0)).unwrap();}
    xlsx_response(workbook.save_to_buffer().unwrap(), "单据汇总查询.xlsx")
}

pub async fn api_purchase_document_list_export(headers: axum::http::HeaderMap, axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let ctx = crate::auth::get_user_ctx(&headers).await;
    let supplier_id: Option<i64> = if ctx.role == "supplier" {
        if ctx.supplier_id > 0 { Some(ctx.supplier_id) } else { Some(-1) }
    } else {
        params.get("supplier_id").and_then(|s|s.parse::<i64>().ok())
    };
    let document_date=params.get("document_date").map(|s|s.as_str()).unwrap_or("");
    let mut sql="SELECT id,supplier_id,supplier_name,document_date,remark,create_at FROM purchase_document WHERE 1=1".to_string();
    let rows=match(supplier_id,document_date.is_empty()){(Some(sid),false)=>{sql.push_str(" AND supplier_id=? AND document_date=? ORDER BY create_at DESC");sqlx::query(AssertSqlSafe(sql.as_str())).bind(sid).bind(document_date).fetch_all(crate::db::pool()).await.unwrap_or_default()},(Some(sid),true)=>{sql.push_str(" AND supplier_id=? ORDER BY create_at DESC");sqlx::query(AssertSqlSafe(sql.as_str())).bind(sid).fetch_all(crate::db::pool()).await.unwrap_or_default()},(None,false)=>{sql.push_str(" AND document_date=? ORDER BY create_at DESC");sqlx::query(AssertSqlSafe(sql.as_str())).bind(document_date).fetch_all(crate::db::pool()).await.unwrap_or_default()},(None,true)=>{sql.push_str(" ORDER BY create_at DESC");sqlx::query(AssertSqlSafe(sql.as_str())).fetch_all(crate::db::pool()).await.unwrap_or_default()},};
    let mut workbook=Workbook::new();let ws=workbook.add_worksheet();ws.set_name("采购单据").unwrap();let hf=xlsx_header_format(0x4472C4);
    for(c,h)in["ID","供应商","单据日期","备注","创建时间"].iter().enumerate(){ws.write_with_format(0,c as u16,*h,&hf).unwrap();}
    ws.set_column_width(0,8).unwrap();ws.set_column_width(1,18).unwrap();ws.set_column_width(2,14).unwrap();ws.set_column_width(3,20).unwrap();ws.set_column_width(4,20).unwrap();
    for(i,row)in rows.iter().enumerate(){let r=(i+1)as u32;ws.write(r,0,row.get::<i64,_>("id")).unwrap();ws.write(r,1,row.get::<String,_>("supplier_name")).unwrap();ws.write(r,2,row.get::<String,_>("document_date")).unwrap();ws.write(r,3,row.get::<Option<String>,_>("remark").unwrap_or_default()).unwrap();ws.write(r,4,row.get::<String,_>("create_at")).unwrap();}
    xlsx_response(workbook.save_to_buffer().unwrap(), "采购单据列表.xlsx")
}

pub async fn api_sales_order_create(headers: axum::http::HeaderMap, Json(req): Json<SalesOrderReq>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/sales_order/create").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let ctx = crate::auth::get_user_ctx(&headers).await;

    // 仅已审核的采购方与商品允许下单
    if !is_audit_confirmed("purchaser", req.purchaser_id).await {
        return (StatusCode::BAD_REQUEST, "该采购方尚未审核通过，暂不能下单".to_string());
    }
    for item in &req.items {
        if !is_audit_confirmed("product", item.product_id).await {
            return (StatusCode::BAD_REQUEST, format!("商品「{}」尚未审核通过，暂不能下单", item.product_name));
        }
    }

    // 行级数据权限：purchaser 只能为自己绑定的采购单位创建销售单
    if ctx.role == "purchaser" {
        let effective_purchaser_id = if req.purchaser_id != 0 { req.purchaser_id } else { ctx.purchaser_id };
        if ctx.purchaser_id == 0 || effective_purchaser_id != ctx.purchaser_id {
            return (StatusCode::FORBIDDEN, "采购单位账号只能为自己创建销售单".to_string());
        }
    }

    let result = sqlx::query(
        "INSERT INTO sales_order(purchaser_id, order_no, order_date, total_amount, discount_rate, amount_reduction, final_amount, warehouse_id, warehouse_name, remark, supplier_company, truck_plate, is_settled) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
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
    .bind(req.supplier_company.as_deref().unwrap_or(""))
    .bind(req.truck_plate.as_deref().unwrap_or(""))
    .bind(req.is_settled.unwrap_or(0))
    .execute(crate::db::pool())
    .await;
    
    match result {
        Ok(res) => {
            let order_id = res.last_insert_rowid();
            if !req.items.is_empty() {
                let placeholders: Vec<String> = req.items.iter()
                    .map(|_| "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)".to_string())
                    .collect();
                let sql = format!(
                    "INSERT INTO sales_order_item(order_id, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, base_quantity, amount, pre_sale_quantity, supplier_id, supplier_name, remark) VALUES {}",
                    placeholders.join(", ")
                );
                
                let mut query = sqlx::query(AssertSqlSafe(sql.as_str()));
                for item in &req.items {
                    // 数量同步：保存时若销售数量为 0 而预售数量 > 0，
                    // 用预售数量兜底，避免后续生成采购时数量为 0 的空明细
                    let quantity = if item.quantity <= 0.0 && item.pre_sale_quantity.unwrap_or(0.0) > 0.0 {
                        item.pre_sale_quantity.unwrap()
                    } else {
                        item.quantity
                    };
                    query = query
                        .bind(order_id)
                        .bind(item.product_id)
                        .bind(&item.product_name)
                        .bind(&item.alias1)
                        .bind(&item.alias2)
                        .bind(&item.spec)
                        .bind(&item.unit)
                        .bind(item.unit_price)
                        .bind(quantity)
                        .bind(item.base_quantity.unwrap_or(0.0))
                        .bind(item.amount)
                        .bind(item.pre_sale_quantity.unwrap_or(0.0))
                        .bind(item.supplier_id)
                        .bind(&item.supplier_name)
                        .bind(&item.remark);
                }
                let _ = query.execute(crate::db::pool()).await;
            }
            crate::auth::log_operation(&ctx, "sales_order.create", "sales_order", &order_id.to_string(),
                &format!("创建销售单 {}（采购单位ID={}）", req.order_no, req.purchaser_id)).await;
            (StatusCode::OK, "创建成功".to_string())
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "创建失败".to_string()),
    }
}

pub async fn api_sales_order_list(headers: axum::http::HeaderMap, axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let keyword_pattern = parse_keyword_pattern(&params);
    
    let page: i64 = params.get("page").and_then(|s| s.parse().ok()).unwrap_or(1);
    let page_size: i64 = params.get("page_size").and_then(|s| s.parse().ok()).unwrap_or(20);
    let offset = (page - 1) * page_size;
    
    // 行级数据权限：purchaser 角色强制只看自己绑定的采购单位
    let ctx = crate::auth::get_user_ctx(&headers).await;
    let purchaser_id: Option<i64> = if ctx.role == "purchaser" {
        if ctx.purchaser_id > 0 { Some(ctx.purchaser_id) } else { Some(-1) /* 未绑定则查不到任何数据 */ }
    } else {
        params.get("purchaser_id").and_then(|s| s.parse().ok())
    };
    
    // 排序处理
    let sort_field = params.get("sort_field").map(|s| s.as_str()).unwrap_or("id");
    let sort_order = params.get("sort_order").map(|s| s.as_str()).unwrap_or("desc");
    let order_clause = match sort_field {
        "order_no" => format!("so.order_no {}", sort_order),
        "order_date" => format!("so.order_date {}", sort_order),
        "unit_name" => format!("p.name {}", sort_order),
        "status" => format!("so.status {}", sort_order),
        _ => format!("so.id {}", sort_order),
    };
    
    let is_settled_filter: Option<String> = match params.get("is_settled").map(|s| s.as_str()) {
        Some("0") | Some("1") => params.get("is_settled").cloned(),
        _ => None,
    };

    let (total_sql, total_params) = match (purchaser_id, is_settled_filter.as_ref()) {
        (Some(pid), Some(isf)) => (
            "SELECT COUNT(*) as count FROM sales_order so JOIN purchaser p ON so.purchaser_id = p.id
             WHERE so.purchaser_id = ? AND so.is_settled = ? AND (so.order_no LIKE ? OR p.name LIKE ? OR so.order_date LIKE ?)",
            vec![pid.to_string(), isf.clone(), keyword_pattern.clone(), keyword_pattern.clone(), keyword_pattern.clone()]
        ),
        (Some(pid), None) => (
            "SELECT COUNT(*) as count FROM sales_order so JOIN purchaser p ON so.purchaser_id = p.id
             WHERE so.purchaser_id = ? AND (so.order_no LIKE ? OR p.name LIKE ? OR so.order_date LIKE ?)",
            vec![pid.to_string(), keyword_pattern.clone(), keyword_pattern.clone(), keyword_pattern.clone()]
        ),
        (None, Some(isf)) => (
            "SELECT COUNT(*) as count FROM sales_order so JOIN purchaser p ON so.purchaser_id = p.id
             WHERE so.is_settled = ? AND (so.order_no LIKE ? OR p.name LIKE ? OR so.order_date LIKE ?)",
            vec![isf.clone(), keyword_pattern.clone(), keyword_pattern.clone(), keyword_pattern.clone()]
        ),
        (None, None) => (
            "SELECT COUNT(*) as count FROM sales_order so JOIN purchaser p ON so.purchaser_id = p.id
             WHERE so.order_no LIKE ? OR p.name LIKE ? OR so.order_date LIKE ?",
            vec![keyword_pattern.clone(), keyword_pattern.clone(), keyword_pattern.clone()]
        ),
    };

    let mut total_query = sqlx::query(total_sql);
    for p in &total_params {
        total_query = total_query.bind(p);
    }
    let total_rows = total_query.fetch_one(crate::db::pool()).await.unwrap();
    let total: i64 = total_rows.get("count");

    let (sql, query_params) = match (purchaser_id, is_settled_filter.as_ref()) {
        (Some(pid), Some(isf)) => (
            format!(
                "SELECT so.id, so.order_no, so.order_date, so.total_amount, so.discount_rate, so.amount_reduction, so.final_amount, so.status, so.remark, so.warehouse_id, so.warehouse_name, so.is_settled, p.name as purchaser_name,
                        (SELECT COUNT(*) FROM order_supplement_item osi WHERE osi.target_order_id = so.id) as supplement_count
                 FROM sales_order so JOIN purchaser p ON so.purchaser_id = p.id
                 WHERE so.purchaser_id = ? AND so.is_settled = ? AND (so.order_no LIKE ? OR p.name LIKE ? OR so.order_date LIKE ?)
                 ORDER BY {} LIMIT ? OFFSET ?",
                order_clause
            ),
            vec![pid.to_string(), isf.clone(), keyword_pattern.clone(), keyword_pattern.clone(), keyword_pattern.clone()]
        ),
        (Some(pid), None) => (
            format!(
                "SELECT so.id, so.order_no, so.order_date, so.total_amount, so.discount_rate, so.amount_reduction, so.final_amount, so.status, so.remark, so.warehouse_id, so.warehouse_name, so.is_settled, p.name as purchaser_name,
                        (SELECT COUNT(*) FROM order_supplement_item osi WHERE osi.target_order_id = so.id) as supplement_count
                 FROM sales_order so JOIN purchaser p ON so.purchaser_id = p.id
                 WHERE so.purchaser_id = ? AND (so.order_no LIKE ? OR p.name LIKE ? OR so.order_date LIKE ?)
                 ORDER BY {} LIMIT ? OFFSET ?",
                order_clause
            ),
            vec![pid.to_string(), keyword_pattern.clone(), keyword_pattern.clone(), keyword_pattern.clone()]
        ),
        (None, Some(isf)) => (
            format!(
                "SELECT so.id, so.order_no, so.order_date, so.total_amount, so.discount_rate, so.amount_reduction, so.final_amount, so.status, so.remark, so.warehouse_id, so.warehouse_name, so.is_settled, p.name as purchaser_name,
                        (SELECT COUNT(*) FROM order_supplement_item osi WHERE osi.target_order_id = so.id) as supplement_count
                 FROM sales_order so JOIN purchaser p ON so.purchaser_id = p.id
                 WHERE so.is_settled = ? AND (so.order_no LIKE ? OR p.name LIKE ? OR so.order_date LIKE ?)
                 ORDER BY {} LIMIT ? OFFSET ?",
                order_clause
            ),
            vec![isf.clone(), keyword_pattern.clone(), keyword_pattern.clone(), keyword_pattern.clone()]
        ),
        (None, None) => (
            format!(
                "SELECT so.id, so.order_no, so.order_date, so.total_amount, so.discount_rate, so.amount_reduction, so.final_amount, so.status, so.remark, so.warehouse_id, so.warehouse_name, so.is_settled, p.name as purchaser_name,
                        (SELECT COUNT(*) FROM order_supplement_item osi WHERE osi.target_order_id = so.id) as supplement_count
                 FROM sales_order so JOIN purchaser p ON so.purchaser_id = p.id
                 WHERE so.order_no LIKE ? OR p.name LIKE ? OR so.order_date LIKE ?
                 ORDER BY {} LIMIT ? OFFSET ?",
                order_clause
            ),
            vec![keyword_pattern.clone(), keyword_pattern.clone(), keyword_pattern.clone()]
        ),
    };
    
    let mut query = sqlx::query(AssertSqlSafe(sql.as_str()));
    for p in &query_params {
        query = query.bind(p);
    }
    query = query.bind(page_size).bind(offset);
    let rows = query.fetch_all(crate::db::pool()).await.unwrap_or_default();
    
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
            "is_reimburse": row.get::<i64, _>("supplement_count") > 0,
            "is_settled": row.get::<i64, _>("is_settled"),
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

pub async fn api_sales_order_accept(headers: axum::http::HeaderMap, Path(id): Path<i64>) -> impl IntoResponse {
    let ctx = crate::auth::get_user_ctx(&headers).await;

    let order_row = sqlx::query(
        "SELECT so.id, so.purchaser_id, so.order_no, so.order_date, so.total_amount, so.discount_rate, so.final_amount, so.remark,
                p.name as purchaser_name, p.address as purchaser_address
         FROM sales_order so JOIN purchaser p ON so.purchaser_id = p.id WHERE so.id = ?"
    )
    .bind(id)
    .fetch_optional(crate::db::pool())
    .await
    .unwrap_or(None);

    if order_row.is_none() {
        return (StatusCode::NOT_FOUND, "订单不存在".to_string());
    }

    // 行级数据权限：仅可查看归属自己的销售单验收单
    let row = order_row.unwrap();
    let row_purchaser_id: i64 = row.get("purchaser_id");
    if !crate::auth::can_access_sales_order(&ctx, row_purchaser_id) {
        return (StatusCode::FORBIDDEN, "您没有权限查看此订单".to_string());
    }
    let discount_rate = row.get::<f64, _>("discount_rate");

    // 1) 真实明细（带分类信息，用于排序）
    let item_rows = sqlx::query(
        "SELECT soi.id, soi.product_id, soi.product_name, soi.alias1, soi.alias2, soi.spec, soi.unit, soi.unit_price, soi.quantity, soi.amount, soi.remark,
                p.category_id, pc.name as category_name, pc2.name as parent_name
         FROM sales_order_item soi
         LEFT JOIN product p ON soi.product_id = p.id
         LEFT JOIN category pc ON p.category_id = pc.id
         LEFT JOIN category pc2 ON pc.parent_id = pc2.id
         WHERE soi.order_id = ?"
    )
    .bind(id)
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();

    use std::collections::HashMap;
    let mut item_map: HashMap<i64, (i64, String, String, String, f64, f64, f64, String, i64)> = HashMap::new(); // sort_key, food_name, spec, unit, unit_price, quantity, amount, remark, original_id
    // product_id -> 真实明细行 id，用于分摊增项 target_order_item_id 失效时回退匹配
    let mut product_to_key: HashMap<i64, i64> = HashMap::new();
    for r in &item_rows {
        let rid = r.get::<i64, _>("id");
        let pid = r.get::<i64, _>("product_id");
        product_to_key.entry(pid).or_insert(rid);
        let alias2 = r.get::<Option<String>, _>("alias2").unwrap_or_default();
        let product_name = r.get::<String, _>("product_name");
        let food_name = if alias2.is_empty() {
            product_name.clone()
        } else {
            format!("{}({})", product_name, alias2)
        };
        let unit = r.get::<Option<String>, _>("unit").unwrap_or_default();
        let spec = r.get::<Option<String>, _>("spec").unwrap_or_default();
        let original_remark = r.get::<Option<String>, _>("remark").unwrap_or_default();
        let category_name = r.get::<Option<String>, _>("category_name").unwrap_or_default();
        let parent_name = r.get::<Option<String>, _>("parent_name").unwrap_or_default();
        let sort_key = get_category_sort_key(&category_name, &parent_name);
        item_map.insert(rid, (sort_key, food_name, spec, unit, r.get::<f64, _>("unit_price"), r.get::<f64, _>("quantity"), r.get::<f64, _>("amount"), original_remark, rid));
    }

    // 2) 合并分摊增项（与验收单导出 Excel 一致的逻辑）
    let supplement_rows = sqlx::query(
        "SELECT id, target_order_id, source_order_id, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, amount, operation_type, target_order_item_id
         FROM order_supplement_item WHERE target_order_id = ?"
    )
    .bind(id)
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();

    for r in &supplement_rows {
        let op_type = r.get::<String, _>("operation_type");
        let target_item_id = r.get::<Option<i64>, _>("target_order_item_id");
        let supp_product_id = r.get::<i64, _>("product_id");
        let qty = r.get::<f64, _>("quantity");
        let amt = r.get::<f64, _>("amount");
        let alias2 = r.get::<Option<String>, _>("alias2").unwrap_or_default();
        let product_name = r.get::<String, _>("product_name");

        // 解析目标明细行 key：优先用 target_order_item_id，失效时用 product_id 回退匹配
        let resolved_key: Option<i64> = match target_item_id {
            Some(tid) if item_map.contains_key(&tid) => Some(tid),
            _ => product_to_key.get(&supp_product_id).copied(),
        };

        // 替换-冲减：负数金额，若归零则移除
        if op_type == "replace_remove" {
            if let Some(tid) = resolved_key {
                if let Some(entry) = item_map.get_mut(&tid) {
                    let new_qty = entry.4 + qty;
                    let new_amt = entry.6 + amt;
                    if new_qty.abs() < 0.001 || new_amt.abs() < 0.001 {
                        item_map.remove(&tid);
                    } else {
                        entry.4 = new_qty;
                        entry.6 = new_amt;
                    }
                }
            }
            continue;
        }

        // 追加数量：叠加到原明细
        if op_type == "increase_quantity" {
            if let Some(tid) = resolved_key {
                if let Some(entry) = item_map.get_mut(&tid) {
                    let new_qty = entry.4 + qty;
                    let new_amt = entry.6 + amt;
                    let new_remark = format!("{}（含增项+{}）", entry.7, qty);
                    entry.4 = new_qty;
                    entry.6 = new_amt;
                    entry.7 = new_remark;
                }
            }
        } else {
            // new_item 或 replace_add：作为新明细
            let food_name = if alias2.is_empty() { product_name.clone() } else { format!("{}({})", product_name, alias2) };
            let unit = r.get::<String, _>("unit");
            let spec = r.get::<Option<String>, _>("spec").unwrap_or_default();
            // 通过 product_id 反查分类名做排序
            let category_query = sqlx::query(
                "SELECT pc.name as category_name, pc2.name as parent_name
                 FROM product p
                 LEFT JOIN category pc ON p.category_id = pc.id
                 LEFT JOIN category pc2 ON pc.parent_id = pc2.id
                 WHERE p.id = ?"
            )
            .bind(r.get::<i64, _>("product_id"))
            .fetch_optional(crate::db::pool())
            .await
            .ok()
            .flatten();
            let (cat_name, parent_name) = if let Some(cr) = category_query {
                (cr.get::<Option<String>, _>("category_name").unwrap_or_default(),
                 cr.get::<Option<String>, _>("parent_name").unwrap_or_default())
            } else {
                (String::new(), String::new())
            };
            let sort_key = get_category_sort_key(&cat_name, &parent_name);
            let remark = if op_type == "replace_add" {
                spec.clone()
            } else {
                if spec.is_empty() { "[增项]".to_string() } else { format!("{}; [增项]", spec) }
            };
            item_map.insert(-r.get::<i64, _>("id"), (sort_key, food_name, spec, unit, r.get::<f64, _>("unit_price"), qty, amt, remark, 0));
        }
    }

    let mut items_vec: Vec<_> = item_map.into_values().collect();
    items_vec.sort_by(|a, b| a.0.cmp(&b.0));

    // 3) 按合并后明细重算验收金额
    let accept_total_amount: f64 = items_vec.iter().map(|item| item.6).sum();
    let accept_final_amount = accept_total_amount * (1.0 - discount_rate / 100.0);

    // 4) 输出 JSON（字段名与原接口一致）
    let items: Vec<serde_json::Value> = items_vec
        .into_iter()
        .map(|(_sort_key, food_name, _spec, unit, unit_price, quantity, amount, remark, original_id)| {
            serde_json::json!({
                "id": original_id,
                "product_id": 0,
                "product_name": food_name.clone(),
                "food_name": food_name,
                "alias2": "",
                "spec": unit,
                "unit": unit,
                "unit_price": unit_price,
                "quantity": quantity,
                "amount": amount,
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
        "total_amount": accept_total_amount,
        "discount_rate": discount_rate,
        "final_amount": accept_final_amount,
        "remark": row.get::<Option<String>, _>("remark"),
        "purchaser_name": row.get::<String, _>("purchaser_name"),
        "purchaser_address": row.get::<Option<String>, _>("purchaser_address"),
        "supplier_name": supplier_name,
        "car_no": car_no,
        "items": items,
    });

    (StatusCode::OK, serde_json::to_string(&accept_data).unwrap())
}

pub async fn api_sales_order_by_purchaser(headers: axum::http::HeaderMap, Path(purchaser_id): Path<i64>) -> impl IntoResponse {
    // 行级数据权限：purchaser 角色只能查自己绑定的采购单位
    let ctx = crate::auth::get_user_ctx(&headers).await;
    let effective_purchaser_id = if ctx.role == "purchaser" {
        if ctx.purchaser_id > 0 { ctx.purchaser_id } else { -1 }
    } else {
        purchaser_id
    };

    let orders = sqlx::query(
        "SELECT so.id, so.order_no, so.order_date, so.total_amount, so.discount_rate, so.final_amount as discount_total, so.status,
                p.name as purchaser_name, so.purchaser_id
         FROM sales_order so LEFT JOIN purchaser p ON so.purchaser_id = p.id
         WHERE so.purchaser_id = ? ORDER BY so.order_date DESC"
    )
    .bind(effective_purchaser_id)
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();

    let order_ids: Vec<i64> = orders.iter().map(|row| row.get::<i64, _>("id")).collect();

    let mut allocation_status_map: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    if !order_ids.is_empty() {
        let placeholders = order_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = format!(
            "SELECT source_order_id, status FROM consumable_allocation WHERE source_order_id IN ({}) ORDER BY id ASC",
            placeholders
        );
        let mut query = sqlx::query(AssertSqlSafe(sql.as_str()));
        for id in &order_ids {
            query = query.bind(*id);
        }
        let rows = query.fetch_all(crate::db::pool()).await.unwrap_or_default();
        for row in rows {
            let source_id = row.get::<i64, _>("source_order_id");
            let status = row.get::<i64, _>("status");
            // 与 summary 保持一致：仅保留最早一条记录
            allocation_status_map.entry(source_id).or_insert(status);
        }
    }

    let result: Vec<serde_json::Value> = orders.iter().map(|row| {
        let order_id = row.get::<i64, _>("id");
        let allocation_status = allocation_status_map.get(&order_id).copied().unwrap_or(-1);
        serde_json::json!({
            "id": order_id,
            "order_no": row.get::<String, _>("order_no"),
            "order_date": row.get::<String, _>("order_date"),
            "total_amount": row.get::<f64, _>("total_amount"),
            "discount_rate": row.get::<f64, _>("discount_rate"),
            "discount_total": row.get::<f64, _>("discount_total"),
            "status": row.get::<String, _>("status"),
            "allocation_status": allocation_status,
            "purchaser_name": row.get::<Option<String>, _>("purchaser_name").unwrap_or_default(),
            "purchaser_id": row.get::<i64, _>("purchaser_id"),
        })
    }).collect();

    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

pub async fn api_allocation_create(Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    let source_order_id = req.get("source_order_id").and_then(|v| v.as_i64()).unwrap_or(0);
    let remark = req.get("remark").and_then(|v| v.as_str()).unwrap_or("");

    // 勾选的来源明细 id 列表
    let item_ids: Vec<i64> = req.get("source_item_ids")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|x| x.as_i64()).collect())
        .unwrap_or_default();

    if item_ids.is_empty() {
        return (StatusCode::BAD_REQUEST, "请至少勾选一条明细").into_response();
    }

    // 检查是否已存在分摊方案，防止重复创建
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM consumable_allocation WHERE source_order_id = ?)"
    )
    .bind(source_order_id)
    .fetch_one(crate::db::pool())
    .await
    .unwrap_or(false);

    if exists {
        return (StatusCode::BAD_REQUEST, "该订单已有分摊方案").into_response();
    }

    // 服务端按勾选明细重算分摊总额，确保金额可信
    let placeholders = item_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT COALESCE(SUM(amount), 0) FROM sales_order_item WHERE order_id = ? AND id IN ({})",
        placeholders
    );
    let mut q = sqlx::query_scalar::<_, f64>(AssertSqlSafe(sql.as_str())).bind(source_order_id);
    for iid in &item_ids {
        q = q.bind(*iid);
    }
    let total_amount: f64 = q.fetch_one(crate::db::pool()).await.unwrap_or(0.0);

    if total_amount <= 0.0 {
        return (StatusCode::BAD_REQUEST, "勾选明细的金额合计为 0，无法初始化").into_response();
    }

    let item_ids_str = item_ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");

    let result = sqlx::query(
        "INSERT INTO consumable_allocation(source_order_id, total_amount, allocated_amount, remaining_balance, status, remark, created_at, source_item_ids) VALUES (?, ?, 0, ?, 0, ?, datetime('now'), ?)"
    )
    .bind(source_order_id)
    .bind(total_amount)
    .bind(total_amount)
    .bind(remark)
    .bind(&item_ids_str)
    .execute(crate::db::pool())
    .await;

    match result {
        Ok(res) => (StatusCode::OK, serde_json::to_string(&serde_json::json!({ "id": res.last_insert_rowid(), "total_amount": total_amount })).unwrap()).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("创建分摊方案失败: {}", e)).into_response(),
    }
}

pub async fn api_allocation_summary(Path(source_order_id): Path<i64>) -> impl IntoResponse {
    let row = sqlx::query(
        "SELECT * FROM consumable_allocation WHERE source_order_id = ? ORDER BY id ASC LIMIT 1"
    )
    .bind(source_order_id)
    .fetch_optional(crate::db::pool())
    .await
    .unwrap_or(None);

    if row.is_none() {
        return (StatusCode::OK, serde_json::to_string(&serde_json::json!({ "exists": false })).unwrap());
    }

    let r = row.unwrap();
    let item_ids_str = r.get::<Option<String>, _>("source_item_ids").unwrap_or_default();
    let source_item_ids: Vec<i64> = item_ids_str.split(',').filter_map(|s| s.trim().parse::<i64>().ok()).collect();
    let summary = serde_json::json!({
        "exists": true,
        "id": r.get::<i64, _>("id"),
        "source_order_id": r.get::<i64, _>("source_order_id"),
        "total_amount": r.get::<f64, _>("total_amount"),
        "allocated_amount": r.get::<f64, _>("allocated_amount"),
        "remaining_balance": r.get::<f64, _>("remaining_balance"),
        "status": r.get::<i64, _>("status"),
        "remark": r.get::<Option<String>, _>("remark").unwrap_or_default(),
        "created_at": r.get::<String, _>("created_at"),
        "completed_at": r.get::<Option<String>, _>("completed_at").unwrap_or_default(),
        "source_item_ids": source_item_ids,
    });

    (StatusCode::OK, serde_json::to_string(&summary).unwrap())
}

pub async fn api_allocation_allocated_orders() -> impl IntoResponse {
    // 列出所有已进入分摊（存在 consumable_allocation 记录）的源订单
    let rows = sqlx::query(
        "SELECT ca.id as alloc_id, ca.source_order_id, ca.total_amount, ca.allocated_amount, ca.remaining_balance,
                ca.status, ca.remark, ca.created_at, ca.completed_at,
                so.order_no, so.order_date, so.purchaser_id,
                p.name as purchaser_name
         FROM consumable_allocation ca
         LEFT JOIN sales_order so ON ca.source_order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         ORDER BY ca.created_at DESC, ca.id DESC"
    )
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();

    let items: Vec<serde_json::Value> = rows.iter().map(|row| {
        serde_json::json!({
            "alloc_id": row.get::<i64, _>("alloc_id"),
            "source_order_id": row.get::<i64, _>("source_order_id"),
            "order_no": row.get::<Option<String>, _>("order_no").unwrap_or_default(),
            "order_date": row.get::<Option<String>, _>("order_date").unwrap_or_default(),
            "purchaser_id": row.get::<Option<i64>, _>("purchaser_id").unwrap_or(0),
            "purchaser_name": row.get::<Option<String>, _>("purchaser_name").unwrap_or_default(),
            "total_amount": row.get::<f64, _>("total_amount"),
            "allocated_amount": row.get::<f64, _>("allocated_amount"),
            "remaining_balance": row.get::<f64, _>("remaining_balance"),
            "status": row.get::<i64, _>("status"),
            "remark": row.get::<Option<String>, _>("remark").unwrap_or_default(),
            "created_at": row.get::<Option<String>, _>("created_at").unwrap_or_default(),
            "completed_at": row.get::<Option<String>, _>("completed_at").unwrap_or_default(),
        })
    }).collect();

    (StatusCode::OK, serde_json::to_string(&items).unwrap())
}

pub async fn api_allocation_terminate(Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    let source_order_id = req.get("source_order_id").and_then(|v| v.as_i64()).unwrap_or(0);
    let remark = req.get("remark").and_then(|v| v.as_str()).unwrap_or("");

    let result = sqlx::query(
        "UPDATE consumable_allocation SET status = 3, remark = ?, completed_at = datetime('now') WHERE source_order_id = ? AND status != 2"
    )
    .bind(remark)
    .bind(source_order_id)
    .execute(crate::db::pool())
    .await;

    match result {
        Ok(res) => {
            if res.rows_affected() > 0 {
                (StatusCode::OK, "终止成功").into_response()
            } else {
                (StatusCode::BAD_REQUEST, "无法终止：订单不存在或已完成").into_response()
            }
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("终止失败: {}", e)).into_response(),
    }
}

pub async fn api_allocation_cancel(Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    let source_order_id = req.get("source_order_id").and_then(|v| v.as_i64()).unwrap_or(0);

    let row = match sqlx::query(
        "SELECT id, allocated_amount, status FROM consumable_allocation WHERE source_order_id = ?"
    )
    .bind(source_order_id)
    .fetch_optional(crate::db::pool())
    .await {
        Ok(Some(r)) => r,
        Ok(None) => return (StatusCode::NOT_FOUND, "分摊方案不存在").into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("查询失败: {}", e)).into_response(),
    };

    let allocated: f64 = row.get("allocated_amount");
    let status: i64 = row.get("status");

    if status == 2 {
        return (StatusCode::BAD_REQUEST, "分摊方案已完成，无法取消").into_response();
    }
    if allocated > 0.0001 {
        return (StatusCode::BAD_REQUEST, format!("尚有已分摊金额 {:.2} 元，请先回滚全部分摊记录后再取消", allocated)).into_response();
    }

    let result = sqlx::query(
        "DELETE FROM consumable_allocation WHERE source_order_id = ? AND allocated_amount <= 0.0001 AND status != 2"
    )
    .bind(source_order_id)
    .execute(crate::db::pool())
    .await;

    match result {
        Ok(res) => {
            if res.rows_affected() > 0 {
                (StatusCode::OK, "取消成功").into_response()
            } else {
                (StatusCode::BAD_REQUEST, "无法取消分摊方案").into_response()
            }
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("取消失败: {}", e)).into_response(),
    }
}

pub async fn api_allocation_complete(Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    let source_order_id = req.get("source_order_id").and_then(|v| v.as_i64()).unwrap_or(0);
    let target_order_id = req.get("target_order_id").and_then(|v| v.as_i64());
    let auto_tail = req.get("auto_tail").and_then(|v| v.as_bool()).unwrap_or(false);

    let row = match sqlx::query(
        "SELECT id, remaining_balance, total_amount, status FROM consumable_allocation WHERE source_order_id = ?"
    )
    .bind(source_order_id)
    .fetch_optional(crate::db::pool())
    .await {
        Ok(Some(r)) => r,
        Ok(None) => return (StatusCode::NOT_FOUND, "分摊方案不存在").into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("查询失败: {}", e)).into_response(),
    };

    let alloc_id: i64 = row.get("id");
    let remaining_balance: f64 = row.get("remaining_balance");
    let total_amount: f64 = row.get("total_amount");
    let status: i64 = row.get("status");

    if status == 2 {
        return (StatusCode::BAD_REQUEST, "分摊方案已完成").into_response();
    }
    if status == 3 {
        return (StatusCode::BAD_REQUEST, "分摊方案已终止").into_response();
    }

    let threshold = 5.0;

    if remaining_balance > threshold && !auto_tail {
        return (StatusCode::BAD_REQUEST, format!("剩余 {:.2} 元未分摊，超过尾差限额（±{:.2} 元），请继续分摊或终止方案", remaining_balance, threshold)).into_response();
    }
    if remaining_balance < -threshold && !auto_tail {
        return (StatusCode::BAD_REQUEST, format!("已超额分摊 {:.2} 元，超过尾差限额（±{:.2} 元），请回滚部分分摊后再确认", remaining_balance.abs(), threshold)).into_response();
    }

    if auto_tail && remaining_balance.abs() > 0.0 {
        if let Some(tid) = target_order_id {
            let target_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sales_order WHERE id = ?)")
                .bind(tid)
                .fetch_one(crate::db::pool())
                .await
                .unwrap_or(false);

            if target_exists {
                let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                let _ = sqlx::query(
                    "INSERT INTO order_supplement_item(target_order_id, source_order_id, source_remark, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, amount, allocate_date, operation_type, target_order_item_id) VALUES (?, ?, ?, ?, ?, '', '', '', '次', 0, 1, ?, ?, 'new_item', NULL)"
                )
                .bind(tid)
                .bind(source_order_id)
                .bind(format!("耗材分摊尾差（源订单: {:.2} 元中剩余 {:.2} 元）", total_amount, remaining_balance))
                .bind(-1)
                .bind("分摊尾差")
                .bind(remaining_balance)
                .bind(remaining_balance)
                .bind(&today)
                .execute(crate::db::pool())
                .await;
            }
        }
    }

    let result = sqlx::query(
        "UPDATE consumable_allocation SET status = 2, completed_at = datetime('now'), remaining_balance = 0 WHERE id = ?"
    )
    .bind(alloc_id)
    .execute(crate::db::pool())
    .await;

    match result {
        Ok(_) => (StatusCode::OK, serde_json::to_string(&serde_json::json!({ "auto_tail": auto_tail && remaining_balance > 0.0 })).unwrap()).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("完成分摊失败: {}", e)).into_response(),
    }
}

pub async fn api_supplement_create(Json(req): Json<OrderSupplementItemReq>) -> impl IntoResponse {
    let result = sqlx::query(
        "INSERT INTO order_supplement_item(target_order_id, source_order_id, source_remark, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, amount, allocate_date, operation_type, target_order_item_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(req.target_order_id)
    .bind(req.source_order_id)
    .bind(req.source_remark)
    .bind(req.product_id)
    .bind(&req.product_name)
    .bind(req.alias1)
    .bind(req.alias2)
    .bind(req.spec)
    .bind(&req.unit)
    .bind(req.unit_price)
    .bind(req.quantity)
    .bind(req.amount)
    .bind(&req.allocate_date)
    .bind(&req.operation_type)
    .bind(req.target_order_item_id)
    .execute(crate::db::pool())
    .await;


    match result {
        Ok(res) => {
            // 所有操作类型（含 replace_remove 冲减负数）都更新分摊余额：
            // 正数(换入/追加/新增)消耗余额，负数(冲减)释放余额，保证账目平衡
            let _ = sqlx::query(
                "UPDATE consumable_allocation SET allocated_amount = allocated_amount + ?, remaining_balance = remaining_balance - ?, status = CASE WHEN remaining_balance - ? <= 0 THEN 2 ELSE 1 END WHERE source_order_id = ?"
            )
            .bind(req.amount)
            .bind(req.amount)
            .bind(req.amount)
            .bind(req.source_order_id)
            .execute(crate::db::pool())
            .await;

            // 冲减后若余额回升（remaining>0），需从"已完成"回退到"分摊中"并清空完结时间
            let _ = sqlx::query(
                "UPDATE consumable_allocation SET completed_at = datetime('now') WHERE source_order_id = ? AND status = 2 AND completed_at IS NULL"
            )
            .bind(req.source_order_id)
            .execute(crate::db::pool())
            .await;

            (StatusCode::OK, serde_json::to_string(&serde_json::json!({ "id": res.last_insert_rowid() })).unwrap())
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("创建增项失败: {}", e)),
    }
}

pub async fn api_supplement_list_by_target(Path(order_id): Path<i64>) -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT soi.id, soi.target_order_id, soi.source_order_id, soi.source_remark, soi.product_id, soi.product_name, soi.alias1, soi.alias2, soi.spec, soi.unit, soi.unit_price, soi.quantity, soi.amount, soi.allocate_date, soi.operation_type, soi.target_order_item_id, so.order_no as source_order_no
         FROM order_supplement_item soi LEFT JOIN sales_order so ON soi.source_order_id = so.id
         WHERE soi.target_order_id = ? ORDER BY soi.allocate_date DESC"
    )
    .bind(order_id)
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();

    let items: Vec<serde_json::Value> = rows.iter().map(|row| {
        serde_json::json!({
            "id": row.get::<i64, _>("id"),
            "target_order_id": row.get::<i64, _>("target_order_id"),
            "source_order_id": row.get::<i64, _>("source_order_id"),
            "source_order_no": row.get::<Option<String>, _>("source_order_no").unwrap_or_default(),
            "source_remark": row.get::<Option<String>, _>("source_remark").unwrap_or_default(),
            "product_id": row.get::<i64, _>("product_id"),
            "product_name": row.get::<String, _>("product_name"),
            "alias1": row.get::<Option<String>, _>("alias1").unwrap_or_default(),
            "alias2": row.get::<Option<String>, _>("alias2").unwrap_or_default(),
            "spec": row.get::<Option<String>, _>("spec").unwrap_or_default(),
            "unit": row.get::<String, _>("unit"),
            "unit_price": row.get::<f64, _>("unit_price"),
            "quantity": row.get::<f64, _>("quantity"),
            "amount": row.get::<f64, _>("amount"),
            "allocate_date": row.get::<String, _>("allocate_date"),
            "operation_type": row.get::<String, _>("operation_type"),
            "target_order_item_id": row.get::<Option<i64>, _>("target_order_item_id"),
        })
    }).collect();

    (StatusCode::OK, serde_json::to_string(&items).unwrap())
}

pub async fn api_adjusted_orders(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    // 列出所有收到分摊增项/调整的目标订单（target_order_id 指向该订单即视为有变更）
    let page = params.get("page").and_then(|v| v.parse::<i64>().ok()).unwrap_or(1).max(1);
    let page_size = params.get("page_size").and_then(|v| v.parse::<i64>().ok()).unwrap_or(10).clamp(1, 100);
    let keyword = params.get("keyword").cloned().unwrap_or_default();
    let purchaser_id = params.get("purchaser_id").and_then(|v| v.parse::<i64>().ok());
    let sort_order = params.get("sort_order").cloned().unwrap_or_default();
    let offset = (page - 1) * page_size;

    // 动态筛选条件（参数化绑定）
    let mut conds: Vec<String> = Vec::new();
    let mut bind_kw: Option<String> = None;
    let mut bind_pid: Option<i64> = None;
    if !keyword.trim().is_empty() {
        bind_kw = Some(format!("%{}%", keyword.trim()));
        conds.push("so.order_no LIKE ?".to_string());
    }
    if let Some(pid) = purchaser_id {
        bind_pid = Some(pid);
        conds.push("so.purchaser_id = ?".to_string());
    }
    let cond_sql = if conds.is_empty() { String::new() } else { format!(" AND {}", conds.join(" AND ")) };

    // 总数
    let count_sql = format!(
        "SELECT COUNT(*) FROM (
            SELECT so.id FROM sales_order so
            INNER JOIN order_supplement_item osi ON osi.target_order_id = so.id
            WHERE 1=1{} GROUP BY so.id
        ) t", cond_sql
    );
    let mut count_q = sqlx::query(AssertSqlSafe(count_sql.as_str()));
    if let Some(kw) = &bind_kw { count_q = count_q.bind(kw); }
    if let Some(pid) = bind_pid { count_q = count_q.bind(pid); }
    let total: i64 = count_q.fetch_one(crate::db::pool()).await.map(|r| r.get::<i64, _>(0)).unwrap_or(0);

    // 合计（所有匹配订单）
    let sum_sql = format!(
        "SELECT COALESCE(SUM(sub.total_amount), 0), COALESCE(SUM(sub.adjust_amount), 0)
         FROM (
            SELECT so.id, so.total_amount, COALESCE(SUM(osi.amount), 0) as adjust_amount
            FROM sales_order so
            INNER JOIN order_supplement_item osi ON osi.target_order_id = so.id
            WHERE 1=1{} GROUP BY so.id, so.total_amount
         ) sub", cond_sql
    );
    let mut sum_q = sqlx::query(AssertSqlSafe(sum_sql.as_str()));
    if let Some(kw) = &bind_kw { sum_q = sum_q.bind(kw); }
    if let Some(pid) = bind_pid { sum_q = sum_q.bind(pid); }
    let (sum_real, sum_adjust) = match sum_q.fetch_one(crate::db::pool()).await {
        Ok(r) => (r.get::<f64, _>(0), r.get::<f64, _>(1)),
        Err(_) => (0.0, 0.0),
    };

    // 排序：点击"订单日期"列头时按订单日期升/降序，否则默认按最近调整日
    let sort_sql = if sort_order == "asc" || sort_order == "desc" {
        format!("ORDER BY so.order_date {}, MAX(osi.allocate_date) DESC, so.order_no DESC", sort_order)
    } else {
        "ORDER BY MAX(osi.allocate_date) DESC, so.order_no DESC".to_string()
    };

    // 分页列表
    let list_sql = format!(
        "SELECT so.id, so.order_no, so.order_date, so.total_amount,
                p.name as purchaser_name, so.purchaser_id,
                COALESCE(SUM(osi.amount), 0) as adjust_amount,
                COUNT(osi.id) as adjust_count,
                MAX(osi.allocate_date) as last_adjust_date
         FROM sales_order so
         INNER JOIN order_supplement_item osi ON osi.target_order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         WHERE 1=1{}
         GROUP BY so.id, so.order_no, so.order_date, so.total_amount, p.name, so.purchaser_id
         {}
         LIMIT ? OFFSET ?", cond_sql, sort_sql
    );
    let mut list_q = sqlx::query(AssertSqlSafe(list_sql.as_str()));
    if let Some(kw) = &bind_kw { list_q = list_q.bind(kw); }
    if let Some(pid) = bind_pid { list_q = list_q.bind(pid); }
    let rows = list_q.bind(page_size).bind(offset).fetch_all(crate::db::pool()).await.unwrap_or_default();

    let items: Vec<serde_json::Value> = rows.iter().map(|row| {
        let total: f64 = row.get::<f64, _>("total_amount");
        let adjust: f64 = row.get::<f64, _>("adjust_amount");
        serde_json::json!({
            "id": row.get::<i64, _>("id"),
            "order_no": row.get::<String, _>("order_no"),
            "order_date": row.get::<String, _>("order_date"),
            "purchaser_name": row.get::<Option<String>, _>("purchaser_name").unwrap_or_default(),
            "purchaser_id": row.get::<i64, _>("purchaser_id"),
            "total_amount": total,
            "adjust_amount": adjust,
            "adjusted_total": total + adjust,
            "adjust_count": row.get::<i64, _>("adjust_count"),
            "last_adjust_date": row.get::<Option<String>, _>("last_adjust_date").unwrap_or_default(),
        })
    }).collect();

    let resp = serde_json::json!({
        "total": total,
        "page": page,
        "page_size": page_size,
        "total_real_amount": sum_real,
        "total_adjust_amount": sum_adjust,
        "total_adjusted_amount": sum_real + sum_adjust,
        "items": items,
    });

    (StatusCode::OK, resp.to_string())
}

pub async fn api_supplement_list_by_source(Path(order_id): Path<i64>) -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT soi.id, soi.target_order_id, soi.source_order_id, soi.source_remark, soi.product_id, soi.product_name, soi.alias1, soi.alias2, soi.spec, soi.unit, soi.unit_price, soi.quantity, soi.amount, soi.allocate_date, soi.operation_type, so.order_no as target_order_no
         FROM order_supplement_item soi LEFT JOIN sales_order so ON soi.target_order_id = so.id
         WHERE soi.source_order_id = ? ORDER BY soi.allocate_date DESC"
    )
    .bind(order_id)
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();

    let items: Vec<serde_json::Value> = rows.iter().map(|row| {
        serde_json::json!({
            "id": row.get::<i64, _>("id"),
            "target_order_id": row.get::<i64, _>("target_order_id"),
            "target_order_no": row.get::<Option<String>, _>("target_order_no").unwrap_or_default(),
            "source_order_id": row.get::<i64, _>("source_order_id"),
            "source_remark": row.get::<Option<String>, _>("source_remark").unwrap_or_default(),
            "product_id": row.get::<i64, _>("product_id"),
            "product_name": row.get::<String, _>("product_name"),
            "alias1": row.get::<Option<String>, _>("alias1").unwrap_or_default(),
            "alias2": row.get::<Option<String>, _>("alias2").unwrap_or_default(),
            "spec": row.get::<Option<String>, _>("spec").unwrap_or_default(),
            "unit": row.get::<String, _>("unit"),
            "unit_price": row.get::<f64, _>("unit_price"),
            "quantity": row.get::<f64, _>("quantity"),
            "amount": row.get::<f64, _>("amount"),
            "allocate_date": row.get::<String, _>("allocate_date"),
            "operation_type": row.get::<String, _>("operation_type"),
        })
    }).collect();

    (StatusCode::OK, serde_json::to_string(&items).unwrap())
}

pub async fn api_supplement_delete(Path(id): Path<i64>) -> impl IntoResponse {
    let row = sqlx::query(
        "SELECT source_order_id, amount, operation_type FROM order_supplement_item WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(crate::db::pool())
    .await
    .unwrap_or(None);

    if row.is_none() {
        return (StatusCode::NOT_FOUND, "增项不存在").into_response();
    }

    let r = row.unwrap();
    let source_order_id: i64 = r.get("source_order_id");
    let amount: f64 = r.get("amount");
    let _operation_type: String = r.get("operation_type");

    let result = sqlx::query("DELETE FROM order_supplement_item WHERE id = ?")
        .bind(id)
        .execute(crate::db::pool())
        .await;

    match result {
        Ok(res) => {
            if res.rows_affected() > 0 {
                // 所有操作类型（含 replace_remove 冲减负数）都回滚分摊金额：
                // 正数(换入/追加/新增)回滚时 allocated 减回、remaining 加回；
                // 负数(冲减)回滚时 allocated 加回、remaining 减回（与创建时反向）
                let _ = sqlx::query(
                    "UPDATE consumable_allocation SET allocated_amount = allocated_amount - ?, remaining_balance = remaining_balance + ?, status = CASE WHEN remaining_balance + ? < total_amount THEN 1 ELSE 0 END WHERE source_order_id = ? AND status != 3"
                )
                .bind(amount)
                .bind(amount)
                .bind(amount)
                .bind(source_order_id)
                .execute(crate::db::pool())
                .await;

                let _ = sqlx::query(
                    "UPDATE consumable_allocation SET completed_at = NULL WHERE source_order_id = ? AND status = 2 AND completed_at IS NOT NULL"
                )
                .bind(source_order_id)
                .execute(crate::db::pool())
                .await;

                // 若已分摊金额已归零，则删除该分摊方案，回到"未分摊"初始态，
                // 允许重新勾选明细并再次初始化分摊
                let _ = sqlx::query(
                    "DELETE FROM consumable_allocation WHERE source_order_id = ? AND status != 3 AND allocated_amount <= 0.0001"
                )
                .bind(source_order_id)
                .execute(crate::db::pool())
                .await;

                (StatusCode::OK, "回滚成功").into_response()
            } else {
                (StatusCode::NOT_FOUND, "增项不存在").into_response()
            }
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("回滚失败: {}", e)).into_response(),
    }
}

pub async fn api_supplement_compare(Path(order_id): Path<i64>) -> impl IntoResponse {
    let item_rows = sqlx::query(
        "SELECT soi.id, soi.product_id, soi.product_name, soi.alias1, soi.alias2, soi.spec, soi.unit, soi.unit_price, soi.quantity, soi.amount, soi.remark,
                p.category_id, pc.name as category_name, pc.parent_id, pc2.name as parent_name
         FROM sales_order_item soi LEFT JOIN product p ON soi.product_id = p.id
         LEFT JOIN category pc ON p.category_id = pc.id
         LEFT JOIN category pc2 ON pc.parent_id = pc2.id
         WHERE soi.order_id = ?"
    )
    .bind(order_id)
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();

    let supplement_rows = sqlx::query(
        "SELECT id, target_order_id, source_order_id, source_remark, product_id, product_name, alias1, alias2, spec, unit, unit_price, quantity, amount, allocate_date, operation_type, target_order_item_id
         FROM order_supplement_item WHERE target_order_id = ?"
    )
    .bind(order_id)
    .fetch_all(crate::db::pool())
    .await
    .unwrap_or_default();

    use std::collections::HashMap;
    let mut item_map: HashMap<i64, serde_json::Value> = HashMap::new();
    let mut real_items: Vec<serde_json::Value> = Vec::new();
    for r in &item_rows {
        let id = r.get::<i64, _>("id");
        let alias2 = r.get::<Option<String>, _>("alias2").unwrap_or_default();
        let product_name = r.get::<String, _>("product_name");
        let display_name = if alias2.is_empty() {
            product_name.clone()
        } else {
            format!("{}({})", product_name, alias2)
        };
        let spec = r.get::<Option<String>, _>("spec").unwrap_or_default();
        let category_name = r.get::<Option<String>, _>("category_name").unwrap_or_default();
        let parent_name = r.get::<Option<String>, _>("parent_name").unwrap_or_default();
        let sort_key = get_category_sort_key(&category_name, &parent_name);
        let item = serde_json::json!({
            "id": id,
            "product_id": r.get::<i64, _>("product_id"),
            "product_name": product_name,
            "display_name": display_name,
            "alias1": r.get::<Option<String>, _>("alias1").unwrap_or_default(),
            "alias2": alias2,
            "spec": spec,
            "unit": r.get::<Option<String>, _>("unit").unwrap_or_default(),
            "unit_price": r.get::<f64, _>("unit_price"),
            "quantity": r.get::<f64, _>("quantity"),
            "amount": r.get::<f64, _>("amount"),
            "remark": r.get::<Option<String>, _>("remark").unwrap_or_default(),
            "category_name": category_name,
            "parent_name": parent_name,
            "sort_key": sort_key,
            "is_increase": false,
            "is_new": false,
            "is_replaced": false,
            "supplement_quantity": 0.0,
            "supplement_amount": 0.0,
            "total_quantity": r.get::<f64, _>("quantity"),
            "total_amount": r.get::<f64, _>("amount"),
        });
        item_map.insert(id, item.clone());
        real_items.push(item);
    }

    let mut new_items: Vec<serde_json::Value> = Vec::new();
    for r in &supplement_rows {
        let op_type = r.get::<String, _>("operation_type");
        let target_item_id = r.get::<Option<i64>, _>("target_order_item_id");
        let qty = r.get::<f64, _>("quantity");
        let amt = r.get::<f64, _>("amount");

        if op_type == "increase_quantity" || op_type == "replace_remove" {
            if let Some(tid) = target_item_id {
                if let Some(existing) = item_map.get_mut(&tid) {
                    let s_qty = existing["supplement_quantity"].as_f64().unwrap_or(0.0) + qty;
                    let s_amt = existing["supplement_amount"].as_f64().unwrap_or(0.0) + amt;
                    let t_qty = existing["quantity"].as_f64().unwrap_or(0.0) + s_qty;
                    let t_amt = existing["amount"].as_f64().unwrap_or(0.0) + s_amt;
                    existing["supplement_quantity"] = serde_json::json!(s_qty);
                    existing["supplement_amount"] = serde_json::json!(s_amt);
                    existing["total_quantity"] = serde_json::json!(t_qty);
                    existing["total_amount"] = serde_json::json!(t_amt);
                    if op_type == "replace_remove" {
                        existing["is_replaced"] = serde_json::json!(true);
                    } else {
                        existing["is_increase"] = serde_json::json!(true);
                    }
                }
            }
        } else {
            let alias2 = r.get::<Option<String>, _>("alias2").unwrap_or_default();
            let product_name = r.get::<String, _>("product_name");
            let display_name = if alias2.is_empty() {
                product_name.clone()
            } else {
                format!("{}({})", product_name, alias2)
            };
            let spec = r.get::<Option<String>, _>("spec").unwrap_or_default();
            let source_remark = r.get::<Option<String>, _>("source_remark").unwrap_or_default();
            new_items.push(serde_json::json!({
                "id": -r.get::<i64, _>("id"),
                "supplement_id": r.get::<i64, _>("id"),
                "product_id": r.get::<i64, _>("product_id"),
                "product_name": product_name,
                "display_name": display_name,
                "alias1": r.get::<Option<String>, _>("alias1").unwrap_or_default(),
                "alias2": alias2,
                "spec": spec,
                "unit": r.get::<String, _>("unit"),
                "unit_price": r.get::<f64, _>("unit_price"),
                "quantity": 0.0,
                "amount": 0.0,
                "remark": source_remark,
                "category_name": "",
                "parent_name": "",
                "sort_key": 9999,
                "is_increase": false,
                "is_new": true,
                "supplement_quantity": qty,
                "supplement_amount": amt,
                "total_quantity": qty,
                "total_amount": amt,
                "allocate_date": r.get::<String, _>("allocate_date"),
            }));
        }
    }

    let mut real_total = 0.0;
    let mut supplement_total = 0.0;
    let mut alloc_total = 0.0;

    let mut combined: Vec<serde_json::Value> = Vec::new();
    for (_, item) in &item_map {
        real_total += item["amount"].as_f64().unwrap_or(0.0);
        supplement_total += item["supplement_amount"].as_f64().unwrap_or(0.0);
        alloc_total += item["total_amount"].as_f64().unwrap_or(0.0);
        combined.push(item.clone());
    }
    for item in &new_items {
        supplement_total += item["supplement_amount"].as_f64().unwrap_or(0.0);
        alloc_total += item["total_amount"].as_f64().unwrap_or(0.0);
        combined.push(item.clone());
    }

    combined.sort_by(|a, b| {
        let sa = a["sort_key"].as_i64().unwrap_or(9999);
        let sb = b["sort_key"].as_i64().unwrap_or(9999);
        sa.cmp(&sb)
    });

    let result = serde_json::json!({
        "real_total": real_total,
        "supplement_total": supplement_total,
        "allocation_total": alloc_total,
        "items": combined,
    });

    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

pub async fn api_sales_order_accept_excel(
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let force = params.get("force").map(|s| s == "1").unwrap_or(false);
    match check_sales_order_access(&headers, id).await {
        Err(e) => return e.into_response(),
        Ok(_) => {}
    }
    build_accept_excel(&headers, id, true, force).await.into_response()
}

pub async fn api_sales_order_real_excel(
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let force = params.get("force").map(|s| s == "1").unwrap_or(false);
    match check_sales_order_access(&headers, id).await {
        Err(e) => return e.into_response(),
        Ok(_) => {}
    }
    build_accept_excel(&headers, id, false, force).await.into_response()
}

pub async fn api_sales_order_sort_items(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let date = params.get("date").cloned().unwrap_or_default().trim().to_string();
    let has_date = !date.is_empty();
    let where_sql = if has_date {
        "WHERE so.order_date = ?"
    } else {
        "WHERE so.status IN ('pending', 'sorting')"
    };
    let sql = format!(
        "SELECT soi.id, soi.product_id, soi.product_name, soi.unit, soi.unit_price, soi.quantity, soi.amount, soi.remark,
                p.name as purchaser_name, so.order_no
         FROM sales_order_item soi 
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         {}
         ORDER BY soi.product_name, so.id, soi.id",
        where_sql
    );
    let mut q = sqlx::query(AssertSqlSafe(sql.as_str()));
    if has_date { q = q.bind(&date); }
    let rows = q.fetch_all(crate::db::pool())
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
    
    let mut items: Vec<serde_json::Value> = items_map.values()
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
    items.sort_by(|a, b| a["product_name"].as_str().unwrap_or("").cmp(b["product_name"].as_str().unwrap_or("")));
    
    (StatusCode::OK, serde_json::to_string(&items).unwrap())
}

pub async fn api_sales_order_sort_items_excel(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let date = params.get("date").cloned().unwrap_or_default().trim().to_string();
    let has_date = !date.is_empty();
    let where_sql = if has_date {
        "WHERE so.order_date = ?"
    } else {
        "WHERE so.status IN ('pending', 'sorting')"
    };
    let sql = format!(
        "SELECT soi.id, soi.product_id, soi.product_name, soi.unit, soi.unit_price, soi.quantity, soi.amount, soi.remark,
                p.name as purchaser_name, so.order_no
         FROM sales_order_item soi 
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         {}
         ORDER BY soi.product_name, so.id, soi.id",
        where_sql
    );
    let mut q = sqlx::query(AssertSqlSafe(sql.as_str()));
    if has_date { q = q.bind(&date); }
    let rows = q.fetch_all(crate::db::pool())
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
    
    let mut items: Vec<serde_json::Value> = items_map.values()
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
    // 固定排序：按商品名称排序，保证每次导出顺序一致
    items.sort_by(|a, b| a["product_name"].as_str().unwrap_or("").cmp(b["product_name"].as_str().unwrap_or("")));

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

pub async fn api_sales_order_sort_items_by_category(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let date = params.get("date").cloned().unwrap_or_default().trim().to_string();
    let has_date = !date.is_empty();
    let where_sql = if has_date {
        "WHERE so.order_date = ?"
    } else {
        "WHERE so.status IN ('pending', 'sorting')"
    };
    let sql = format!(
        "SELECT soi.id as item_id, soi.product_id, soi.product_name, soi.unit, soi.unit_price, soi.quantity, soi.amount, soi.remark,
                p.name as purchaser_name, so.order_no, c.name as category_name
         FROM sales_order_item soi 
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         LEFT JOIN product pr ON soi.product_id = pr.id
         LEFT JOIN category c ON pr.category_id = c.id
         {}
         ORDER BY c.name, p.name, soi.product_name, so.id, soi.id",
        where_sql
    );
    let mut q = sqlx::query(AssertSqlSafe(sql.as_str()));
    if has_date { q = q.bind(&date); }
    let rows = q.fetch_all(crate::db::pool())
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

pub async fn api_sales_order_sort_items_by_category_excel(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let date = params.get("date").cloned().unwrap_or_default().trim().to_string();
    let has_date = !date.is_empty();
    let where_sql = if has_date {
        "WHERE so.order_date = ?"
    } else {
        "WHERE so.status IN ('pending', 'sorting')"
    };
    let sql = format!(
        "SELECT soi.product_id, soi.product_name, soi.unit, soi.quantity, soi.remark,
                p.name as purchaser_name, so.order_no, c.name as category_name
         FROM sales_order_item soi 
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         LEFT JOIN product pr ON soi.product_id = pr.id
         LEFT JOIN category c ON pr.category_id = c.id
         {}
         ORDER BY c.name, p.name, soi.product_name, so.id, soi.id",
        where_sql
    );
    let mut q = sqlx::query(AssertSqlSafe(sql.as_str()));
    if has_date { q = q.bind(&date); }
    let rows = q.fetch_all(crate::db::pool())
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
            "pre_sale_quantity": r.get::<Option<f64>, _>("pre_sale_quantity").unwrap_or(0.0),
            "amount": r.get::<Option<f64>, _>("amount").unwrap_or(0.0),
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

pub async fn api_sales_order_sort_items_by_supplier(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    // 无日期：当前待分拣（pending/sorting）；有日期：检索该日期的历史分拣清单
    let date = params.get("date").cloned().unwrap_or_default().trim().to_string();
    let has_date = !date.is_empty();
    let where_sql = if has_date {
        "WHERE so.order_date = ?"
    } else {
        "WHERE so.status IN ('pending', 'sorting')"
    };
    let sql = format!(
        "SELECT soi.id as item_id, soi.product_id, soi.product_name, soi.unit, soi.unit_price, soi.quantity, soi.amount, soi.remark,
                soi.supplier_id, s.name as supplier_name, p.name as purchaser_name, so.order_no
         FROM sales_order_item soi 
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         LEFT JOIN supplier s ON soi.supplier_id = s.id
         {}
         ORDER BY s.name, p.name, soi.product_name, so.id, soi.id", where_sql
    );
    let mut q = sqlx::query(AssertSqlSafe(sql.as_str()));
    if has_date { q = q.bind(&date); }
    let rows = q.fetch_all(crate::db::pool()).await.unwrap_or_default();
    
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

pub async fn api_sales_order_sort_items_by_supplier_excel(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    // 无日期：当前待分拣（pending/sorting）；有日期：检索该日期的历史分拣清单
    let date = params.get("date").cloned().unwrap_or_default().trim().to_string();
    let has_date = !date.is_empty();
    // 可选：是否输出实量/单价/金额数值。不传或非 1/true 时为打印手填模式（三列留空）
    let print_values = matches!(
        params.get("print_values").map(|v| v.trim().to_lowercase()).as_deref(),
        Some("1") | Some("true") | Some("yes") | Some("on")
    );
    let where_sql = if has_date {
        "WHERE so.order_date = ?"
    } else {
        "WHERE so.status IN ('pending', 'sorting')"
    };
    let sql = format!(
        "SELECT soi.product_id, soi.product_name, soi.unit, soi.quantity, soi.pre_sale_quantity, soi.amount, soi.remark,
                soi.supplier_id, s.name as supplier_name, p.name as purchaser_name, so.order_no
         FROM sales_order_item soi 
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         LEFT JOIN supplier s ON soi.supplier_id = s.id
         {}
         ORDER BY s.name, p.name, soi.product_name, so.id, soi.id", where_sql
    );
    let mut q = sqlx::query(AssertSqlSafe(sql.as_str()));
    if has_date { q = q.bind(&date); }
    let rows = q.fetch_all(crate::db::pool()).await.unwrap_or_default();
    
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
            "pre_sale_quantity": r.get::<Option<f64>, _>("pre_sale_quantity").unwrap_or(0.0),
            "amount": r.get::<Option<f64>, _>("amount").unwrap_or(0.0),
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

        let cell_right_format = Format::new()
            .set_font_size(10)
            .set_align(FormatAlign::Right)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin);

        let purchaser_format = Format::new()
            .set_bold()
            .set_font_size(11)
            .set_align(FormatAlign::Left)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#E5E7EB")
            .set_font_color("#374151");

        let col_widths = [4, 16, 4, 6, 6, 6, 8, 8];
        let headers = ["序号", "品名规格", "单位", "订量", "实量", "单价", "金额", "备注"];
        let display_date = if has_date {
            date.clone()
        } else {
            Local::now().format("%Y-%m-%d").to_string()
        };

        let date_format = Format::new()
            .set_font_size(10)
            .set_align(FormatAlign::Right)
            .set_align(FormatAlign::VerticalCenter);

        let summary_format = Format::new()
            .set_bold()
            .set_font_size(10)
            .set_align(FormatAlign::Left)
            .set_align(FormatAlign::VerticalCenter);

        let summary_right_format = Format::new()
            .set_bold()
            .set_font_size(10)
            .set_align(FormatAlign::Right)
            .set_align(FormatAlign::VerticalCenter);

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

        let max_col = 7u16;

        if result.is_empty() {
            let worksheet = workbook.add_worksheet();
            worksheet.set_name("无数据")?;
            worksheet.merge_range(0, 0, 0, max_col, "暂无分拣数据", &title_format)?;
        } else {
            for supplier in &result {
                let supplier_name = supplier["supplier_name"].as_str().unwrap_or("未分配供应商");
                // Excel sheet 名称最多 31 个字符，且禁止 \ / ? * [ ] : 字符。
                // 按字符（而非字节）安全截断，避免中文供应商名按字节切片时 panic
                let sheet_name: String = supplier_name
                    .chars()
                    .filter(|c| !matches!(c, '\\' | '/' | '?' | '*' | '[' | ']' | ':'))
                    .take(31)
                    .collect();
                let worksheet = workbook.add_worksheet();
                worksheet.set_name(sheet_name.as_str())?;

                worksheet.set_landscape();
                // 上边距1cm，下边距1.27cm留出页脚间隙，页脚1cm，左右为0
                worksheet.set_margins(0.0, 0.0, 0.4, 0.5, 0.0, 0.2);
                worksheet.set_print_center_vertically(false);
                worksheet.set_print_center_horizontally(true);
                worksheet.set_header("");
                worksheet.set_footer("&C第 &P 页，共 &N 页");
                // 每页重复打印标题、日期、表头(行0-2)，保证多页时每页都有页头和日期
                worksheet.set_repeat_rows(0, 2)?;

                for (i, w) in col_widths.iter().enumerate() {
                    worksheet.set_column_width(i as u16, *w)?;
                }

                let mut current_row = 0;
                let title = format!("{} - 采购分拣清单", supplier_name);
                worksheet.merge_range(current_row, 0, current_row, max_col, title.as_str(), &title_format)?;
                worksheet.set_row_height(current_row, 28)?;
                current_row += 1;

                worksheet.merge_range(current_row, 4, current_row, max_col, display_date.as_str(), &date_format)?;
                worksheet.set_row_height(current_row, 14)?;
                current_row += 1;

                for (i, header) in headers.iter().enumerate() {
                    worksheet.write_with_format(current_row, i as u16, *header, &header_format)?;
                }
                current_row += 1;

                let mut grand_total_items = 0i64;
                let mut grand_total_amount = 0.0;

                if let Some(purchasers) = supplier["purchasers"].as_array() {
                    for purchaser in purchasers {
                        let purchaser_name = purchaser["purchaser_name"].as_str().unwrap_or("");
                        let purchaser_title = format!("├── {}", purchaser_name);
                        worksheet.merge_range(current_row, 0, current_row, max_col, purchaser_title.as_str(), &purchaser_format)?;
                        worksheet.set_row_height(current_row, 20)?;
                        current_row += 1;

                        if let Some(items) = purchaser["items"].as_array() {
                            // 按单位分组（保持插入顺序）
                            let mut unit_groups: Vec<(String, Vec<&serde_json::Value>)> = Vec::new();
                            for item in items {
                                let unit = item["unit"].as_str().unwrap_or("").to_string();
                                if let Some(pos) = unit_groups.iter().position(|(u, _)| u == &unit) {
                                    unit_groups[pos].1.push(item);
                                } else {
                                    unit_groups.push((unit, vec![item]));
                                }
                            }

                            let mut purchaser_total_items = 0i64;
                            let mut purchaser_total_amount = 0.0;
                            let mut purchaser_seq = 1;

                            for (unit, group_items) in &unit_groups {
                                let mut unit_amount = 0.0;
                                for item in group_items {
                                    let product_name = item["product_name"].as_str().unwrap_or("");
                                    let quantity = item["quantity"].as_f64().unwrap_or(0.0);
                                    let pre_sale_quantity = item["pre_sale_quantity"].as_f64().unwrap_or(0.0);
                                    let amount = item["amount"].as_f64().unwrap_or(0.0);
                                    let remark = item["remark"].as_str().unwrap_or("");

                                    unit_amount += amount;

                                    worksheet.write_with_format(current_row, 0, purchaser_seq as f64, &cell_format)?;
                                    worksheet.write_with_format(current_row, 1, product_name, &cell_left_format)?;
                                    worksheet.write_with_format(current_row, 2, unit.as_str(), &cell_format)?;
                                    worksheet.write_with_format(current_row, 3, pre_sale_quantity, &cell_format)?;
                                    if print_values {
                                        worksheet.write_with_format(current_row, 4, quantity, &cell_format)?;
                                        let unit_price = if quantity != 0.0 { amount / quantity } else { 0.0 };
                                        worksheet.write_with_format(current_row, 5, unit_price, &cell_format)?;
                                        worksheet.write_with_format(current_row, 6, amount, &cell_right_format)?;
                                    } else {
                                        worksheet.write_with_format(current_row, 4, "", &cell_format)?;
                                        worksheet.write_with_format(current_row, 5, "", &cell_format)?;
                                        worksheet.write_with_format(current_row, 6, "", &cell_right_format)?;
                                    }
                                    worksheet.write_with_format(current_row, 7, remark, &cell_left_format)?;
                                    worksheet.set_row_height(current_row, 20.0)?;//设置供应商分拣页面的导出xlsx的行高
                                    current_row += 1;
                                    purchaser_seq += 1;
                                }
                                let unit_count = group_items.len();
                                purchaser_total_items += unit_count as i64;
                                purchaser_total_amount += unit_amount;
                            }

                            // 采购单位小计
                            let purchaser_total = format!("小计: 包装数量 {}", purchaser_total_items);
                            worksheet.merge_range(current_row, 0, current_row, 5, purchaser_total.as_str(), &summary_format)?;
                            if print_values {
                                worksheet.write_with_format(current_row, 6, purchaser_total_amount, &summary_right_format)?;
                            } else {
                                worksheet.write_with_format(current_row, 6, "", &summary_right_format)?;
                            }
                            worksheet.set_row_height(current_row, 18)?;
                            current_row += 1;

                            grand_total_items += purchaser_total_items;
                            grand_total_amount += purchaser_total_amount;
                        }
                    }
                }

                // 供应商总计
                let grand_total = format!("总计: 包装数量 {}", grand_total_items);
                worksheet.merge_range(current_row, 0, current_row, 5, grand_total.as_str(), &grand_total_format)?;
                if print_values {
                    worksheet.write_with_format(current_row, 6, grand_total_amount, &grand_total_right_format)?;
                } else {
                    worksheet.write_with_format(current_row, 6, "", &grand_total_right_format)?;
                }
                worksheet.set_row_height(current_row, 22)?;
            }
        }

        let buf = workbook.save_to_buffer()?;
        Ok(buf)
    })();

    match excel_result {
        Ok(buf) => {
            let filename = if has_date {
                format!("采购分拣清单_按供应商_{}.xlsx", date)
            } else {
                "采购分拣清单_按供应商.xlsx".to_string()
            };
            let headers = [
                ("Content-Type", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
                ("Content-Disposition", &format!("attachment; filename=\"{}\"", filename)),
            ];
            (StatusCode::OK, headers, buf).into_response()
        }
        Err(e) => {
            eprintln!("Excel export error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "导出失败").into_response()
        }
    }
}

// ===== 今日进价采集（按供应商去重合并） =====
// 数据源：当天/指定日期的销售订单明细，按供应商分组，组内按商品合并去重
// 用途：向各供应商采集当日进价，录入后回写 product.purchase_price（最近价），
//       供销售订单生成采购订单时使用正确的进价

pub async fn api_product_today_price_items(
    headers: axum::http::HeaderMap,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    // 今日进价采集仅超级管理员可用
    let ctx = crate::auth::get_user_ctx(&headers).await;
    if ctx.role != "super_admin" {
        return (StatusCode::FORBIDDEN, serde_json::json!({ "success": false, "message": "只有超级管理员可以使用今日进价采集" }).to_string());
    }
    let date = params.get("date").cloned().unwrap_or_default().trim().to_string();
    let has_date = !date.is_empty();
    let where_sql = if has_date {
        "WHERE so.order_date = ?"
    } else {
        "WHERE so.status IN ('pending', 'sorting')"
    };
    let sql = format!(
        "SELECT soi.product_id, MAX(soi.product_name) as product_name, MAX(COALESCE(soi.spec,'')) as spec,
                MAX(soi.supplier_id) as supplier_id, MAX(s.name) as supplier_name,
                MAX(p.base_unit) as base_unit,
                MAX(p.purchase_price) as purchase_price,
                MAX(p.max_purchase_price) as max_purchase_price,
                MAX(p.min_purchase_price) as min_purchase_price,
                SUM(soi.quantity) as total_qty
         FROM sales_order_item soi
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN supplier s ON soi.supplier_id = s.id
         LEFT JOIN product p ON soi.product_id = p.id
         {} AND soi.product_id > 0
         GROUP BY soi.supplier_id, soi.product_id
         ORDER BY MAX(s.name), MAX(soi.product_name)", where_sql
    );
    let mut q = sqlx::query(AssertSqlSafe(sql.as_str()));
    if has_date {
        q = q.bind(&date);
    }
    let rows = q.fetch_all(crate::db::pool()).await.unwrap_or_default();

    // 供应商 → 商品去重结果
    let mut supplier_map: std::collections::HashMap<i64, (String, Vec<serde_json::Value>)> = std::collections::HashMap::new();
    for r in &rows {
        let supplier_id = r.get::<i64, _>("supplier_id");
        let supplier_name = r.get::<Option<String>, _>("supplier_name").unwrap_or_else(|| {
            if supplier_id == 0 { "未分配供应商".to_string() } else { format!("供应商{}", supplier_id) }
        });
        let total_qty: f64 = r.get::<Option<f64>, _>("total_qty").unwrap_or(0.0);
        let item = serde_json::json!({
            "product_id": r.get::<i64, _>("product_id"),
            "product_name": r.get::<Option<String>, _>("product_name").unwrap_or_default(),
            "spec": r.get::<Option<String>, _>("spec").unwrap_or_default(),
            "base_unit": r.get::<Option<String>, _>("base_unit").unwrap_or_default(),
            "total_qty": total_qty,
            "purchase_price": r.get::<Option<f64>, _>("purchase_price").unwrap_or(0.0),
            "max_purchase_price": r.get::<Option<f64>, _>("max_purchase_price").unwrap_or(0.0),
            "min_purchase_price": r.get::<Option<f64>, _>("min_purchase_price").unwrap_or(0.0),
        });
        let entry = supplier_map.entry(supplier_id).or_insert_with(|| (supplier_name.clone(), Vec::new()));
        entry.1.push(item);
    }

    let mut result: Vec<serde_json::Value> = Vec::new();
    for (supplier_id, (supplier_name, items)) in supplier_map {
        let total_qty: f64 = items.iter().map(|i| i["total_qty"].as_f64().unwrap_or(0.0)).sum();
        result.push(serde_json::json!({
            "supplier_id": supplier_id,
            "supplier_name": supplier_name,
            "items": items,
            "total_quantity": total_qty,
        }));
    }
    result.sort_by(|a, b| a["supplier_name"].as_str().unwrap_or("").cmp(b["supplier_name"].as_str().unwrap_or("")));

    (StatusCode::OK, serde_json::to_string(&result).unwrap())
}

pub async fn api_product_today_price_save(
    headers: axum::http::HeaderMap,
    Json(data): Json<std::collections::HashMap<String, serde_json::Value>>,
) -> impl IntoResponse {
    // 今日进价采集仅超级管理员可录入
    let ctx = crate::auth::get_user_ctx(&headers).await;
    if ctx.role != "super_admin" {
        return (StatusCode::FORBIDDEN, serde_json::json!({ "success": false, "message": "只有超级管理员可以录入今日进价" }).to_string());
    }
    let items = data.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let mut updated = 0i64;
    for item in items {
        let product_id = item.get("product_id").and_then(|v| v.as_i64());
        let price = item.get("price").and_then(|v| v.as_f64());
        if let (Some(pid), Some(price)) = (product_id, price) {
            if pid <= 0 || price <= 0.0 {
                continue;
            }
            let row = sqlx::query(
                "SELECT purchase_price, max_purchase_price, min_purchase_price FROM product WHERE id = ?"
            )
            .bind(pid)
            .fetch_optional(crate::db::pool())
            .await
            .unwrap_or(None);
            if let Some(r) = row {
                let old_purchase: f64 = r.get("purchase_price");
                let old_max: f64 = r.get("max_purchase_price");
                let old_min: f64 = r.get("min_purchase_price");
                let new_max = if old_max > 0.0 { old_max.max(price) } else { price };
                let new_min = if old_min > 0.0 { old_min.min(price) } else { price };
                let res = sqlx::query(
                    "UPDATE product SET purchase_price = ?, max_purchase_price = ?, min_purchase_price = ? WHERE id = ?"
                )
                .bind(price)
                .bind(new_max)
                .bind(new_min)
                .bind(pid)
                .execute(crate::db::pool())
                .await;
                if res.is_ok() {
                    updated += 1;
                    log_price_change(pid, "purchase_price", old_purchase, price, "today_price_collect", None, Some("今日进价采集录入")).await;
                    recalc_base_price_by_markup(pid, "today_price_collect", None).await;
                }
            }
        }
    }
    (StatusCode::OK, serde_json::json!({ "success": true, "message": format!("已保存 {} 个商品的今日进价", updated) }).to_string())
}

pub async fn api_product_today_price_excel(
    headers: axum::http::HeaderMap,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    // 今日进价采集仅超级管理员可用
    let ctx = crate::auth::get_user_ctx(&headers).await;
    if ctx.role != "super_admin" {
        return (StatusCode::FORBIDDEN, serde_json::json!({ "success": false, "message": "只有超级管理员可以使用今日进价采集" }).to_string()).into_response();
    }
    let date = params.get("date").cloned().unwrap_or_default().trim().to_string();
    let has_date = !date.is_empty();
    // print_values=0：今日价列留空，供打印后手写采集；=1：今日价列显示当前最近价
    let print_values = matches!(
        params.get("print_values").map(|v| v.trim().to_lowercase()).as_deref(),
        Some("1") | Some("true") | Some("yes") | Some("on")
    );
    let where_sql = if has_date {
        "WHERE so.order_date = ?"
    } else {
        "WHERE so.status IN ('pending', 'sorting')"
    };
    let sql = format!(
        "SELECT soi.product_id, MAX(soi.product_name) as product_name, MAX(COALESCE(soi.spec,'')) as spec,
                MAX(soi.supplier_id) as supplier_id, MAX(s.name) as supplier_name,
                MAX(p.base_unit) as base_unit,
                MAX(p.purchase_price) as purchase_price,
                MAX(p.max_purchase_price) as max_purchase_price,
                MAX(p.min_purchase_price) as min_purchase_price,
                SUM(soi.quantity) as total_qty
         FROM sales_order_item soi
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN supplier s ON soi.supplier_id = s.id
         LEFT JOIN product p ON soi.product_id = p.id
         {} AND soi.product_id > 0
         GROUP BY soi.supplier_id, soi.product_id
         ORDER BY MAX(s.name), MAX(soi.product_name)", where_sql
    );
    let mut q = sqlx::query(AssertSqlSafe(sql.as_str()));
    if has_date {
        q = q.bind(&date);
    }
    let rows = q.fetch_all(crate::db::pool()).await.unwrap_or_default();

    let mut supplier_map: std::collections::HashMap<i64, (String, Vec<serde_json::Value>)> = std::collections::HashMap::new();
    for r in &rows {
        let supplier_id = r.get::<i64, _>("supplier_id");
        let supplier_name = r.get::<Option<String>, _>("supplier_name").unwrap_or_else(|| {
            if supplier_id == 0 { "未分配供应商".to_string() } else { format!("供应商{}", supplier_id) }
        });
        let item = serde_json::json!({
            "product_id": r.get::<i64, _>("product_id"),
            "product_name": r.get::<Option<String>, _>("product_name").unwrap_or_default(),
            "spec": r.get::<Option<String>, _>("spec").unwrap_or_default(),
            "base_unit": r.get::<Option<String>, _>("base_unit").unwrap_or_default(),
            "total_qty": r.get::<Option<f64>, _>("total_qty").unwrap_or(0.0),
            "purchase_price": r.get::<Option<f64>, _>("purchase_price").unwrap_or(0.0),
            "max_purchase_price": r.get::<Option<f64>, _>("max_purchase_price").unwrap_or(0.0),
            "min_purchase_price": r.get::<Option<f64>, _>("min_purchase_price").unwrap_or(0.0),
        });
        let entry = supplier_map.entry(supplier_id).or_insert_with(|| (supplier_name.clone(), Vec::new()));
        entry.1.push(item);
    }

    let excel_result: Result<Vec<u8>, XlsxError> = (|| {
        let mut workbook = Workbook::new();

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

        let date_format = Format::new()
            .set_font_size(10)
            .set_align(FormatAlign::Right)
            .set_align(FormatAlign::VerticalCenter);

        let grand_total_format = Format::new()
            .set_bold()
            .set_font_size(11)
            .set_align(FormatAlign::Left)
            .set_align(FormatAlign::VerticalCenter)
            .set_background_color("#E5E7EB")
            .set_font_color("#374151");

        let col_widths = [4, 16, 6, 8, 8, 8, 8, 8];
        let headers = ["序号", "品名规格", "基本单位", "合计数量", "最高价", "最低价", "最近价", "今日价"];
        let display_date = if has_date {
            date.clone()
        } else {
            Local::now().format("%Y-%m-%d").to_string()
        };

        let max_col = 7u16;

        if supplier_map.is_empty() {
            let worksheet = workbook.add_worksheet();
            worksheet.set_name("无数据")?;
            worksheet.merge_range(0, 0, 0, max_col, "暂无采购数据", &title_format)?;
        } else {
            // 固定排序：供应商按名称排序，保证每次导出 sheet 顺序一致
            let mut supplier_list: Vec<(i64, String, Vec<serde_json::Value>)> = supplier_map
                .into_iter()
                .map(|(sid, (sname, sitems))| (sid, sname, sitems))
                .collect();
            supplier_list.sort_by(|a, b| a.1.cmp(&b.1));

            for (supplier_id, supplier_name, items) in supplier_list {
                let sheet_name: String = supplier_name
                    .chars()
                    .filter(|c| !matches!(c, '\\' | '/' | '?' | '*' | '[' | ']' | ':'))
                    .take(31)
                    .collect();
                let worksheet = workbook.add_worksheet();
                worksheet.set_name(sheet_name.as_str())?;

                worksheet.set_landscape();
                worksheet.set_margins(0.0, 0.0, 0.4, 0.5, 0.0, 0.2);
                worksheet.set_print_center_vertically(false);
                worksheet.set_print_center_horizontally(true);
                worksheet.set_header("");
                worksheet.set_footer("&C第 &P 页，共 &N 页");
                worksheet.set_repeat_rows(0, 2)?;

                for (i, w) in col_widths.iter().enumerate() {
                    worksheet.set_column_width(i as u16, *w)?;
                }

                let mut current_row = 0;
                let title = format!("{} - 今日进价采集清单", supplier_name);
                worksheet.merge_range(current_row, 0, current_row, max_col, title.as_str(), &title_format)?;
                worksheet.set_row_height(current_row, 28)?;
                current_row += 1;

                worksheet.merge_range(current_row, 4, current_row, max_col, display_date.as_str(), &date_format)?;
                worksheet.set_row_height(current_row, 14)?;
                current_row += 1;

                for (i, header) in headers.iter().enumerate() {
                    worksheet.write_with_format(current_row, i as u16, *header, &header_format)?;
                }
                current_row += 1;

                let mut grand_total_qty = 0.0;
                let mut seq = 1i64;
                for item in &items {
                    let product_name = item["product_name"].as_str().unwrap_or("");
                    let spec = item["spec"].as_str().unwrap_or("");
                    let name_with_spec = if spec.is_empty() {
                        product_name.to_string()
                    } else {
                        format!("{} {}", product_name, spec)
                    };
                    let base_unit = item["base_unit"].as_str().unwrap_or("");
                    let total_qty = item["total_qty"].as_f64().unwrap_or(0.0);
                    let max_p = item["max_purchase_price"].as_f64().unwrap_or(0.0);
                    let min_p = item["min_purchase_price"].as_f64().unwrap_or(0.0);
                    let purchase_price = item["purchase_price"].as_f64().unwrap_or(0.0);

                    worksheet.write_with_format(current_row, 0, seq as f64, &cell_format)?;
                    worksheet.write_with_format(current_row, 1, name_with_spec.as_str(), &cell_left_format)?;
                    worksheet.write_with_format(current_row, 2, base_unit, &cell_format)?;
                    worksheet.write_with_format(current_row, 3, total_qty, &cell_format)?;
                    worksheet.write_with_format(current_row, 4, max_p, &cell_format)?;
                    worksheet.write_with_format(current_row, 5, min_p, &cell_format)?;
                    worksheet.write_with_format(current_row, 6, purchase_price, &cell_format)?;
                    if print_values {
                        worksheet.write_with_format(current_row, 7, purchase_price, &cell_format)?;
                    } else {
                        worksheet.write_with_format(current_row, 7, "", &cell_format)?;
                    }
                    worksheet.set_row_height(current_row, 20.0)?;
                    current_row += 1;
                    seq += 1;
                    grand_total_qty += total_qty;
                }

                let grand_total = format!("合计数量 {}", grand_total_qty);
                worksheet.merge_range(current_row, 0, current_row, 6, grand_total.as_str(), &grand_total_format)?;
                worksheet.set_row_height(current_row, 22)?;
                let _ = supplier_id;
            }
        }

        let buf = workbook.save_to_buffer()?;
        Ok(buf)
    })();

    match excel_result {
        Ok(buf) => {
            let filename = if has_date {
                format!("今日进价采集清单_{}.xlsx", date)
            } else {
                "今日进价采集清单.xlsx".to_string()
            };
            let headers = [
                ("Content-Type", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
                ("Content-Disposition", &format!("attachment; filename=\"{}\"", filename)),
            ];
            (StatusCode::OK, headers, buf).into_response()
        }
        Err(e) => {
            eprintln!("今日进价采集清单导出失败: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "导出失败").into_response()
        }
    }
}

// ===== 今日进价采集·A4 打印页 =====
// 按供应商分组，A4 纵向三栏排版（每栏四列：序号/品名规格/单位/今日价，栏间空一列，共 14 列），左边距 1cm
pub async fn api_product_today_price_a4(
    headers: axum::http::HeaderMap,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    // 今日进价采集仅超级管理员可用
    let ctx = crate::auth::get_user_ctx(&headers).await;
    if ctx.role != "super_admin" {
        return (StatusCode::FORBIDDEN, serde_json::json!({ "success": false, "message": "只有超级管理员可以使用今日进价采集" }).to_string()).into_response();
    }
    let date = params.get("date").cloned().unwrap_or_default().trim().to_string();
    let has_date = !date.is_empty();
    // print_values=1：今日价列显示最近价（purchase_price）；=0：留空手写
    let print_values = matches!(
        params.get("print_values").map(|v| v.trim().to_lowercase()).as_deref(),
        Some("1") | Some("true") | Some("yes") | Some("on")
    );
    let where_sql = if has_date {
        "WHERE so.order_date = ?"
    } else {
        "WHERE so.status IN ('pending', 'sorting')"
    };
    let sql = format!(
        "SELECT soi.product_id, MAX(soi.product_name) as product_name, MAX(COALESCE(soi.spec,'')) as spec,
                MAX(soi.supplier_id) as supplier_id, MAX(s.name) as supplier_name,
                MAX(p.base_unit) as base_unit,
                MAX(p.purchase_price) as purchase_price,
                MAX(p.max_purchase_price) as max_purchase_price,
                MAX(p.min_purchase_price) as min_purchase_price,
                SUM(soi.quantity) as total_qty
         FROM sales_order_item soi
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN supplier s ON soi.supplier_id = s.id
         LEFT JOIN product p ON soi.product_id = p.id
         {} AND soi.product_id > 0
         GROUP BY soi.supplier_id, soi.product_id
         ORDER BY MAX(s.name), MAX(soi.product_name)", where_sql
    );
    let mut q = sqlx::query(AssertSqlSafe(sql.as_str()));
    if has_date {
        q = q.bind(&date);
    }
    let rows = q.fetch_all(crate::db::pool()).await.unwrap_or_default();

    // 供应商 → 商品去重结果（与采集页数据一致）
    let mut supplier_map: std::collections::HashMap<i64, (String, Vec<serde_json::Value>)> = std::collections::HashMap::new();
    for r in &rows {
        let supplier_id = r.get::<i64, _>("supplier_id");
        let supplier_name = r.get::<Option<String>, _>("supplier_name").unwrap_or_else(|| {
            if supplier_id == 0 { "未分配供应商".to_string() } else { format!("供应商{}", supplier_id) }
        });
        let item = serde_json::json!({
            "product_id": r.get::<i64, _>("product_id"),
            "product_name": r.get::<Option<String>, _>("product_name").unwrap_or_default(),
            "spec": r.get::<Option<String>, _>("spec").unwrap_or_default(),
            "base_unit": r.get::<Option<String>, _>("base_unit").unwrap_or_default(),
            "total_qty": r.get::<Option<f64>, _>("total_qty").unwrap_or(0.0),
            "purchase_price": r.get::<Option<f64>, _>("purchase_price").unwrap_or(0.0),
            "max_purchase_price": r.get::<Option<f64>, _>("max_purchase_price").unwrap_or(0.0),
            "min_purchase_price": r.get::<Option<f64>, _>("min_purchase_price").unwrap_or(0.0),
        });
        let entry = supplier_map.entry(supplier_id).or_insert_with(|| (supplier_name.clone(), Vec::new()));
        entry.1.push(item);
    }

    // 供应商按名称排序
    let mut supplier_list: Vec<(i64, String, Vec<serde_json::Value>)> = supplier_map
        .into_iter()
        .map(|(sid, (sname, sitems))| (sid, sname, sitems))
        .collect();
    supplier_list.sort_by(|a, b| a.1.cmp(&b.1));

    let display_date = if has_date { date.clone() } else { Local::now().format("%Y-%m-%d").to_string() };

    // A4 三栏排版：14 列 = 3 个大栏（每栏 4 列：序号/品名规格/单位/今日价）+ 2 个间隔列。
    // 供应商分组排列，每栏可容纳多家供应商（按名称排序依次填充），序号按栏竖向从 1 递增。
    // 每栏有固定行容量，放不下时切到下一栏；超过 3 栏则分 sheet（每 sheet 一页 A4）。
    // 每栏行容量：A4 纵向一页（标题28+日期16+表头22≈66pt，内容行18pt）
    // 可打印高度 ≈ 799pt，40 行内容可完整落在一页内
    const MAX_ROWS_PER_COL: usize = 40;
    const COL_BASE: [u16; 3] = [0, 5, 10];

    let excel_result: Result<Vec<u8>, XlsxError> = (|| {
        let mut workbook = Workbook::new();

        let title_format = Format::new()
            .set_bold()
            .set_font_size(14)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter);

        let date_format = Format::new()
            .set_font_size(10)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter);

        let header_format = Format::new()
            .set_bold()
            .set_font_size(10)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin);

        let supplier_format = Format::new()
            .set_bold()
            .set_font_size(10)
            .set_align(FormatAlign::Left)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin)
            .set_background_color("#FDE68A");

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

        let cell_right_format = Format::new()
            .set_font_size(10)
            .set_align(FormatAlign::Right)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin);

        // 展平商品（保留供应商分组，供应商已按名称排序）
        struct FlatItem {
            supplier: String,
            name_spec: String,
            base_unit: String,
            /// print_values=1 时为价格，否则为 None（今日价留空手写）
            price: Option<f64>,
        }
        let mut flat: Vec<FlatItem> = Vec::new();
        for (_sid, sname, items) in &supplier_list {
            for it in items {
                let name = it["product_name"].as_str().unwrap_or("");
                let spec = it["spec"].as_str().unwrap_or("");
                let name_spec = if spec.is_empty() { name.to_string() } else { format!("{} {}", name, spec) };
                let price = if print_values {
                    let raw = it["purchase_price"].as_f64().unwrap_or(0.0);
                    Some((raw * 100.0).round() / 100.0)
                } else {
                    None
                };
                flat.push(FlatItem {
                    supplier: sname.clone(),
                    name_spec,
                    base_unit: it["base_unit"].as_str().unwrap_or("").to_string(),
                    price,
                });
            }
        }

        if flat.is_empty() {
            let worksheet = workbook.add_worksheet();
            worksheet.set_name("无数据")?;
            worksheet.merge_range(0, 0, 0, 13, "暂无采购数据", &title_format)?;
            let buf = workbook.save_to_buffer()?;
            return Ok(buf);
        }

        // 按供应商分组切块（保持供应商名称排序）；单供应商超过栏容量时拆成多块，
        // 每块最多 MAX_ROWS_PER_COL-1 个商品（留 1 行给供应商标题），避免单栏溢出分页
        let mut blocks: Vec<(String, Vec<&FlatItem>)> = Vec::new();
        for it in &flat {
            if let Some(last) = blocks.last_mut() {
                if last.0 == it.supplier {
                    last.1.push(it);
                    continue;
                }
            }
            blocks.push((it.supplier.clone(), vec![it]));
        }
        let mut split_blocks: Vec<(String, Vec<&FlatItem>)> = Vec::new();
        for (sname, items) in blocks {
            for chunk in items.chunks(MAX_ROWS_PER_COL - 1) {
                split_blocks.push((sname.clone(), chunk.to_vec()));
            }
        }

        // 把供应商块分配到"栏"：每栏容量 MAX_ROWS_PER_COL（含供应商标题行），放不下切下一栏；满 3 栏则分页
        let mut pages: Vec<Vec<(String, Vec<&FlatItem>, u16)>> = Vec::new(); // 每页一组块，元素 (供应商名, 商品, 栏起始列)
        let mut cur_page: Vec<(String, Vec<&FlatItem>, u16)> = Vec::new();
        let mut col_index = 0usize;
        let mut rows_used = 0usize;

        for (sname, items) in split_blocks {
            let need = 1 + items.len(); // 供应商标题行 + 商品行
            if rows_used > 0 && rows_used + need > MAX_ROWS_PER_COL {
                col_index += 1;
                rows_used = 0;
                if col_index >= 3 {
                    pages.push(std::mem::take(&mut cur_page));
                    col_index = 0;
                }
            }
            cur_page.push((sname.clone(), items, COL_BASE[col_index]));
            rows_used += need;
            if rows_used >= MAX_ROWS_PER_COL {
                col_index += 1;
                rows_used = 0;
                if col_index >= 3 {
                    pages.push(std::mem::take(&mut cur_page));
                    col_index = 0;
                }
            }
        }
        if !cur_page.is_empty() {
            pages.push(cur_page);
        }

        for (page_idx, blocks_in_sheet) in pages.iter().enumerate() {
            let worksheet = workbook.add_worksheet();
            worksheet.set_name(format!("第{}页", page_idx + 1).as_str())?;

            // A4 纵向、左边距 1cm（约 0.394 英寸），其余边距 0.5cm
            worksheet.set_portrait();
            worksheet.set_paper_size(9); // A4 paper size
            worksheet.set_margins(0.394, 0.2, 0.2, 0.2, 0.2, 0.2);
            worksheet.set_print_center_horizontally(false);
            worksheet.set_header("");
            worksheet.set_footer("&C第 &P 页，共 &N 页");

            // 列宽：每栏 [序号2.5, 品名规格13, 单位4.5, 今日价6.5]，间隔列1.2，总计 14 列
            let col_widths = [2.5, 13.0, 4.5, 6.5, 1.2, 2.5, 13.0, 4.5, 6.5, 1.2, 2.5, 13.0, 4.5, 6.5];
            for (i, w) in col_widths.iter().enumerate() {
                worksheet.set_column_width(i as u16, *w)?;
            }

            // 标题 + 日期
            worksheet.merge_range(0, 0, 0, 13, "今日进价采集清单", &title_format)?;
            worksheet.set_row_height(0, 28)?;
            worksheet.merge_range(1, 0, 1, 13, display_date.as_str(), &date_format)?;
            worksheet.set_row_height(1, 16)?;

            // 表头（3 组，组间空一列）
            let headers = ["序号", "品名规格", "单位", "今日价"];
            for base in COL_BASE.iter() {
                for (hi, h) in headers.iter().enumerate() {
                    worksheet.write_with_format(2, base + hi as u16, *h, &header_format)?;
                }
            }
            worksheet.set_row_height(2, 22)?;

            // 每栏从第 3 行开始独立向下填充，栏内序号连续递增
            for col_idx in 0..3usize {
                let base = COL_BASE[col_idx];
                let col_blocks: Vec<&(String, Vec<&FlatItem>, u16)> = blocks_in_sheet.iter().filter(|b| b.2 == base).collect();
                if col_blocks.is_empty() {
                    continue;
                }
                let mut row = 3u32;
                for (sname, items, _b) in col_blocks {
                    worksheet.merge_range(row, base, row, base + 3, sname.as_str(), &supplier_format)?;
                    worksheet.set_row_height(row, 18)?;
                    row += 1;
                    // 序号按供应商分组重新从 1 编号
                    let mut seq = 1i64;
                    for it in items {
                        worksheet.write_with_format(row, base, seq as f64, &cell_format)?;
                        worksheet.write_with_format(row, base + 1, it.name_spec.as_str(), &cell_left_format)?;
                        worksheet.write_with_format(row, base + 2, it.base_unit.as_str(), &cell_format)?;
                        match it.price {
                            Some(p) => { worksheet.write_with_format(row, base + 3, p, &cell_right_format)?; }
                            None => { worksheet.write_with_format(row, base + 3, "", &cell_right_format)?; }
                        }
                        worksheet.set_row_height(row, 18)?;
                        row += 1;
                        seq += 1;
                    }
                }
            }
        }

        let buf = workbook.save_to_buffer()?;
        Ok(buf)
    })();

    match excel_result {
        Ok(buf) => {
            let filename = if has_date {
                format!("今日进价采集清单_A4_{}.xlsx", date)
            } else {
                "今日进价采集清单_A4.xlsx".to_string()
            };
            let headers = [
                ("Content-Type", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
                ("Content-Disposition", &format!("attachment; filename=\"{}\"", filename)),
            ];
            (StatusCode::OK, headers, buf).into_response()
        }
        Err(e) => {
            eprintln!("今日进价采集A4导出失败: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "导出失败").into_response()
        }
    }
}

// ===== 今日进价采集·按商品分类导出 =====
// 去除采购单位/仓库/供应商分组，按商品分类排序，便于整体采集当日进价
pub async fn api_product_today_price_excel_by_category(
    headers: axum::http::HeaderMap,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    // 今日进价采集仅超级管理员可用
    let ctx = crate::auth::get_user_ctx(&headers).await;
    if ctx.role != "super_admin" {
        return (StatusCode::FORBIDDEN, serde_json::json!({ "success": false, "message": "只有超级管理员可以使用今日进价采集" }).to_string()).into_response();
    }
    let date = params.get("date").cloned().unwrap_or_default().trim().to_string();
    let has_date = !date.is_empty();
    let print_values = matches!(
        params.get("print_values").map(|v| v.trim().to_lowercase()).as_deref(),
        Some("1") | Some("true") | Some("yes") | Some("on")
    );
    let where_sql = if has_date {
        "WHERE so.order_date = ?"
    } else {
        "WHERE so.status IN ('pending', 'sorting')"
    };
    let sql = format!(
        "SELECT soi.product_id, MAX(soi.product_name) as product_name, MAX(COALESCE(soi.spec,'')) as spec,
                MAX(p.base_unit) as base_unit,
                MAX(p.purchase_price) as purchase_price,
                MAX(p.max_purchase_price) as max_purchase_price,
                MAX(p.min_purchase_price) as min_purchase_price,
                SUM(soi.quantity) as total_qty,
                MAX(c.name) as category_name,
                MAX(c2.name) as parent_name
         FROM sales_order_item soi
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN product p ON soi.product_id = p.id
         LEFT JOIN category c ON p.category_id = c.id
         LEFT JOIN category c2 ON c.parent_id = c2.id
         {} AND soi.product_id > 0
         GROUP BY soi.product_id
         ORDER BY MAX(c2.name), MAX(c.name), MAX(soi.product_name)", where_sql
    );
    let mut q = sqlx::query(AssertSqlSafe(sql.as_str()));
    if has_date {
        q = q.bind(&date);
    }
    let rows = q.fetch_all(crate::db::pool()).await.unwrap_or_default();

    // 按分类排序：分类 sort_key 优先，再按商品名
    let mut items: Vec<serde_json::Value> = rows.iter().map(|r| {
        let category_name = r.get::<Option<String>, _>("category_name").unwrap_or_default();
        let parent_name = r.get::<Option<String>, _>("parent_name").unwrap_or_default();
        let sort_key = crate::get_category_sort_key(&category_name, &parent_name);
        serde_json::json!({
            "product_id": r.get::<i64, _>("product_id"),
            "product_name": r.get::<Option<String>, _>("product_name").unwrap_or_default(),
            "spec": r.get::<Option<String>, _>("spec").unwrap_or_default(),
            "base_unit": r.get::<Option<String>, _>("base_unit").unwrap_or_default(),
            "total_qty": r.get::<Option<f64>, _>("total_qty").unwrap_or(0.0),
            "purchase_price": r.get::<Option<f64>, _>("purchase_price").unwrap_or(0.0),
            "max_purchase_price": r.get::<Option<f64>, _>("max_purchase_price").unwrap_or(0.0),
            "min_purchase_price": r.get::<Option<f64>, _>("min_purchase_price").unwrap_or(0.0),
            "category_name": category_name,
            "sort_key": sort_key,
        })
    }).collect();
    items.sort_by(|a, b| {
        let sk = a["sort_key"].as_i64().unwrap_or(999).cmp(&b["sort_key"].as_i64().unwrap_or(999));
        if sk != std::cmp::Ordering::Equal { return sk; }
        a["product_name"].as_str().unwrap_or("").cmp(b["product_name"].as_str().unwrap_or(""))
    });

    let excel_result: Result<Vec<u8>, XlsxError> = (|| {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();

        worksheet.set_landscape();
        worksheet.set_margins(0.2, 0.2, 0.2, 0.2, 0.2, 0.2);
        worksheet.set_print_center_vertically(false);
        worksheet.set_print_center_horizontally(true);
        worksheet.set_header("");
        worksheet.set_footer("&C第 &P 页，共 &N 页");
        worksheet.set_repeat_rows(0, 2)?;

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

        let date_format = Format::new()
            .set_font_size(10)
            .set_align(FormatAlign::Right)
            .set_align(FormatAlign::VerticalCenter);

        let col_widths = [4, 16, 6, 8, 8, 8, 8, 8];
        // 第1列为序号，第2列为品名规格，第3列基本单位，第4列合计数量，第5~7列价格，第8列今日价
        let headers = ["序号", "品名规格", "基本单位", "合计数量", "最高价", "最低价", "最近价", "今日价"];
        let display_date = if has_date {
            date.clone()
        } else {
            Local::now().format("%Y-%m-%d").to_string()
        };
        let max_col = 7u16;

        let mut current_row = 0;
        worksheet.merge_range(current_row, 0, current_row, max_col, "今日进价采集清单（按分类）", &title_format)?;
        worksheet.set_row_height(current_row, 28)?;
        current_row += 1;

        worksheet.merge_range(current_row, 4, current_row, max_col, display_date.as_str(), &date_format)?;
        worksheet.set_row_height(current_row, 14)?;
        current_row += 1;

        for (i, header) in headers.iter().enumerate() {
            worksheet.write_with_format(current_row, i as u16, *header, &header_format)?;
        }
        worksheet.set_row_height(current_row, 20)?;
        current_row += 1;

        // 按分类分组打印分类标题
        let mut last_sort_key: i64 = -1;
        let mut seq = 1i64;
        for item in &items {
            let sort_key = item["sort_key"].as_i64().unwrap_or(999);
            if sort_key != last_sort_key {
                if last_sort_key != -1 {
                    current_row += 1;
                }
                let category_name = item["category_name"].as_str().unwrap_or("未分类");
                worksheet.merge_range(current_row, 0, current_row, max_col, format!("【{}】", category_name).as_str(), &header_format)?;
                worksheet.set_row_height(current_row, 20)?;
                current_row += 1;
                last_sort_key = sort_key;
            }

            let product_name = item["product_name"].as_str().unwrap_or("");
            let spec = item["spec"].as_str().unwrap_or("");
            let name_with_spec = if spec.is_empty() {
                product_name.to_string()
            } else {
                format!("{} {}", product_name, spec)
            };
            let base_unit = item["base_unit"].as_str().unwrap_or("");
            let total_qty = item["total_qty"].as_f64().unwrap_or(0.0);
            let max_p = item["max_purchase_price"].as_f64().unwrap_or(0.0);
            let min_p = item["min_purchase_price"].as_f64().unwrap_or(0.0);
            let purchase_price = item["purchase_price"].as_f64().unwrap_or(0.0);

            worksheet.write_with_format(current_row, 0, seq as f64, &cell_format)?;
            worksheet.write_with_format(current_row, 1, name_with_spec.as_str(), &cell_left_format)?;
            worksheet.write_with_format(current_row, 2, base_unit, &cell_format)?;
            worksheet.write_with_format(current_row, 3, total_qty, &cell_format)?;
            worksheet.write_with_format(current_row, 4, max_p, &cell_format)?;
            worksheet.write_with_format(current_row, 5, min_p, &cell_format)?;
            worksheet.write_with_format(current_row, 6, purchase_price, &cell_format)?;
            if print_values {
                worksheet.write_with_format(current_row, 7, purchase_price, &cell_format)?;
            } else {
                worksheet.write_with_format(current_row, 7, "", &cell_format)?;
            }
            worksheet.set_row_height(current_row, 20.0)?;
            current_row += 1;
            seq += 1;
        }

        for (i, w) in col_widths.iter().enumerate() {
            worksheet.set_column_width(i as u16, *w)?;
        }

        let buf = workbook.save_to_buffer()?;
        Ok(buf)
    })();

    match excel_result {
        Ok(buf) => {
            let filename = if has_date {
                format!("今日进价采集清单_按分类_{}.xlsx", date)
            } else {
                "今日进价采集清单_按分类.xlsx".to_string()
            };
            let headers = [
                ("Content-Type", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
                ("Content-Disposition", &format!("attachment; filename=\"{}\"", filename)),
            ];
            (StatusCode::OK, headers, buf).into_response()
        }
        Err(e) => {
            eprintln!("今日进价采集清单(按分类)导出失败: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "导出失败").into_response()
        }
    }
}

pub async fn api_sales_order_sort_items_by_purchaser(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let date = params.get("date").cloned().unwrap_or_default().trim().to_string();
    let has_date = !date.is_empty();
    let where_sql = if has_date {
        "WHERE so.order_date = ?"
    } else {
        "WHERE so.status IN ('pending', 'sorting')"
    };
    let sql = format!(
        "SELECT soi.id, soi.product_id, soi.product_name, soi.unit, soi.unit_price, soi.quantity, soi.amount, soi.remark,
                p.id as purchaser_id, p.name as purchaser_name, so.order_no
         FROM sales_order_item soi
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         {}
         ORDER BY p.name, soi.product_name, so.id, soi.id",
        where_sql
    );
    let mut q = sqlx::query(AssertSqlSafe(sql.as_str()));
    if has_date { q = q.bind(&date); }
    let rows = q.fetch_all(crate::db::pool())
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
    
    let mut purchasers: Vec<serde_json::Value> = purchaser_map.values().cloned().collect();
    purchasers.sort_by(|a, b| a["purchaser_name"].as_str().unwrap_or("").cmp(b["purchaser_name"].as_str().unwrap_or("")));
    
    (StatusCode::OK, serde_json::to_string(&purchasers).unwrap())
}

pub async fn api_sales_order_sort_items_by_purchaser_excel(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let date = params.get("date").cloned().unwrap_or_default().trim().to_string();
    let has_date = !date.is_empty();
    let where_sql = if has_date {
        "WHERE so.order_date = ?"
    } else {
        "WHERE so.status IN ('pending', 'sorting')"
    };
    let sql = format!(
        "SELECT soi.id, soi.product_id, soi.product_name, soi.unit, soi.unit_price, soi.quantity, soi.amount, soi.remark,
                p.id as purchaser_id, p.name as purchaser_name, so.order_no,
                pc.name as category_name, pc.parent_id, pc2.name as parent_name
         FROM sales_order_item soi
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         LEFT JOIN product pr ON soi.product_id = pr.id
         LEFT JOIN category pc ON pr.category_id = pc.id
         LEFT JOIN category pc2 ON pc.parent_id = pc2.id
         {}
         ORDER BY p.name, soi.product_name, so.id, soi.id",
        where_sql
    );
    let mut q = sqlx::query(AssertSqlSafe(sql.as_str()));
    if has_date { q = q.bind(&date); }
    let rows = q.fetch_all(crate::db::pool())
        .await
        .unwrap_or_default();
    
    let price_rows = sqlx::query(
        "SELECT poi.product_id, MAX(poi.unit_price) as max_price, MIN(poi.unit_price) as min_price,
                (SELECT unit_price FROM purchase_order_item WHERE product_id = poi.product_id ORDER BY id DESC LIMIT 1) as latest_price
         FROM purchase_order_item poi
         GROUP BY poi.product_id"
    )
    .fetch_all(crate::db::pool())
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
    // 固定排序：按采购单位名称排序，保证每次导出顺序一致
    purchasers.sort_by(|a, b| a["purchaser_name"].as_str().unwrap_or("").cmp(b["purchaser_name"].as_str().unwrap_or("")));
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

pub async fn api_sales_order_sort_comprehensive(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let date = params.get("date").cloned().unwrap_or_default().trim().to_string();
    let has_date = !date.is_empty();
    let where_sql = if has_date {
        "WHERE so.order_date = ?"
    } else {
        "WHERE so.status IN ('pending', 'sorting')"
    };
    let sql = format!(
        "SELECT soi.id, soi.product_id, soi.product_name, soi.unit, soi.unit_price, soi.quantity, soi.amount, soi.remark,
                p.id as purchaser_id, p.name as purchaser_name, so.order_no,
                c.name as category_name
         FROM sales_order_item soi 
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         LEFT JOIN product pr ON soi.product_id = pr.id
         LEFT JOIN category c ON pr.category_id = c.id
         {}
         ORDER BY p.name, c.name, soi.product_name, so.id, soi.id",
        where_sql
    );
    let mut q = sqlx::query(AssertSqlSafe(sql.as_str()));
    if has_date { q = q.bind(&date); }
    let rows = q.fetch_all(crate::db::pool())
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

pub async fn api_sales_order_sort_comprehensive_excel(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let date = params.get("date").cloned().unwrap_or_default().trim().to_string();
    let has_date = !date.is_empty();
    let where_sql = if has_date {
        "WHERE so.order_date = ?"
    } else {
        "WHERE so.status IN ('pending', 'sorting')"
    };
    let sql = format!(
        "SELECT soi.id, soi.product_id, soi.product_name, soi.unit, soi.quantity, soi.remark,
                p.id as purchaser_id, p.name as purchaser_name, so.order_no,
                c.name as category_name
         FROM sales_order_item soi 
         LEFT JOIN sales_order so ON soi.order_id = so.id
         LEFT JOIN purchaser p ON so.purchaser_id = p.id
         LEFT JOIN product pr ON soi.product_id = pr.id
         LEFT JOIN category c ON pr.category_id = c.id
         {}
         ORDER BY p.name, c.name, soi.product_name, so.id, soi.id",
        where_sql
    );
    let mut q = sqlx::query(AssertSqlSafe(sql.as_str()));
    if has_date { q = q.bind(&date); }
    let rows = q.fetch_all(crate::db::pool())
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

/// 设置销售订单结算状态（0=未结 1=已结），操作列直接调用
pub async fn api_sales_order_settle(headers: axum::http::HeaderMap, Path(id): Path<i64>, Json(data): Json<serde_json::Value>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/sales_order/settle").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let ctx = crate::auth::get_user_ctx(&headers).await;

    let order = sqlx::query("SELECT purchaser_id FROM sales_order WHERE id = ?")
        .bind(id)
        .fetch_optional(crate::db::pool())
        .await
        .ok()
        .flatten();
    let order = match order {
        Some(o) => o,
        None => return (StatusCode::NOT_FOUND, "订单不存在".to_string()),
    };
    let order_purchaser_id: i64 = order.get("purchaser_id");
    if !crate::auth::can_access_sales_order(&ctx, order_purchaser_id) {
        return (StatusCode::FORBIDDEN, "您没有权限操作此订单".to_string());
    }

    let is_settled: i64 = data["is_settled"].as_i64().unwrap_or(0);
    if is_settled != 0 && is_settled != 1 {
        return (StatusCode::BAD_REQUEST, "参数无效".to_string());
    }

    let result = sqlx::query("UPDATE sales_order SET is_settled = ? WHERE id = ?")
        .bind(is_settled)
        .bind(id)
        .execute(crate::db::pool())
        .await;

    match result {
        Ok(res) if res.rows_affected() > 0 => {
            let label = if is_settled == 1 { "已结" } else { "未结" };
            crate::auth::log_operation(&ctx, "sales_order.settle", "sales_order", &id.to_string(),
                &format!("设置销售单结算状态为「{}」", label)).await;
            (StatusCode::OK, "操作成功".to_string())
        }
        _ => (StatusCode::CONFLICT, "订单不存在或状态已变化".to_string()),
    }
}

pub async fn api_sales_order_update_status(headers: axum::http::HeaderMap, Json(req): Json<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/sales_order/update_status").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let ctx = crate::auth::get_user_ctx(&headers).await;

    let id = req.get("id").and_then(|s| s.parse::<i64>().ok());
    let new_status = req.get("status");
    
    if id.is_none() || new_status.is_none() {
        return (StatusCode::BAD_REQUEST, "缺少参数".to_string());
    }
    
    let id = id.unwrap();
    let new_status = new_status.unwrap();
    
    let valid_statuses = vec!["pending", "confirmed", "sorting", "sorted", "delivering", "delivered", "accepted"];
    if !valid_statuses.contains(&new_status.as_str()) {
        return (StatusCode::BAD_REQUEST, "无效状态".to_string());
    }
    
    let current_status: Option<String> = sqlx::query_scalar("SELECT status FROM sales_order WHERE id = ?")
        .bind(id)
        .fetch_one(crate::db::pool())
        .await
        .ok();
    
    let current_status = current_status.unwrap_or_else(|| "pending".to_string());

    // 行级数据权限：仅可操作归属自己的销售单
    let order_purchaser_id: i64 = sqlx::query_scalar("SELECT purchaser_id FROM sales_order WHERE id = ?")
        .bind(id)
        .fetch_one(crate::db::pool())
        .await
        .unwrap_or(-1);
    if !crate::auth::can_access_sales_order(&ctx, order_purchaser_id) {
        return (StatusCode::FORBIDDEN, "您没有权限操作此订单".to_string());
    }
    
    // 状态机：待审核 → 已审核 → 分拣中 → 已分拣 → 配送中 → 已送达 → 已验收 → 已结算
    // confirmed（已审核）为锁定态，允许反审核回 pending 或进入分拣
    let allowed_transitions = match current_status.as_str() {
        "pending" => vec!["confirmed", "sorting"],
        "confirmed" => vec!["pending", "sorting"],
        "sorting" => vec!["pending", "sorted"],
        "sorted" => vec!["sorting", "delivering"],
        "delivering" => vec!["sorted", "delivered"],
        "delivered" => vec!["delivering", "accepted"],
        "accepted" => vec!["delivered"],
        _ => vec![],
    };
    
    if !allowed_transitions.contains(&new_status.as_str()) {
        return (StatusCode::BAD_REQUEST, format!("状态不允许从 {} 转换到 {}", current_status, new_status));
    }
    
    // 原子状态转换 + 版本递增：WHERE 校验当前状态，防止并发重复流转
    let result = sqlx::query("UPDATE sales_order SET status = ?, version = version + 1 WHERE id = ? AND status = ?")
        .bind(new_status)
        .bind(id)
        .bind(&current_status)
        .execute(crate::db::pool())
        .await;
    
    match result {
        Ok(res) if res.rows_affected() == 0 => {
            (StatusCode::CONFLICT, "订单状态已变化，请刷新后重试".to_string())
        }
        Ok(_) => {
            crate::auth::log_operation(&ctx, "sales_order.update_status", "sales_order", &id.to_string(),
                &format!("销售单 {} 状态 {} -> {}", id, current_status, new_status)).await;
            (StatusCode::OK, "状态更新成功".to_string())
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "状态更新失败".to_string()),
    }
}

pub async fn api_sales_order_approve(headers: axum::http::HeaderMap, Path(id): Path<i64>, Json(data): Json<serde_json::Value>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/sales_order/approve").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let ctx = crate::auth::get_user_ctx(&headers).await;

    let order = sqlx::query("SELECT purchaser_id, status FROM sales_order WHERE id = ?")
        .bind(id)
        .fetch_optional(crate::db::pool())
        .await
        .ok()
        .flatten();
    let order = match order {
        Some(o) => o,
        None => return (StatusCode::NOT_FOUND, "订单不存在".to_string()),
    };
    let order_purchaser_id: i64 = order.get("purchaser_id");
    let order_status: String = order.get("status");
    if !crate::auth::can_access_sales_order(&ctx, order_purchaser_id) {
        return (StatusCode::FORBIDDEN, "您没有权限操作此订单".to_string());
    }
    if order_status == "confirmed" {
        return (StatusCode::BAD_REQUEST, "订单已审核，请勿重复操作".to_string());
    }
    if order_status != "pending" {
        return (StatusCode::BAD_REQUEST, format!("当前订单状态为「{}」，仅待审核状态的订单允许审核", order_status));
    }

    let reason = data["reason"].as_str().unwrap_or("").trim().to_string();
    let result = sqlx::query("UPDATE sales_order SET status = 'confirmed', is_settled = 1, version = version + 1 WHERE id = ? AND status = 'pending'")
        .bind(id)
        .execute(crate::db::pool())
        .await;

    match result {
        Ok(res) if res.rows_affected() > 0 => {
            crate::auth::log_operation(&ctx, "sales_order.approve", "sales_order", &id.to_string(),
                &format!("审核通过销售单 ID={}（{}）", id, if reason.is_empty() { "无备注" } else { &reason })).await;
            (StatusCode::OK, "审核成功，订单已锁定".to_string())
        }
        _ => (StatusCode::CONFLICT, "订单状态已变化，请刷新后重试".to_string()),
    }
}

pub async fn api_sales_order_unapprove(headers: axum::http::HeaderMap, Path(id): Path<i64>, Json(data): Json<serde_json::Value>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/sales_order/unapprove").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let ctx = crate::auth::get_user_ctx(&headers).await;

    let reason = data["reason"].as_str().unwrap_or("").trim().to_string();
    if reason.is_empty() {
        return (StatusCode::BAD_REQUEST, "反审核必须填写原因".to_string());
    }

    let order = sqlx::query("SELECT purchaser_id, status FROM sales_order WHERE id = ?")
        .bind(id)
        .fetch_optional(crate::db::pool())
        .await
        .ok()
        .flatten();
    let order = match order {
        Some(o) => o,
        None => return (StatusCode::NOT_FOUND, "订单不存在".to_string()),
    };
    let order_purchaser_id: i64 = order.get("purchaser_id");
    let order_status: String = order.get("status");
    if !crate::auth::can_access_sales_order(&ctx, order_purchaser_id) {
        return (StatusCode::FORBIDDEN, "您没有权限操作此订单".to_string());
    }
    // 超级管理员拥有任何时刻的反审核权限；其他角色仅允许对已审核（confirmed）订单反审核
    let is_super_admin = ctx.role == "super_admin";
    if !is_super_admin && order_status != "confirmed" {
        return (StatusCode::BAD_REQUEST, format!("当前订单状态为「{}」，仅已审核状态的订单允许反审核", order_status));
    }

    let update_sql = if is_super_admin {
        "UPDATE sales_order SET status = 'pending', is_settled = 0, version = version + 1 WHERE id = ?"
    } else {
        "UPDATE sales_order SET status = 'pending', is_settled = 0, version = version + 1 WHERE id = ? AND status = 'confirmed'"
    };
    let result = sqlx::query(update_sql)
        .bind(id)
        .execute(crate::db::pool())
        .await;

    match result {
        Ok(res) if res.rows_affected() > 0 => {
            crate::auth::log_operation(&ctx, "sales_order.unapprove", "sales_order", &id.to_string(),
                &format!("反审核销售单 ID={}，原因：{}", id, reason)).await;
            (StatusCode::OK, "反审核成功，订单已解锁".to_string())
        }
        _ => (StatusCode::CONFLICT, "订单状态已变化，请刷新后重试".to_string()),
    }
}

pub async fn api_sales_order_correction(headers: axum::http::HeaderMap, Json(data): Json<std::collections::HashMap<String, serde_json::Value>>) -> impl IntoResponse {
    match crate::auth::check_api_permission(&headers, "/api/sales_order/correction").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let ctx = crate::auth::get_user_ctx(&headers).await;

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
            .execute(crate::db::pool())
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
            .execute(crate::db::pool())
            .await;
            
            if let Ok(r) = result {
                updated_count += r.rows_affected() as i64;
            }
        }
    }
    
    crate::auth::log_operation(&ctx, "sales_order.correction", "sales_order", "", 
        &format!("批量修正销售单数量，共修正 {} 条记录", updated_count)).await;

    (StatusCode::OK, format!("成功修正 {} 条记录", updated_count))
}

pub async fn api_accept_create(Json(req): Json<AcceptReq>) -> impl IntoResponse {
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
    .execute(crate::db::pool())
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
                .execute(crate::db::pool())
                .await
                .ok();
            }
            StatusCode::OK
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub async fn api_accept_list() -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT fa.id, fa.supplier_id, fa.purchaser_id, fa.car_no, fa.supply_time, fa.total_price, fa.discount_rate, fa.final_price, fa.status,
                s.name as supplier_name, p.name as purchaser_name
         FROM food_accept fa 
         JOIN supplier s ON fa.supplier_id = s.id 
         JOIN purchaser p ON fa.purchaser_id = p.id 
         ORDER BY fa.id DESC"
    )
    .fetch_all(crate::db::pool())
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
