use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::AssertSqlSafe;
use sqlx::SqlitePool;
use std::sync::OnceLock;

pub static DB_POOL: OnceLock<SqlitePool> = OnceLock::new();

/// 库存流水重放 SQL：从已审核采购单（confirmed）与已验收/已结算销售单（accepted/settled）
/// 全量重建 stock_movement。balance_after 用窗口函数按商品分区累计（从 0 开始）。
/// 供 v10 迁移与「重新生成台账」接口共用，保证口径一致。
pub const STOCK_MOVEMENT_REPLAY_SQL: &str = r#"
                WITH all_movements AS (
                    SELECT
                        poi.product_id AS product_id,
                        'in' AS direction,
                        CASE WHEN poi.base_quantity > 0 THEN poi.base_quantity
                             ELSE poi.quantity * COALESCE(
                                 (SELECT pu.ratio FROM product_unit pu
                                  WHERE pu.product_id = poi.product_id AND pu.unit_name = poi.unit), 1)
                        END AS eff_base,
                        poi.quantity AS orig_quantity,
                        poi.unit AS orig_unit,
                        po.order_date AS order_date,
                        po.id AS order_id,
                        poi.id AS item_seq
                    FROM purchase_order_item poi
                    JOIN purchase_order po ON po.id = poi.order_id
                    WHERE po.status = 'confirmed'
                    UNION ALL
                    SELECT
                        soi.product_id,
                        'out',
                        CASE WHEN soi.base_quantity > 0 THEN soi.base_quantity
                             ELSE soi.quantity * COALESCE(
                                 (SELECT pu.ratio FROM product_unit pu
                                  WHERE pu.product_id = soi.product_id AND pu.unit_name = soi.unit), 1)
                        END,
                        soi.quantity,
                        soi.unit,
                        so.order_date,
                        so.id,
                        soi.id
                    FROM sales_order_item soi
                    JOIN sales_order so ON so.id = soi.order_id
                    -- v10 口径：仅已验收/已结算的销售单视为已出库
                    WHERE so.status IN ('accepted','settled')
                )
                INSERT INTO stock_movement (
                    product_id, warehouse_id, direction, movement_type,
                    base_quantity, orig_quantity, orig_unit, balance_after,
                    ref_type, ref_id, remark, order_date, created_at
                )
                SELECT
                    am.product_id,
                    1,
                    am.direction,
                    CASE WHEN am.direction='in' THEN 'purchase' ELSE 'sales' END AS movement_type,
                    am.eff_base,
                    am.orig_quantity,
                    am.orig_unit,
                    SUM(CASE WHEN am.direction='in' THEN am.eff_base ELSE -am.eff_base END)
                        OVER (PARTITION BY am.product_id
                              ORDER BY am.order_date, am.order_id, am.item_seq)
                    AS balance_after,
                    CASE WHEN am.direction='in' THEN 'purchase' ELSE 'sales' END AS ref_type,
                    am.order_id AS ref_id,
                    CASE WHEN am.direction='in' THEN '历史补录-采购入库'
                         ELSE '历史补录-销售出库' END AS remark,
                    am.order_date AS order_date,
                    am.order_date AS created_at
                FROM all_movements am
                ORDER BY am.order_date, am.order_id, am.item_seq
                "#;

/// 按 stock_movement 全部流水带符号求和重算 inventory（不依赖 balance_after/MAX(id)，口径最稳）。
/// 供 v10 迁移与「重新生成台账」接口共用。
pub const INVENTORY_RECALC_SQL: &str = r#"
                INSERT INTO inventory (product_id, warehouse_id, quantity, last_update)
                SELECT product_id, 1,
                       SUM(CASE WHEN direction='in' THEN base_quantity ELSE -base_quantity END),
                       datetime('now','localtime')
                FROM stock_movement GROUP BY product_id
                ON CONFLICT(product_id, warehouse_id) DO UPDATE
                    SET quantity = excluded.quantity, last_update = datetime('now','localtime')
                "#;

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

    // 先把连接池注册到 OnceLock，再执行 init_tables / 数据修复等可能通过 crate::db::pool()
    // 取池的代码。放在此处确保后续 init_tables 中的「user_version 修复段」以及 init_db
    // 中的「end_date 重建」都能拿到池。
    let _ = DB_POOL.set(pool.clone());

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

    // 一次性修复：清理 legacy_migration 记录后，重算剩余记录的 end_date 连续性。
    // init_tables 阶段不能调 rebuild_price_schedule_end_dates（那时 DB_POOL 未注册），
    // 所以放在这里：init_tables 已把 legacy_migration 数据删除（user_version=5），此处统一重建。
    let needs_rebuild: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
    if needs_rebuild == 5 {
        let affected: Vec<(i64, String)> = sqlx::query_as(
            "SELECT DISTINCT product_id, price_type FROM product_price_schedule"
        )
        .fetch_all(&pool)
        .await
        .unwrap_or_default();
        for (pid, pt) in affected {
            let _ = crate::rebuild_price_schedule_end_dates(pid, &pt).await;
        }
        // 重建完成后升到 6，下次启动不再跑
        let _ = sqlx::query("PRAGMA user_version = 6").execute(&pool).await;
    }

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

    // 连接池在 init_db 顶部已注册，此处不再重复 set。
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
            create_at DATETIME DEFAULT (datetime('now','localtime')),
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
            create_at DATETIME DEFAULT (datetime('now','localtime'))
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
            create_at DATETIME DEFAULT (datetime('now','localtime'))
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
            create_at DATETIME DEFAULT (datetime('now','localtime')),
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

    // 保质期（商品自身属性，如 "7天"、"180天"）
    let _ = sqlx::query("ALTER TABLE product ADD COLUMN shelf_life TEXT")
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
            changed_at DATETIME DEFAULT (datetime('now','localtime')),
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

    // 定点供应商档案：定点标记/编号、负责人、资质证照、合同信息
    let _ = sqlx::query("ALTER TABLE supplier ADD COLUMN is_designated INTEGER NOT NULL DEFAULT 0")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE supplier ADD COLUMN designated_no TEXT")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE supplier ADD COLUMN legal_person TEXT")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE supplier ADD COLUMN credit_code TEXT")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE supplier ADD COLUMN license_no TEXT")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE supplier ADD COLUMN license_expire TEXT")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE supplier ADD COLUMN contract_no TEXT")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE supplier ADD COLUMN contract_start TEXT")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE supplier ADD COLUMN contract_end TEXT")
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
            create_at DATETIME DEFAULT (datetime('now','localtime')),
            update_at DATETIME DEFAULT (datetime('now','localtime'))
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
            last_update DATETIME DEFAULT (datetime('now','localtime')),
            FOREIGN KEY(product_id) REFERENCES product(id),
            FOREIGN KEY(warehouse_id) REFERENCES warehouse(id),
            UNIQUE(product_id, warehouse_id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // SQLite 不支持 `ADD COLUMN IF NOT EXISTS` 语法，需要先检查列是否存在再 ALTER
    let inv_has_wh: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('inventory') WHERE name='warehouse_id'"
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    if inv_has_wh == 0 {
        let _ = sqlx::query("ALTER TABLE inventory ADD COLUMN warehouse_id INTEGER DEFAULT 1")
            .execute(pool)
            .await;
        // 旧表迁移：把所有现存行的 warehouse_id 显式置为 1（ALTER 加列时默认值已生效，
        // 但 SQLite 某些版本对已有行的默认值处理不一致，显式 UPDATE 保险）
        let _ = sqlx::query("UPDATE inventory SET warehouse_id = 1 WHERE warehouse_id IS NULL")
            .execute(pool)
            .await;
    }
    // 为 (product_id, warehouse_id) 建唯一索引：审核流程中的 UPSERT
    // `ON CONFLICT(product_id, warehouse_id) DO UPDATE` 需要唯一约束才能生效，
    // 旧表 CREATE TABLE 里的 UNIQUE 约束只对新表生效，老库要靠这个索引补齐。
    // 幂等：IF NOT EXISTS。若历史存在重复 (product_id, warehouse_id) 行会失败，
    // 先做一次去重（保留 id 最小的一行）。
    let _ = sqlx::query(
        "DELETE FROM inventory WHERE id NOT IN (
            SELECT MIN(id) FROM inventory GROUP BY product_id, warehouse_id
        )"
    ).execute(pool).await;
    let _ = sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_inventory_product_warehouse ON inventory(product_id, warehouse_id)"
    ).execute(pool).await;

    sqlx::query(
        "INSERT OR IGNORE INTO warehouse (id, name, code, status) VALUES (1, '默认仓库', 'WH001', 1)"
    )
    .execute(pool)
    .await?;

    // 库存流水表（Append-Only）：订单审核时写入一条流水，并同步更新 inventory.quantity
    // 设计要点：
    //   - 只增不改：反审核写冲销流水（direction 相反、base_quantity 取负），不删除原流水
    //   - base_quantity 统一基础单位口径；orig_quantity/orig_unit 保留原始下单单位以便展示
    //   - balance_after 为快照：写入时算好的当前余额，台账直接 SELECT，无需窗口函数反算
    //   - ref_type/ref_id 关联到 purchase_order / sales_order 等
    //   - movement_type: purchase(采购入库) / sales(销售出库) / adjust_in / adjust_out / transfer_in / transfer_out / opening
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS stock_movement (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            product_id INTEGER NOT NULL,
            warehouse_id INTEGER NOT NULL DEFAULT 1,
            direction TEXT NOT NULL CHECK(direction IN ('in','out')),
            movement_type TEXT NOT NULL,
            base_quantity REAL NOT NULL,
            orig_quantity REAL NOT NULL DEFAULT 0,
            orig_unit TEXT,
            balance_after REAL NOT NULL,
            ref_type TEXT,
            ref_id INTEGER,
            remark TEXT,
            order_date TEXT,
            created_at DATETIME DEFAULT (datetime('now','localtime')),
            FOREIGN KEY(product_id) REFERENCES product(id),
            FOREIGN KEY(warehouse_id) REFERENCES warehouse(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_sm_product ON stock_movement(product_id, created_at)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_sm_ref ON stock_movement(ref_type, ref_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_sm_type ON stock_movement(movement_type, created_at)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_sm_warehouse ON stock_movement(warehouse_id, product_id, created_at)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_sm_order_date ON stock_movement(product_id, order_date)").execute(pool).await;

    // 出入库记录（数量保管账）账本快照列：append-only 流水在写入时冗余业务溯源信息，免 join 且不怕主数据后改
    let _ = sqlx::query("ALTER TABLE stock_movement ADD COLUMN ref_item_id INTEGER").execute(pool).await;
    let _ = sqlx::query("ALTER TABLE stock_movement ADD COLUMN production_date TEXT").execute(pool).await;
    let _ = sqlx::query("ALTER TABLE stock_movement ADD COLUMN batch_no TEXT").execute(pool).await;
    let _ = sqlx::query("ALTER TABLE stock_movement ADD COLUMN ref_no TEXT").execute(pool).await;
    let _ = sqlx::query("ALTER TABLE stock_movement ADD COLUMN party_name TEXT").execute(pool).await;
    // 快照版本标记：新代码写入的流水恒为 1（写入即携带真实明细/批次快照）；历史行为 NULL，由下方迁移回填。
    // 用标记而非时间分界，可对任何环境（含未来全新部署的客户库）幂等重算，且绝不覆盖新行的时点快照。
    let _ = sqlx::query("ALTER TABLE stock_movement ADD COLUMN snapshot_version INTEGER").execute(pool).await;

    // 历史流水一次性回填（幂等：只补 NULL/空）。单据号/往来单位为单据级，直接关联回填
    let _ = sqlx::query(
        "UPDATE stock_movement SET
            ref_no = COALESCE((SELECT po.order_no FROM purchase_order po WHERE po.id = stock_movement.ref_id), ref_no),
            party_name = COALESCE((SELECT s.name FROM purchase_order po JOIN supplier s ON po.supplier_id = s.id WHERE po.id = stock_movement.ref_id), party_name)
         WHERE movement_type = 'purchase' AND (ref_no IS NULL OR ref_no = '')"
    ).execute(pool).await;
    let _ = sqlx::query(
        "UPDATE stock_movement SET
            ref_no = COALESCE((SELECT so.order_no FROM sales_order so WHERE so.id = stock_movement.ref_id), ref_no),
            party_name = COALESCE((SELECT pu.name FROM sales_order so JOIN purchaser pu ON so.purchaser_id = pu.id WHERE so.id = stock_movement.ref_id), party_name)
         WHERE movement_type = 'sales' AND (ref_no IS NULL OR ref_no = '')"
    ).execute(pool).await;

    // 明细级回填：一轮审核按明细顺序逐行写流水（同单同商品可能有多行，如补采/分行验收），
    // 故同方向流水按 id 的轮次序号，对同单同商品明细按 id 顺序取模配对（反审核/重审的多轮同样成立）；
    // 冲销方向独立编号配对。配对失败（明细已被编辑删除）保持 NULL。
    let _ = sqlx::query(
        "UPDATE stock_movement
         SET ref_item_id = (
            SELECT m.id FROM (
                SELECT poi.id, poi.order_id, poi.product_id,
                       (SELECT COUNT(*) FROM purchase_order_item x
                         WHERE x.order_id = poi.order_id AND x.product_id = poi.product_id AND x.id <= poi.id) AS irn
                FROM purchase_order_item poi
            ) m
            WHERE m.order_id = stock_movement.ref_id AND m.product_id = stock_movement.product_id
              AND m.irn = (
                  ((SELECT COUNT(*) FROM stock_movement y
                     WHERE y.ref_id = stock_movement.ref_id AND y.product_id = stock_movement.product_id
                       AND y.movement_type = stock_movement.movement_type AND y.direction = stock_movement.direction
                       AND y.id <= stock_movement.id) - 1)
                  %
                  (SELECT COUNT(*) FROM purchase_order_item z
                     WHERE z.order_id = stock_movement.ref_id AND z.product_id = stock_movement.product_id)
              ) + 1
         )
         WHERE movement_type = 'purchase' AND snapshot_version IS NULL"
    ).execute(pool).await;
    let _ = sqlx::query(
        "UPDATE stock_movement
         SET ref_item_id = (
            SELECT m.id FROM (
                SELECT soi.id, soi.order_id, soi.product_id,
                       (SELECT COUNT(*) FROM sales_order_item x
                         WHERE x.order_id = soi.order_id AND x.product_id = soi.product_id AND x.id <= soi.id) AS irn
                FROM sales_order_item soi
            ) m
            WHERE m.order_id = stock_movement.ref_id AND m.product_id = stock_movement.product_id
              AND m.irn = (
                  ((SELECT COUNT(*) FROM stock_movement y
                     WHERE y.ref_id = stock_movement.ref_id AND y.product_id = stock_movement.product_id
                       AND y.movement_type = stock_movement.movement_type AND y.direction = stock_movement.direction
                       AND y.id <= stock_movement.id) - 1)
                  %
                  (SELECT COUNT(*) FROM sales_order_item z
                     WHERE z.order_id = stock_movement.ref_id AND z.product_id = stock_movement.product_id)
              ) + 1
         )
         WHERE movement_type = 'sales' AND snapshot_version IS NULL"
    ).execute(pool).await;
    // 批次按配对后的 ref_item_id 回填（历史流水未存批次，取明细当前值）
    let _ = sqlx::query(
        "UPDATE stock_movement SET
            production_date = (SELECT poi.production_date FROM purchase_order_item poi WHERE poi.id = stock_movement.ref_item_id),
            batch_no = (SELECT poi.batch_no FROM purchase_order_item poi WHERE poi.id = stock_movement.ref_item_id)
         WHERE movement_type = 'purchase' AND snapshot_version IS NULL"
    ).execute(pool).await;
    let _ = sqlx::query(
        "UPDATE stock_movement SET
            production_date = (SELECT soi.production_date FROM sales_order_item soi WHERE soi.id = stock_movement.ref_item_id),
            batch_no = (SELECT soi.batch_no FROM sales_order_item soi WHERE soi.id = stock_movement.ref_item_id)
         WHERE movement_type = 'sales' AND snapshot_version IS NULL"
    ).execute(pool).await;
    // 回填完成，历史行打上版本标记（配对失败的也标记，避免每次启动重复尝试）
    let _ = sqlx::query("UPDATE stock_movement SET snapshot_version = 1 WHERE snapshot_version IS NULL").execute(pool).await;

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
            create_at DATETIME DEFAULT (datetime('now','localtime')),
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

    // 食材溯源：生产日期/批号是"每一批到货实物"的属性，同一商品每批不同，
    // 放采购明细表（进货查验记录），不放 product（会互相覆盖）也不放订单主表（粒度不对）
    let _ = sqlx::query("ALTER TABLE purchase_order_item ADD COLUMN production_date TEXT")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE purchase_order_item ADD COLUMN batch_no TEXT")
        .execute(pool)
        .await;

    // 验收单导出快照：确认验收时按 FIFO 取当前库存最早批次的 生产日期/批号 写入销售明细，
    // 导出验收单/报销单直接读快照，不再实时计算
    let _ = sqlx::query("ALTER TABLE sales_order_item ADD COLUMN production_date TEXT")
        .execute(pool)
        .await;

    let _ = sqlx::query("ALTER TABLE sales_order_item ADD COLUMN batch_no TEXT")
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
            create_at DATETIME DEFAULT (datetime('now','localtime')),
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
            create_at DATETIME DEFAULT (datetime('now','localtime'))
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
            created_at DATETIME DEFAULT (datetime('now','localtime'))
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
            create_at DATETIME DEFAULT (datetime('now','localtime')),
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
            create_at DATETIME DEFAULT (datetime('now','localtime')),
            update_at DATETIME DEFAULT (datetime('now','localtime'))
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
            create_at DATETIME DEFAULT (datetime('now','localtime'))
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
            create_at DATETIME DEFAULT (datetime('now','localtime')),
            update_at DATETIME DEFAULT (datetime('now','localtime'))
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

    // 政采平台价 / 超市比价 时间段表
    // 解决原 product_price 表 UNIQUE(product_id, price_type) 只能存"最新价"的问题：
    // 现在按 (product_id, price_type, effective_date) 维护多条历史记录，
    // 任意 order_date 可通过查找当日生效的记录得到当时使用的价格，
    // 支持补录历史销售单时按 order_date 自动匹配对应时段价。
    //
    // 时段连续性约束：
    //   - 同一 (product_id, price_type) 下，多条记录的 effective_date 不可重复
    //   - 同一 (product_id, price_type) 下，后一条 effective_date 之前的所有日期都按前一条价格生效
    //     （end_date 由下一条记录的 effective_date 推得；最后一条 end_date = NULL 表示至今有效）
    //
    // 字段说明：
    //   price_type: gov_procurement / supermarket_1 / supermarket_2 / supermarket_3 / ai_realtime
    //   effective_date: 该价格从该日期（含）开始生效；时间戳存 DATE 字符串 'YYYY-MM-DD'
    //   source: 录入来源（manual / excel_import / ...）
    //   remark: 备注
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS product_price_schedule (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            product_id INTEGER NOT NULL,
            price_type TEXT NOT NULL,
            price REAL NOT NULL DEFAULT 0,
            effective_date TEXT NOT NULL,
            end_date TEXT,
            source TEXT,
            remark TEXT,
            create_at DATETIME DEFAULT (datetime('now','localtime')),
            FOREIGN KEY(product_id) REFERENCES product(id),
            UNIQUE(product_id, price_type, effective_date)
        )
        "#,
    )
    .execute(pool)
    .await?;

    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_pps_product_type ON product_price_schedule(product_id, price_type, effective_date)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_pps_effective ON product_price_schedule(effective_date)").execute(pool).await;

    // 价格策略表初始化为空表：不再从 product_price 自动迁移。
    // 由用户按实际情况在【价格策略】页面手动录入各商品各时段的价格。
    // 一次性清理：清空早期版本自动迁移/试录入的全部记录，从零开始。
    // 幂等：用 PRAGMA user_version >= 5 作为闸门。
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    if version < 5 {
        let _ = sqlx::query("DELETE FROM product_price_schedule")
            .execute(pool)
            .await;
        let _ = sqlx::query("PRAGMA user_version = 5").execute(pool).await;
    }

    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_supplier_category_id ON supplier(category_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_purchaser_category_id ON purchaser(category_id)").execute(pool).await;

    // 一次性补录：把历史已生效的采购/销售订单明细回填到 stock_movement。
    // user_version >= 9 作为闸门，仅运行一次。
    // 版本历史：
    //   - v7：初版补录 SQL 引用了 inventory.warehouse_id 列，但旧库无此列导致 SQL 静默失败；
    //         错误被 .ok() 吞掉，user_version 仍被升到 7，stock_movement 实际为空。
    //   - v8：修复 inventory 表 warehouse_id 列后重跑补录，但只匹配 status IN
    //         ('confirmed','received')，漏掉销售状态机后续的 accepted(已验收)/settled(已结算)，
    //         且未处理 base_quantity=0 的历史明细，台账仍严重不完整。
    //   - v9：①状态范围覆盖采购 confirmed、销售 confirmed/accepted/settled；
    //         ②base_quantity 缺失时用 quantity × product_unit.ratio 现算；
    //         ③整体重算（清除旧补录流水后重新全量生成），保证余额时序自洽。
    //   - v10：业务口径调整——销售出库时点从「审核」后移到「确认验收」，
    //          历史补录销售单状态范围收窄为 accepted/settled（confirmed 不再补出库），
    //          inventory 按全部流水带符号求和重算。
    // 补录策略：
    //   - balance_after = SUM(带符号有效数量) OVER (PARTITION BY product_id ORDER BY 日期)
    //     即从 0 开始的累计余额；最后一条 = 该商品历史净流入量
    //   - 补录完成后，UPSERT inventory.quantity = 各 product 的最新 balance_after
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    if version < 9 {
        // 保护：若已存在「非历史补录」的真实流水（用户已用新版审核过订单），
        // 不能整体清除重算（会破坏真实流水的时序），跳过，由更精细的迁移处理。
        let real_cnt: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM stock_movement WHERE remark NOT LIKE '历史补录%'"
        )
        .fetch_one(pool)
        .await
        .unwrap_or(0);
        if real_cnt == 0 {
            // 清除 v7/v8 遗留的补录流水与库存，整体重算
            let _ = sqlx::query("DELETE FROM stock_movement WHERE remark LIKE '历史补录%'")
                .execute(pool).await;
            let _ = sqlx::query("DELETE FROM inventory").execute(pool).await;

            // 全量补录：eff_base 为有效基础单位数量（base_quantity 优先，缺失则现算）
            let _ = sqlx::query(
                r#"
                WITH all_movements AS (
                    SELECT
                        poi.product_id AS product_id,
                        'in' AS direction,
                        CASE WHEN poi.base_quantity > 0 THEN poi.base_quantity
                             ELSE poi.quantity * COALESCE(
                                 (SELECT pu.ratio FROM product_unit pu
                                  WHERE pu.product_id = poi.product_id AND pu.unit_name = poi.unit), 1)
                        END AS eff_base,
                        poi.quantity AS orig_quantity,
                        poi.unit AS orig_unit,
                        po.order_date AS order_date,
                        po.id AS order_id,
                        poi.id AS item_seq
                    FROM purchase_order_item poi
                    JOIN purchase_order po ON po.id = poi.order_id
                    WHERE po.status = 'confirmed'
                    UNION ALL
                    SELECT
                        soi.product_id,
                        'out',
                        CASE WHEN soi.base_quantity > 0 THEN soi.base_quantity
                             ELSE soi.quantity * COALESCE(
                                 (SELECT pu.ratio FROM product_unit pu
                                  WHERE pu.product_id = soi.product_id AND pu.unit_name = soi.unit), 1)
                        END,
                        soi.quantity,
                        soi.unit,
                        so.order_date,
                        so.id,
                        soi.id
                    FROM sales_order_item soi
                    JOIN sales_order so ON so.id = soi.order_id
                    WHERE so.status IN ('confirmed','accepted','settled')
                )
                INSERT INTO stock_movement (
                    product_id, warehouse_id, direction, movement_type,
                    base_quantity, orig_quantity, orig_unit, balance_after,
                    ref_type, ref_id, remark, created_at
                )
                SELECT
                    am.product_id,
                    1,
                    am.direction,
                    CASE WHEN am.direction='in' THEN 'purchase' ELSE 'sales' END AS movement_type,
                    am.eff_base,
                    am.orig_quantity,
                    am.orig_unit,
                    -- balance_after = 截至本条累计的带符号有效数量（从 0 开始）
                    SUM(CASE WHEN am.direction='in' THEN am.eff_base ELSE -am.eff_base END)
                        OVER (PARTITION BY am.product_id
                              ORDER BY am.order_date, am.order_id, am.item_seq)
                    AS balance_after,
                    CASE WHEN am.direction='in' THEN 'purchase' ELSE 'sales' END AS ref_type,
                    am.order_id AS ref_id,
                    CASE WHEN am.direction='in' THEN '历史补录-采购入库'
                         ELSE '历史补录-销售出库' END AS remark,
                    am.order_date AS created_at
                FROM all_movements am
                ORDER BY am.order_date, am.order_id, am.item_seq
                "#,
            )
            .execute(pool)
            .await;

            // 用每个 product 的最新 balance_after UPSERT 到 inventory.quantity，
            // 后续审核流程从正确的初始余额继续累加
            let _ = sqlx::query(
                r#"
                INSERT INTO inventory (product_id, warehouse_id, quantity, last_update)
                SELECT product_id, 1, balance_after, datetime('now','localtime')
                FROM stock_movement
                WHERE id IN (SELECT MAX(id) FROM stock_movement GROUP BY product_id)
                ON CONFLICT(product_id, warehouse_id) DO UPDATE
                    SET quantity = excluded.quantity, last_update = datetime('now','localtime')
                "#,
            )
            .execute(pool)
            .await;
        }
        let _ = sqlx::query("PRAGMA user_version = 9").execute(pool).await;
    }

    // v10：销售出库口径从「审核」改为「确认验收」，重算历史补录。
    if version < 10 {
        // 与 v9 相同的保护：存在真实运行时流水时不整体重算，避免补录/真实流水重复。
        let real_cnt: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM stock_movement WHERE remark NOT LIKE '历史补录%'"
        )
        .fetch_one(pool)
        .await
        .unwrap_or(0);
        if real_cnt == 0 {
            let _ = sqlx::query("DELETE FROM stock_movement WHERE remark LIKE '历史补录%'")
                .execute(pool).await;
            let _ = sqlx::query("DELETE FROM inventory").execute(pool).await;

            let _ = sqlx::query(STOCK_MOVEMENT_REPLAY_SQL)
            .execute(pool)
            .await;

            // inventory 按全部流水带符号求和重算（不依赖 balance_after/MAX(id)，口径最稳）
            let _ = sqlx::query(INVENTORY_RECALC_SQL)
            .execute(pool)
            .await;
        }
        let _ = sqlx::query("PRAGMA user_version = 10").execute(pool).await;
    }

    // v11：历史时间戳 UTC → 本地时间（北京时间）一次性校正。
    // 此前所有 DEFAULT CURRENT_TIMESTAMP / CURRENT_TIMESTAMP 写入均为 UTC（慢 8 小时）；
    // 代码已统一切换为 datetime('now','localtime')，此处仅校正存量数据，幂等由 user_version 闸门保证。
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    if version < 11 {
        let fixes: [(&str, &str); 20] = [
            ("category", "create_at"),
            ("supplier", "create_at"),
            ("purchaser", "create_at"),
            ("product", "create_at"),
            ("product_price_log", "changed_at"),
            ("warehouse", "create_at"),
            ("warehouse", "update_at"),
            ("inventory", "last_update"),
            ("stock_movement", "created_at"),
            ("purchase_order", "create_at"),
            ("sales_order", "create_at"),
            ("purchase_document", "create_at"),
            ("operation_log", "created_at"),
            ("food_accept", "create_at"),
            ("system_config", "create_at"),
            ("system_config", "update_at"),
            ("backup_record", "create_at"),
            ("user_account", "create_at"),
            ("user_account", "update_at"),
            ("user_account", "last_login_time"),
        ];
        for (table, col) in fixes {
            let sql = format!(
                "UPDATE {} SET {} = datetime({}, '+8 hours') WHERE {} IS NOT NULL",
                table, col, col, col
            );
            let _ = sqlx::query(AssertSqlSafe(sql)).execute(pool).await;
        }
        let _ = sqlx::query("PRAGMA user_version = 11").execute(pool).await;
    }

    // v12：stock_movement 增加 order_date（单据业务日期）快照列。
    // 时间归属口径：收发存统计按单据日期（定量），而非审核/验收的实际操作时间（可能延后 N 天）。
    // 冲销流水同样归属原单据日期，保证同月审核又反审核时净影响为 0。
    if version < 12 {
        let _ = sqlx::query("ALTER TABLE stock_movement ADD COLUMN order_date TEXT").execute(pool).await;
        // 按 movement_type 从对应订单表回填；找不到订单（历史补录等极端情况）时退回 DATE(created_at)
        let _ = sqlx::query(
            "UPDATE stock_movement SET order_date = COALESCE(
                CASE WHEN movement_type='purchase' THEN (SELECT order_date FROM purchase_order WHERE id=stock_movement.ref_id) END,
                CASE WHEN movement_type='sales' THEN (SELECT order_date FROM sales_order WHERE id=stock_movement.ref_id) END,
                DATE(created_at))
             WHERE order_date IS NULL"
        ).execute(pool).await;
        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_sm_order_date ON stock_movement(product_id, order_date)").execute(pool).await;
        let _ = sqlx::query("PRAGMA user_version = 12").execute(pool).await;
    }

    Ok(())
}