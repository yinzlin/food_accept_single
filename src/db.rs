use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::AssertSqlSafe;
use sqlx::SqlitePool;
use std::sync::OnceLock;

pub static DB_POOL: OnceLock<SqlitePool> = OnceLock::new();

pub async fn repair_db_corruption(pool: &SqlitePool) {
    // 1. 先尝试 REINDEX + VACUUM
    let _ = sqlx::query("REINDEX").execute(pool).await;
    let _ = sqlx::query("VACUUM").execute(pool).await;
    let check: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(pool)
        .await
        .unwrap_or_default();
    if check == "ok" { return; }
    eprintln!("REINDEX+VACUUM 后仍异常: {}", check);

    // 2. 修复 NUMERIC value in ...status 类型错误
    // 检查 purchase_order.status 是否有数值类型
    if check.contains("NUMERIC value in purchase_order.status") {
        let bad_rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT id, CAST(status AS TEXT) as status FROM purchase_order WHERE typeof(status) != 'text'"
        )
            .fetch_all(pool)
            .await
            .unwrap_or_default();
        for (id, _) in &bad_rows {
            let _ = sqlx::query("UPDATE purchase_order SET status = 'pending' WHERE id = ?")
                .bind(id).execute(pool).await;
            eprintln!("  修复 purchase_order.status: ID={}", id);
        }
    }
    // 检查 sales_order.status 是否有数值类型
    if check.contains("NUMERIC value in sales_order.status") {
        let bad_rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT id, CAST(status AS TEXT) as status FROM sales_order WHERE typeof(status) != 'text'"
        )
            .fetch_all(pool)
            .await
            .unwrap_or_default();
        for (id, _) in &bad_rows {
            let _ = sqlx::query("UPDATE sales_order SET status = 'pending' WHERE id = ?")
                .bind(id).execute(pool).await;
            eprintln!("  修复 sales_order.status: ID={}", id);
        }
    }

    // 3. 检查并修复重复 order_no
    for table in &["sales_order", "purchase_order"] {
        let dupes: Vec<(i64, String)> = {
            let sql = format!("SELECT id, order_no FROM {} WHERE order_no IN (SELECT order_no FROM {} GROUP BY order_no HAVING COUNT(*) > 1) ORDER BY order_no, id", table, table);
            sqlx::query_as::<_, (i64, String)>(AssertSqlSafe(sql))
        }
            .fetch_all(pool)
            .await
            .unwrap_or_default();
        for (i, (id, order_no)) in dupes.iter().enumerate() {
            let new_no = format!("{}-fix-{}", order_no, i);
            let sql = format!("UPDATE {} SET order_no = ? WHERE id = ?", table);
            let _ = sqlx::query(AssertSqlSafe(sql))
                .bind(&new_no).bind(id).execute(pool).await;
            eprintln!("  修复 {} 重复 order_no: ID={}, {} -> {}", table, id, order_no, new_no);
        }
    }

    // 4. 再次 REINDEX + VACUUM
    let _ = sqlx::query("REINDEX").execute(pool).await;
    let _ = sqlx::query("VACUUM").execute(pool).await;

    let final_check: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(pool)
        .await
        .unwrap_or_default();
    if final_check == "ok" {
        eprintln!("repair_db_corruption 修复成功");
    } else {
        eprintln!("repair_db_corruption 修复后仍异常: {}", final_check);
    }
}

pub async fn init_pool() {
    let pool = SqlitePoolOptions::new()
        .max_connections(16)
        .min_connections(4)
        .idle_timeout(std::time::Duration::from_secs(300))
        .max_lifetime(std::time::Duration::from_secs(3600))
        .after_connect(|conn, _meta| Box::pin(async move {
            // 连接级 PRAGMA：每次新连接拉出时都重新设置，
            // 避免 sqlx 连接池复用/重建时 busy_timeout 被重置回默认
            use sqlx::Executor;
            let _ = conn.execute("PRAGMA busy_timeout = 5000").await;
            let _ = conn.execute("PRAGMA journal_mode = DELETE").await;
            let _ = conn.execute("PRAGMA synchronous = NORMAL").await;
            let _ = conn.execute("PRAGMA temp_store = MEMORY").await;
            let _ = conn.execute("PRAGMA cache_size = -20000").await;
            let _ = conn.execute("PRAGMA locking_mode = NORMAL").await;
            let _ = conn.execute("PRAGMA auto_vacuum = INCREMENTAL").await;
            let _ = conn.execute("PRAGMA page_size = 4096").await;
            Ok(())
        }))
        .connect_with(
            SqliteConnectOptions::new()
                .filename("food_accept_v3.db")
                .create_if_missing(true)
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Delete)
                .pragma("cache_size", "-20000")
                .pragma("synchronous", "NORMAL")
                .pragma("temp_store", "MEMORY")
                .pragma("journal_mode", "DELETE")
                // busy_timeout 在 after_connect 中设置，避免被 connect_with pragma 列表冲掉
                .pragma("busy_timeout", "5000"),
        )
        .await
        .expect("数据库连接失败");
    
    let _ = sqlx::query("PRAGMA cache_size = -20000").execute(&pool).await;
    let _ = sqlx::query("PRAGMA synchronous = NORMAL").execute(&pool).await;
    let _ = sqlx::query("PRAGMA temp_store = MEMORY").execute(&pool).await;
    let _ = sqlx::query("PRAGMA journal_mode = DELETE").execute(&pool).await;
    let _ = sqlx::query("PRAGMA locking_mode = NORMAL").execute(&pool).await;
    let _ = sqlx::query("PRAGMA auto_vacuum = INCREMENTAL").execute(&pool).await;
    let _ = sqlx::query("PRAGMA page_size = 4096").execute(&pool).await;
    // 写并发：让 BEGIN IMMEDIATE 拿不到写锁时自动等待 5s 再报错（默认是立即 SQLITE_BUSY），
    // 配合 order update 中使用 BEGIN IMMEDIATE 事务，可消除连点保存时的并发丢明细问题。
    let _ = sqlx::query("PRAGMA busy_timeout = 5000").execute(&pool).await;
    
    // 使用 integrity_check 检测数据库损坏
    let integrity_check: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&pool)
        .await
        .unwrap_or_default();
    if integrity_check != "ok" {
        eprintln!("数据库损坏: {}", integrity_check);
        // 尝试修复常见的损坏类型
        repair_db_corruption(&pool).await;
        // 最终检查
        let final_check: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&pool)
            .await
            .unwrap_or_default();
        if final_check == "ok" {
            eprintln!("数据库修复成功");
        } else {
            eprintln!("数据库修复失败: {}", final_check);
        }
    } else {
        eprintln!("数据库完整性检查通过");
    }
    
    init_tables(&pool).await.expect("初始化数据表失败");

    // 一次性修复：重算所有耗材分摊方案的 allocated_amount 与 remaining_balance
    // 修复历史 bug（replace_remove 冲减负数未计入分摊金额）导致的数据偏差
    // allocated_amount = SUM(对应 order_supplement_item.amount，含正负)
    // remaining_balance = total_amount - allocated_amount
    let _ = sqlx::query(
        "UPDATE consumable_allocation SET allocated_amount = COALESCE((SELECT SUM(amount) FROM order_supplement_item WHERE source_order_id = consumable_allocation.source_order_id), 0), remaining_balance = total_amount - COALESCE((SELECT SUM(amount) FROM order_supplement_item WHERE source_order_id = consumable_allocation.source_order_id), 0)"
    ).execute(&pool).await;

    // 清理所有孤儿数据（有商品名称的记录保留，用于客户开单备注场景）
    let _ = sqlx::query("DELETE FROM sales_order_item WHERE (unit_price IS NULL OR quantity IS NULL OR quantity = 0 OR amount = 0) AND (product_name IS NULL OR product_name = '')").execute(&pool).await;
    let _ = sqlx::query("DELETE FROM purchase_order_item WHERE (unit_price IS NULL OR quantity IS NULL OR quantity = 0 OR amount = 0) AND (product_name IS NULL OR product_name = '')").execute(&pool).await;
    let _ = sqlx::query("DELETE FROM sales_order_item WHERE order_id NOT IN (SELECT id FROM sales_order)").execute(&pool).await;
    let _ = sqlx::query("DELETE FROM purchase_order_item WHERE order_id NOT IN (SELECT id FROM purchase_order)").execute(&pool).await;
    let _ = sqlx::query("DELETE FROM sales_order_item WHERE product_id NOT IN (SELECT id FROM product)").execute(&pool).await;
    let _ = sqlx::query("DELETE FROM purchase_order_item WHERE product_id NOT IN (SELECT id FROM product)").execute(&pool).await;
    let _ = sqlx::query("DELETE FROM sales_order WHERE id NOT IN (SELECT DISTINCT order_id FROM sales_order_item)").execute(&pool).await;
    let _ = sqlx::query("DELETE FROM purchase_order WHERE id NOT IN (SELECT DISTINCT order_id FROM purchase_order_item)").execute(&pool).await;
    let _ = sqlx::query("DELETE FROM sales_order WHERE purchaser_id NOT IN (SELECT id FROM purchaser)").execute(&pool).await;
    let _ = sqlx::query("DELETE FROM purchase_order WHERE supplier_id NOT IN (SELECT id FROM supplier)").execute(&pool).await;
    let _ = sqlx::query("DELETE FROM food_item WHERE accept_id NOT IN (SELECT id FROM food_accept)").execute(&pool).await;
    let _ = sqlx::query("DELETE FROM food_accept WHERE supplier_id NOT IN (SELECT id FROM supplier) OR purchaser_id NOT IN (SELECT id FROM purchaser)").execute(&pool).await;
    let _ = sqlx::query("DELETE FROM inventory WHERE product_id NOT IN (SELECT id FROM product) OR warehouse_id NOT IN (SELECT id FROM warehouse)").execute(&pool).await;
    let _ = sqlx::query("DELETE FROM product_unit WHERE product_id NOT IN (SELECT id FROM product)").execute(&pool).await;
    let _ = sqlx::query("DELETE FROM product_price WHERE product_id NOT IN (SELECT id FROM product)").execute(&pool).await;
    let _ = sqlx::query("VACUUM").execute(&pool).await;
    
    DB_POOL.set(pool).expect("数据库连接池已初始化");
}

pub fn pool() -> &'static SqlitePool {
    DB_POOL.get().expect("数据库连接池未初始化")
}

pub async fn init_tables(pool: &SqlitePool) -> Result<(), anyhow::Error> {
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
            audit_status TEXT NOT NULL DEFAULT 'pending',
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
            audit_status TEXT NOT NULL DEFAULT 'pending',
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
            max_purchase_price REAL DEFAULT 0,
            min_purchase_price REAL DEFAULT 0,
            category_id INTEGER REFERENCES category(id),
            audit_status TEXT NOT NULL DEFAULT 'pending',
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

    // 最高进价、最低进价（purchase_price 作为当前进价）
    let _ = sqlx::query("ALTER TABLE product ADD COLUMN max_purchase_price REAL DEFAULT 0")
        .execute(pool)
        .await;

    // 加成率（毛利率）：base_price = purchase_price * (1 + markup_rate)
    let _ = sqlx::query("ALTER TABLE product ADD COLUMN markup_rate REAL DEFAULT 0.5")
        .execute(pool)
        .await;

    // 是否启用售价自动更新（true=按加成率自动算；false=人工维护 base_price）
    let _ = sqlx::query("ALTER TABLE product ADD COLUMN auto_update_price INTEGER DEFAULT 0")
        .execute(pool)
        .await;

    // 价格变更日志表：记录每次进价/售价变更，便于审计和对账
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS product_price_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            product_id INTEGER NOT NULL,
            price_type TEXT NOT NULL,
            old_price REAL DEFAULT 0,
            new_price REAL DEFAULT 0,
            source TEXT,
            ref_id INTEGER,
            remark TEXT,
            changed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(product_id) REFERENCES product(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_price_log_product ON product_price_log(product_id, changed_at)")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE product ADD COLUMN min_purchase_price REAL DEFAULT 0")
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
            audit_status TEXT NOT NULL DEFAULT 'pending',
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

    let _ = sqlx::query("ALTER TABLE purchase_order_item ADD COLUMN ordered_quantity REAL NOT NULL DEFAULT 0")
        .execute(pool)
        .await;

    // 采购订单：是否已结算（0=未结 1=已结），幂等迁移
    let _ = sqlx::query("ALTER TABLE purchase_order ADD COLUMN is_settled INTEGER NOT NULL DEFAULT 0")
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
            customer_order_image TEXT,
            signed_order_image TEXT,
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

    // 销售订单：是否已结算（0=未结 1=已结），幂等迁移
    let _ = sqlx::query("ALTER TABLE sales_order ADD COLUMN is_settled INTEGER NOT NULL DEFAULT 0")
        .execute(pool)
        .await;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS consumable_allocation (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_order_id INTEGER NOT NULL,
            total_amount REAL NOT NULL,
            allocated_amount REAL NOT NULL DEFAULT 0,
            remaining_balance REAL NOT NULL,
            status INTEGER NOT NULL DEFAULT 0,
            remark TEXT,
            created_at TEXT NOT NULL,
            completed_at TEXT,
            source_item_ids TEXT,
            FOREIGN KEY(source_order_id) REFERENCES sales_order(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS order_supplement_item (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            target_order_id INTEGER NOT NULL,
            source_order_id INTEGER NOT NULL,
            source_remark TEXT,
            product_id INTEGER NOT NULL,
            product_name TEXT NOT NULL,
            alias1 TEXT,
            alias2 TEXT,
            spec TEXT,
            unit TEXT NOT NULL,
            unit_price REAL NOT NULL,
            quantity REAL NOT NULL DEFAULT 0,
            amount REAL NOT NULL DEFAULT 0,
            allocate_date TEXT NOT NULL,
            operation_type TEXT NOT NULL DEFAULT 'new_item',
            target_order_item_id INTEGER,
            FOREIGN KEY(target_order_id) REFERENCES sales_order(id),
            FOREIGN KEY(source_order_id) REFERENCES sales_order(id),
            FOREIGN KEY(product_id) REFERENCES product(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // 采购单据表：按供应商+日期采集多张单据图片
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS purchase_document (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            supplier_id INTEGER NOT NULL,
            supplier_name TEXT NOT NULL,
            document_date TEXT NOT NULL,
            image_url TEXT NOT NULL,
            remark TEXT,
            create_at DATETIME DEFAULT CURRENT_TIMESTAMP
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

    // 销售订单图片：客户订单图片，已验收签字图片
    let _ = sqlx::query("ALTER TABLE sales_order ADD COLUMN customer_order_image TEXT")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE sales_order ADD COLUMN signed_order_image TEXT")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE purchase_order ADD COLUMN warehouse_id INTEGER DEFAULT 0")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE purchase_order ADD COLUMN warehouse_name TEXT")
        .execute(pool)
        .await;

    // 采购订单明细级仓库：同一订单的每行商品可分别入不同仓库
    let _ = sqlx::query("ALTER TABLE purchase_order_item ADD COLUMN warehouse_id INTEGER DEFAULT 0")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE purchase_order_item ADD COLUMN warehouse_name TEXT")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE purchase_order ADD COLUMN user_id INTEGER DEFAULT 0")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE purchase_order ADD COLUMN handler_phone TEXT")
        .execute(pool)
        .await;

    // 用户表增加联系方式字段（用于采购单/销售单打印）
    let _ = sqlx::query("ALTER TABLE user_account ADD COLUMN phone TEXT")
        .execute(pool)
        .await;

    // 用户表增加行级数据权限关联字段：supplier 账号绑定供应商，purchaser 账号绑定采购单位
    // 用于"只能查看/操作自己的单据"的行级数据权限
    let _ = sqlx::query("ALTER TABLE user_account ADD COLUMN supplier_id INTEGER DEFAULT 0")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE user_account ADD COLUMN purchaser_id INTEGER DEFAULT 0")
        .execute(pool)
        .await;

    // 联系方式：销售订单主表单上"联系方式"输入框的最近一次输入值，用于导出验收单/报销单时填入 xlsx。
    // 跨设备共享，按当前登录用户更新。
    let _ = sqlx::query("ALTER TABLE user_account ADD COLUMN contact_phone TEXT")
        .execute(pool)
        .await;

    // 操作审计日志表：记录所有关键写操作（谁、何时、做了什么）
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS operation_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER DEFAULT 0,
            username TEXT DEFAULT '',
            action TEXT NOT NULL,
            target_type TEXT DEFAULT '',
            target_id TEXT DEFAULT '',
            detail TEXT DEFAULT '',
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;

    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_op_log_created ON operation_log(created_at)")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE purchase_order_item ADD COLUMN remark TEXT")
        .execute(pool)
        .await;

    // 销售单位：销售订单明细的实际单位，可能与商品基础单位不同，用于生成采购订单时区分明细
    let _ = sqlx::query("ALTER TABLE purchase_order_item ADD COLUMN sales_unit TEXT")
        .execute(pool)
        .await;

    // 销售单来源：标识该采购明细由哪张销售单生成，便于同供应商多单合并时精确重算单张销售单的贡献
    let _ = sqlx::query("ALTER TABLE purchase_order_item ADD COLUMN source_sales_order_id INTEGER DEFAULT 0")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE purchase_order ADD COLUMN source_sales_order_id INTEGER DEFAULT 0")
        .execute(pool)
        .await;

    // 乐观锁版本号：审核/反审核与并发修改防护，已有数据默认 version=1 不受影响
    let _ = sqlx::query("ALTER TABLE purchase_order ADD COLUMN version INTEGER DEFAULT 1")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE sales_order ADD COLUMN version INTEGER DEFAULT 1")
        .execute(pool)
        .await;

    // 销售订单"供应商名称"与"供货车牌号"：验收单/报销单导出时使用，替代原代码中硬编码的"湖南食全味美..."和"湘A·NY360"。
    // 前端在新建销售订单时默认填入占位文本，保存时写入主表；编辑/回显时从主表读出。
    let _ = sqlx::query("ALTER TABLE sales_order ADD COLUMN supplier_company TEXT")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE sales_order ADD COLUMN truck_plate TEXT")
        .execute(pool)
        .await;

    // 销售订单"曾生成过的采购订单明细 id 快照"（JSON 数组形式存文本）：
    // 解决 force=true 重新生成采购订单时，to_consume 池因 source=本单id 严格过滤而漏掉
    // "用户已删除的明细"和"其他销售单贡献的同 (P,U) 明细"造成的重复插入 BUG。
    // 每次 force 生成时按快照判定：
    //   - 快照中 id 仍在 PO 中 → 按主键 UPDATE 同步
    //   - 快照中 id 已从 PO 消失（被用户删除）→ 补回 INSERT（source=本单）
    //   - 快照之外的 PO 明细（其他销售单贡献）→ 不动
    //   - 销售单当前明细不在快照中 → 新出现的明细，INSERT（source=本单）
    let _ = sqlx::query("ALTER TABLE sales_order ADD COLUMN generated_purchase_item_ids TEXT")
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

    let _ = sqlx::query("ALTER TABLE sales_order_item ADD COLUMN pre_sale_quantity REAL NOT NULL DEFAULT 0")
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

    // 基础数据审核状态迁移：为旧库补充 audit_status 字段。
    // pending=待审核，confirmed=已审核；存量数据（本次 ALTER 成功）统一视为已审核，不影响现有业务；
    // 后续新增/修改的记录走 pending 待审核流程，由超级管理员审核。
    for table in ["supplier", "purchaser", "product", "warehouse"] {
        let sql = format!("ALTER TABLE {} ADD COLUMN audit_status TEXT NOT NULL DEFAULT 'pending'", table);
        if sqlx::query(AssertSqlSafe(sql.as_str())).execute(pool).await.is_ok() {
            let update_sql = format!("UPDATE {} SET audit_status = 'confirmed'", table);
            let _ = sqlx::query(AssertSqlSafe(update_sql.as_str())).execute(pool).await;
        }
    }

    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_sales_order_purchaser_id ON sales_order(purchaser_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_sales_order_order_no ON sales_order(order_no)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_sales_order_order_date ON sales_order(order_date)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_sales_order_item_order_id ON sales_order_item(order_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_sales_order_item_product_id ON sales_order_item(product_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_supplement_target_order_id ON order_supplement_item(target_order_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_supplement_source_order_id ON order_supplement_item(source_order_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_consumable_allocation_source ON consumable_allocation(source_order_id)").execute(pool).await;
    
    let _ = sqlx::query("ALTER TABLE order_supplement_item ADD COLUMN operation_type TEXT NOT NULL DEFAULT 'new_item'").execute(pool).await;
    let _ = sqlx::query("ALTER TABLE order_supplement_item ADD COLUMN target_order_item_id INTEGER").execute(pool).await;
    // 分摊细化到明细：新增 source_item_ids 列（存储勾选的来源明细 id，逗号分隔）。
    // 首次迁移（列新增成功）时清空旧的整单级分摊数据，按新模型重建。
    if sqlx::query("ALTER TABLE consumable_allocation ADD COLUMN source_item_ids TEXT").execute(pool).await.is_ok() {
        let _ = sqlx::query("DELETE FROM order_supplement_item").execute(pool).await;
        let _ = sqlx::query("DELETE FROM consumable_allocation").execute(pool).await;
    }
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_purchase_order_supplier_id ON purchase_order(supplier_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_purchase_order_order_no ON purchase_order(order_no)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_purchase_order_item_order_id ON purchase_order_item(order_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_purchase_order_item_product_id ON purchase_order_item(product_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_product_category_id ON product(category_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_product_name ON product(name)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_inventory_product_id ON inventory(product_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_inventory_warehouse_id ON inventory(warehouse_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_food_accept_supplier_id ON food_accept(supplier_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_food_accept_purchaser_id ON food_accept(purchaser_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_food_item_accept_id ON food_item(accept_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_product_unit_product_id ON product_unit(product_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_product_price_product_id ON product_price(product_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_supplier_category_id ON supplier(category_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_purchaser_category_id ON purchaser(category_id)").execute(pool).await;

    Ok(())
}