//! 一次性脚本: 给 product.image_url 填入 picsum 占位图,用于本地测试商品图片点击弹窗
//!
//! 用法:  cargo run --example seed_mock_images
//!
//! 注意: 运行前请确保主服务 food_accept_single.exe 没有运行(避免 DB 锁)。
//! 数据源: https://picsum.photos (免 key 永久可用,seed 决定图片内容)

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;
use std::time::Duration;

const SEEDS: &[&str] = &[
    "apple", "banana", "cabbage", "pork", "chicken",
    "egg", "rice", "wheat", "tomato", "potato",
    "carrot", "fish", "beef", "milk", "tofu",
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = std::env::current_dir()?.join("food_accept_v3.db");
    if !db_path.exists() {
        eprintln!("[ERROR] 数据库不存在: {}", db_path.display());
        eprintln!("        请先启动一次主服务以初始化数据库");
        std::process::exit(1);
    }

    println!("[INFO] 连接数据库: {}", db_path.display());
    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.display()))?
        .create_if_missing(false)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await?;

    // 校验 product.image_url 列存在
    let cols: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(product)")
            .fetch_all(&pool)
            .await?;
    let has_image_url = cols.iter().any(|c| c.1 == "image_url");
    if !has_image_url {
        eprintln!("[ERROR] product 表缺少 image_url 列");
        std::process::exit(1);
    }

    // 1) 填充无图商品(随机图)
    let r = sqlx::query(
        "UPDATE product SET image_url = 'https://picsum.photos/seed/product' || id || '/600/600' \
         WHERE image_url IS NULL OR image_url = ''",
    )
    .execute(&pool)
    .await?;
    println!("[OK] 无图商品填充完成,影响行数: {}", r.rows_affected());

    // 2) 给前 N 条商品替换为种子图(让前几行看起来像食材)
    let ids: Vec<(i64,)> =
        sqlx::query_as("SELECT id FROM product ORDER BY id LIMIT ?")
            .bind(SEEDS.len() as i64)
            .fetch_all(&pool)
            .await?;
    for (i, (id,)) in ids.iter().enumerate() {
        let url = format!("https://picsum.photos/seed/{}/600/600", SEEDS[i]);
        sqlx::query("UPDATE product SET image_url = ? WHERE id = ?")
            .bind(&url)
            .bind(*id)
            .execute(&pool)
            .await?;
    }
    println!("[OK] 前 {} 条商品已替换为种子图", ids.len());

    // 3) 校验输出
    let sample: Vec<(i64, String, Option<String>)> = sqlx::query_as(
        "SELECT id, name, image_url FROM product \
         WHERE image_url IS NOT NULL AND image_url != '' \
         ORDER BY id LIMIT 12",
    )
    .fetch_all(&pool)
    .await?;
    println!("\n[INFO] 已设置图片的商品样例(显示 {} 条):", sample.len());
    for (id, name, url) in &sample {
        println!("  id={:<3} {:<16} {}", id, name, url.as_deref().unwrap_or(""));
    }

    let (total,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM product WHERE image_url IS NOT NULL AND image_url != ''",
    )
    .fetch_one(&pool)
    .await?;
    println!("\n[INFO] 当前带图商品总数: {}", total);

    pool.close().await;
    println!("\n[DONE] 重启服务后,进入商品管理页点击任意带图缩略图即可看到弹窗。");
    Ok(())
}
