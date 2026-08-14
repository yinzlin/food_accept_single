use axum::{
    http::{header, StatusCode},
    response::IntoResponse,
};
use chrono::Local;
use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use std::collections::HashMap;
use tray_icon::Icon;

// ===== 导出通用辅助 =====
pub fn xlsx_header_format(color: u32) -> Format {
    Format::new()
        .set_bold()
        .set_background_color(Color::RGB(color))
        .set_font_color(Color::White)
        .set_align(FormatAlign::Center)
        .set_border(FormatBorder::Thin)
}

pub fn xlsx_response(buf: Vec<u8>, filename: &str) -> axum::response::Response {
    let content_disposition =
        format!("attachment; filename*=UTF-8''{}", urlencode_filename(filename));
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string()),
            (header::CONTENT_DISPOSITION, content_disposition),
        ],
        buf,
    )
        .into_response()
}

pub fn urlencode_filename(name: &str) -> String {
    let mut out = String::new();
    for b in name.as_bytes() {
        let c = *b;
        if c.is_ascii_alphanumeric() || c == b'.' || c == b'-' || c == b'_' {
            out.push(c as char);
        } else {
            out.push_str(&format!("%{:02X}", c));
        }
    }
    out
}

pub fn sidebar_html() -> String {
    String::from(r#"
        <div class="sidebar">
            <div class="sidebar-header">
                <div class="logo"><span class="logo-icon">🍽️</span></div>
                <div class="logo-text">颍上食材配送管理系统</div>
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
                        <li class="tree-node leaf" data-path="/supplement" data-role="admin">
                            <a href="/supplement"><span class="node-icon">🔄</span><span class="node-label">耗材分摊管理</span></a>
                        </li>
                        <li class="tree-node leaf" data-path="/mobile/today_price" data-role="super_admin">
                            <a href="/mobile/today_price"><span class="node-icon">💰</span><span class="node-label">今日进价采集</span></a>
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
                                <li class="tree-node leaf" data-path="/query/purchase_document">
                                    <a href="/query/purchase_document"><span class="node-icon">🧾</span><span class="node-label">采购单据列表</span></a>
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
                                <li class="tree-node leaf" data-path="/query/reimburse_summary">
                                    <a href="/query/reimburse_summary"><span class="node-icon">🧾</span><span class="node-label">报销口径汇总</span></a>
                                </li>
                                <li class="tree-node leaf" data-path="/query/allocation_source">
                                    <a href="/query/allocation_source"><span class="node-icon">🔀</span><span class="node-label">分摊来源统计</span></a>
                                </li>
                                <li class="tree-node leaf" data-path="/query/order_adjust">
                                    <a href="/query/order_adjust"><span class="node-icon">✏️</span><span class="node-label">订单调整与同屏比对</span></a>
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
                                <li class="tree-node leaf" data-path="/query/stock_summary">
                                    <a href="/query/stock_summary"><span class="node-icon">📈</span><span class="node-label">真实出入库统计</span></a>
                                </li>
                                <li class="tree-node leaf" data-path="/query/stock_summary_reimburse">
                                    <a href="/query/stock_summary_reimburse"><span class="node-icon">🧾</span><span class="node-label">报销出入库统计</span></a>
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
                        <li class="tree-node leaf" data-path="/system/operation_log">
                            <a href="/system/operation_log"><span class="node-icon">📝</span><span class="node-label">操作日志</span></a>
                        </li>
                    </ul>
                </li>
            </ul>
        </div>
    "#)
}

pub fn layout_html(title: &str, page: &str, content: &str) -> String {
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
        .search-results li.active {{
            background: #d9e6ff;
            border-left: 3px solid #2E75B6;
            padding-left: 9px;
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
        .text-right {{ text-align: right !important; }}
        .form-control-sm.text-right {{ text-align: right; }}
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

    <!-- 价格提示确认模态框（支持换行，替代原生 confirm） -->
    <div class="modal fade" id="priceConfirmModal" tabindex="-1" style="z-index:1070;">
        <div class="modal-dialog modal-dialog-centered">
            <div class="modal-content">
                <div class="modal-header">
                    <h5 class="modal-title">价格提示</h5>
                    <button type="button" class="btn-close" data-bs-dismiss="modal"></button>
                </div>
                <div class="modal-body" id="priceConfirmBody" style="white-space:pre-line;line-height:1.7;"></div>
                <div class="modal-footer">
                    <button type="button" class="btn btn-secondary" data-bs-dismiss="modal" id="priceConfirmCancel">取消</button>
                    <button type="button" class="btn btn-primary" id="priceConfirmOk">确定</button>
                </div>
            </div>
        </div>
    </div>

    <!-- 通用提示模态框（支持换行，替代原生 alert） -->
    <div class="modal fade" id="appAlertModal" tabindex="-1" style="z-index:1070;">
        <div class="modal-dialog modal-dialog-centered">
            <div class="modal-content">
                <div class="modal-header">
                    <h5 class="modal-title" id="appAlertTitle">提示</h5>
                    <button type="button" class="btn-close" data-bs-dismiss="modal"></button>
                </div>
                <div class="modal-body" id="appAlertBody" style="white-space:pre-line;line-height:1.7;"></div>
                <div class="modal-footer">
                    <button type="button" class="btn btn-primary" data-bs-dismiss="modal" id="appAlertOk">确定</button>
                </div>
            </div>
        </div>
    </div>

    <script>
        // 价格提示确认（支持换行的 confirm 替代）
        function priceConfirm(message) {{
            return new Promise(function(resolve) {{
                const el = document.getElementById('priceConfirmModal');
                const body = document.getElementById('priceConfirmBody');
                const okBtn = document.getElementById('priceConfirmOk');
                const cancelBtn = document.getElementById('priceConfirmCancel');
                const modal = bootstrap.Modal.getOrCreateInstance(el);
                body.textContent = message;
                let settled = false;
                const finish = function(result) {{
                    if (settled) return;
                    settled = true;
                    okBtn.removeEventListener('click', onOk);
                    cancelBtn.removeEventListener('click', onCancel);
                    el.removeEventListener('hidden.bs.modal', onHidden);
                    resolve(result);
                }};
                const onOk = function() {{ modal.hide(); finish(true); }};
                const onCancel = function() {{ modal.hide(); finish(false); }};
                const onHidden = function() {{ finish(false); }};
                okBtn.addEventListener('click', onOk);
                cancelBtn.addEventListener('click', onCancel);
                el.addEventListener('hidden.bs.modal', onHidden);
                modal.show();
            }});
        }}

        // 通用提示（支持换行，替代原生 alert）
        function priceAlert(message, title) {{
            return new Promise(function(resolve) {{
                const el = document.getElementById('appAlertModal');
                const body = document.getElementById('appAlertBody');
                const titleEl = document.getElementById('appAlertTitle');
                const okBtn = document.getElementById('appAlertOk');
                const modal = bootstrap.Modal.getOrCreateInstance(el);
                body.textContent = message;
                titleEl.textContent = title || '提示';
                const onHidden = function() {{
                    okBtn.removeEventListener('click', onOk);
                    el.removeEventListener('hidden.bs.modal', onHidden);
                    resolve();
                }};
                const onOk = function() {{ modal.hide(); }};
                okBtn.addEventListener('click', onOk);
                el.addEventListener('hidden.bs.modal', onHidden, {{ once: true }});
                modal.show();
            }});
        }}

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
            if (!await priceConfirm('确定要删除分类"' + currentCtxTarget.name + '"吗？\n\n注意：有子分类或已被引用的分类无法删除。')) return;
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
                    await priceAlert(text || '删除失败', '提示');
                }}
            }} catch(e) {{ await priceAlert('删除失败: ' + e.message, '提示'); }}
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
            if (!await priceConfirm('确定要删除分类"' + currentCtxTarget.name + '"吗？\n\n注意：有子分类或已被引用的分类无法删除。')) return;
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
                    await priceAlert(text || '删除失败', '提示');
                }}
            }} catch(e) {{ await priceAlert('删除失败: ' + e.message, '提示'); }}
        }}
        
        function ctxRefreshSupplierCategoryTree() {{
            hideContextMenu();
            loadSupplierCategories();
        }}
        
        function filterSuppliersByCategory(catId, catName) {{
            if (typeof loadSuppliersByCategory === 'function') {{
                if (typeof setCurrentSupplierCategory === 'function') {{
                    setCurrentSupplierCategory(catId, catName);
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
            if (!await priceConfirm('确定要删除分类"' + currentCtxTarget.name + '"吗？\n\n注意：有子分类或已被引用的分类无法删除。')) return;
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
                    await priceAlert(text || '删除失败', '提示');
                }}
            }} catch(e) {{ await priceAlert('删除失败: ' + e.message, '提示'); }}
        }}
        
        function ctxRefreshPurchaserCategoryTree() {{
            hideContextMenu();
            loadPurchaserCategories();
        }}
        
        function filterPurchasersByCategory(catId, catName) {{
            if (typeof loadPurchasersByCategory === 'function') {{
                if (typeof setCurrentPurchaserCategory === 'function') {{
                    setCurrentPurchaserCategory(catId, catName);
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

pub fn round_to_allowed_last_digit(price: f64) -> f64 {
    if price <= 0.0 {
        return price;
    }
    // 截断到分
    let cents = (price * 100.0).round() / 100.0;
    // 取出末位
    let last = (cents * 100.0).round() as i64 % 10;
    let mapped = match last {
        0 | 1 | 2 => 0,
        3 | 4 | 5 => 5,
        6 => 6,
        7 | 8 => 8,
        9 => 9,
        _ => last,
    };
    let integer_cents = (cents * 100.0).round() as i64;
    let tens = integer_cents / 10;
    let new_cents = tens * 10 + mapped;
    new_cents as f64 / 100.0
}

pub fn parse_keyword_pattern(params: &HashMap<String, String>) -> String {
    match params.get("keyword").filter(|s| !s.is_empty()) {
        Some(k) => format!("%{}%", k),
        None => "%".to_string(),
    }
}

pub fn parse_csv(content: &str) -> Vec<Vec<String>> {
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

pub fn sanitize_filename_prefix(input: &str) -> String {
    let cleaned: String = input
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed.chars().take(60).collect()
    }
}

// 将图片URL转换为服务器文件路径（兼容旧格式 /api/product/image/ 与新格式 /api/uploads/...）
pub fn image_url_to_path(url: &str) -> Option<String> {
    if let Some(rest) = url.strip_prefix("/api/uploads/") {
        Some(format!("uploads/{}", rest))
    } else if let Some(rest) = url.strip_prefix("/api/product/image/") {
        Some(format!("uploads/{}", rest))
    } else {
        None
    }
}

pub fn operation_action_label(action: &str) -> String {
    let map = [
        ("purchase_order.create", "创建采购单"),
        ("purchase_order.update", "修改采购单"),
        ("purchase_order.delete", "删除采购单"),
        ("purchase_order.approve", "采购单审核"),
        ("purchase_order.unapprove", "采购单反审核"),
        ("purchase_order.import", "导入采购单"),
        ("sales_order.create", "创建销售单"),
        ("sales_order.update", "修改销售单"),
        ("sales_order.delete", "删除销售单"),
        ("sales_order.approve", "销售单审核"),
        ("sales_order.unapprove", "销售单反审核"),
        ("sales_order.update_status", "销售单状态流转"),
        ("sales_order.adjust_price", "销售单调价"),
        ("sales_order.import", "导入销售单"),
        ("sales_order.correction", "批量修正数量"),
        ("sales_order.generate_purchase", "生成采购订单"),
        ("sales_order.upload_image", "上传订单图片"),
        ("sales_order.delete_image", "删除订单图片"),
        ("purchase_document.upload", "上传采购单据"),
        ("purchase_document.delete", "删除采购单据"),
    ];
    for (k, v) in map {
        if action == k {
            return v.to_string();
        }
    }
    action.to_string()
}

pub fn build_category_tree_json(
    rows: &[SqliteRow],
    parent_id: Option<i64>,
    entity_type: &str,
) -> Vec<serde_json::Value> {
    let mut result = vec![];
    for row in rows {
        let et: String = row.get("entity_type");
        let pid: Option<i64> = row.get("parent_id");
        if et != entity_type {
            continue;
        }
        if pid != parent_id {
            continue;
        }
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

pub async fn generate_order_no(order_type: &str, order_date: &str) -> String {
    let prefix = if order_type == "sales" { "SO" } else { "PO" };

    let date_str: Vec<&str> = order_date.split('-').collect();
    let date_part = format!("{}{}{}", date_str[0], date_str[1], date_str[2]);

    let max_seq: i64 = if order_type == "sales" {
        sqlx::query_scalar(
            "SELECT COALESCE(MAX(CAST(SUBSTR(order_no, 11, 3) AS INTEGER)), 0) FROM sales_order WHERE order_date = ?",
        )
        .bind(order_date)
        .fetch_one(crate::db::pool())
        .await
        .unwrap_or(0)
    } else {
        sqlx::query_scalar(
            "SELECT COALESCE(MAX(CAST(SUBSTR(order_no, 11, 3) AS INTEGER)), 0) FROM purchase_order WHERE order_date = ?",
        )
        .bind(order_date)
        .fetch_one(crate::db::pool())
        .await
        .unwrap_or(0)
    };

    format!("{}{}{:03}", prefix, date_part, max_seq + 1)
}

pub fn make_app_icon() -> Icon {
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

pub fn open_browser() {
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", "http://127.0.0.1:3000"])
        .spawn();
}