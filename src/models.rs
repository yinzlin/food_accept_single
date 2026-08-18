use serde::{Deserialize, Serialize};

/// 当前登录用户的上下文（角色 + 用户ID + 行级数据权限关联）
#[derive(Debug, Clone)]
pub struct UserCtx {
    pub role: String,
    pub user_id: i64,
    pub supplier_id: i64,
    pub purchaser_id: i64,
}

#[derive(Deserialize, Serialize)]
pub struct SupplierReq {
    pub id: Option<i64>,
    pub name: String,
    pub contact: Option<String>,
    pub phone: Option<String>,
    pub address: Option<String>,
    pub business_scope: Option<String>,
    pub remark: Option<String>,
    pub category_id: Option<i64>,
}

#[derive(Deserialize, Serialize)]
pub struct PurchaserReq {
    pub id: Option<i64>,
    pub name: String,
    pub contact: Option<String>,
    pub phone: Option<String>,
    pub address: Option<String>,
    pub business_scope: Option<String>,
    pub remark: Option<String>,
    pub category_id: Option<i64>,
}

#[derive(Deserialize, Serialize)]
pub struct DeleteReq {
    pub id: i64,
}

#[derive(Deserialize)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize, Serialize)]
pub struct ProductReq {
    pub id: Option<i64>,
    pub name: String,
    pub spec: Option<String>,
    pub alias1: Option<String>,
    pub alias2: Option<String>,
    pub unit: Option<String>,
    pub base_unit: Option<String>,
    pub base_price: Option<f64>,
    pub purchase_price: Option<f64>,
    pub image_url: Option<String>,
    pub category_id: Option<i64>,
}

#[derive(Deserialize, Serialize)]
pub struct ProductUnitReq {
    pub product_id: i64,
    pub unit_name: String,
    pub ratio: f64,
    pub unit_price: Option<f64>,
    pub purchase_price: Option<f64>,
    pub sort_order: Option<i32>,
}

#[derive(Deserialize, Serialize)]
pub struct ProductPriceReq {
    pub product_id: i64,
    pub price_type: String,
    pub price: Option<f64>,
    pub collected_at: Option<String>,
    pub source: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct CategoryReq {
    pub name: String,
    pub parent_id: Option<i64>,
    pub entity_type: String,
    pub sort_order: Option<i32>,
}

#[derive(Deserialize, Serialize)]
pub struct PurchaseOrderReq {
    pub id: Option<i64>,
    pub supplier_id: i64,
    pub order_no: String,
    pub order_date: String,
    pub total_amount: f64,
    pub discount_rate: f64,
    pub amount_reduction: f64,
    pub final_amount: f64,
    pub warehouse_id: i64,
    pub warehouse_name: String,
    pub user_id: Option<i64>,
    pub handler_phone: Option<String>,
    pub items: Vec<PurchaseOrderItemReq>,
    pub remark: Option<String>,
    /// 乐观锁版本号：编辑时从详情接口取得，提交时校验，防止覆盖他人修改
    pub version: Option<i64>,
    /// 是否已结算（0=未结 1=已结），用于财务查询
    pub is_settled: Option<i64>,
}

#[derive(Deserialize, Serialize)]
pub struct PurchaseOrderItemReq {
    /// 明细主键 id：编辑回传时用于按 id 精确同步，保留 source_sales_order_id 归属
    pub id: Option<i64>,
    pub product_id: i64,
    pub product_name: String,
    pub alias1: Option<String>,
    pub alias2: Option<String>,
    pub spec: Option<String>,
    pub unit: Option<String>,
    pub unit_price: f64,
    pub quantity: f64,
    pub base_quantity: Option<f64>,
    pub amount: f64,
    pub ordered_quantity: Option<f64>,
    pub remark: Option<String>,
    /// 明细级仓库：同一订单各行可入不同仓库
    pub warehouse_id: Option<i64>,
    pub warehouse_name: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct SalesOrderReq {
    pub id: Option<i64>,
    pub purchaser_id: i64,
    pub order_no: String,
    pub order_date: String,
    pub total_amount: f64,
    pub discount_rate: f64,
    pub amount_reduction: f64,
    pub final_amount: f64,
    pub warehouse_id: i64,
    pub warehouse_name: String,
    pub items: Vec<SalesOrderItemReq>,
    pub remark: Option<String>,
    /// 验收单/报销单导出时使用的供应商名称；可空，前端新建时默认填充"湖南食全味美餐饮管理有限公司"。
    pub supplier_company: Option<String>,
    /// 验收单/报销单导出时使用的供货车牌号；可空，前端新建时默认填充"湘A·BE9312"。
    pub truck_plate: Option<String>,
    /// 乐观锁版本号：编辑时从详情接口取得，提交时校验，防止覆盖他人修改
    pub version: Option<i64>,
    /// 是否已结算（0=未结 1=已结），用于财务查询
    pub is_settled: Option<i64>,
}

#[derive(Deserialize, Serialize)]
pub struct SalesOrderItemReq {
    pub product_id: i64,
    pub product_name: String,
    pub alias1: Option<String>,
    pub alias2: Option<String>,
    pub spec: Option<String>,
    pub unit: Option<String>,
    pub unit_price: f64,
    pub quantity: f64,
    pub base_quantity: Option<f64>,
    pub amount: f64,
    pub pre_sale_quantity: Option<f64>,
    pub supplier_id: i64,
    pub supplier_name: String,
    pub category_id: Option<i64>,
    pub remark: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct OrderSupplementItemReq {
    pub target_order_id: i64,
    pub source_order_id: i64,
    pub source_remark: Option<String>,
    pub product_id: i64,
    pub product_name: String,
    pub alias1: Option<String>,
    pub alias2: Option<String>,
    pub spec: Option<String>,
    pub unit: String,
    pub unit_price: f64,
    pub quantity: f64,
    pub amount: f64,
    pub allocate_date: String,
    pub operation_type: String,
    pub target_order_item_id: Option<i64>,
}

#[derive(Deserialize, Serialize)]
pub struct AcceptReq {
    pub supplier_id: i64,
    pub purchaser_id: i64,
    pub car_no: Option<String>,
    pub supply_time: String,
    pub total_price: f64,
    pub discount_rate: f64,
    pub final_price: f64,
    pub items: Vec<FoodItemReq>,
}

#[derive(Deserialize, Serialize)]
pub struct FoodItemReq {
    pub food_name: String,
    pub spec: Option<String>,
    pub unit_price: f64,
    pub quantity: f64,
    pub sub_total: f64,
    pub produce_batch: Option<String>,
    pub shelf_life: Option<String>,
    pub has_veg_report: bool,
    pub has_meat_quarantine: bool,
    pub has_abnormal: bool,
    pub pass_check: bool,
    pub remark: Option<String>,
}

#[derive(Deserialize)]
pub struct ProductUpdateReq {
    pub id: i64,
    pub name: String,
    pub spec: Option<String>,
    pub alias1: Option<String>,
    pub alias2: Option<String>,
    pub unit: Option<String>,
    pub base_unit: Option<String>,
    pub base_price: Option<f64>,
    pub purchase_price: Option<f64>,
    pub image_url: Option<String>,
    pub category_id: Option<i64>,
    pub markup_rate: Option<f64>,
    pub auto_update_price: Option<i64>,
}

#[derive(Deserialize)]
pub struct CategoryRenameReq {
    pub id: i64,
    pub name: String,
}

#[derive(Deserialize)]
pub struct WarehouseCreateReq {
    pub name: String,
    pub code: Option<String>,
    pub address: Option<String>,
    pub contact: Option<String>,
    pub phone: Option<String>,
    pub sort_order: Option<i32>,
}

#[derive(Deserialize)]
pub struct WarehouseUpdateReq {
    pub id: i64,
    pub name: String,
    pub code: Option<String>,
    pub address: Option<String>,
    pub contact: Option<String>,
    pub phone: Option<String>,
    pub status: Option<i32>,
    pub sort_order: Option<i32>,
}