use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Row;
use std::collections::{HashMap, HashSet};

#[tokio::main]
async fn main() {
    let pool = SqlitePoolOptions::new().connect("sqlite:food_accept_v3.db").await.unwrap();

    let src_ids: Vec<i64> = sqlx::query_scalar::<_,i64>("SELECT DISTINCT source_order_id FROM consumable_allocation").fetch_all(&pool).await.unwrap();
    let src: HashSet<i64> = src_ids.into_iter().collect();

    let mut m: HashMap<i64,(String,f64,f64)> = HashMap::new();

    // real (all)
    let real = sqlx::query("SELECT so.purchaser_id, p.name, SUM(soi.amount) a FROM sales_order_item soi JOIN sales_order so ON soi.order_id=so.id JOIN purchaser p ON so.purchaser_id=p.id GROUP BY so.purchaser_id").fetch_all(&pool).await.unwrap();
    for r in &real { let e=m.entry(r.get::<i64,_>("purchaser_id")).or_insert((r.get::<String,_>("name"),0.0,0.0)); e.1+=r.get::<f64,_>("a"); }

    // target increments
    let tgt = sqlx::query("SELECT so.purchaser_id, p.name, SUM(osi.amount) a FROM order_supplement_item osi JOIN sales_order so ON osi.target_order_id=so.id JOIN purchaser p ON so.purchaser_id=p.id GROUP BY so.purchaser_id").fetch_all(&pool).await.unwrap();
    for r in &tgt { let e=m.entry(r.get::<i64,_>("purchaser_id")).or_insert((r.get::<String,_>("name"),0.0,0.0)); e.2+=r.get::<f64,_>("a"); }

    // source real amounts (only source orders)
    let sr = sqlx::query("SELECT so.purchaser_id, p.name, so.id oid, SUM(soi.amount) a FROM sales_order_item soi JOIN sales_order so ON soi.order_id=so.id JOIN purchaser p ON so.purchaser_id=p.id GROUP BY so.id").fetch_all(&pool).await.unwrap();
    for r in &sr { if !src.contains(&r.get::<i64,_>("oid")) {continue;} let e=m.entry(r.get::<i64,_>("purchaser_id")).or_insert((r.get::<String,_>("name"),0.0,0.0)); e.2-=r.get::<f64,_>("a"); }

    let (mut tr,mut ts,mut tre)=(0.0,0.0,0.0);
    for (_,(name,real,supp)) in &m { println!("{:16} 真实={:>10.2} 净额={:>7.2} 报销={:>10.2}", name, real, supp, real+supp); tr+=real; ts+=supp; tre+=real+supp; }
    println!("{:16} 真实={:>10.2} 净额={:>7.2} 报销={:>10.2}", "合计", tr, ts, tre);
}
