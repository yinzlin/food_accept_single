use axum::response::Html;
use sqlx::Row;
use chrono::Local;

pub async fn page_index(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/").await {
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
    
    Html(crate::layout_html("进销存管理系统", "/", &content))
}

pub async fn page_supplier(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/supplier").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    
    let cat_rows = sqlx::query("SELECT id, name FROM category WHERE entity_type='supplier' ORDER BY sort_order, id")
        .fetch_all(crate::pool())
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
    
    Html(crate::layout_html("供应商管理", "/supplier", &content))
}

pub async fn page_purchaser(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/purchaser").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let cat_rows = sqlx::query("SELECT id, name FROM category WHERE entity_type='purchaser' ORDER BY sort_order, id")
        .fetch_all(crate::pool())
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
    
    Html(crate::layout_html("采购方管理", "/purchaser", &content))
}

pub async fn page_product(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/product").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let cat_rows = sqlx::query("SELECT id, name FROM category WHERE entity_type='product' ORDER BY sort_order, id")
        .fetch_all(crate::pool())
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
        <style>
            .page-content {{ padding-top: 0 !important; }}
            .product-sticky-header {{
                position: sticky;
                top: 0;
                z-index: 30;
                background: #f5f7fa;
                padding-top: 12px;
                padding-bottom: 8px;
            }}
            .product-sticky-table thead th {{
                position: sticky;
                top: var(--product-thead-top, 0px);
                z-index: 20;
                background: white;
                box-shadow: 0 1px 0 rgba(0,0,0,0.1);
            }}
            .product-list-section {{ padding-top: 6px; }}
        </style>
        <div class="product-sticky-header">
            <div class="card mb-4" style="margin-bottom:0 !important;">
                <div class="card-body" style="padding:12px;">
                    <h4 style="margin-bottom:8px;">新增商品</h4>
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
                                <input type="number" step="0.01" name="purchase_price" placeholder="进价" class="form-control purchase-price-field">
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

            <div class="d-flex justify-content-between align-items-center" style="padding:10px 0;background:#f5f7fa;">
                <h5 id="productListTitle" style="margin:0;">全部商品</h5>
                <div class="d-flex gap-2 align-items-center">
                    <input type="text" id="searchKeyword" placeholder="搜索商品名称" class="form-control form-control-sm" style="width:200px" onkeydown="if(event.key==='Enter')searchProducts()">
                    <button class="btn btn-sm btn-outline-primary" onclick="searchProducts()">搜索</button>
                    <button class="btn btn-sm btn-outline-secondary" onclick="resetSearch()">显示全部</button>
                    <button class="btn btn-sm btn-success" onclick="batchSetAutoUpdate(1)">全部开启自动更新售价</button>
                    <button class="btn btn-sm btn-secondary" onclick="batchSetAutoUpdate(0)">全部关闭自动更新售价</button>
                    <a href="/api/product/export" class="btn btn-sm btn-success">导出</a>
                    <button class="btn btn-sm btn-warning" onclick="importProducts()">导入</button>
                    <input type="file" id="productFileInput" style="display:none" accept=".xlsx,.csv" onchange="handleProductFile(this)">
                </div>
            </div>
        </div>

        <div class="product-list-section">
        <table class="table table-bordered product-sticky-table">
            <thead><tr><th>ID</th><th>图片</th><th>名称</th><th>规格</th><th>显示单位</th><th>基础单位</th><th>售价</th><th class="purchase-price-col">进价</th><th>多单位</th><th>分类</th><th>状态</th><th style="width:140px">操作</th></tr></thead>
            <tbody id="productTableBody">
                <tr><td colspan="12" class="text-center text-muted">加载中...</td></tr>
            </tbody>
        </table>
        <div id="productPagination" class="mt-3"></div>
        </div>

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
                                <div class="col-md-4" id="purchasePriceField">
                                    <label class="form-label">当前进价（每基础单位，最近采购价）</label>
                                    <input type="number" step="0.01" name="purchase_price" class="form-control purchase-price-field">
                                </div>
                            </div>
                            <div class="row mt-3">
                                <div class="col-md-4">
                                    <label class="form-label">分类</label>
                                    <select name="category_id" class="form-control">{0}</select>
                                </div>
                                <div class="col-md-2">
                                    <label class="form-label">加成率（毛利率）</label>
                                    <input type="number" step="0.01" min="0" max="5" name="markup_rate" class="form-control" oninput="onMarkupRateChange()">
                                </div>
                                <div class="col-md-2">
                                    <label class="form-label">售价自动更新</label>
                                    <select name="auto_update_price" class="form-control">
                                        <option value="0">关闭</option>
                                        <option value="1">开启</option>
                                    </select>
                                </div>
                                <div class="col-md-2 purchase-price-col">
                                    <label class="form-label">历史最高进价（自动）</label>
                                    <input type="number" step="0.01" name="max_purchase_price" class="form-control" readonly style="background-color:#f5f5f5;">
                                </div>
                                <div class="col-md-2 purchase-price-col">
                                    <label class="form-label">历史最低进价（自动）</label>
                                    <input type="number" step="0.01" name="min_purchase_price" class="form-control" readonly style="background-color:#f5f5f5;">
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
                                        <button type="button" class="btn btn-sm btn-danger" onclick="deleteProductImage()">🗑️ 删除图片</button>
                                    </div>
                                </div>
                                <div>
                                    <input type="file" id="productImageInput" accept="image/*" style="display:none" onchange="uploadProductImage()">
                                    <button type="button" class="btn btn-sm btn-outline-primary" onclick="document.getElementById('productImageInput').click()">📷 上传图片</button>
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
                                <thead><tr><th>单位名称</th><th>换算比例（1本单位=?基础单位）</th><th>单位售价（留0则按比例自动算）</th><th class="purchase-price-col">单位采购价（留0则按进价比例算）</th><th>排序</th><th>操作</th></tr></thead>
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
            let currentPage = 1;
            let pageSize = 20;
            let totalPages = 1;
            let totalCount = 0;
            let editingProductId = null;
            let pendingProductData = null;

            // 动态计算表头 sticky 偏移量（等于置顶表单+工具栏的高度）
            function updateStickyHeaderOffset() {{
                const header = document.querySelector('.product-sticky-header');
                const table = document.querySelector('.product-sticky-table');
                if (header && table) {{
                    const h = header.offsetHeight;
                    table.style.setProperty('--product-thead-top', h + 'px');
                }}
            }}
            window.addEventListener('load', updateStickyHeaderOffset);
            window.addEventListener('resize', updateStickyHeaderOffset);
            document.addEventListener('DOMContentLoaded', updateStickyHeaderOffset);

            async function loadProductsByCategory(categoryId, page) {{
                const categoryChanged = categoryId !== undefined && categoryId !== currentCategoryId;
                if (categoryChanged) currentCategoryId = categoryId;
                if (page !== undefined) {{
                    currentPage = page;
                }} else if (categoryChanged || (categoryId === null && currentCategoryId !== null)) {{
                    currentPage = 1;
                }}
                if (categoryId === null) currentCategoryId = null;
                let params = [];
                if (currentCategoryId) {{ params.push('category_id=' + currentCategoryId); }}
                if (currentKeyword) {{ params.push('keyword=' + encodeURIComponent(currentKeyword)); }}
                params.push('page=' + currentPage);
                params.push('page_size=' + pageSize);
                let url = '/api/product/list?' + params.join('&');
                try {{
                    const res = await fetch(url);
                    const result = await res.json();
                    const products = result.data || result || [];
                    allProducts = products;
                    totalCount = result.total || products.length;
                    totalPages = result.total_pages || 1;
                    if (result.page) currentPage = result.page;
                    renderProductTable(products);
                    renderProductPagination();
                    updateCategoryTitle(currentCategoryId);
                    setFormCategory(currentCategoryId);
                }} catch(e) {{
                    console.error('加载商品失败:', e);
                }}
            }}

            function renderProductPagination() {{
                const container = document.getElementById('productPagination');
                if (!container) return;
                if (totalPages <= 1) {{ container.innerHTML = '<p class="text-center text-muted">共 ' + totalCount + ' 条商品</p>'; return; }}
                let html = '<nav><ul class="pagination justify-content-center">';
                html += '<li class="page-item ' + (currentPage <= 1 ? 'disabled' : '') + '"><a class="page-link" onclick="loadProductsByCategory(undefined, ' + (currentPage - 1) + ')">上一页</a></li>';
                const startPage = Math.max(1, currentPage - 2);
                const endPage = Math.min(totalPages, currentPage + 2);
                for (let i = startPage; i <= endPage; i++) {{
                    html += '<li class="page-item ' + (i === currentPage ? 'active' : '') + '"><a class="page-link" onclick="loadProductsByCategory(undefined, ' + i + ')">' + i + '</a></li>';
                }}
                html += '<li class="page-item ' + (currentPage >= totalPages ? 'disabled' : '') + '"><a class="page-link" onclick="loadProductsByCategory(undefined, ' + (currentPage + 1) + ')">下一页</a></li>';
                html += '</ul></nav>';
                html += '<p class="text-center text-muted mt-2">共 ' + totalCount + ' 条商品，当前第 ' + currentPage + '/' + totalPages + ' 页</p>';
                container.innerHTML = html;
            }}

            function renderProductTable(products) {{
                const tbody = document.getElementById('productTableBody');
                if (!products || products.length === 0) {{
                    tbody.innerHTML = '<tr><td colspan="12" class="text-center text-muted">暂无商品数据</td></tr>';
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
                    let nameDisplay = escapeHtml(p.name);
                    if (p.alias2 && p.alias2.trim() !== '') {{
                        nameDisplay += '(' + escapeHtml(p.alias2.trim()) + ')';
                    }}
                    let statusBadge = p.status === 1 ? '<span class="badge bg-success">启用</span>' : '<span class="badge bg-secondary">停用</span>';
                    let toggleBtnClass = p.status === 1 ? 'btn-outline-warning' : 'btn-outline-success';
                    let toggleBtnText = p.status === 1 ? '停用' : '启用';
                    let autoBadge = (p.auto_update_price === 1) ? '<span class="badge bg-info" title="开启自动更新售价">自动</span>' : '<span class="badge bg-light text-dark" title="人工维护售价">人工</span>';
                    let autoBtnClass = (p.auto_update_price === 1) ? 'btn-outline-secondary' : 'btn-outline-info';
                    let autoBtnText = (p.auto_update_price === 1) ? '关闭自动' : '开启自动';
                    html += '<tr><td>' + p.id + '</td><td>' + imageHtml + '</td><td>' + nameDisplay + '</td><td>' + escapeHtml(p.spec || '') + '</td><td>' + escapeHtml(p.unit || '') + '</td><td>' + escapeHtml(p.base_unit || '') + '</td><td>' + p.base_price + '</td>' + (isSuperAdmin ? '<td>' + (p.purchase_price || 0) + '</td>' : '') + '<td>' + escapeHtml(unitsText) + '</td><td>' + escapeHtml(p.category_name || '无分类') + '</td><td>' + statusBadge + ' ' + autoBadge + '</td>';
                    html += '<td><button class="btn btn-sm btn-outline-primary me-1" onclick="editProduct(' + p.id + ')">编辑</button><button class="btn btn-sm ' + toggleBtnClass + ' me-1" onclick="toggleProductStatus(' + p.id + ')">' + toggleBtnText + '</button><button class="btn btn-sm ' + autoBtnClass + ' me-1" onclick="toggleProductAutoUpdate(' + p.id + ', ' + (p.auto_update_price || 0) + ')">' + autoBtnText + '</button><button class="btn btn-sm btn-outline-danger" onclick="deleteProduct(' + p.id + ')">删除</button></td></tr>';
                }});
                tbody.innerHTML = html;
            }}

            function searchProducts() {{
                currentKeyword = document.getElementById('searchKeyword').value.trim();
                currentPage = 1;
                loadProductsByCategory(currentCategoryId, 1);
            }}

            function resetSearch() {{
                document.getElementById('searchKeyword').value = '';
                currentKeyword = '';
                currentCategoryId = null;
                currentPage = 1;
                loadProductsByCategory(null, 1);
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
                // 应用统一尾数规则
                form.selling_price.value = roundToAllowedLastDigit(sellingPrice).toFixed(2);
            }}

            // 加成率变更时的客户端预览（仅在自动更新开启且有进价时）
            function onMarkupRateChange() {{
                const form = document.getElementById('editForm');
                const auto = parseInt(form.auto_update_price.value);
                if (auto !== 1) return;
                const purchase = parseFloat(form.purchase_price.value) || 0;
                const markup = parseFloat(form.markup_rate.value) || 0;
                if (purchase > 0) {{
                    const raw = purchase * (1 + markup);
                    const preview = roundToAllowedLastDigit(raw);
                    form.selling_price.value = preview.toFixed(2);
                }}
            }}

            // 客户端取整（与后端 round_to_allowed_last_digit 保持一致）
            function roundToAllowedLastDigit(price) {{
                if (price <= 0) return price;
                let cents = Math.round(price * 100);
                let last = cents % 10;
                let mapped;
                if (last <= 2) mapped = 0;
                else if (last <= 5) mapped = 5;
                else if (last === 6) mapped = 6;
                else if (last <= 8) mapped = 8;
                else mapped = 9;
                return (Math.floor(cents / 10) * 10 + mapped) / 100;
            }}

            // 通用：拉取商品最近采购价并在采购/销售单选商品后做同基础单位对比提示
            // kind = 'purchase'：采购价对比最近采购价
            // kind = 'sales'：销售零售价对比最近采购价
            async function checkPriceAfterSelect(productId, currentBaseUnit, currentPrice, kind, productName) {{
                try {{
                    const res = await fetch('/api/product/last_purchase_price?product_id=' + productId);
                    if (!res.ok) return;
                    const data = await res.json();
                    const lastPrice = parseFloat(data.purchase_price) || 0;
                    const lastUnit = data.base_unit || '';
                    if (lastPrice <= 0) return;
                    // 必须同基础单位才比较（避免五花肉/斤 与 五花肉/块 误比）
                    if (lastUnit !== currentBaseUnit) {{
                        return;
                    }}
                    if (kind === 'purchase' && Math.abs(currentPrice - lastPrice) >= 0.01) {{
                        const diff = currentPrice - lastPrice;
                        const sign = diff > 0 ? '上涨' : '下降';
                        const tip = '【价格提示】\\n商品：' + productName + '\\n最近采购价（基础单位 ' + lastUnit + '）：' + lastPrice.toFixed(2) + '\\n本次采购价：' + currentPrice.toFixed(2) + '\\n' + sign + ' ' + Math.abs(diff).toFixed(2) + '（' + (Math.abs(diff / lastPrice * 100)).toFixed(1) + '%）';
                        if (!confirm(tip + '\\n\\n是否继续？')) {{
                            // 用户取消：不清空已选商品，仅提示
                        }}
                    }} else if (kind === 'sales' && currentPrice < lastPrice) {{
                        const tip = '【价格提示】\\n商品：' + productName + '\\n最近采购价（基础单位 ' + lastUnit + '）：' + lastPrice.toFixed(2) + '\\n本次零售价：' + currentPrice.toFixed(2) + '\\n零售价低于采购价 ' + (lastPrice - currentPrice).toFixed(2);
                        if (!confirm(tip + '\\n\\n是否继续？')) {{
                        }}
                    }}
                }} catch(e) {{
                    console.error('价格比较失败:', e);
                }}
            }}

            // 批量设置所有商品的自动更新售价开关
            async function batchSetAutoUpdate(auto) {{
                const text = auto === 1 ? '开启' : '关闭';
                if (!confirm('确定要' + text + '所有商品的自动更新售价吗？')) return;
                const res = await fetch('/api/product/batch_set_auto_update_price', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ auto_update_price: auto }})
                }});
                if (res.ok) {{
                    alert('已' + text + '所有商品的自动更新售价');
                    loadProductsByCategory(currentCategoryId);
                }} else {{
                    alert('操作失败');
                }}
            }}

            // 单个商品切换自动更新售价
            async function toggleProductAutoUpdate(pid, currentAuto) {{
                const nextAuto = currentAuto === 1 ? 0 : 1;
                const text = nextAuto === 1 ? '开启' : '关闭';
                if (!confirm('确定要' + text + '该商品的自动更新售价吗？')) return;
                const res = await fetch('/api/product/set_auto_update_price', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ product_id: pid, auto_update_price: nextAuto }})
                }});
                if (res.ok) {{
                    loadProductsByCategory(currentCategoryId);
                }} else {{
                    alert('操作失败');
                }}
            }}

            // 商品编辑页价格校验：进价/售价为零普通提醒；进价>售价严重提醒
            function validateProductPrices() {{
                const form = document.getElementById('editForm');
                const purchase = parseFloat(form.purchase_price.value) || 0;
                const selling = parseFloat(form.base_price.value) || 0;
                const warnings = [];
                if (purchase <= 0) warnings.push('当前进价为 0');
                if (selling <= 0) warnings.push('当前售价为 0');
                if (purchase > 0 && selling > 0 && purchase > selling) {{
                    const msg = '警告：当前进价（{{0}}）高于当前售价（{{1}}），请确认是否倒置！'.replace('{{0}}', purchase.toFixed(2)).replace('{{1}}', selling.toFixed(2));
                    if (!confirm('【严重提醒】\\n' + msg + '\\n\\n是否仍要保存？')) {{
                        return false;
                    }}
                    return warnings.length > 0 ? confirm('【普通提醒】以下价格为零：' + warnings.join('、') + '\\n\\n是否仍要保存？') : true;
                }}
                if (warnings.length > 0) {{
                    return confirm('【普通提醒】以下价格为零：' + warnings.join('、') + '\\n\\n是否仍要保存？');
                }}
                return true;
            }}

            // 进价字段仅超级管理员可见可编辑：非 super_admin 禁用并隐藏所有进价输入框及进价列
            let isSuperAdmin = false;
            function applyPurchasePriceRestriction() {{
                if (isSuperAdmin) return;
                document.querySelectorAll('.purchase-price-field').forEach(el => {{
                    el.disabled = true;
                    el.style.display = 'none';
                }});
                document.querySelectorAll('.purchase-price-col, #purchasePriceField').forEach(el => {{
                    el.style.display = 'none';
                }});
            }}
            fetch('/api/login/check').then(r => r.json()).then(d => {{
                if (d && d.logged_in) {{
                    isSuperAdmin = (d.user.role === 'super_admin');
                }}
                applyPurchasePriceRestriction();
            }});

            function addUnitRow(unitData) {{
                const tbody = document.getElementById('unitTableBody');
                const tr = document.createElement('tr');
                tr.innerHTML = `
                    <td><input type="text" class="form-control form-control-sm" name="unit_name" value="${{unitData ? escapeHtml(unitData.unit_name) : ''}}"></td>
                    <td><input type="number" step="0.01" class="form-control form-control-sm" name="ratio" value="${{unitData ? unitData.ratio : 1}}"></td>
                    <td><input type="number" step="0.01" class="form-control form-control-sm" name="unit_price" value="${{unitData ? unitData.unit_price : 0}}"></td>
                    ${{isSuperAdmin ? '<td><input type="number" step="0.01" class="form-control form-control-sm purchase-price-field" name="purchase_price" value="' + (unitData ? (unitData.purchase_price || 0) : 0) + '"></td>' : ''}}
                    <td><input type="number" class="form-control form-control-sm" name="sort_order" value="${{unitData ? unitData.sort_order : 0}}"></td>
                    <td><button class="btn btn-sm btn-danger" onclick="this.parentElement.parentElement.remove()">删除</button></td>
                `;
                tbody.appendChild(tr);
                applyPurchasePriceRestriction();
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
                form.max_purchase_price.value = p.max_purchase_price || 0;
                form.min_purchase_price.value = p.min_purchase_price || 0;
                form.category_id.value = p.category_id || '';
                form.markup_rate.value = (p.markup_rate !== undefined && p.markup_rate !== null) ? p.markup_rate : 0.5;
                form.auto_update_price.value = (p.auto_update_price !== undefined && p.auto_update_price !== null) ? p.auto_update_price : 0;

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
                applyPurchasePriceRestriction();
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
                if (!validateProductPrices()) return;
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
                    category_id: form.category_id.value ? parseInt(form.category_id.value) : null,
                    markup_rate: parseFloat(form.markup_rate.value) || 0.5,
                    auto_update_price: parseInt(form.auto_update_price.value)
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
    
    Html(crate::layout_html("商品管理", "/product", &content))
}

pub async fn page_warehouse(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/warehouse").await {
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
    Html(crate::layout_html("仓库管理", "/warehouse", &content))
}

pub async fn page_inventory(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/inventory").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let rows = sqlx::query(
        "SELECT i.id, i.product_id, p.name, p.spec, i.quantity, i.min_stock, i.max_stock 
         FROM inventory i JOIN product p ON i.product_id = p.id ORDER BY i.id DESC"
    )
    .fetch_all(crate::pool())
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
    
    Html(crate::layout_html("库存管理", "/inventory", &content))
}

pub async fn page_purchase(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/purchase").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let supplier_rows = sqlx::query("SELECT id, name FROM supplier")
        .fetch_all(crate::pool())
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
                    <div class="col-md-3">
                        <label>经手人：</label>
                        <select id="handlerSelect" class="form-control" onchange="document.getElementById('handlerId').value = this.value">
                            <option value="">请选择经手人</option>
                        </select>
                        <input type="hidden" id="handlerId" value="">
                    </div>
                </div>

                <table class="table table-bordered">
                    <thead>
                        <tr><th style="min-width:180px">商品名称</th><th style="width:55px">规格</th><th style="width:75px">单位</th><th style="width:85px">订购数量</th><th style="width:75px">数量</th><th style="width:85px">单价</th><th style="width:110px">金额</th><th style="width:110px">仓库</th><th style="width:120px">备注</th><th style="width:65px">操作</th></tr>
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
            <input type="text" id="searchInput" class="form-control" placeholder="搜索订单号、供应商、日期..." oninput="searchOrders()" style="width: 250px; display: inline-block;">
            <div class="position-relative" style="display: inline-block; width: 200px; vertical-align: top;">
                <select id="supplierSelect" class="form-control" onchange="searchOrders()">
                    <option value="">全部供应商</option>
                </select>
            </div>
            <button onclick="searchOrders()" class="btn btn-primary ml-2">搜索</button>
            <button onclick="resetSearch()" class="btn btn-secondary ml-2">重置</button>
            <button onclick="cancelOrder()" class="btn btn-warning ml-2">取消</button>
            <a href="javascript:void(0)" onclick="exportFilteredPurchaseOrders()" class="btn btn-success ml-2">导出</a>
            <button onclick="importPurchaseOrders()" class="btn btn-warning ml-2">导入</button>
            <input type="file" id="purchaseOrderFileInput" style="display:none" accept=".csv" onchange="handlePurchaseOrderFile(this)">
        </div>
        <table class="table table-bordered">
            <thead><tr><th>ID</th><th onclick="sortOrders('order_no')" style="cursor:pointer">订单号<span id="sortIndicator_order_no"></span></th><th onclick="sortOrders('order_date')" style="cursor:pointer">日期<span id="sortIndicator_order_date"></span></th><th onclick="sortOrders('unit_name')" style="cursor:pointer">供应商<span id="sortIndicator_unit_name"></span></th><th>入库仓库</th><th>金额</th><th>下浮后</th><th>折减</th><th>最终金额</th><th onclick="sortOrders('status')" style="cursor:pointer">状态<span id="sortIndicator_status"></span></th><th>操作</th></tr></thead>
            <tbody id="orderListBody"></tbody>
        </table>

        <div id="pagination" class="mt-3"></div>

        <script>
            let suppliers = [];
            let items = [];
            let sortField = '';
            let sortOrder = 'desc';

            function sortOrders(field) {{
                if (sortField === field) {{
                    sortOrder = sortOrder === 'asc' ? 'desc' : 'asc';
                }} else {{
                    sortField = field;
                    sortOrder = 'asc';
                }}
                updateSortIndicators();
                loadOrders();
            }}

            function updateSortIndicators() {{
                const fields = ['order_no', 'order_date', 'unit_name', 'status'];
                fields.forEach(f => {{
                    const el = document.getElementById('sortIndicator_' + f);
                    if (el) {{
                        el.textContent = (sortField === f) ? (sortOrder === 'asc' ? ' ▲' : ' ▼') : '';
                    }}
                }});
            }}

            async function loadSuppliers() {{
                const res = await fetch('/api/supplier/list');
                suppliers = await res.json();
                const select = document.getElementById('supplierSelect');
                if (select && suppliers.length > 0) {{
                    suppliers.forEach(s => {{
                        select.innerHTML += '<option value="' + s.id + '">' + s.name + '</option>';
                    }});
                }}
            }}
            loadSuppliers();

            let warehouses = [];
            async function loadWarehouses() {{
                const res = await fetch('/api/warehouse/list');
                warehouses = await res.json();
            }}
            loadWarehouses();

            // 明细行仓库选择（每行独立）：单击弹下拉，双击可输入搜索
            function showItemWarehouseDropdown(index, filter) {{
                const dropdown = document.getElementById('warehouseDropdown_' + index);
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
                    html += '<li onclick="selectItemWarehouse(' + index + ', this)" data-id="' + w.id + '" data-name="' + w.name.replace(/&/g, '&amp;').replace(/"/g, '&quot;') + '">' + w.name + '</li>';
                }});
                html += '</ul>';
                dropdown.innerHTML = html;
                dropdown.style.display = 'block';
            }}

            function selectItemWarehouse(index, li) {{
                const input = document.getElementById('warehouseInput_' + index);
                const dropdown = document.getElementById('warehouseDropdown_' + index);
                if (li && input) {{
                    items[index].warehouse_id = parseInt(li.getAttribute('data-id')) || 0;
                    items[index].warehouse_name = li.getAttribute('data-name');
                    input.value = items[index].warehouse_name;
                    input.readOnly = true;
                    if (dropdown) dropdown.style.display = 'none';
                }}
            }}

            // 加载用户列表
            let users = [];
            async function loadUsers() {{
                try {{
                    const res = await fetch('/api/user/list');
                    if (res.ok) {{
                        users = await res.json();
                        const handlerSelect = document.getElementById('handlerSelect');
                        if (handlerSelect) {{
                            const currentVal = handlerSelect.value;
                            handlerSelect.innerHTML = '<option value="">请选择经手人</option>';
                            users.forEach(u => {{
                                const name = u.nickname || u.username || '';
                                if (name) {{
                                    handlerSelect.innerHTML += '<option value="' + u.id + '">' + name + (u.phone ? ' (' + u.phone + ')' : '') + '</option>';
                                }}
                            }});
                            if (currentVal) handlerSelect.value = currentVal;
                        }}
                    }}
                }} catch (e) {{}}
            }}
            loadUsers();

            // 导出采购单（打印模板样式）
            async function exportPurchaseOrder(orderId) {{
                if (users.length === 0) {{ await loadUsers(); }}
                let uid = 0;
                let opts = '0. 无经手人（使用订单已存）\n';
                users.forEach((u, idx) => {{
                    const name = u.nickname || u.username || '';
                    if (name) {{ opts += (idx+1) + '. ' + name + (u.phone ? ' (' + u.phone + ')' : '') + '\n'; }}
                }});
                const input = prompt('请选择经手人编号：\n' + opts, '0');
                if (input === null) return;
                const choice = parseInt(input);
                if (isNaN(choice)) return;
                if (choice === 0) {{
                    uid = 0;
                }} else if (choice > 0 && choice <= users.length) {{
                    const selected = users[choice - 1];
                    uid = selected ? (selected.id || 0) : 0;
                }}
                window.location = '/api/purchase_order/export_print/' + orderId + (uid > 0 ? '?user_id=' + uid : '');
            }}

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
            // 超级管理员任何时刻都拥有反审核权限，故其反审核按钮不受订单状态限制
            let isSuperAdmin = false;
            fetch('/api/login/check').then(r => r.json()).then(d => {{
                if (d && d.logged_in) {{
                    isSuperAdmin = (d.user.role === 'super_admin');
                }}
                if (isSuperAdmin) loadOrders();
            }});

            function resetSearch() {{
                document.getElementById('searchInput').value = '';
                document.getElementById('supplierSelect').value = '';
                currentKeyword = '';
                currentPage = 1;
                loadOrders();
            }}

            async function searchOrders() {{
                currentKeyword = document.getElementById('searchInput').value.trim();
                currentPage = 1;
                await loadOrders();
            }}

            // 按当前筛选条件（搜索关键字 + 供应商）跳转导出，参数与列表保持一致
            function exportFilteredPurchaseOrders() {{
                const params = new URLSearchParams();
                const kw = document.getElementById('searchInput').value.trim();
                if (kw) params.set('keyword', kw);
                const supplierId = document.getElementById('supplierSelect').value;
                if (supplierId) params.set('supplier_id', supplierId);
                if (sortField) {{
                    params.set('sort_field', sortField);
                    params.set('sort_order', sortOrder);
                }}
                const qs = params.toString();
                window.location = '/api/purchase_order/export' + (qs ? '?' + qs : '');
            }}

            async function loadOrders(page) {{
                if (page !== undefined) currentPage = page;
                let url = '/api/purchase_order/list?page=' + currentPage + '&page_size=20';
                if (currentKeyword) {{
                    url += '&keyword=' + encodeURIComponent(currentKeyword);
                }}
                const supplierId = document.getElementById('supplierSelect').value;
                if (supplierId) {{
                    url += '&supplier_id=' + supplierId;
                }}
                if (sortField) {{
                    url += '&sort_field=' + sortField + '&sort_order=' + sortOrder;
                }}
                const res = await fetch(url);
                const result = await res.json();
                const orders = result.data || [];
                const tbody = document.getElementById('orderListBody');
                tbody.innerHTML = '';
                let sumAmount = 0, sumDiscounted = 0, sumReduction = 0, sumFinal = 0;
                orders.forEach(order => {{
                    const amount = order.total_amount;
                    const discounted = amount * (1 - (order.discount_rate || 0) / 100);
                    const reduction = order.amount_reduction || 0;
                    const finalAmt = order.final_amount || 0;
                    sumAmount += amount;
                    sumDiscounted += discounted;
                    sumReduction += reduction;
                    sumFinal += finalAmt;
                    tbody.innerHTML += '<tr onclick="loadOrderDetail(' + order.id + ')" style="cursor: pointer;">' +
                        '<td>' + order.id + '</td>' +
                        '<td>' + order.order_no + '</td>' +
                        '<td>' + order.order_date + '</td>' +
                        '<td>' + order.supplier_name + '</td>' +
                        '<td>' + (order.warehouse_name || '') + '</td>' +
                        '<td>' + amount.toFixed(2) + '</td>' +
                        '<td>' + discounted.toFixed(2) + '</td>' +
                        '<td>' + reduction.toFixed(2) + '</td>' +
                        '<td>' + finalAmt.toFixed(2) + '</td>' +
                        '<td>' + orderStatusLabel(order.status) + '</td>' +
                        '<td>' +
                        (order.status === 'pending' ? '<button onclick="event.stopPropagation(); approveOrder(' + order.id + ')" class="btn btn-success btn-sm me-1">审核</button>' : '') +
                        ((order.status === 'confirmed' || isSuperAdmin) ? '<button onclick="event.stopPropagation(); unapproveOrder(' + order.id + ')" class="btn btn-warning btn-sm me-1">反审核</button>' : '') +
                        '<button onclick="event.stopPropagation(); exportPurchaseOrder(' + order.id + ')" class="btn btn-info btn-sm me-1">导出采购单</button>' +
                        '<button onclick="event.stopPropagation(); deleteOrder(' + order.id + ')" class="btn btn-danger btn-sm">删除</button>' +
                        '</td></tr>';
                }});
                if (orders.length > 0) {{
                    tbody.innerHTML += '<tr class="table-active fw-bold"><td colspan="5" class="text-end">合计</td><td>' + sumAmount.toFixed(2) + '</td><td>' + sumDiscounted.toFixed(2) + '</td><td>' + sumReduction.toFixed(2) + '</td><td>' + sumFinal.toFixed(2) + '</td><td colspan="2"></td></tr>';
                }}
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
                items.push({{ product_id: 0, product_name: '', alias1: '', alias2: '', spec: '', unit: '', base_unit: '', unit_price: 0, purchase_price: 0, quantity: 0, base_quantity: 0, amount: 0, ordered_quantity: 0, ratio: 1, units: [], warehouse_id: 0, warehouse_name: '' }});
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
                                           onkeydown="handleProductNameKeydown(event, ${{index}}, this)"
                                           class="form-control-sm product-search-input" 
                                           placeholder="输入商品名称搜索"
                                           enterkeyhint="next">
                                    <div id="searchDropdown_${{index}}" class="search-dropdown"></div>
                                </div>
                            </td>
                            <td style="width:55px"><input type="text" value="${{item.spec}}" onchange="updateSpec(${{index}}, this)" onkeydown="handleEnterKey(event, ${{index}}, 'spec')" class="form-control-sm"></td>
                            <td style="width:75px">
                                <select onchange="updateUnit(${{index}}, this)" class="form-control-sm">
                                    ${{unitOptions}}
                                </select>
                            </td>
                            <td style="width:85px"><input type="text" value="${{item.ordered_quantity != null && item.ordered_quantity > 0 ? item.ordered_quantity.toFixed(2) : ''}}" onchange="updateOrderedQty(${{index}}, this)" onkeydown="handleEnterKey(event, ${{index}}, 'ordered_quantity')" class="form-control-sm text-right" placeholder="订购数量" enterkeyhint="next"></td>
                            <td style="width:75px"><input type="text" value="${{item.quantity && item.quantity > 0 ? item.quantity.toFixed(2) : ''}}" onchange="updateQty(${{index}}, this)" onkeydown="handleEnterKey(event, ${{index}}, 'quantity')" class="form-control-sm text-right" enterkeyhint="next"></td>
                            <td style="width:85px"><input type="text" value="${{(item.unit_price || 0).toFixed(2)}}" onchange="updatePrice(${{index}}, this)" onkeydown="handleEnterKey(event, ${{index}}, 'unit_price')" class="form-control-sm text-right" enterkeyhint="next"></td>
                            <td style="width:110px">${{item.amount.toFixed(2)}}</td>
                            <td style="width:110px">
                                <div class="position-relative">
                                    <input type="text" id="warehouseInput_${{index}}" class="form-control-sm" placeholder="单击选/双击搜" readonly
                                           value="${{item.warehouse_name || ''}}"
                                           onclick="showItemWarehouseDropdown(${{index}}, '')"
                                           ondblclick="this.readOnly=false;this.value='';showItemWarehouseDropdown(${{index}},'')"
                                           oninput="showItemWarehouseDropdown(${{index}}, this.value)"
                                           onblur="setTimeout(function(){{var d=document.getElementById('warehouseDropdown_'+${{index}});if(d){{d.style.display='none';}}}},200)"
                                           onkeydown="handleEnterKey(event, ${{index}}, 'warehouse')">
                                    <input type="hidden" id="warehouseId_${{index}}" value="${{item.warehouse_id || 0}}">
                                    <div id="warehouseDropdown_${{index}}" class="search-dropdown"></div>
                                </div>
                            </td>
                            <td style="width:120px"><input type="text" value="${{item.remark || ''}}" onchange="updateRemark(${{index}}, this)" onkeydown="handleEnterKey(event, ${{index}}, 'remark')" class="form-control-sm" placeholder="单品备注" enterkeyhint="next"></td>
                            <td style="width:65px"><button onclick="removeItem(${{index}})" class="btn btn-danger btn-sm">删除</button></td>
                        </tr>
                    `;
                }});
                document.getElementById('totalAmount').textContent = total.toFixed(2);
                updateFinalAmount();
            }}

            let searchTimeout = null;
            let productSearchActiveIndex = -1;

            // WPS 风格：回车后跳到下一行商品名称；最后一行则新增明细并聚焦新行的商品名称
            function focusNextProductName(index) {{
                const nextIndex = index + 1;
                const tbody = document.getElementById('itemsTable');
                if (tbody && nextIndex < items.length && tbody.rows[nextIndex]) {{
                    const targetInput = tbody.rows[nextIndex].querySelector('.product-search-input');
                    if (targetInput) {{
                        targetInput.focus();
                        try {{ targetInput.select(); }} catch(e) {{}}
                        return;
                    }}
                }}
                // 最后一行：新增明细，焦点留在新增行的商品名称
                addItem();
                const newTbody = document.getElementById('itemsTable');
                if (newTbody && newTbody.rows[items.length - 1]) {{
                    const newInput = newTbody.rows[items.length - 1].querySelector('.product-search-input');
                    if (newInput) newInput.focus();
                }}
            }}

            // WPS 风格：↑/↓ 同列上下移动焦点（按当前所在列定位上一行/下一行同一列的输入框）
            function moveSameColumnFocus(index, delta, event) {{
                const input = event.target;
                const td = input.closest('td');
                if (!td) return;
                const cellIndex = td.cellIndex;
                const targetIndex = index + delta;
                const tbody = document.getElementById('itemsTable');
                if (!tbody || targetIndex < 0 || targetIndex >= items.length) return;
                const targetRow = tbody.rows[targetIndex];
                if (!targetRow || !targetRow.cells[cellIndex]) return;
                const targetInput = targetRow.cells[cellIndex].querySelector('input, select');
                if (targetInput) {{
                    targetInput.focus();
                    try {{ targetInput.select(); }} catch(e) {{}}
                }}
            }}

            // WPS 风格键盘录入：商品名称输入框
            // - 有模糊搜索下拉时：↑/↓ 移动高亮，Enter 选中，Esc 关闭
            // - 无下拉时：Enter 跳到下一行商品名称，最后一行则新增明细并聚焦
            function handleProductNameKeydown(event, index, input) {{
                const dropdown = document.getElementById('searchDropdown_' + index);
                const lis = dropdown ? dropdown.querySelectorAll('li') : [];
                const dropdownVisible = dropdown && dropdown.style.display !== 'none' && lis.length > 0;

                if (event.key === 'ArrowDown') {{
                    if (dropdownVisible) {{
                        event.preventDefault();
                        if (productSearchActiveIndex >= 0) lis[productSearchActiveIndex].classList.remove('active');
                        productSearchActiveIndex = (productSearchActiveIndex + 1) % lis.length;
                        lis[productSearchActiveIndex].classList.add('active');
                        lis[productSearchActiveIndex].scrollIntoView({{ block: 'nearest' }});
                    }} else {{
                        // 无下拉：同列下一行
                        event.preventDefault();
                        moveSameColumnFocus(index, 1, event);
                    }}
                    return;
                }}
                if (event.key === 'ArrowUp') {{
                    if (dropdownVisible) {{
                        event.preventDefault();
                        if (productSearchActiveIndex >= 0) lis[productSearchActiveIndex].classList.remove('active');
                        productSearchActiveIndex = (productSearchActiveIndex - 1 + lis.length) % lis.length;
                        lis[productSearchActiveIndex].classList.add('active');
                        lis[productSearchActiveIndex].scrollIntoView({{ block: 'nearest' }});
                    }} else {{
                        // 无下拉：同列上一行
                        event.preventDefault();
                        moveSameColumnFocus(index, -1, event);
                    }}
                    return;
                }}
                if (event.key === 'Escape') {{
                    if (dropdownVisible) {{
                        event.preventDefault();
                        dropdown.style.display = 'none';
                        productSearchActiveIndex = -1;
                    }}
                    return;
                }}
                if (event.key === 'Enter' || event.keyCode === 13) {{
                    event.preventDefault();
                    if (dropdownVisible) {{
                        // 下拉可见：选中当前高亮项（未高亮时默认第一项），
                        // 选中完成后继续 WPS 录入：跳到下一行商品名称，末行则新增明细并聚焦
                        const li = productSearchActiveIndex >= 0 ? lis[productSearchActiveIndex] : lis[0];
                        productSearchActiveIndex = -1;
                        selectProduct(index, li, function() {{ focusNextProductName(index); }});
                        return;
                    }}
                    // 无下拉：回车跳下一行商品名称，末行则新增明细并聚焦（WPS 风格）
                    focusNextProductName(index);
                }}
            }}

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
                        productSearchActiveIndex = -1;
                        dropdown.style.display = 'block';
                    }} else {{
                        dropdown.innerHTML = '';
                        dropdown.style.display = 'none';
                    }}
                }}, 300);
            }}

            // 拉取商品最近采购价并在选中商品后做同基础单位对比提示
            // kind = 'purchase'：采购价对比最近采购价
            // kind = 'sales'：销售零售价对比最近采购价
            async function checkPriceAfterSelect(productId, currentBaseUnit, currentPrice, kind, productName) {{
                try {{
                    const res = await fetch('/api/product/last_purchase_price?product_id=' + productId);
                    if (!res.ok) return;
                    const data = await res.json();
                    const lastPrice = parseFloat(data.purchase_price) || 0;
                    const lastUnit = data.base_unit || '';
                    if (lastPrice <= 0) return;
                    if (lastUnit !== currentBaseUnit) {{
                        return;
                    }}
                    if (kind === 'purchase' && Math.abs(currentPrice - lastPrice) >= 0.01) {{
                        const diff = currentPrice - lastPrice;
                        const sign = diff > 0 ? '上涨' : '下降';
                        const tip = '【价格提示】\\n商品：' + productName + '\\n最近采购价（基础单位 ' + lastUnit + '）：' + lastPrice.toFixed(2) + '\\n本次采购价：' + currentPrice.toFixed(2) + '\\n' + sign + ' ' + Math.abs(diff).toFixed(2) + '（' + (Math.abs(diff / lastPrice * 100)).toFixed(1) + '%）';
                        if (!confirm(tip + '\\n\\n是否继续？')) {{
                        }}
                    }} else if (kind === 'sales' && currentPrice < lastPrice) {{
                        const tip = '【价格提示】\\n商品：' + productName + '\\n最近采购价（基础单位 ' + lastUnit + '）：' + lastPrice.toFixed(2) + '\\n本次零售价：' + currentPrice.toFixed(2) + '\\n零售价低于采购价 ' + (lastPrice - currentPrice).toFixed(2);
                        if (!confirm(tip + '\\n\\n是否继续？')) {{
                        }}
                    }}
                }} catch(e) {{
                    console.error('价格比较失败:', e);
                }}
            }}

            function selectProduct(index, li, afterSelect) {{
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

                // 采购单：本次采购价与最近采购价对比（同基础单位）
                checkPriceAfterSelect(items[index].product_id, items[index].base_unit, items[index].unit_price, 'purchase', items[index].product_name);

                fetch('/api/product/unit/list?product_id=' + items[index].product_id)
                    .then(res => res.json())
                    .then(units => {{
                        items[index].units = units;
                        renderItems();
                        if (afterSelect) afterSelect();
                    }})
                    .catch(() => {{
                        items[index].units = [];
                        renderItems();
                        if (afterSelect) afterSelect();
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
                    items[index].unit_price = Math.round(purchasePrice * 100) / 100;
                }} else {{
                    const basePrice = parseFloat(opt.getAttribute('data-base-price')) || items[index].unit_price;
                    items[index].unit_price = Math.round(basePrice * ratio * 100) / 100;
                }}
                items[index].base_quantity = Math.round(items[index].quantity * ratio * 100) / 100;
                items[index].amount = Math.round(items[index].unit_price * items[index].quantity * 100) / 100;
                renderItems();
            }}

            function updateName(index, input) {{ items[index].product_name = input.value; }}
            function updateAlias1(index, input) {{ items[index].alias1 = input.value; }}
            function updateAlias2(index, input) {{ items[index].alias2 = input.value; }}
            function updateSpec(index, input) {{ items[index].spec = input.value; }}
            function updatePrice(index, input) {{ 
                items[index].unit_price = Math.round((parseFloat(input.value) || 0) * 100) / 100; 
                items[index].amount = Math.round(items[index].unit_price * items[index].quantity * 100) / 100;
                renderItems();
            }}
            function updateQty(index, input) {{ 
                items[index].quantity = Math.round((parseFloat(input.value) || 0) * 100) / 100; 
                items[index].base_quantity = Math.round(items[index].quantity * (items[index].ratio || 1) * 100) / 100;
                items[index].amount = Math.round(items[index].unit_price * items[index].quantity * 100) / 100;
                renderItems();
            }}
            function updateOrderedQty(index, input) {{ 
                items[index].ordered_quantity = Math.round((parseFloat(input.value) || 0) * 100) / 100; 
            }}

            function handleEnterKey(event, index, field) {{
                // Enter: 同列下一行 (WPS 风格)
                // Tab:   同行下一格，行末换到下一行第一格 (WPS 风格)
                if (event.key === 'Tab') {{
                    event.preventDefault();
                    handleCellNavigation(event.target, 'next-in-row', index, field);
                    return;
                }}
                // ↑/↓：同列上下移动焦点（WPS 风格）
                if (event.key === 'ArrowUp') {{
                    event.preventDefault();
                    moveSameColumnFocus(index, -1, event);
                    return;
                }}
                if (event.key === 'ArrowDown') {{
                    event.preventDefault();
                    moveSameColumnFocus(index, 1, event);
                    return;
                }}
                const enterKeys = ['Enter', 'Next', 'Go', 'Done'];
                if (enterKeys.includes(event.key) || event.keyCode === 13) {{
                    event.preventDefault();

                    const input = event.target;
                    // renderItems 会重建 DOM，先捕获当前列索引用于回车后同列下移
                    const td = input.closest('td');
                    const cellIndex = td ? td.cellIndex : -1;
                    // 回车时同步当前输入值（与 onchange 一致），避免 renderItems 覆盖未提交的值
                    if (field === 'quantity') {{
                        items[index].quantity = parseFloat(input.value) || 0;
                        items[index].base_quantity = items[index].quantity * (items[index].ratio || 1);
                        items[index].amount = items[index].unit_price * items[index].quantity;
                    }} else if (field === 'unit_price') {{
                        items[index].unit_price = parseFloat(input.value) || 0;
                        items[index].amount = items[index].unit_price * items[index].quantity;
                    }} else if (field === 'ordered_quantity') {{
                        items[index].ordered_quantity = Math.round((parseFloat(input.value) || 0) * 100) / 100;
                    }} else if (field === 'spec') {{
                        items[index].spec = input.value;
                    }} else if (field === 'remark') {{
                        items[index].remark = input.value.trim();
                    }}
                    renderItems();

                    // WPS 风格：回车同列下一行
                    const nextIndex = index + 1;
                    if (cellIndex >= 0 && nextIndex < items.length) {{
                        const tbody = document.getElementById('itemsTable');
                        if (tbody && tbody.rows[nextIndex] && tbody.rows[nextIndex].cells[cellIndex]) {{
                            const targetInput = tbody.rows[nextIndex].cells[cellIndex].querySelector('input, select');
                            if (targetInput) {{
                                targetInput.focus();
                                try {{ targetInput.select(); }} catch(e) {{}}
                            }}
                        }}
                    }}
                }}
            }}

            // WPS 风格 Tab 同行导航：同 row 从左到右，行末换下一行第一个 input
            // 跳过 select（单位/供应商等下拉），用户用方向键展开
            function handleCellNavigation(currentInput, direction, index, field) {{
                const tr = currentInput.closest('tr');
                if (!tr) return;
                const tbody = tr.parentElement;
                if (!tbody) return;
                const cells = Array.from(tr.cells);
                const currentCell = currentInput.closest('td');
                if (!currentCell) return;
                const currentCellIndex = cells.indexOf(currentCell);

                // 同行下一个可聚焦的 input（跳过 select）
                for (let i = currentCellIndex + 1; i < cells.length; i++) {{
                    const inputs = cells[i].querySelectorAll('input');
                    if (inputs.length > 0) {{
                        inputs[0].focus();
                        try {{ inputs[0].select(); }} catch(e) {{}}
                        return;
                    }}
                }}

                // 同行无更多 input，换到下一行第一个 input
                const nextRow = tbody.rows[index + 1];
                if (nextRow) {{
                    const firstInput = nextRow.querySelector('input');
                    if (firstInput) {{
                        firstInput.focus();
                        try {{ firstInput.select(); }} catch(e) {{}}
                    }}
                }}
            }}

            function updateRemark(index, input) {{
                items[index].remark = input.value.trim();
            }}

            let currentOrderId = null;
            let currentVersion = 1;

            function orderStatusLabel(s) {{
                const map = {{ 'pending': '待审核', 'confirmed': '已审核', 'sorting': '分拣中', 'sorted': '已分拣', 'delivering': '配送中', 'delivered': '已送达', 'accepted': '已验收', 'settled': '已结算', 'cancelled': '已作废' }};
                return map[s] || s;
            }}

            // 审核：pending → confirmed，锁定订单（需审核权限）
            async function approveOrder(id) {{
                if (!confirm('确定审核通过该订单？审核后订单将被锁定，修改需管理员反审核。')) return;
                const res = await fetch('/api/purchase_order/approve/' + id, {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ reason: '' }})
                }});
                const text = await res.text();
                if (res.ok) {{
                    alert('审核成功');
                    loadOrders();
                }} else {{
                    alert(text || '审核失败');
                }}
            }}

            // 反审核：confirmed → pending，解锁订单（仅管理员，强制原因）
            async function unapproveOrder(id) {{
                const reason = prompt('请输入反审核原因（必填）：');
                if (reason === null) return;
                if (!reason.trim()) {{
                    alert('反审核必须填写原因');
                    return;
                }}
                const res = await fetch('/api/purchase_order/unapprove/' + id, {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ reason: reason.trim() }})
                }});
                const text = await res.text();
                if (res.ok) {{
                    alert('反审核成功，订单已解锁');
                    loadOrders();
                }} else {{
                    alert(text || '反审核失败');
                }}
            }}

            async function saveOrder() {{
                const supplierId = document.getElementById('supplierId').value;
                if (!supplierId) {{
                    alert('请选择供应商');
                    return;
                }}
                const validItems = items.filter(item => item.product_id > 0 && item.product_name.trim() !== '');
                if (validItems.length === 0) {{
                    alert('请添加商品明细');
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
                    warehouse_id: 0,
                    warehouse_name: '',
                    user_id: parseInt(document.getElementById('handlerId').value) || null,
                    items: validItems,
                    remark: document.getElementById('remarkInput').value || null,
                    version: currentVersion
                }};
                const url = currentOrderId ? '/api/purchase_order/update' : '/api/purchase_order/create';
                const res = await fetch(url, {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify(data)
                }});
                if (res.ok) {{
                    location.reload();
                }} else {{
                    const text = await res.text();
                    alert(text || '保存失败');
                }}
            }}

            async function loadOrderDetail(id) {{
                const res = await fetch('/api/purchase_order/detail/' + id);
                const order = await res.json();
                currentOrderId = order.id;
                currentVersion = order.version || 1;
                document.getElementById('supplierId').value = order.supplier_id;
                document.getElementById('supplierInput').value = order.supplier_name;
                document.getElementById('orderNoInput').value = order.order_no;
                document.getElementById('orderDateInput').value = order.order_date;
                document.getElementById('remarkInput').value = order.remark || '';
                document.getElementById('discountRateInput').value = order.discount_rate || 0;
                document.getElementById('amountReductionInput').value = order.amount_reduction || 0;
                // 回显经手人
                const handlerSelect = document.getElementById('handlerSelect');
                const handlerId = order.user_id || 0;
                if (handlerSelect) {{
                    if (!handlerSelect.querySelector('option[value=\"' + handlerId + '\"]') && handlerId > 0) {{
                        // 用户列表可能还没加载好，先放默认
                        await loadUsers();
                    }}
                    handlerSelect.value = String(handlerId);
                    document.getElementById('handlerId').value = String(handlerId);
                }}
                
                items = [];
                for (const item of order.items) {{
                    const itemData = {{
                        id: item.id || null,
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
                        ordered_quantity: item.ordered_quantity || 0,
                        remark: item.remark || '',
                        warehouse_id: item.warehouse_id || 0,
                        warehouse_name: item.warehouse_name || '',
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
                document.getElementById('orderNoInput').value = '';
                const d = new Date();
                document.getElementById('orderDateInput').value = d.getFullYear() + '-' + String(d.getMonth() + 1).padStart(2, '0') + '-' + String(d.getDate()).padStart(2, '0');
                document.getElementById('remarkInput').value = '';
                document.getElementById('discountRateInput').value = '0';
                items = [];
                renderItems();
                generateOrderNo('purchase');
            }}
        </script>
    "#, now);
    
    Html(crate::layout_html("采购订单", "/purchase", &content))
}

pub async fn page_sales(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/sales").await {
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
                        <tr><th style="min-width:180px">商品名称</th><th style="width:55px">规格</th><th style="width:75px">单位</th><th style="width:85px">预售数量</th><th style="width:75px">数量</th><th style="width:85px">单价</th><th style="width:110px">金额</th><th style="width:120px">供应商</th><th style="width:120px">备注</th><th style="width:65px">操作</th></tr>
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

                <div class="mt-3 d-flex align-items-center flex-wrap" style="gap:8px;">
                    <button onclick="saveOrder()" class="btn btn-success" id="saveBtn">保存销售订单</button>
                    <button onclick="resetForm()" class="btn btn-secondary">新建订单</button>
                    <button onclick="updatePrices()" class="btn btn-warning" id="updatePricesBtn" style="display:none">一键更新售价</button>

                    <input type="file" id="customerOrderImageInput" accept="image/*" style="display:none" onchange="uploadSalesOrderImage('customer')">
                    <button type="button" class="btn btn-outline-primary" onclick="document.getElementById('customerOrderImageInput').click()">📷 上传客户订单</button>
                    <a id="customerOrderImageLink" href="javascript:void(0)" target="_blank" style="display:none;">
                        <img id="customerOrderImageThumb" src="" style="height:38px;width:38px;object-fit:cover;border-radius:4px;border:1px solid #ddd;">
                    </a>
                    <button type="button" class="btn btn-sm btn-outline-danger" id="customerOrderImageDeleteBtn" onclick="deleteSalesOrderImage('customer')" style="display:none">删除</button>

                    <input type="file" id="signedOrderImageInput" accept="image/*" style="display:none" onchange="uploadSalesOrderImage('signed')">
                    <button type="button" class="btn btn-outline-primary" onclick="document.getElementById('signedOrderImageInput').click()">📷 上传已验收签字订单</button>
                    <a id="signedOrderImageLink" href="javascript:void(0)" target="_blank" style="display:none;">
                        <img id="signedOrderImageThumb" src="" style="height:38px;width:38px;object-fit:cover;border-radius:4px;border:1px solid #ddd;">
                    </a>
                    <button type="button" class="btn btn-sm btn-outline-danger" id="signedOrderImageDeleteBtn" onclick="deleteSalesOrderImage('signed')" style="display:none">删除</button>
                </div>
            </div>
        </div>

        <!-- 售价变更提示弹窗 -->
        <style>
            #priceChangeModal {{ display: none; position: fixed; z-index: 9999; left: 0; top: 0; width: 100%; height: 100%; background: rgba(0,0,0,0.5); }}
            #priceChangeModal .modal-dialog {{ margin: 80px auto; max-width: 600px; }}
            #priceChangeModal .modal-content {{ background: #fff; border-radius: 8px; box-shadow: 0 4px 20px rgba(0,0,0,0.3); }}
            #priceChangeModal .modal-header {{ padding: 12px 16px; border-bottom: 1px solid #dee2e6; display: flex; justify-content: space-between; align-items: center; }}
            #priceChangeModal .modal-body {{ padding: 12px 16px; max-height: 400px; overflow-y: auto; }}
            #priceChangeModal .modal-footer {{ padding: 12px 16px; border-top: 1px solid #dee2e6; text-align: right; }}
            #priceChangeModal .close {{ border: none; background: none; font-size: 24px; cursor: pointer; }}
        </style>
        <div id="priceChangeModal" class="modal" style="display:none;">
            <div class="modal-dialog">
                <div class="modal-content">
                    <div class="modal-header">
                        <h5 class="modal-title">售价变更明细</h5>
                        <button type="button" class="close" onclick="document.getElementById('priceChangeModal').style.display='none'">&times;</button>
                    </div>
                    <div class="modal-body" style="max-height:400px;overflow-y:auto;">
                        <table class="table table-sm table-bordered">
                            <thead><tr><th>商品名称</th><th>原售价</th><th>新售价</th><th>变动</th></tr></thead>
                            <tbody id="priceChangeBody"></tbody>
                        </table>
                    </div>
                    <div class="modal-footer">
                        <button type="button" class="btn btn-secondary" onclick="document.getElementById('priceChangeModal').style.display='none'">关闭</button>
                    </div>
                </div>
            </div>
        </div>

        <h4>销售订单列表</h4>
        <div class="mb-3">
            <input type="text" id="searchInput" class="form-control" placeholder="搜索订单号、采购单位、日期..." oninput="searchOrders()" style="width: 250px; display: inline-block;">
            <div class="position-relative" style="display: inline-block; width: 200px; vertical-align: top;">
                <select id="purchaserSelect" class="form-control" onchange="searchOrders()">
                    <option value="">全部采购单位</option>
                </select>
            </div>
            <button onclick="searchOrders()" class="btn btn-primary ml-2">搜索</button>
            <button onclick="resetSearch()" class="btn btn-secondary ml-2">重置</button>
            <button onclick="cancelOrder()" class="btn btn-warning ml-2">取消</button>
            <a href="javascript:void(0)" onclick="exportFilteredSalesOrders()" class="btn btn-success ml-2">导出</a>
            <button onclick="importSalesOrders()" class="btn btn-warning ml-2">导入</button>
            <input type="file" id="salesOrderFileInput" style="display:none" accept=".csv" onchange="handleSalesOrderFile(this)">
        </div>
        <table class="table table-bordered">
            <thead><tr><th>ID</th><th onclick="sortOrders('order_no')" style="cursor:pointer">订单号<span id="sortIndicator_order_no"></span></th><th onclick="sortOrders('order_date')" style="cursor:pointer">日期<span id="sortIndicator_order_date"></span></th><th onclick="sortOrders('unit_name')" style="cursor:pointer">采购单位<span id="sortIndicator_unit_name"></span></th><th>金额</th><th>下浮后</th><th>折减</th><th>最终金额</th><th onclick="sortOrders('status')" style="cursor:pointer">状态<span id="sortIndicator_status"></span></th><th>操作</th></tr></thead>
            <tbody id="orderListBody"></tbody>
        </table>

        <div id="pagination" class="mt-3"></div>

        <script>
            let purchasers = [];
            let items = [];
            let sortField = '';
            let sortOrder = 'desc';

            function sortOrders(field) {{
                if (sortField === field) {{
                    sortOrder = sortOrder === 'asc' ? 'desc' : 'asc';
                }} else {{
                    sortField = field;
                    sortOrder = 'asc';
                }}
                updateSortIndicators();
                loadOrders();
            }}

            function updateSortIndicators() {{
                const fields = ['order_no', 'order_date', 'unit_name', 'status'];
                fields.forEach(f => {{
                    const el = document.getElementById('sortIndicator_' + f);
                    if (el) {{
                        el.textContent = (sortField === f) ? (sortOrder === 'asc' ? ' ▲' : ' ▼') : '';
                    }}
                }});
            }}

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
                items.push({{ product_id: 0, product_name: '', alias1: '', alias2: '', spec: '', unit: '', base_unit: '', unit_price: 0, quantity: 0, base_quantity: 0, amount: 0, pre_sale_quantity: 0, ratio: 1, units: [], supplier_id: 0, supplier_name: '' }});
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
                                           onkeydown="handleProductNameKeydown(event, ${{index}}, this)"
                                           class="form-control-sm product-search-input" 
                                           placeholder="输入商品名称搜索"
                                           enterkeyhint="next">
                                    <div id="searchDropdown_${{index}}" class="search-dropdown"></div>
                                </div>
                            </td>
                            <td style="width:55px"><input type="text" value="${{item.spec}}" onchange="updateSpec(${{index}}, this)" onkeydown="handleEnterKey(event, ${{index}}, 'spec')" class="form-control-sm"></td>
                            <td style="width:75px">
                                <select onchange="updateUnit(${{index}}, this)" class="form-control-sm">
                                    ${{unitOptions}}
                                </select>
                            </td>
                            <td style="width:85px"><input type="text" value="${{item.pre_sale_quantity != null && item.pre_sale_quantity > 0 ? item.pre_sale_quantity.toFixed(2) : ''}}" onchange="updatePreSaleQty(${{index}}, this)" onkeydown="handleEnterKey(event, ${{index}}, 'pre_sale_quantity')" class="form-control-sm text-right" placeholder="预售数量" enterkeyhint="next"></td>
                            <td style="width:75px"><input type="text" value="${{item.quantity && item.quantity > 0 ? item.quantity.toFixed(2) : ''}}" onchange="updateQty(${{index}}, this)" onkeydown="handleEnterKey(event, ${{index}}, 'quantity')" class="form-control-sm text-right" enterkeyhint="next"></td>
                            <td style="width:85px"><input type="text" value="${{(item.unit_price || 0).toFixed(2)}}" onchange="updatePrice(${{index}}, this)" onkeydown="handleEnterKey(event, ${{index}}, 'unit_price')" class="form-control-sm text-right" enterkeyhint="next"></td>
                            <td style="width:110px">${{item.amount.toFixed(2)}}</td>
                            <td style="width:120px">
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
                            <td style="width:120px"><input type="text" value="${{item.remark || ''}}" onchange="updateRemark(${{index}}, this)" onkeydown="handleEnterKey(event, ${{index}}, 'remark')" class="form-control-sm" placeholder="单品备注" enterkeyhint="next"></td>
                            <td style="width:65px"><button onclick="removeItem(${{index}})" class="btn btn-danger btn-sm">删除</button></td>
                        </tr>
                    `;
                }});
                document.getElementById('totalAmount').textContent = total.toFixed(2);
                updateFinalAmount();
            }}

            let searchTimeout = null;
            let productSearchActiveIndex = -1;

            // WPS 风格：回车后跳到下一行商品名称；最后一行则新增明细并聚焦新行的商品名称
            function focusNextProductName(index) {{
                const nextIndex = index + 1;
                const tbody = document.getElementById('itemsTable');
                if (tbody && nextIndex < items.length && tbody.rows[nextIndex]) {{
                    const targetInput = tbody.rows[nextIndex].querySelector('.product-search-input');
                    if (targetInput) {{
                        targetInput.focus();
                        try {{ targetInput.select(); }} catch(e) {{}}
                        return;
                    }}
                }}
                // 最后一行：新增明细，焦点留在新增行的商品名称
                addItem();
                const newTbody = document.getElementById('itemsTable');
                if (newTbody && newTbody.rows[items.length - 1]) {{
                    const newInput = newTbody.rows[items.length - 1].querySelector('.product-search-input');
                    if (newInput) newInput.focus();
                }}
            }}

            // WPS 风格：↑/↓ 同列上下移动焦点（按当前所在列定位上一行/下一行同一列的输入框）
            function moveSameColumnFocus(index, delta, event) {{
                const input = event.target;
                const td = input.closest('td');
                if (!td) return;
                const cellIndex = td.cellIndex;
                const targetIndex = index + delta;
                const tbody = document.getElementById('itemsTable');
                if (!tbody || targetIndex < 0 || targetIndex >= items.length) return;
                const targetRow = tbody.rows[targetIndex];
                if (!targetRow || !targetRow.cells[cellIndex]) return;
                const targetInput = targetRow.cells[cellIndex].querySelector('input, select');
                if (targetInput) {{
                    targetInput.focus();
                    try {{ targetInput.select(); }} catch(e) {{}}
                }}
            }}

            // WPS 风格键盘录入：商品名称输入框
            // - 有模糊搜索下拉时：↑/↓ 移动高亮，Enter 选中，Esc 关闭
            // - 无下拉时：Enter 跳到下一行商品名称，最后一行则新增明细并聚焦
            function handleProductNameKeydown(event, index, input) {{
                const dropdown = document.getElementById('searchDropdown_' + index);
                const lis = dropdown ? dropdown.querySelectorAll('li') : [];
                const dropdownVisible = dropdown && dropdown.style.display !== 'none' && lis.length > 0;

                if (event.key === 'ArrowDown') {{
                    if (dropdownVisible) {{
                        event.preventDefault();
                        if (productSearchActiveIndex >= 0) lis[productSearchActiveIndex].classList.remove('active');
                        productSearchActiveIndex = (productSearchActiveIndex + 1) % lis.length;
                        lis[productSearchActiveIndex].classList.add('active');
                        lis[productSearchActiveIndex].scrollIntoView({{ block: 'nearest' }});
                    }} else {{
                        // 无下拉：同列下一行
                        event.preventDefault();
                        moveSameColumnFocus(index, 1, event);
                    }}
                    return;
                }}
                if (event.key === 'ArrowUp') {{
                    if (dropdownVisible) {{
                        event.preventDefault();
                        if (productSearchActiveIndex >= 0) lis[productSearchActiveIndex].classList.remove('active');
                        productSearchActiveIndex = (productSearchActiveIndex - 1 + lis.length) % lis.length;
                        lis[productSearchActiveIndex].classList.add('active');
                        lis[productSearchActiveIndex].scrollIntoView({{ block: 'nearest' }});
                    }} else {{
                        // 无下拉：同列上一行
                        event.preventDefault();
                        moveSameColumnFocus(index, -1, event);
                    }}
                    return;
                }}
                if (event.key === 'Escape') {{
                    if (dropdownVisible) {{
                        event.preventDefault();
                        dropdown.style.display = 'none';
                        productSearchActiveIndex = -1;
                    }}
                    return;
                }}
                if (event.key === 'Enter' || event.keyCode === 13) {{
                    event.preventDefault();
                    if (dropdownVisible) {{
                        // 下拉可见：选中当前高亮项（未高亮时默认第一项），
                        // 选中完成后继续 WPS 录入：跳到下一行商品名称，末行则新增明细并聚焦
                        const li = productSearchActiveIndex >= 0 ? lis[productSearchActiveIndex] : lis[0];
                        productSearchActiveIndex = -1;
                        selectProduct(index, li, function() {{ focusNextProductName(index); }});
                        return;
                    }}
                    // 无下拉：回车跳下一行商品名称，末行则新增明细并聚焦（WPS 风格）
                    focusNextProductName(index);
                }}
            }}

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
                        productSearchActiveIndex = -1;
                        dropdown.style.display = 'block';
                    }} else {{
                        dropdown.innerHTML = '';
                        dropdown.style.display = 'none';
                    }}
                }}, 300);
            }}

            // 拉取商品最近采购价并在选中商品后做同基础单位对比提示
            // kind = 'purchase'：采购价对比最近采购价
            // kind = 'sales'：销售零售价对比最近采购价
            async function checkPriceAfterSelect(productId, currentBaseUnit, currentPrice, kind, productName) {{
                try {{
                    const res = await fetch('/api/product/last_purchase_price?product_id=' + productId);
                    if (!res.ok) return;
                    const data = await res.json();
                    const lastPrice = parseFloat(data.purchase_price) || 0;
                    const lastUnit = data.base_unit || '';
                    if (lastPrice <= 0) return;
                    if (lastUnit !== currentBaseUnit) {{
                        return;
                    }}
                    if (kind === 'purchase' && Math.abs(currentPrice - lastPrice) >= 0.01) {{
                        const diff = currentPrice - lastPrice;
                        const sign = diff > 0 ? '上涨' : '下降';
                        const tip = '【价格提示】\\n商品：' + productName + '\\n最近采购价（基础单位 ' + lastUnit + '）：' + lastPrice.toFixed(2) + '\\n本次采购价：' + currentPrice.toFixed(2) + '\\n' + sign + ' ' + Math.abs(diff).toFixed(2) + '（' + (Math.abs(diff / lastPrice * 100)).toFixed(1) + '%）';
                        if (!confirm(tip + '\\n\\n是否继续？')) {{
                        }}
                    }} else if (kind === 'sales' && currentPrice < lastPrice) {{
                        const tip = '【价格提示】\\n商品：' + productName + '\\n最近采购价（基础单位 ' + lastUnit + '）：' + lastPrice.toFixed(2) + '\\n本次零售价：' + currentPrice.toFixed(2) + '\\n零售价低于采购价 ' + (lastPrice - currentPrice).toFixed(2);
                        if (!confirm(tip + '\\n\\n是否继续？')) {{
                        }}
                    }}
                }} catch(e) {{
                    console.error('价格比较失败:', e);
                }}
            }}

            function selectProduct(index, li, afterSelect) {{
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

                // 销售单：本次零售价对比最近采购价（同基础单位），低于则提醒
                checkPriceAfterSelect(items[index].product_id, items[index].base_unit, items[index].unit_price, 'sales', items[index].product_name);

                fetch('/api/product/unit/list?product_id=' + items[index].product_id)
                    .then(res => res.json())
                    .then(units => {{
                        items[index].units = units;
                        renderItems();
                        if (afterSelect) afterSelect();
                    }})
                    .catch(() => {{
                        items[index].units = [];
                        renderItems();
                        if (afterSelect) afterSelect();
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
                    items[index].unit_price = Math.round(unitPrice * 100) / 100;
                }} else {{
                    const basePrice = parseFloat(opt.getAttribute('data-base-price')) || items[index].unit_price;
                    items[index].unit_price = Math.round(basePrice * ratio * 100) / 100;
                }}
                items[index].base_quantity = Math.round(items[index].quantity * ratio * 100) / 100;
                items[index].amount = Math.round(items[index].unit_price * items[index].quantity * 100) / 100;
                renderItems();
            }}

            function updateName(index, input) {{ items[index].product_name = input.value; }}
            function updateAlias1(index, input) {{ items[index].alias1 = input.value; }}
            function updateAlias2(index, input) {{ items[index].alias2 = input.value; }}
            function updateSpec(index, input) {{ items[index].spec = input.value; }}
            function updatePrice(index, input) {{ 
                items[index].unit_price = Math.round((parseFloat(input.value) || 0) * 100) / 100; 
                items[index].amount = Math.round(items[index].unit_price * items[index].quantity * 100) / 100;
                renderItems();
            }}
            function updateQty(index, input) {{ 
                items[index].quantity = Math.round((parseFloat(input.value) || 0) * 100) / 100; 
                items[index].base_quantity = Math.round(items[index].quantity * (items[index].ratio || 1) * 100) / 100;
                items[index].amount = Math.round(items[index].unit_price * items[index].quantity * 100) / 100;
                renderItems();
            }}
            function updatePreSaleQty(index, input) {{ 
                items[index].pre_sale_quantity = Math.round((parseFloat(input.value) || 0) * 100) / 100;
                if (parseFloat(input.value) > 0) {{
                    items[index].quantity = items[index].pre_sale_quantity;
                    items[index].amount = items[index].unit_price * items[index].quantity;
                }}
                renderItems();
            }}

            function handleEnterKey(event, index, field) {{
                // Enter: 同列下一行 (WPS 风格)
                // Tab:   同行下一格，行末换到下一行第一格 (WPS 风格)
                if (event.key === 'Tab') {{
                    event.preventDefault();
                    handleCellNavigation(event.target, 'next-in-row', index, field);
                    return;
                }}
                // ↑/↓：同列上下移动焦点（WPS 风格）
                if (event.key === 'ArrowUp') {{
                    event.preventDefault();
                    moveSameColumnFocus(index, -1, event);
                    return;
                }}
                if (event.key === 'ArrowDown') {{
                    event.preventDefault();
                    moveSameColumnFocus(index, 1, event);
                    return;
                }}
                const enterKeys = ['Enter', 'Next', 'Go', 'Done'];
                if (enterKeys.includes(event.key) || event.keyCode === 13) {{
                    event.preventDefault();

                    const input = event.target;
                    // renderItems 会重建 DOM，先捕获当前列索引用于回车后同列下移
                    const td = input.closest('td');
                    const cellIndex = td ? td.cellIndex : -1;
                    // 回车时同步当前输入值（与 onchange 一致），避免 renderItems 覆盖未提交的值
                    if (field === 'quantity') {{
                        items[index].quantity = parseFloat(input.value) || 0;
                        items[index].base_quantity = items[index].quantity * (items[index].ratio || 1);
                        items[index].amount = items[index].unit_price * items[index].quantity;
                    }} else if (field === 'unit_price') {{
                        items[index].unit_price = parseFloat(input.value) || 0;
                        items[index].amount = items[index].unit_price * items[index].quantity;
                    }} else if (field === 'pre_sale_quantity') {{
                        items[index].pre_sale_quantity = Math.round((parseFloat(input.value) || 0) * 100) / 100;
                        if (items[index].pre_sale_quantity > 0) {{
                            items[index].quantity = items[index].pre_sale_quantity;
                            items[index].amount = items[index].unit_price * items[index].quantity;
                        }}
                    }} else if (field === 'spec') {{
                        items[index].spec = input.value;
                    }} else if (field === 'remark') {{
                        items[index].remark = input.value.trim();
                    }}
                    renderItems();

                    // WPS 风格：回车同列下一行
                    const nextIndex = index + 1;
                    if (cellIndex >= 0 && nextIndex < items.length) {{
                        const tbody = document.getElementById('itemsTable');
                        if (tbody && tbody.rows[nextIndex] && tbody.rows[nextIndex].cells[cellIndex]) {{
                            const targetInput = tbody.rows[nextIndex].cells[cellIndex].querySelector('input, select');
                            if (targetInput) {{
                                targetInput.focus();
                                try {{ targetInput.select(); }} catch(e) {{}}
                            }}
                        }}
                    }}
                }}
            }}

            // WPS 风格 Tab 同行导航：同 row 从左到右，行末换下一行第一个 input
            // 跳过 select（单位/供应商等下拉），用户用方向键展开
            function handleCellNavigation(currentInput, direction, index, field) {{
                const tr = currentInput.closest('tr');
                if (!tr) return;
                const tbody = tr.parentElement;
                if (!tbody) return;
                const cells = Array.from(tr.cells);
                const currentCell = currentInput.closest('td');
                if (!currentCell) return;
                const currentCellIndex = cells.indexOf(currentCell);

                // 同行下一个可聚焦的 input（跳过 select）
                for (let i = currentCellIndex + 1; i < cells.length; i++) {{
                    const inputs = cells[i].querySelectorAll('input');
                    if (inputs.length > 0) {{
                        inputs[0].focus();
                        try {{ inputs[0].select(); }} catch(e) {{}}
                        return;
                    }}
                }}

                // 同行无更多 input，换到下一行第一个 input
                const nextRow = tbody.rows[index + 1];
                if (nextRow) {{
                    const firstInput = nextRow.querySelector('input');
                    if (firstInput) {{
                        firstInput.focus();
                        try {{ firstInput.select(); }} catch(e) {{}}
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
            let currentVersion = 1;

            // 反审核：confirmed → pending，解锁订单（仅管理员，强制原因）
            async function unapproveSalesOrder(id) {{
                const reason = prompt('请输入反审核原因（必填）：');
                if (reason === null) return;
                if (!reason.trim()) {{
                    alert('反审核必须填写原因');
                    return;
                }}
                const res = await fetch('/api/sales_order/unapprove/' + id, {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ reason: reason.trim() }})
                }});
                const text = await res.text();
                if (res.ok) {{
                    alert('反审核成功，订单已解锁');
                    loadOrders();
                }} else {{
                    alert(text || '反审核失败');
                }}
            }}

            async function saveOrder() {{
                const purchaserId = document.getElementById('purchaserId').value;
                if (!purchaserId) {{
                    alert('请选择采购单位');
                    return;
                }}
                const validItems = items.filter(item => item.product_id > 0 && item.product_name.trim() !== '');
                if (validItems.length === 0) {{
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
                    items: validItems,
                    remark: document.getElementById('remarkInput').value || null,
                    version: currentVersion
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
                    const errText = await res.text();
                    alert('保存失败: ' + (errText || res.statusText));
                }}
            }}

            async function uploadSalesOrderImage(imageType) {{
                if (!currentOrderId) {{
                    alert('请先保存销售订单后再上传图片');
                    return;
                }}
                const inputId = imageType === 'customer' ? 'customerOrderImageInput' : 'signedOrderImageInput';
                const input = document.getElementById(inputId);
                if (!input.files || input.files.length === 0) return;
                const formData = new FormData();
                formData.append('file', input.files[0]);
                const res = await fetch('/api/sales_order/upload_image?order_id=' + currentOrderId + '&type=' + imageType, {{
                    method: 'POST',
                    body: formData
                }});
                if (res.ok) {{
                    const result = await res.json();
                    setSalesOrderImage(imageType, result.url);
                }} else {{
                    const text = await res.text();
                    alert('图片上传失败：' + text);
                }}
                input.value = '';
            }}

            async function deleteSalesOrderImage(imageType) {{
                if (!currentOrderId) return;
                if (!confirm('确定删除该图片？')) return;
                const res = await fetch('/api/sales_order/delete_image?order_id=' + currentOrderId + '&type=' + imageType, {{
                    method: 'POST'
                }});
                if (res.ok) {{
                    setSalesOrderImage(imageType, null);
                }} else {{
                    alert('删除失败');
                }}
            }}

            function setSalesOrderImage(imageType, url) {{
                const prefix = imageType === 'customer' ? 'customerOrderImage' : 'signedOrderImage';
                const link = document.getElementById(prefix + 'Link');
                const thumb = document.getElementById(prefix + 'Thumb');
                const delBtn = document.getElementById(prefix + 'DeleteBtn');
                if (url) {{
                    link.href = url;
                    thumb.src = url;
                    link.style.display = 'inline-block';
                    delBtn.style.display = 'inline-block';
                }} else {{
                    link.href = '#';
                    thumb.src = '';
                    link.style.display = 'none';
                    delBtn.style.display = 'none';
                }}
            }}
            let currentPage = 1;
            let currentKeyword = '';
            // 超级管理员任何时刻都拥有反审核权限，故其反审核按钮不受订单状态限制
            let isSuperAdmin = false;
            fetch('/api/login/check').then(r => r.json()).then(d => {{
                if (d && d.logged_in) {{
                    isSuperAdmin = (d.user.role === 'super_admin');
                }}
                if (isSuperAdmin) loadOrders();
            }});

            function resetSearch() {{
                document.getElementById('searchInput').value = '';
                document.getElementById('purchaserSelect').value = '';
                currentKeyword = '';
                currentPage = 1;
                loadOrders();
            }}

            async function searchOrders() {{
                currentKeyword = document.getElementById('searchInput').value.trim();
                currentPage = 1;
                await loadOrders();
            }}

            // 按当前筛选条件（搜索关键字 + 采购单位）跳转导出，参数与列表保持一致
            function exportFilteredSalesOrders() {{
                const params = new URLSearchParams();
                const kw = document.getElementById('searchInput').value.trim();
                if (kw) params.set('keyword', kw);
                const purchaserId = document.getElementById('purchaserSelect').value;
                if (purchaserId) params.set('purchaser_id', purchaserId);
                if (sortField) {{
                    params.set('sort_field', sortField);
                    params.set('sort_order', sortOrder);
                }}
                const qs = params.toString();
                window.location = '/api/sales_order/export' + (qs ? '?' + qs : '');
            }}

            async function loadOrders(page) {{
                if (page !== undefined) currentPage = page;
                let url = '/api/sales_order/list?page=' + currentPage + '&page_size=20';
                if (currentKeyword) {{
                    url += '&keyword=' + encodeURIComponent(currentKeyword);
                }}
                const purchaserId = document.getElementById('purchaserSelect').value;
                if (purchaserId) {{
                    url += '&purchaser_id=' + purchaserId;
                }}
                if (sortField) {{
                    url += '&sort_field=' + sortField + '&sort_order=' + sortOrder;
                }}
                const res = await fetch(url);
                const result = await res.json();
                const orders = result.data || [];
                const tbody = document.getElementById('orderListBody');
                tbody.innerHTML = '';
                let sumAmount = 0, sumDiscounted = 0, sumReduction = 0, sumFinal = 0;
                orders.forEach(order => {{
                    const amount = order.total_amount;
                    const discounted = amount * (1 - (order.discount_rate || 0) / 100);
                    const reduction = order.amount_reduction || 0;
                    const finalAmt = order.final_amount || 0;
                    sumAmount += amount;
                    sumDiscounted += discounted;
                    sumReduction += reduction;
                    sumFinal += finalAmt;
                    const selected = currentOrderId === order.id ? ' style="cursor: pointer; background-color: #fff3cd;"' : ' style="cursor: pointer;"';
                    const statusMap = {{
                        'pending': '{{"text":"待审核","class":"bg-secondary"}}',
                        'confirmed': '{{"text":"已审核","class":"bg-primary"}}',
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
                        'pending': '{{"text":"审核","status":"confirmed"}}',
                        'confirmed': '{{"text":"开始分拣","status":"sorting"}}',
                        'sorting': '{{"text":"完成分拣","status":"sorted"}}',
                        'sorted': '{{"text":"开始配送","status":"delivering"}}',
                        'delivering': '{{"text":"确认送达","status":"delivered"}}',
                        'delivered': '{{"text":"确认验收","status":"accepted"}}',
                        'accepted': '{{"text":"确认结算","status":"settled"}}',
                        'settled': '{{"text":"","status":""}}'
                    }};
                    const nextInfo = JSON.parse(nextStatusMap[order.status] || '{{"text":"","status":""}}');
                    const nextBtn = nextInfo.text ? '<button onclick="event.stopPropagation(); updateOrderStatus(' + order.id + ', \'' + nextInfo.status + '\')" class="btn btn-primary btn-sm">' + nextInfo.text + '</button> ' : '';
                    const unapproveBtn = (order.status === 'confirmed' || isSuperAdmin) ? '<button onclick="event.stopPropagation(); unapproveSalesOrder(' + order.id + ')" class="btn btn-warning btn-sm">反审核</button> ' : '';
                    const reimburseBtn = order.is_reimburse ? '<button onclick="event.stopPropagation(); exportAcceptExcel(' + order.id + ')" class="btn btn-warning btn-sm">导出报销单</button> ' : '';
                    tbody.innerHTML += '<tr onclick="loadOrderDetail(' + order.id + ')"' + selected + '>' +
                        '<td>' + order.id + '</td>' +
                        '<td>' + order.order_no + '</td>' +
                        '<td>' + order.order_date + '</td>' +
                        '<td>' + order.purchaser_name + '</td>' +
                        '<td>' + amount.toFixed(2) + '</td>' +
                        '<td>' + discounted.toFixed(2) + '</td>' +
                        '<td>' + reduction.toFixed(2) + '</td>' +
                        '<td>' + finalAmt.toFixed(2) + '</td>' +
                        '<td>' + statusBadge + '</td>' +
                        '<td>' +
                        nextBtn +
                        unapproveBtn +
                        '<button onclick="event.stopPropagation(); exportRealExcel(' + order.id + ')" class="btn btn-success btn-sm">导出验收单</button> ' +
                        reimburseBtn +
                        '<button onclick="event.stopPropagation(); generatePurchaseOrders(' + order.id + ')" class="btn btn-info btn-sm">生成采购订单</button> ' +
                        '<button onclick="event.stopPropagation(); deleteOrder(' + order.id + ')" class="btn btn-danger btn-sm">删除</button>' +
                        '</td></tr>';
                }});
                if (orders.length > 0) {{
                    tbody.innerHTML += '<tr class="table-active fw-bold"><td colspan="4" class="text-end">合计</td><td>' + sumAmount.toFixed(2) + '</td><td>' + sumDiscounted.toFixed(2) + '</td><td>' + sumReduction.toFixed(2) + '</td><td>' + sumFinal.toFixed(2) + '</td><td colspan="2"></td></tr>';
                }}
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
                currentVersion = order.version || 1;
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
                setSalesOrderImage('customer', order.customer_order_image || null);
                setSalesOrderImage('signed', order.signed_order_image || null);
                
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
                        pre_sale_quantity: item.pre_sale_quantity || 0,
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
                document.getElementById('updatePricesBtn').style.display = 'inline-block';
            }}

            async function updatePrices() {{
                if (!currentOrderId) {{ alert('请先选择要编辑的订单'); return; }}
                if (!confirm('将自动填入商品最新基础售价，请核对后手动保存修改。\n确定继续？')) return;
                const btn = document.getElementById('updatePricesBtn');
                btn.disabled = true;
                btn.textContent = '获取中...';
                try {{
                    const res = await fetch('/api/sales_order/update_prices/' + currentOrderId, {{ method: 'POST' }});
                    const data = await res.json();
                    if (data.errors && data.errors.length > 0) {{
                        alert('部分商品获取售价失败：\n' + data.errors.join('\n'));
                    }}
                    // 记录变动明细
                    const changes = [];
                    const priceMap = {{}};
                    data.items.forEach(i => {{ priceMap[i.product_id] = i; }});
                    items.forEach((item, idx) => {{
                        const newData = priceMap[item.product_id];
                        if (newData) {{
                            const oldPrice = item.unit_price;
                            const newPrice = newData.unit_price;
                            const diff = newPrice - oldPrice;
                            if (Math.abs(diff) > 0.001) {{
                                changes.push({{
                                    name: item.product_name,
                                    oldPrice: oldPrice,
                                    newPrice: newPrice,
                                    diff: diff,
                                }});
                            }}
                            item.unit_price = newPrice;
                            item.amount = newData.amount;
                        }}
                    }});
                    renderItems();
                    // 显示变动弹窗
                    if (changes.length > 0) {{
                        const tbody = document.getElementById('priceChangeBody');
                        tbody.innerHTML = changes.map(c => {{
                            const cls = c.diff > 0 ? 'text-success' : 'text-danger';
                            const arrow = c.diff > 0 ? '↑' : '↓';
                            return `<tr>
                                <td>${{c.name}}</td>
                                <td>¥${{c.oldPrice.toFixed(2)}}</td>
                                <td>¥${{c.newPrice.toFixed(2)}}</td>
                                <td class="${{cls}}">${{arrow}} ¥${{Math.abs(c.diff).toFixed(2)}}</td>
                            </tr>`;
                        }}).join('');
                        document.getElementById('priceChangeModal').style.display = 'block';
                    }} else {{
                        alert('已获取 ' + data.items.length + ' 项商品最新售价，无变动。');
                    }}
                }} catch (e) {{
                    alert('获取失败：' + e.message);
                }} finally {{
                    btn.disabled = false;
                    btn.textContent = '一键更新售价';
                }}
            }}

            function printAccept(id) {{
                window.open('/accept?order_id=' + id, '_blank');
            }}
            
            async function downloadExcel(url, id, force) {{
                const f = force ? 1 : 0;
                const res = await fetch(url + id + '?force=' + f);
                const contentType = res.headers.get('content-type') || '';
                if (contentType.includes('application/json')) {{
                    const data = await res.json();
                    if (data.warning) {{
                        if (confirm(data.message + '\n\n确定导出？')) {{
                            downloadExcel(url, id, true);
                        }}
                    }} else if (data.error) {{
                        alert(data.message);
                    }}
                    return;
                }}
                const blob = await res.blob();
                const disposition = res.headers.get('content-disposition') || '';
                const match = disposition.match(/filename\*?=(?:UTF-8''|"?)([^;"]+)/);
                const filename = match ? decodeURIComponent(match[1]) : 'export.xlsx';
                const link = document.createElement('a');
                link.href = URL.createObjectURL(blob);
                link.download = filename;
                document.body.appendChild(link);
                link.click();
                document.body.removeChild(link);
                URL.revokeObjectURL(link.href);
            }}
            function exportAcceptExcel(id) {{
                downloadExcel('/api/sales_order/accept_excel/', id, false);
            }}
            function exportRealExcel(id) {{
                downloadExcel('/api/sales_order/real_excel/', id, false);
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

            async function generatePurchaseOrders(id, force) {{
                const f = force ? 1 : 0;
                const res = await fetch('/api/sales_order/generate_purchase/' + id + '?force=' + f, {{ method: 'POST' }});
                const contentType = res.headers.get('content-type') || '';
                if (contentType.includes('application/json')) {{
                    const data = await res.json();
                    // 已处理的采购订单：不允许重新生成，直接提示
                    if (data.error) {{
                        alert(data.message);
                        return;
                    }}
                    // pending 状态的重复：提示确认后删除旧单重新生成
                    if (data.warning) {{
                        if (confirm(data.message)) {{
                            generatePurchaseOrders(id, true);
                        }}
                        return;
                    }}
                    if (res.ok) {{
                        alert('成功生成 ' + data.count + ' 张采购订单');
                        loadOrders();
                    }} else {{
                        alert('生成失败：' + (data.message || '未知错误'));
                    }}
                    return;
                }}
                if (res.ok) {{
                    alert('生成成功');
                    loadOrders();
                }} else {{
                    alert('生成失败：未知错误');
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
                const d = new Date();
                document.getElementById('orderDateInput').value = d.getFullYear() + '-' + String(d.getMonth() + 1).padStart(2, '0') + '-' + String(d.getDate()).padStart(2, '0');
                document.getElementById('remarkInput').value = '';
                document.getElementById('discountRateInput').value = '20';
                setSalesOrderImage('customer', null);
                setSalesOrderImage('signed', null);
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
                const select = document.getElementById('purchaserSelect');
                if (select && purchasers.length > 0) {{
                    purchasers.forEach(p => {{
                        select.innerHTML += '<option value="' + p.id + '">' + p.name + '</option>';
                    }});
                }}
            }}
        </script>
    "#, now);
    
    Html(crate::layout_html("销售订单", "/sales", &content))
}

pub async fn page_query_purchase_order(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/query/purchase_order").await {
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
                            <thead><tr><th>商品名称</th><th>规格</th><th>单位</th><th>订购数量</th><th>数量</th><th>单价</th><th>金额</th></tr></thead>
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
                    tbody.innerHTML += '<tr><td>' + (item.product_name || '') + '</td><td>' + (item.spec || '-') + '</td><td>' + (item.unit || '') + '</td><td>' + (item.ordered_quantity || 0).toFixed(2) + '</td><td>' + (item.quantity || 0).toFixed(2) + '</td><td>¥' + (item.unit_price || 0).toFixed(2) + '</td><td>¥' + (item.amount || 0).toFixed(2) + '</td></tr>';
                });
                tbody.innerHTML += '<tr class="table-active fw-bold"><td colspan="6">合计</td><td>¥' + itemTotal.toFixed(2) + '</td></tr>';

                const modal = new bootstrap.Modal(document.getElementById('detailModal'));
                modal.show();
            }
            loadSuppliers();
        </script>
    "#;
    Html(crate::layout_html("采购订单查询", "/query/purchase_order", &content))
}

pub async fn page_query_purchase_document(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/query/purchase_document").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let today = Local::now().format("%Y-%m-%d").to_string();
    let content = format!(r#"
        <div class="card p-4">
            <h3>采购单据列表</h3>
            <p class="text-muted small">按供应商、日期采集采购单据图片。支持手机连续拍照/多选上传，图片按供应商+日期分组归档。</p>
            <div class="row">
                <div class="col-md-4">
                    <label class="form-label">供应商</label>
                    <select id="docSupplier" class="form-control">
                        <option value="">请选择供应商</option>
                    </select>
                </div>
                <div class="col-md-3">
                    <label class="form-label">单据日期</label>
                    <input type="date" id="docDate" class="form-control" value="{today}">
                </div>
                <div class="col-md-3">
                    <label class="form-label">备注（可选）</label>
                    <input type="text" id="docRemark" class="form-control" placeholder="如：验收单/送货单">
                </div>
                <div class="col-md-2 d-flex align-items-end">
                    <button class="btn btn-primary" onclick="loadDocuments()">查询</button>
                </div>
            </div>
            <div class="mt-3">
                <input type="file" id="docFileInput" accept="image/*" capture="environment" multiple style="display:none" onchange="uploadDocuments()">
                <button type="button" class="btn btn-success" onclick="startCapture()">📷 连续采集单据（拍照/多选）</button>
                <span id="uploadStatus" class="text-muted small ml-2"></span>
            </div>
        </div>

        <div class="card p-3 mt-3">
            <div id="docGroups"></div>
        </div>

        <div class="modal fade" id="docImageModal" tabindex="-1">
            <div class="modal-dialog modal-lg modal-dialog-centered">
                <div class="modal-content">
                    <div class="modal-body text-center p-0">
                        <img id="docModalImage" src="" style="max-width:100%;max-height:85vh;">
                    </div>
                </div>
            </div>
        </div>

        <script>
            async function loadSuppliersForDoc() {{
                const res = await fetch('/api/supplier/list');
                const suppliers = await res.json();
                const sel = document.getElementById('docSupplier');
                suppliers.forEach(s => {{
                    sel.innerHTML += '<option value="' + s.id + '">' + s.name + '</option>';
                }});
            }}

            function startCapture() {{
                const sid = document.getElementById('docSupplier').value;
                const date = document.getElementById('docDate').value;
                if (!sid) {{ alert('请先选择供应商'); return; }}
                if (!date) {{ alert('请先选择单据日期'); return; }}
                document.getElementById('docFileInput').click();
            }}

            async function uploadDocuments() {{
                const input = document.getElementById('docFileInput');
                if (!input.files || input.files.length === 0) return;
                const sel = document.getElementById('docSupplier');
                const sid = sel.value;
                const sname = sel.options[sel.selectedIndex].text;
                const date = document.getElementById('docDate').value;
                const remark = document.getElementById('docRemark').value || '';
                const statusEl = document.getElementById('uploadStatus');

                const files = Array.from(input.files);
                let done = 0;
                for (const file of files) {{
                    statusEl.textContent = '正在上传 ' + (done + 1) + '/' + files.length + ' ...';
                    const formData = new FormData();
                    formData.append('file', file);
                    formData.append('supplier_id', sid);
                    formData.append('supplier_name', sname);
                    formData.append('document_date', date);
                    formData.append('remark', remark);
                    try {{
                        const res = await fetch('/api/purchase_document/upload', {{ method: 'POST', body: formData }});
                        if (res.ok) done++;
                    }} catch (e) {{}}
                }}
                statusEl.textContent = '本次上传完成：' + done + '/' + files.length + ' 张';
                input.value = '';
                loadDocuments();
            }}

            async function loadDocuments() {{
                const sid = document.getElementById('docSupplier').value;
                const date = document.getElementById('docDate').value;
                let url = '/api/purchase_document/list?';
                if (sid) url += 'supplier_id=' + sid + '&';
                if (date) url += 'document_date=' + date;
                const res = await fetch(url);
                const docs = await res.json();
                const container = document.getElementById('docGroups');
                if (!docs || docs.length === 0) {{
                    container.innerHTML = '<p class="text-center text-muted">暂无单据</p>';
                    return;
                }}
                // 按 供应商+日期 分组
                const groups = {{}};
                docs.forEach(d => {{
                    const key = d.supplier_name + ' | ' + d.document_date;
                    if (!groups[key]) groups[key] = [];
                    groups[key].push(d);
                }});
                let html = '';
                Object.keys(groups).forEach(key => {{
                    const items = groups[key];
                    html += '<div class="mb-4">';
                    html += '<h6 class="border-bottom pb-1">' + key + ' <span class="badge badge-info">' + items.length + ' 张</span></h6>';
                    html += '<div class="d-flex flex-wrap" style="gap:10px;">';
                    items.forEach(d => {{
                        html += '<div style="position:relative;width:120px;">';
                        html += '<img src="' + d.image_url + '" style="width:120px;height:120px;object-fit:cover;border-radius:6px;border:1px solid #ddd;cursor:pointer;" onclick="showDocImage(\'' + d.image_url + '\')">';
                        html += '<button type="button" class="btn btn-sm btn-danger" style="position:absolute;top:2px;right:2px;padding:0 6px;" onclick="deleteDocument(' + d.id + ')">×</button>';
                        if (d.remark) html += '<div class="small text-muted text-truncate">' + d.remark + '</div>';
                        html += '</div>';
                    }});
                    html += '</div></div>';
                }});
                container.innerHTML = html;
            }}

            function showDocImage(url) {{
                document.getElementById('docModalImage').src = url;
                const modal = new bootstrap.Modal(document.getElementById('docImageModal'));
                modal.show();
            }}

            async function deleteDocument(id) {{
                if (!confirm('确定删除该单据图片？')) return;
                const res = await fetch('/api/purchase_document/delete/' + id, {{ method: 'DELETE' }});
                if (res.ok) {{
                    loadDocuments();
                }} else {{
                    alert('删除失败');
                }}
            }}

            loadSuppliersForDoc();
            loadDocuments();
        </script>
    "#, today = today);
    Html(crate::layout_html("采购单据列表", "/query/purchase_document", &content))
}

pub async fn page_query_sales_order(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/query/sales_order").await {
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
                <thead><tr><th onclick="sortOrders('order_no')" style="cursor:pointer">订单号<span id="sortIndicator_order_no"></span></th><th onclick="sortOrders('unit_name')" style="cursor:pointer">采购单位<span id="sortIndicator_unit_name"></span></th><th onclick="sortOrders('order_date')" style="cursor:pointer">日期<span id="sortIndicator_order_date"></span></th><th onclick="sortOrders('total_amount')" style="cursor:pointer">金额<span id="sortIndicator_total_amount"></span></th><th>下浮后</th><th onclick="sortOrders('status')" style="cursor:pointer">状态<span id="sortIndicator_status"></span></th><th>操作</th></tr></thead>
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
                            <thead><tr><th>商品名称</th><th>规格</th><th>单位</th><th>预售数量</th><th>数量</th><th>单价</th><th>金额</th></tr></thead>
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
            let sortField = '';
            let sortOrder = 'desc';

            function sortOrders(field) {
                if (sortField === field) {
                    sortOrder = sortOrder === 'asc' ? 'desc' : 'asc';
                } else {
                    sortField = field;
                    sortOrder = 'asc';
                }
                updateSortIndicators();
                loadData();
            }

            function updateSortIndicators() {
                const fields = ['order_no', 'unit_name', 'order_date', 'total_amount', 'status'];
                fields.forEach(f => {
                    const el = document.getElementById('sortIndicator_' + f);
                    if (el) {
                        el.textContent = (sortField === f) ? (sortOrder === 'asc' ? ' ▲' : ' ▼') : '';
                    }
                });
            }

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
                let url = '/api/query/sales_order?purchaser_id=' + document.getElementById('purchaserId').value +
                    '&start_date=' + document.getElementById('startDate').value +
                    '&end_date=' + document.getElementById('endDate').value +
                    '&status=' + document.getElementById('status').value +
                    '&page=' + currentPage + '&page_size=20';
                if (sortField) {
                    url += '&sort_field=' + sortField + '&sort_order=' + sortOrder;
                }
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
                    tbody.innerHTML += '<tr><td>' + (item.product_name || '') + '</td><td>' + (item.spec || '-') + '</td><td>' + (item.unit || '') + '</td><td>' + (item.pre_sale_quantity || 0).toFixed(2) + '</td><td>' + (item.quantity || 0).toFixed(2) + '</td><td>¥' + (item.unit_price || 0).toFixed(2) + '</td><td>¥' + (item.amount || 0).toFixed(2) + '</td></tr>';
                });
                tbody.innerHTML += '<tr class="table-active fw-bold"><td colspan="6">合计</td><td>¥' + itemTotal.toFixed(2) + '</td></tr>';

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
    Html(crate::layout_html("销售订单查询", "/query/sales_order", &content))
}

pub async fn page_query_stock_balance(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/query/stock_balance").await {
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
                let detail = '库存流水:\n';
                data.forEach(flow => {
                    detail += flow.type + ' ' + flow.quantity.toFixed(2) + ' ' + flow.create_time + '\n';
                });
                alert(detail);
            }
            loadCategories();
            searchStock();
        </script>
    "#;
    Html(crate::layout_html("实时库存余额查询", "/query/stock_balance", &content))
}

pub async fn page_query_overview() -> Html<String> {
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
    Html(crate::layout_html("进销存汇总报表", "/query/overview", &content))
}

pub async fn page_query_purchase_price() -> Html<String> {
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
            <a href="/api/query/purchase_price/export" class="btn btn-success ml-2">导出Excel</a>
        </div>
        <div class="card p-4 mt-4">
            <table class="table table-bordered">
                <thead><tr><th>商品名称</th><th>规格</th><th>供应商</th><th id="thPurchaseUnitPrice">采购单价</th><th>采购日期</th><th>采购数量</th></tr></thead>
                <tbody id="resultTable"></tbody>
            </table>
            <div id="pagination" class="mt-3"></div>
        </div>
        <script>
            let currentPage = 1;
            // 采购单价为进价信息，仅超级管理员可见
            let isSuperAdmin = false;
            fetch('/api/login/check').then(r => r.json()).then(d => {
                if (d && d.logged_in) {
                    isSuperAdmin = (d.user.role === 'super_admin');
                }
                if (!isSuperAdmin) {
                    const th = document.getElementById('thPurchaseUnitPrice');
                    if (th) th.style.display = 'none';
                }
            });
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
                    tbody.innerHTML += '<tr><td>' + item.product_name + '</td><td>' + (item.spec || '-') + '</td><td>' + item.supplier_name + '</td>' + (isSuperAdmin ? '<td>¥' + item.unit_price.toFixed(2) + '/' + (item.unit || '') + '</td>' : '') + '<td>' + item.order_date + '</td><td>' + item.quantity.toFixed(2) + (item.unit || '') + '</td></tr>';
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
    Html(crate::layout_html("采购价格查询", "/query/purchase_price", &content))
}

pub async fn page_query_sales_price(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/query/sales_price").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let content = r#"
        <style>
            .search-suggest {
                position: absolute;
                z-index: 1000;
                top: 100%;
                left: 0;
                right: 0;
                max-height: 320px;
                overflow-y: auto;
                background: #fff;
                border: 1px solid #ced4da;
                border-top: none;
                border-radius: 0 0 4px 4px;
                box-shadow: 0 4px 12px rgba(0,0,0,0.08);
            }
            .search-suggest ul { list-style: none; margin: 0; padding: 0; }
            .search-suggest li {
                padding: 8px 12px;
                cursor: pointer;
                border-bottom: 1px solid #f0f0f0;
            }
            .search-suggest li:hover, .search-suggest li.active {
                background-color: #e9ecef;
            }
            .search-suggest li strong { color: #0d6efd; }
            .search-suggest li small { color: #6c757d; }
        </style>
        <div class="card p-4">
            <h3>销售价格查询</h3>
            <div class="row mb-3">
                <div class="col-md-4" style="position:relative;">
                    <label>商品名称：</label>
                    <input type="text" id="productKeyword" class="form-control" placeholder="输入商品名或别称进行模糊搜索" autocomplete="off">
                    <input type="hidden" id="productId" value="">
                    <div id="productSuggest" class="search-suggest" style="display:none;"></div>
                </div>
                <div class="col-md-3">
                    <label>开始日期：</label>
                    <input type="date" id="startDate" class="form-control">
                </div>
                <div class="col-md-3">
                    <label>结束日期：</label>
                    <input type="date" id="endDate" class="form-control">
                </div>
                <div class="col-md-2">
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
        <div class="card p-4 mt-4" id="trendCard" style="display:none;">
            <h4 id="trendTitle">价格趋势</h4>
            <p class="text-muted small">进价来源于历史采购单实际成交价，售价来源于历史销售单实际成交价（均已换算为同一基础单位），保留历史原值。</p>
            <div style="position:relative; height:380px;">
                <canvas id="trendChart"></canvas>
            </div>
        </div>
        <script src="/static/chart.umd.min.js"></script>
        <script>
            let currentPage = 1;
            let trendChartObj = null;
            async function loadPurchasers() {
                const res = await fetch('/api/purchaser/list');
                const purchasers = await res.json();
                const select = document.getElementById('purchaserId');
                purchasers.forEach(p => {
                    select.innerHTML += '<option value="' + p.id + '">' + p.name + '</option>';
                });
            }
            let productSearchTimeout = null;
            // 商品模糊搜索：输入时调用 /api/product/search（匹配商品名/别称）
            async function handleProductSearchInput() {
                const keyword = document.getElementById('productKeyword').value.trim();
                const suggest = document.getElementById('productSuggest');
                if (keyword.length < 1) {
                    suggest.style.display = 'none';
                    document.getElementById('productId').value = '';
                    return;
                }
                if (productSearchTimeout) clearTimeout(productSearchTimeout);
                productSearchTimeout = setTimeout(async () => {
                    const res = await fetch('/api/product/search?keyword=' + encodeURIComponent(keyword));
                    const products = await res.json();
                    if (products.length === 0) {
                        suggest.innerHTML = '<div class="p-2 text-muted">无匹配商品</div>';
                        suggest.style.display = 'block';
                        return;
                    }
                    let html = '<ul>';
                    products.slice(0, 50).forEach(p => {
                        const label = p.name + (p.spec ? ' (' + p.spec + ')' : '') + (p.base_unit ? ' / ' + p.base_unit : '');
                        html += '<li onclick="selectSuggestProduct(this)" data-id="' + p.id + '" data-name="' + p.name + '">'
                            + '<strong>' + label + '</strong>'
                            + (p.purchase_price ? '<br><small>进价: ' + p.purchase_price + '</small>' : '')
                            + '</li>';
                    });
                    html += '</ul>';
                    suggest.innerHTML = html;
                    suggest.style.display = 'block';
                }, 250);
            }
            function selectSuggestProduct(li) {
                const id = li.getAttribute('data-id');
                const name = li.getAttribute('data-name');
                document.getElementById('productId').value = id;
                document.getElementById('productKeyword').value = name;
                document.getElementById('productSuggest').style.display = 'none';
            }
            // 页面点击别处关闭下拉
            document.addEventListener('click', function(e) {
                const suggest = document.getElementById('productSuggest');
                const input = document.getElementById('productKeyword');
                if (suggest && input && !suggest.contains(e.target) && e.target !== input) {
                    suggest.style.display = 'none';
                }
            });
            async function searchSalesPrice() {
                currentPage = 1;
                loadData();
                loadPriceTrend();
            }
            async function loadData(page) {
                if (page !== undefined) currentPage = page;
                const productId = document.getElementById('productId').value;
                const productName = productId ? document.getElementById('productKeyword').value.trim() : '';
                const url = '/api/query/sales_price?product_name=' + encodeURIComponent(productName) +
                    '&product_id=' + productId +
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
            async function loadPriceTrend() {
                const productId = document.getElementById('productId').value;
                const card = document.getElementById('trendCard');
                if (!productId) {
                    card.style.display = 'none';
                    if (trendChartObj) { trendChartObj.destroy(); trendChartObj = null; }
                    return;
                }
                const startDate = document.getElementById('startDate').value;
                const endDate = document.getElementById('endDate').value;
                const url = '/api/query/product_price_trend?product_id=' + productId
                    + (startDate ? '&start_date=' + startDate : '')
                    + (endDate ? '&end_date=' + endDate : '');
                const res = await fetch(url);
                const data = await res.json();
                card.style.display = 'block';
                document.getElementById('trendTitle').textContent = '价格趋势 - ' + data.product_name + (data.base_unit ? '（基础单位：' + data.base_unit + '）' : '');
                const purchasePoints = data.purchase_points || [];
                const sellingPoints = data.selling_points || [];

                // 合并进价与售价的所有日期作为统一时间轴（去重 + 排序）
                const dateSet = new Set();
                purchasePoints.forEach(p => dateSet.add(p.date.substring(0, 10)));
                sellingPoints.forEach(p => {
                    const d = (p.date || '').substring(0, 10);
                    if (d) dateSet.add(d);
                });
                const labels = Array.from(dateSet).sort();
                const purchaseMap = new Map();
                purchasePoints.forEach(p => purchaseMap.set(p.date.substring(0, 10), p.price));
                const sellingMap = new Map();
                sellingPoints.forEach(p => {
                    const d = (p.date || '').substring(0, 10);
                    if (d) sellingMap.set(d, p.price);
                });
                const purchaseData = labels.map(l => purchaseMap.has(l) ? purchaseMap.get(l) : null);
                const sellingData = labels.map(l => sellingMap.has(l) ? sellingMap.get(l) : null);

                const ctx = document.getElementById('trendChart').getContext('2d');
                if (trendChartObj) trendChartObj.destroy();
                trendChartObj = new Chart(ctx, {
                    type: 'line',
                    data: {
                        labels: labels,
                        datasets: [
                            {
                                label: '进价（基础单位）',
                                data: purchaseData,
                                borderColor: '#dc3545',
                                backgroundColor: 'rgba(220, 53, 69, 0.1)',
                                tension: 0.2,
                                pointRadius: 3,
                                spanGaps: true,
                                fill: false
                            },
                            {
                                label: '售价',
                                data: sellingData,
                                borderColor: '#0d6efd',
                                backgroundColor: 'rgba(13, 110, 253, 0.1)',
                                tension: 0.2,
                                pointRadius: 3,
                                spanGaps: true,
                                fill: false
                            }
                        ]
                    },
                    options: {
                        responsive: true,
                        maintainAspectRatio: false,
                        plugins: {
                            legend: { position: 'top' },
                            tooltip: { mode: 'index', intersect: false }
                        },
                        scales: {
                            x: { title: { display: true, text: '日期' } },
                            y: { title: { display: true, text: '价格（元）' }, beginAtZero: false }
                        }
                    }
                });
            }
            loadPurchasers();
            // 商品模糊搜索输入监听
            document.getElementById('productKeyword').addEventListener('input', handleProductSearchInput);
            searchSalesPrice();
        </script>
    "#;
    Html(crate::layout_html("销售价格查询", "/query/sales_price", &content))
}

pub async fn page_query_supplier_balance() -> Html<String> {
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
    Html(crate::layout_html("供应商往来对账", "/query/supplier_balance", &content))
}

pub async fn page_query_purchaser_balance(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/query/purchaser_balance").await {
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
    Html(crate::layout_html("采购方应收对账", "/query/purchaser_balance", &content))
}

pub async fn page_query_purchase_summary() -> Html<String> {
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
    Html(crate::layout_html("采购汇总统计", "/query/purchase_summary", &content))
}

pub async fn page_query_sales_summary(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/query/sales_summary").await {
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
            <a href="/api/query/sales_summary/export" class="btn btn-success ml-2">导出Excel</a>
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
    Html(crate::layout_html("销售汇总报表", "/query/sales_summary", &content))
}

pub async fn page_query_product_rank(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/query/product_rank").await {
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
    Html(crate::layout_html("畅销滞销商品查询", "/query/product_rank", &content))
}

pub async fn page_query_reimburse_summary(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/query/reimburse_summary").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let content = r#"
        <div class="card p-4">
            <h3>报销口径汇总</h3>
            <p class="text-muted small">口径说明：仅统计分摊后的目标单（真实明细 + 分摊增项净影响），耗材来源单已作为分摊来源单独统计、不计入此处，确保金额不重计。</p>
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
            <button onclick="searchReimburse()" class="btn btn-primary">查询</button>
            <a href="/api/query/reimburse_summary/export" class="btn btn-success ml-2">导出Excel</a>
        </div>
        <div class="card p-4 mt-4">
            <h4>按采购单位汇总（报销口径）</h4>
            <table class="table table-bordered">
                <thead><tr><th>采购单位</th><th>真实金额</th><th>分摊增项净额</th><th>报销金额</th></tr></thead>
                <tbody id="purchaserSummary"></tbody>
            </table>
        </div>
        <div class="card p-4 mt-4">
            <h4>按商品汇总（报销口径）</h4>
            <table class="table table-bordered">
                <thead><tr><th>商品名称</th><th>规格</th><th>数量</th><th>报销金额</th></tr></thead>
                <tbody id="productSummary"></tbody>
            </table>
        </div>
        <script>
            async function searchReimburse() {
                const url = '/api/query/reimburse_summary?start_date=' + document.getElementById('startDate').value + '&end_date=' + document.getElementById('endDate').value;
                const res = await fetch(url);
                const data = await res.json();

                let ph = '';
                if (data.by_purchaser.length === 0) {
                    ph = '<tr><td colspan="4" class="text-center text-muted">暂无数据</td></tr>';
                } else {
                    let tReal = 0, tSupp = 0, tReim = 0;
                    data.by_purchaser.forEach(item => {
                        tReal += item.real_amount;
                        tSupp += item.supplement_amount;
                        tReim += item.reimburse_amount;
                        ph += '<tr><td>' + item.name + '</td><td>¥' + item.real_amount.toFixed(2) + '</td><td>¥' + item.supplement_amount.toFixed(2) + '</td><td><strong>¥' + item.reimburse_amount.toFixed(2) + '</strong></td></tr>';
                    });
                    ph += '<tr class="table-active fw-bold"><td>合计</td><td>¥' + tReal.toFixed(2) + '</td><td>¥' + tSupp.toFixed(2) + '</td><td>¥' + tReim.toFixed(2) + '</td></tr>';
                }
                document.getElementById('purchaserSummary').innerHTML = ph;

                let pd = '';
                if (data.by_product.length === 0) {
                    pd = '<tr><td colspan="4" class="text-center text-muted">暂无数据</td></tr>';
                } else {
                    let tQty = 0, tAmt = 0;
                    data.by_product.forEach(item => {
                        tQty += item.quantity;
                        tAmt += item.reimburse_amount;
                        pd += '<tr><td>' + item.product_name + '</td><td>' + (item.spec || '-') + '</td><td>' + item.quantity.toFixed(2) + '</td><td>¥' + item.reimburse_amount.toFixed(2) + '</td></tr>';
                    });
                    pd += '<tr class="table-active fw-bold"><td colspan="2">合计</td><td>' + tQty.toFixed(2) + '</td><td>¥' + tAmt.toFixed(2) + '</td></tr>';
                }
                document.getElementById('productSummary').innerHTML = pd;
            }
            searchReimburse();
        </script>
    "#;
    Html(crate::layout_html("报销口径汇总", "/query/reimburse_summary", &content))
}

pub async fn page_query_allocation_source(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/query/allocation_source").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let content = r#"
        <div class="card p-4">
            <h3>分摊来源统计</h3>
            <p class="text-muted small">统计所有作为分摊来源的订单（如耗材单），显示其金额、已分摊金额、剩余、状态及分摊去向，供来源侧单独核账。</p>
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
            <button onclick="searchAllocationSource()" class="btn btn-primary">查询</button>
        </div>
        <div class="card p-4 mt-4">
            <h4>分摊来源单汇总</h4>
            <table class="table table-bordered">
                <thead><tr><th>来源订单</th><th>日期</th><th>来源金额</th><th>已分摊</th><th>剩余</th><th>状态</th><th>分摊去向</th></tr></thead>
                <tbody id="sourceTable"></tbody>
            </table>
        </div>
        <script>
            const statusMap = { 0: '未分摊', 1: '分摊中', 2: '已完成', 3: '已终止' };
            async function searchAllocationSource() {
                const url = '/api/query/allocation_source?start_date=' + document.getElementById('startDate').value + '&end_date=' + document.getElementById('endDate').value;
                const res = await fetch(url);
                const data = await res.json();
                const tbody = document.getElementById('sourceTable');
                if (data.length === 0) {
                    tbody.innerHTML = '<tr><td colspan="7" class="text-center text-muted">暂无数据</td></tr>';
                    return;
                }
                let html = '';
                let tTotal = 0, tAlloc = 0, tRemain = 0;
                data.forEach(item => {
                    tTotal += item.total_amount;
                    tAlloc += item.allocated_amount;
                    tRemain += item.remaining_balance;
                    const targets = (item.targets || []).map(t => t.order_no + '(¥' + t.amount.toFixed(2) + ')').join('、') || '-';
                    html += '<tr><td>' + item.order_no + '</td><td>' + item.order_date + '</td><td>¥' + item.total_amount.toFixed(2) + '</td><td>¥' + item.allocated_amount.toFixed(2) + '</td><td>¥' + item.remaining_balance.toFixed(2) + '</td><td>' + (statusMap[item.status] || '未知') + '</td><td class="small">' + targets + '</td></tr>';
                });
                html += '<tr class="table-active fw-bold"><td colspan="2">合计</td><td>¥' + tTotal.toFixed(2) + '</td><td>¥' + tAlloc.toFixed(2) + '</td><td>¥' + tRemain.toFixed(2) + '</td><td colspan="2"></td></tr>';
                tbody.innerHTML = html;
            }
            searchAllocationSource();
        </script>
    "#;
    Html(crate::layout_html("分摊来源统计", "/query/allocation_source", &content))
}

pub async fn page_order_adjust(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/query/order_adjust").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let content = r####"
        <div class="card p-4">
            <h3>订单调整与同屏比对</h3>
            <p class="text-muted small">在真实订单基础上虚增商品明细或变更数量，调整内容独立记录、可回滚。真实订单始终保留做底根，同屏比对差异。报销口径统一采用调整后订单。</p>
            <div class="row mb-2">
                <div class="col-md-4">
                    <label>订单号搜索：</label>
                    <div class="position-relative">
                        <input type="text" id="orderInput" class="form-control" placeholder="输入订单号搜索" autocomplete="off">
                        <div id="orderDropdown" class="search-dropdown" style="display:none;position:absolute;z-index:1000;background:#fff;border:1px solid #ddd;width:100%;max-height:260px;overflow-y:auto;"></div>
                    </div>
                </div>
            </div>
            <div class="mt-3">
                <div class="d-flex justify-content-between align-items-center mb-1">
                    <h6 class="mb-0">有变更的订单列表 <span class="badge badge-info" id="adjustedOrdersCount">0</span></h6>
                    <button class="btn btn-xs btn-outline-secondary" onclick="loadAdjustedOrders()">刷新</button>
                </div>
                <div class="row mb-2">
                    <div class="col-md-4">
                        <label class="small">订单号搜索：</label>
                        <input type="text" id="adjOrderKeyword" class="form-control form-control-sm" placeholder="输入订单号搜索" oninput="onAdjOrderFilter()">
                    </div>
                    <div class="col-md-4">
                        <label class="small">采购单位筛选：</label>
                        <select id="adjPurchaserFilter" class="form-control form-control-sm" onchange="onAdjOrderFilter()">
                            <option value="">全部单位</option>
                        </select>
                    </div>
                </div>
                <div style="border:1px solid #eee;">
                    <table class="table table-sm table-bordered mb-0" id="adjustedOrdersTable">
                        <thead class="thead-light"><tr>
                            <th>订单号</th><th>采购单位</th>
                            <th style="cursor:pointer;white-space:nowrap;" onclick="toggleAdjOrderSort()">订单日期 <span id="adjSortArrow"></span></th>
                            <th>真实金额</th><th>调整金额</th><th>调整后金额</th>
                            <th>调整条数</th><th>最近调整日</th><th>操作</th>
                        </tr></thead>
                        <tbody><tr><td colspan="9" class="text-center text-muted small">暂无</td></tr></tbody>
                    </table>
                </div>
                <div class="d-flex justify-content-between align-items-center mt-2">
                    <div class="small text-muted">
                        合计：真实金额 <strong>¥<span id="adjSumReal">0.00</span></strong>
                        ｜ 调整金额 <strong>¥<span id="adjSumAdjust">0.00</span></strong>
                        ｜ 调整后金额 <strong>¥<span id="adjSumAdjusted">0.00</span></strong>
                    </div>
                    <nav><ul class="pagination pagination-sm mb-0" id="adjustedOrdersPager"></ul></nav>
                </div>
            </div>
        </div>

        <div id="adjustArea" style="display:none;">
            <div class="card p-3 mt-3">
                <h5 id="adjustOrderTitle"></h5>
                <div class="row">
                    <div class="col-md-6">
                        <div class="card border-secondary">
                            <div class="card-header bg-secondary text-white py-1"><small><strong>真实订单（底根）</strong> - ¥<span id="realTotalLabel">0.00</span></small></div>
                            <div class="card-body p-1" style="max-height:340px;overflow-y:auto;">
                                <table class="table table-sm table-bordered mb-0" id="realTable">
                                    <thead class="thead-light"><tr><th>商品</th><th>数量</th><th>金额</th></tr></thead>
                                    <tbody></tbody>
                                </table>
                            </div>
                        </div>
                    </div>
                    <div class="col-md-6">
                        <div class="card border-success">
                            <div class="card-header bg-success text-white py-1"><small><strong>调整后订单（报销口径）</strong> - ¥<span id="adjTotalLabel">0.00</span> <span class="badge badge-warning ml-1">差异 <span id="diffLabel">0.00</span></span></small></div>
                            <div class="card-body p-1" style="max-height:340px;overflow-y:auto;">
                                <table class="table table-sm table-bordered mb-0" id="adjTable">
                                    <thead class="thead-light"><tr><th>商品</th><th>数量</th><th>金额</th></tr></thead>
                                    <tbody></tbody>
                                </table>
                            </div>
                        </div>
                    </div>
                </div>
            </div>

            <div class="card p-3 mt-3">
                <h6>调整操作</h6>
                <div class="row mb-2">
                    <div class="col-md-12">
                        <label class="radio-inline mr-4"><input type="radio" name="adjType" value="new_item" checked onchange="toggleAdjType()"> 虚增商品明细</label>
                        <label class="radio-inline mr-4"><input type="radio" name="adjType" value="increase_quantity" onchange="toggleAdjType()"> 变更已有商品数量</label>
                        <label class="radio-inline"><input type="radio" name="adjType" value="replace" onchange="toggleAdjType()"> 替换明细</label>
                    </div>
                </div>
                <div class="row mb-2 align-items-end" id="adjNewSection">
                    <div class="col-md-4">
                        <label>选择商品</label>
                        <div class="position-relative">
                            <input type="text" id="adjProductInput" class="form-control form-control-sm" placeholder="点击选择商品" readonly>
                            <div id="adjProductDropdown" class="search-dropdown" style="display:none;position:absolute;z-index:1000;background:#fff;border:1px solid #ddd;width:100%;max-height:220px;overflow-y:auto;"></div>
                        </div>
                    </div>
                    <div class="col-md-2"><label>数量</label><input type="number" step="0.01" id="adjNewQty" class="form-control form-control-sm" oninput="calcAdjNew()"></div>
                    <div class="col-md-2"><label>单价</label><input type="number" step="0.01" id="adjNewPrice" class="form-control form-control-sm" oninput="calcAdjNew()"></div>
                    <div class="col-md-2"><label>金额</label><input type="number" step="0.01" id="adjNewAmount" class="form-control form-control-sm" readonly></div>
                </div>
                <div class="row mb-2 align-items-end" id="adjIncSection" style="display:none;">
                    <div class="col-md-4">
                        <label>选择目标商品</label>
                        <select id="adjIncSelect" class="form-control form-control-sm"></select>
                    </div>
                    <div class="col-md-2"><label>追加数量</label><input type="number" step="0.01" id="adjIncQty" class="form-control form-control-sm" oninput="calcAdjInc()"></div>
                    <div class="col-md-2"><label>单价</label><input type="text" id="adjIncPrice" class="form-control form-control-sm" readonly></div>
                    <div class="col-md-2"><label>追加金额</label><input type="text" id="adjIncAmount" class="form-control form-control-sm" readonly></div>
                    <div class="col-md-2"><span class="text-muted small" id="adjIncHint">合计数量: 0</span></div>
                </div>
                <div id="adjReplaceSection" style="display:none;">
                    <div class="row mb-2 align-items-end">
                        <div class="col-md-4">
                            <label>被替换的原明细</label>
                            <select id="adjReplaceSourceSelect" class="form-control form-control-sm" onchange="updateAdjReplaceDiff()"></select>
                        </div>
                    </div>
                    <div class="row mb-2 align-items-end">
                        <div class="col-md-4">
                            <label>替换商品</label>
                            <div class="position-relative">
                                <input type="text" id="adjReplaceProductInput" class="form-control form-control-sm" placeholder="点击选择替换商品" readonly>
                                <div id="adjReplaceProductDropdown" class="search-dropdown" style="display:none;position:absolute;z-index:1000;background:#fff;border:1px solid #ddd;width:100%;max-height:220px;overflow-y:auto;"></div>
                            </div>
                        </div>
                        <div class="col-md-2"><label>数量</label><input type="number" step="0.01" id="adjReplaceQty" class="form-control form-control-sm" oninput="calcAdjReplaceLine()"></div>
                        <div class="col-md-2"><label>单价</label><input type="number" step="0.01" id="adjReplacePrice" class="form-control form-control-sm" oninput="calcAdjReplaceLine()"></div>
                        <div class="col-md-2"><label>金额</label><input type="number" step="0.01" id="adjReplaceAmount" class="form-control form-control-sm" readonly></div>
                        <div class="col-md-2"><label>&nbsp;</label><br><button class="btn btn-sm btn-outline-primary" onclick="addAdjReplaceLine()">加行</button></div>
                    </div>
                    <table class="table table-sm table-bordered" id="adjReplaceLineList">
                        <thead><tr><th>替换商品</th><th>数量</th><th>单价</th><th>金额</th><th>操作</th></tr></thead>
                        <tbody></tbody>
                    </table>
                    <div>替换合计: <span id="adjReplaceLinesTotal">0.00</span> 元　<span id="adjReplaceDiffHint" class="small"></span></div>
                </div>
                <div class="row">
                    <div class="col-md-2"><label>调整日期</label><input type="date" id="adjDate" class="form-control form-control-sm"></div>
                    <div class="col-md-10"><label>&nbsp;</label><br><button class="btn btn-sm btn-primary" onclick="addAdjustment()">保存调整</button></div>
                </div>
                <h6 class="mt-3">本单调整记录（可回滚）</h6>
                <table class="table table-sm table-bordered" id="adjRecordList">
                    <thead><tr><th>类型</th><th>商品</th><th>数量</th><th>金额</th><th>日期</th><th>操作</th></tr></thead>
                    <tbody></tbody>
                </table>
            </div>
        </div>

        <script>
            let adjOrder = null;
            let adjRealItems = [];
            let adjSelectedProduct = null;
            let adjReplaceLines = [];
            let adjSelectedReplaceProduct = null;

            (function initOrderSearch() {
                const input = document.getElementById('orderInput');
                const dropdown = document.getElementById('orderDropdown');
                input.addEventListener('input', async function() {
                    const kw = this.value.trim();
                    const res = await fetch('/api/sales_order/list?keyword=' + encodeURIComponent(kw) + '&page=1&page_size=20');
                    const data = await res.json();
                    const orders = data.items || data.data || [];
                    dropdown.innerHTML = '';
                    orders.forEach(o => {
                        const li = document.createElement('div');
                        li.className = 'search-item';
                        li.style.padding = '6px 10px';
                        li.style.cursor = 'pointer';
                        li.textContent = o.order_no + ' | ' + (o.purchaser_name || '') + ' | ' + o.order_date + ' | ¥' + (o.final_amount || o.total_amount || 0).toFixed(2);
                        li.onmousedown = () => selectAdjOrder(o);
                        dropdown.appendChild(li);
                    });
                    dropdown.style.display = orders.length > 0 ? 'block' : 'none';
                });
                input.addEventListener('blur', function() { setTimeout(() => { dropdown.style.display = 'none'; }, 200); });
            })();

            async function selectAdjOrder(o) {
                adjOrder = o;
                document.getElementById('orderInput').value = o.order_no;
                document.getElementById('orderDropdown').style.display = 'none';
                document.getElementById('adjustArea').style.display = 'block';
                document.getElementById('adjustOrderTitle').textContent = o.order_no + '（' + (o.purchaser_name || '') + '）';
                document.getElementById('adjDate').value = new Date().toISOString().slice(0, 10);
                const res = await fetch('/api/sales_order/detail/' + o.id);
                const data = await res.json();
                adjRealItems = data.items || [];
                adjReplaceLines = [];
                adjSelectedReplaceProduct = null;
                initAdjIncSelect();
                initAdjReplaceSelect();
                initAdjProductSearch();
                initAdjReplaceProductSearch();
                renderAdjReplaceLines();
                await loadCompare();
                await loadAdjRecords();
            }

            async function loadCompare() {
                const res = await fetch('/api/supplement/compare/' + adjOrder.id);
                const data = await res.json();
                document.getElementById('realTotalLabel').textContent = data.real_total.toFixed(2);
                document.getElementById('adjTotalLabel').textContent = data.allocation_total.toFixed(2);
                document.getElementById('diffLabel').textContent = (data.allocation_total - data.real_total).toFixed(2);
                const realTbody = document.querySelector('#realTable tbody');
                const adjTbody = document.querySelector('#adjTable tbody');
                realTbody.innerHTML = '';
                adjTbody.innerHTML = '';
                data.items.forEach(item => {
                    const name = item.display_name || item.product_name;
                    const rr = document.createElement('tr');
                    const ar = document.createElement('tr');
                    if (item.is_new) {
                        rr.innerHTML = '<td colspan="3" class="text-center text-muted small">—</td>';
                        ar.style.backgroundColor = '#fff3cd';
                        ar.innerHTML = '<td><strong>[虚增]</strong> ' + name + '</td><td>' + item.total_quantity.toFixed(2) + '</td><td>' + item.total_amount.toFixed(2) + '</td>';
                    } else if (item.is_replaced) {
                        rr.innerHTML = '<td>' + name + '</td><td>' + item.quantity.toFixed(2) + '</td><td>' + item.amount.toFixed(2) + '</td>';
                        ar.style.backgroundColor = '#f8d7da';
                        ar.innerHTML = '<td><del>' + name + '</del> <span class="badge badge-danger">已替换</span></td><td>' + item.total_quantity.toFixed(2) + '</td><td>' + item.total_amount.toFixed(2) + '</td>';
                    } else if (item.is_increase) {
                        rr.innerHTML = '<td>' + name + '</td><td>' + item.quantity.toFixed(2) + '</td><td>' + item.amount.toFixed(2) + '</td>';
                        ar.style.backgroundColor = '#d4edda';
                        ar.innerHTML = '<td>' + name + ' <span class="badge badge-success">+' + item.supplement_quantity.toFixed(2) + '</span></td><td>' + item.total_quantity.toFixed(2) + '</td><td>' + item.total_amount.toFixed(2) + '</td>';
                    } else {
                        rr.innerHTML = '<td>' + name + '</td><td>' + item.quantity.toFixed(2) + '</td><td>' + item.amount.toFixed(2) + '</td>';
                        ar.innerHTML = '<td>' + name + '</td><td>' + item.total_quantity.toFixed(2) + '</td><td>' + item.total_amount.toFixed(2) + '</td>';
                    }
                    realTbody.appendChild(rr);
                    adjTbody.appendChild(ar);
                });
            }

            async function loadAdjRecords() {
                const res = await fetch('/api/supplement/list_by_target/' + adjOrder.id);
                const list = await res.json();
                const tbody = document.querySelector('#adjRecordList tbody');
                tbody.innerHTML = '';
                const typeMap = { 'new_item': '虚增明细', 'increase_quantity': '变更数量', 'replace_remove': '替换-冲减', 'replace_add': '替换-换入' };
                list.forEach(r => {
                    const tr = document.createElement('tr');
                    tr.innerHTML = '<td>' + (typeMap[r.operation_type] || r.operation_type) + '</td><td>' + r.product_name + '</td><td>' + (r.quantity||0).toFixed(2) + '</td><td>' + (r.amount||0).toFixed(2) + '</td><td>' + (r.allocate_date||'') + '</td>' +
                        '<td><button class="btn btn-xs btn-danger" onclick="rollbackAdj(' + r.id + ')">回滚</button></td>';
                    tbody.appendChild(tr);
                });
            }

            function toggleAdjType() {
                const t = document.querySelector('input[name="adjType"]:checked').value;
                document.getElementById('adjNewSection').style.display = t === 'new_item' ? 'flex' : 'none';
                document.getElementById('adjIncSection').style.display = t === 'increase_quantity' ? 'flex' : 'none';
                document.getElementById('adjReplaceSection').style.display = t === 'replace' ? 'block' : 'none';
            }

            function initAdjReplaceSelect() {
                const sel = document.getElementById('adjReplaceSourceSelect');
                sel.innerHTML = '';
                adjRealItems.forEach((item, idx) => {
                    const opt = document.createElement('option');
                    opt.value = idx;
                    opt.textContent = item.product_name + ' (' + item.quantity.toFixed(2) + ' ' + (item.unit || '') + ' = ' + item.amount.toFixed(2) + '元)';
                    sel.appendChild(opt);
                });
                if (adjRealItems.length > 0) sel.selectedIndex = 0;
            }

            function initAdjReplaceProductSearch() {
                const input = document.getElementById('adjReplaceProductInput');
                const dropdown = document.getElementById('adjReplaceProductDropdown');
                if (input._init) return;
                input.addEventListener('click', () => showAdjReplaceProducts(''));
                input.addEventListener('dblclick', function() { this.readOnly = false; this.value = ''; this.focus(); });
                input.addEventListener('input', function() { showAdjReplaceProducts(this.value.trim()); });
                input.addEventListener('blur', () => setTimeout(() => { dropdown.style.display = 'none'; }, 200));
                input._init = true;
            }

            async function showAdjReplaceProducts(kw) {
                const res = await fetch('/api/product/list?keyword=' + encodeURIComponent(kw || '') + '&page_size=50');
                const data = await res.json();
                const products = data.items || data.data || [];
                const dropdown = document.getElementById('adjReplaceProductDropdown');
                dropdown.innerHTML = '';
                products.forEach(p => {
                    const li = document.createElement('div');
                    li.className = 'search-item';
                    li.style.padding = '6px 10px';
                    li.style.cursor = 'pointer';
                    const a2 = p.alias2 ? '(' + p.alias2 + ')' : '';
                    const price = p.selling_price || p.base_price || 0;
                    li.textContent = p.name + a2 + ' - ' + price.toFixed(2) + '元/' + (p.unit || '');
                    li.onmousedown = () => selectAdjReplaceProduct(p);
                    dropdown.appendChild(li);
                });
                dropdown.style.display = products.length > 0 ? 'block' : 'none';
            }

            function selectAdjReplaceProduct(p) {
                adjSelectedReplaceProduct = p;
                const input = document.getElementById('adjReplaceProductInput');
                input.value = p.name + (p.alias2 ? '(' + p.alias2 + ')' : '');
                input.readOnly = false;
                document.getElementById('adjReplaceProductDropdown').style.display = 'none';
                document.getElementById('adjReplacePrice').value = (p.selling_price || p.base_price || 0).toFixed(2);
                calcAdjReplaceLine();
            }

            function calcAdjReplaceLine() {
                const qty = parseFloat(document.getElementById('adjReplaceQty').value) || 0;
                const price = parseFloat(document.getElementById('adjReplacePrice').value) || 0;
                document.getElementById('adjReplaceAmount').value = (qty * price).toFixed(2);
            }

            function addAdjReplaceLine() {
                if (!adjSelectedReplaceProduct) { alert('请选择替换商品'); return; }
                const qty = parseFloat(document.getElementById('adjReplaceQty').value) || 0;
                const price = parseFloat(document.getElementById('adjReplacePrice').value) || 0;
                const amount = qty * price;
                if (qty <= 0 || amount <= 0) { alert('请输入有效的数量和单价'); return; }
                adjReplaceLines.push({
                    product_id: adjSelectedReplaceProduct.id,
                    product_name: adjSelectedReplaceProduct.name,
                    alias1: adjSelectedReplaceProduct.alias1 || '',
                    alias2: adjSelectedReplaceProduct.alias2 || '',
                    spec: adjSelectedReplaceProduct.spec || '',
                    unit: adjSelectedReplaceProduct.unit || '',
                    unit_price: price, quantity: qty, amount: amount,
                });
                adjSelectedReplaceProduct = null;
                document.getElementById('adjReplaceProductInput').value = '';
                document.getElementById('adjReplaceQty').value = '';
                document.getElementById('adjReplacePrice').value = '';
                document.getElementById('adjReplaceAmount').value = '';
                renderAdjReplaceLines();
            }

            function removeAdjReplaceLine(index) {
                adjReplaceLines.splice(index, 1);
                renderAdjReplaceLines();
            }

            function renderAdjReplaceLines() {
                const tbody = document.querySelector('#adjReplaceLineList tbody');
                tbody.innerHTML = '';
                let total = 0;
                adjReplaceLines.forEach((line, index) => {
                    total += line.amount;
                    const a2 = line.alias2 ? '(' + line.alias2 + ')' : '';
                    const tr = document.createElement('tr');
                    tr.innerHTML = '<td>' + line.product_name + a2 + '</td><td>' + line.quantity.toFixed(2) + '</td><td>' + line.unit_price.toFixed(2) + '</td><td>' + line.amount.toFixed(2) + '</td>' +
                        '<td><button class="btn btn-xs btn-danger" onclick="removeAdjReplaceLine(' + index + ')">删除</button></td>';
                    tbody.appendChild(tr);
                });
                document.getElementById('adjReplaceLinesTotal').textContent = total.toFixed(2);
                updateAdjReplaceDiff();
            }

            function updateAdjReplaceDiff() {
                const idx = parseInt(document.getElementById('adjReplaceSourceSelect').value);
                const hint = document.getElementById('adjReplaceDiffHint');
                if (isNaN(idx) || !adjRealItems[idx]) { hint.textContent = ''; return; }
                const origAmount = adjRealItems[idx].amount;
                const replaceTotal = adjReplaceLines.reduce((s, l) => s + l.amount, 0);
                const diff = replaceTotal - origAmount;
                if (Math.abs(diff) <= 5.0) {
                    hint.textContent = '差额 ' + diff.toFixed(2) + ' 元（在±5元内，可提交）';
                    hint.className = 'text-success small';
                } else {
                    hint.textContent = '差额 ' + diff.toFixed(2) + ' 元（超过±5元限制）';
                    hint.className = 'text-danger small';
                }
            }

            function initAdjIncSelect() {
                const sel = document.getElementById('adjIncSelect');
                sel.innerHTML = '';
                adjRealItems.forEach((item, idx) => {
                    const opt = document.createElement('option');
                    opt.value = idx;
                    opt.textContent = item.product_name + ' (' + item.quantity.toFixed(2) + ' ' + (item.unit || '') + ')';
                    sel.appendChild(opt);
                });
                sel.onchange = function() {
                    const item = adjRealItems[parseInt(this.value)];
                    if (item) { document.getElementById('adjIncPrice').value = item.unit_price.toFixed(2); calcAdjInc(); }
                };
                if (adjRealItems.length > 0) { sel.selectedIndex = 0; sel.onchange(); }
            }

            function calcAdjInc() {
                const qty = parseFloat(document.getElementById('adjIncQty').value) || 0;
                const price = parseFloat(document.getElementById('adjIncPrice').value) || 0;
                document.getElementById('adjIncAmount').value = (qty * price).toFixed(2);
                const item = adjRealItems[parseInt(document.getElementById('adjIncSelect').value)];
                if (item) document.getElementById('adjIncHint').textContent = '合计数量: ' + (item.quantity + qty).toFixed(2);
            }

            function initAdjProductSearch() {
                const input = document.getElementById('adjProductInput');
                const dropdown = document.getElementById('adjProductDropdown');
                if (input._init) return;
                input.addEventListener('click', () => showAdjProducts(''));
                input.addEventListener('dblclick', function() { this.readOnly = false; this.value = ''; this.focus(); });
                input.addEventListener('input', function() { showAdjProducts(this.value.trim()); });
                input.addEventListener('blur', () => setTimeout(() => { dropdown.style.display = 'none'; }, 200));
                input._init = true;
            }

            async function showAdjProducts(kw) {
                const res = await fetch('/api/product/list?keyword=' + encodeURIComponent(kw || '') + '&page_size=50');
                const data = await res.json();
                const products = data.items || data.data || [];
                const dropdown = document.getElementById('adjProductDropdown');
                dropdown.innerHTML = '';
                products.forEach(p => {
                    const li = document.createElement('div');
                    li.className = 'search-item';
                    li.style.padding = '6px 10px';
                    li.style.cursor = 'pointer';
                    const a2 = p.alias2 ? '(' + p.alias2 + ')' : '';
                    const price = p.selling_price || p.base_price || 0;
                    li.textContent = p.name + a2 + ' - ' + price.toFixed(2) + '元/' + (p.unit || '');
                    li.onmousedown = () => selectAdjProduct(p);
                    dropdown.appendChild(li);
                });
                dropdown.style.display = products.length > 0 ? 'block' : 'none';
            }

            function selectAdjProduct(p) {
                adjSelectedProduct = p;
                const input = document.getElementById('adjProductInput');
                input.value = p.name + (p.alias2 ? '(' + p.alias2 + ')' : '');
                input.readOnly = false;
                document.getElementById('adjProductDropdown').style.display = 'none';
                document.getElementById('adjNewPrice').value = (p.selling_price || p.base_price || 0).toFixed(2);
                calcAdjNew();
            }

            function calcAdjNew() {
                const qty = parseFloat(document.getElementById('adjNewQty').value) || 0;
                const price = parseFloat(document.getElementById('adjNewPrice').value) || 0;
                document.getElementById('adjNewAmount').value = (qty * price).toFixed(2);
            }

            async function addAdjustment() {
                if (!adjOrder) return;
                const t = document.querySelector('input[name="adjType"]:checked').value;
                const allocDate = document.getElementById('adjDate').value;

                if (t === 'replace') {
                    const idx = parseInt(document.getElementById('adjReplaceSourceSelect').value);
                    const src = adjRealItems[idx];
                    if (!src) { alert('请选择被替换的原明细'); return; }
                    if (adjReplaceLines.length === 0) { alert('请至少添加一条替换商品'); return; }
                    const replaceTotal = adjReplaceLines.reduce((s, l) => s + l.amount, 0);
                    const diff = replaceTotal - src.amount;
                    if (Math.abs(diff) > 5.0) {
                        alert('替换总金额 ' + replaceTotal.toFixed(2) + ' 元与原明细 ' + src.amount.toFixed(2) + ' 元差额 ' + diff.toFixed(2) + ' 元，超过±5元限制');
                        return;
                    }
                    const srcRemark = adjOrder.order_no + ' 订单调整-替换[' + src.product_name + ']';
                    // 冲减原明细
                    const removeBody = {
                        target_order_id: adjOrder.id, source_order_id: adjOrder.id,
                        source_remark: srcRemark + ' 冲减',
                        product_id: src.product_id, product_name: src.product_name,
                        alias1: src.alias1 || '', alias2: src.alias2 || '',
                        spec: src.spec || '', unit: src.unit || '',
                        unit_price: src.unit_price, quantity: -src.quantity, amount: -src.amount,
                        allocate_date: allocDate, operation_type: 'replace_remove', target_order_item_id: src.id,
                    };
                    let r1 = await fetch('/api/supplement/create', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(removeBody) });
                    if (!r1.ok) { alert('保存失败(冲减): ' + await r1.text()); return; }
                    // 换入替换商品
                    for (const line of adjReplaceLines) {
                        const addBody = {
                            target_order_id: adjOrder.id, source_order_id: adjOrder.id,
                            source_remark: srcRemark + ' 换入',
                            product_id: line.product_id, product_name: line.product_name,
                            alias1: line.alias1 || '', alias2: line.alias2 || '',
                            spec: line.spec || '', unit: line.unit || '',
                            unit_price: line.unit_price, quantity: line.quantity, amount: line.amount,
                            allocate_date: allocDate, operation_type: 'replace_add', target_order_item_id: src.id,
                        };
                        let r2 = await fetch('/api/supplement/create', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(addBody) });
                        if (!r2.ok) { alert('保存失败(换入): ' + await r2.text()); return; }
                    }
                    adjReplaceLines = [];
                    renderAdjReplaceLines();
                    await loadCompare();
                    await loadAdjRecords();
                    await loadAdjustedOrders();
                    alert('替换已保存');
                    return;
                }

                let body;
                if (t === 'new_item') {
                    if (!adjSelectedProduct) { alert('请选择要虚增的商品'); return; }
                    const qty = parseFloat(document.getElementById('adjNewQty').value) || 0;
                    const price = parseFloat(document.getElementById('adjNewPrice').value) || 0;
                    const amount = qty * price;
                    if (qty <= 0 || amount <= 0) { alert('请输入有效的数量和单价'); return; }
                    body = {
                        target_order_id: adjOrder.id, source_order_id: adjOrder.id,
                        source_remark: adjOrder.order_no + ' 订单调整-虚增',
                        product_id: adjSelectedProduct.id, product_name: adjSelectedProduct.name,
                        alias1: adjSelectedProduct.alias1 || '', alias2: adjSelectedProduct.alias2 || '',
                        spec: adjSelectedProduct.spec || '', unit: adjSelectedProduct.unit || '',
                        unit_price: price, quantity: qty, amount: amount,
                        allocate_date: allocDate,
                        operation_type: 'new_item', target_order_item_id: null,
                    };
                } else {
                    const idx = parseInt(document.getElementById('adjIncSelect').value);
                    const item = adjRealItems[idx];
                    if (!item) { alert('请选择目标商品'); return; }
                    const qty = parseFloat(document.getElementById('adjIncQty').value) || 0;
                    if (qty === 0) { alert('请输入变更数量'); return; }
                    const amount = qty * item.unit_price;
                    body = {
                        target_order_id: adjOrder.id, source_order_id: adjOrder.id,
                        source_remark: adjOrder.order_no + ' 订单调整-变更数量',
                        product_id: item.product_id, product_name: item.product_name,
                        alias1: item.alias1 || '', alias2: item.alias2 || '',
                        spec: item.spec || '', unit: item.unit || '',
                        unit_price: item.unit_price, quantity: qty, amount: amount,
                        allocate_date: allocDate,
                        operation_type: 'increase_quantity', target_order_item_id: item.id,
                    };
                }
                const res = await fetch('/api/supplement/create', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
                if (res.ok) {
                    document.getElementById('adjNewQty').value = '';
                    document.getElementById('adjIncQty').value = '';
                    await loadCompare();
                    await loadAdjRecords();
                    await loadAdjustedOrders();
                    alert('调整已保存');
                } else {
                    alert('保存失败: ' + await res.text());
                }
            }

            async function rollbackAdj(id) {
                if (!confirm('确定回滚该调整记录？')) return;
                const res = await fetch('/api/supplement/delete/' + id, { method: 'DELETE' });
                if (res.ok) {
                    await loadCompare();
                    await loadAdjRecords();
                    await loadAdjustedOrders();
                    alert('回滚成功');
                } else {
                    alert('回滚失败: ' + await res.text());
                }
            }

            let adjOrdersPage = 1;
            const adjOrdersPageSize = 10;
            let adjOrderFilterTimer = null;
            let adjOrderSortOrder = 'desc'; // 订单日期排序：desc 降序 / asc 升序 / 空串 默认(最近调整日)

            function toggleAdjOrderSort() {
                adjOrderSortOrder = adjOrderSortOrder === 'desc' ? 'asc' : 'desc';
                adjOrdersPage = 1;
                loadAdjustedOrders();
            }

            function onAdjOrderFilter() {
                adjOrdersPage = 1;
                clearTimeout(adjOrderFilterTimer);
                adjOrderFilterTimer = setTimeout(loadAdjustedOrders, 300);
            }

            async function initAdjPurchaserFilter() {
                try {
                    const res = await fetch('/api/purchaser/list');
                    const list = await res.json();
                    const sel = document.getElementById('adjPurchaserFilter');
                    sel.innerHTML = '<option value="">全部单位</option>';
                    (list || []).forEach(p => {
                        const opt = document.createElement('option');
                        opt.value = p.id;
                        opt.textContent = p.name;
                        sel.appendChild(opt);
                    });
                } catch (e) { console.error('加载采购单位失败', e); }
            }

            async function loadAdjustedOrders() {
                const keyword = document.getElementById('adjOrderKeyword').value.trim();
                const purchaserId = document.getElementById('adjPurchaserFilter').value;
                const res = await fetch('/api/supplement/adjusted_orders?page=' + adjOrdersPage + '&page_size=' + adjOrdersPageSize +
                    '&keyword=' + encodeURIComponent(keyword) + '&purchaser_id=' + encodeURIComponent(purchaserId) +
                    '&sort_order=' + adjOrderSortOrder);
                const data = await res.json();
                const list = data.items || [];
                window._adjustedOrdersMap = {};
                list.forEach(o => { window._adjustedOrdersMap[o.id] = o; });
                document.getElementById('adjustedOrdersCount').textContent = data.total || 0;
                document.getElementById('adjSortArrow').textContent = adjOrderSortOrder === 'asc' ? '▲' : '▼';
                document.getElementById('adjSumReal').textContent = (data.total_real_amount || 0).toFixed(2);
                document.getElementById('adjSumAdjust').textContent = (data.total_adjust_amount || 0).toFixed(2);
                document.getElementById('adjSumAdjusted').textContent = (data.total_adjusted_amount || 0).toFixed(2);
                const tbody = document.querySelector('#adjustedOrdersTable tbody');
                tbody.innerHTML = '';
                if (!list.length) {
                    tbody.innerHTML = '<tr><td colspan="9" class="text-center text-muted small">暂无</td></tr>';
                    renderAdjustedPager(data.total || 0);
                    return;
                }
                list.forEach(o => {
                    const tr = document.createElement('tr');
                    tr.style.cursor = 'pointer';
                    const diff = (o.adjust_amount || 0);
                    const diffColor = diff > 0 ? 'text-success' : (diff < 0 ? 'text-danger' : 'text-muted');
                    tr.innerHTML =
                        '<td><a href="javascript:void(0)" onclick="selectAdjOrderById(' + o.id + ')">' + o.order_no + '</a></td>' +
                        '<td>' + (o.purchaser_name || '') + '</td>' +
                        '<td>' + (o.order_date || '') + '</td>' +
                        '<td class="text-right">' + (o.total_amount || 0).toFixed(2) + '</td>' +
                        '<td class="text-right ' + diffColor + '">' + (diff >= 0 ? '+' : '') + diff.toFixed(2) + '</td>' +
                        '<td class="text-right"><strong>' + (o.adjusted_total || 0).toFixed(2) + '</strong></td>' +
                        '<td class="text-center">' + (o.adjust_count || 0) + '</td>' +
                        '<td>' + (o.last_adjust_date || '') + '</td>' +
                        '<td><button class="btn btn-xs btn-outline-primary" onclick="selectAdjOrderById(' + o.id + ')">查看</button></td>';
                    tbody.appendChild(tr);
                });
                renderAdjustedPager(data.total || 0);
            }

            function renderAdjustedPager(total) {
                const pages = Math.max(1, Math.ceil(total / adjOrdersPageSize));
                const ul = document.getElementById('adjustedOrdersPager');
                ul.innerHTML = '';
                const prevLi = document.createElement('li');
                prevLi.className = 'page-item' + (adjOrdersPage <= 1 ? ' disabled' : '');
                prevLi.innerHTML = '<a class="page-link" href="javascript:void(0)">上一页</a>';
                prevLi.onclick = () => { if (adjOrdersPage > 1) { adjOrdersPage--; loadAdjustedOrders(); } };
                ul.appendChild(prevLi);
                const maxShow = 5;
                let start = Math.max(1, adjOrdersPage - Math.floor(maxShow / 2));
                let end = Math.min(pages, start + maxShow - 1);
                start = Math.max(1, end - maxShow + 1);
                for (let p = start; p <= end; p++) {
                    const li = document.createElement('li');
                    li.className = 'page-item' + (p === adjOrdersPage ? ' active' : '');
                    li.innerHTML = '<a class="page-link" href="javascript:void(0)">' + p + '</a>';
                    li.onclick = (() => { const pp = p; return () => { adjOrdersPage = pp; loadAdjustedOrders(); }; })();
                    ul.appendChild(li);
                }
                const nextLi = document.createElement('li');
                nextLi.className = 'page-item' + (adjOrdersPage >= pages ? ' disabled' : '');
                nextLi.innerHTML = '<a class="page-link" href="javascript:void(0)">下一页</a>';
                nextLi.onclick = () => { if (adjOrdersPage < pages) { adjOrdersPage++; loadAdjustedOrders(); } };
                ul.appendChild(nextLi);
            }

            function selectAdjOrderById(id) {
                const o = window._adjustedOrdersMap && window._adjustedOrdersMap[id];
                if (o) selectAdjOrder(o);
            }

            // 页面初始载入采购单位列表与变更订单列表
            initAdjPurchaserFilter();
            loadAdjustedOrders();
        </script>
    "####;
    Html(crate::layout_html("订单调整与同屏比对", "/query/order_adjust", &content))
}

pub async fn page_query_stock_flow() -> Html<String> {
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
    Html(crate::layout_html("库存明细台账", "/query/stock_flow", &content))
}

pub async fn page_query_stock_summary(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/query/stock_summary").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let content = r#"
        <div class="card p-4">
            <h3>真实出入库统计</h3>
            <p class="text-muted small">真实账套口径：按日汇总采购入库金额与销售出库金额，下浮后出库金额 = 出库金额 × (1 - 销售订单下浮率/100)，毛利 = 下浮后出库金额 - 入库金额。</p>
            <div class="row mb-3">
                <div class="col-md-3">
                    <label>开始日期：</label>
                    <input type="date" id="startDate" class="form-control">
                </div>
                <div class="col-md-3">
                    <label>结束日期：</label>
                    <input type="date" id="endDate" class="form-control">
                </div>
                <div class="col-md-6 d-flex align-items-end">
                    <button onclick="searchStockSummary()" class="btn btn-primary">查询</button>
                    <button onclick="exportStockSummary()" class="btn btn-success ml-2">导出Excel</button>
                    <button onclick="resetDate('today')" class="btn btn-outline-secondary ml-2">今日</button>
                    <button onclick="resetDate('7d')" class="btn btn-outline-secondary ml-2">近7天</button>
                    <button onclick="resetDate('30d')" class="btn btn-outline-secondary ml-2">近30天</button>
                    <button onclick="resetDate('month')" class="btn btn-outline-secondary ml-2">本月</button>
                    <button onclick="resetDate('all')" class="btn btn-outline-secondary ml-2">全部</button>
                </div>
            </div>
        </div>
        <div class="row mt-3">
            <div class="col-md-3">
                <div class="card bg-light">
                    <div class="card-body py-2 text-center">
                        <div class="small text-muted">合计入库金额</div>
                        <div class="h4 text-success mb-0" id="totalIn">0.00</div>
                    </div>
                </div>
            </div>
            <div class="col-md-3">
                <div class="card bg-light">
                    <div class="card-body py-2 text-center">
                        <div class="small text-muted">合计出库金额</div>
                        <div class="h4 text-danger mb-0" id="totalOut">0.00</div>
                    </div>
                </div>
            </div>
            <div class="col-md-3">
                <div class="card bg-light">
                    <div class="card-body py-2 text-center">
                        <div class="small text-muted">下浮后合计出库金额</div>
                        <div class="h4 text-danger mb-0" id="totalDiscountedOut">0.00</div>
                    </div>
                </div>
            </div>
            <div class="col-md-3">
                <div class="card bg-light">
                    <div class="card-body py-2 text-center">
                        <div class="small text-muted">合计毛利</div>
                        <div class="h4 mb-0" id="totalGrossProfit">0.00</div>
                    </div>
                </div>
            </div>
        </div>
        <div class="card p-3 mt-3">
            <table class="table table-bordered table-sm">
                <thead class="thead-light">
                    <tr>
                        <th>日期</th>
                        <th>仓库</th>
                        <th class="text-right">入库金额</th>
                        <th class="text-center">入库单数</th>
                        <th class="text-center">入库条数</th>
                        <th class="text-right">出库金额</th>
                        <th class="text-right">下浮后出库金额</th>
                        <th class="text-center">出库单数</th>
                        <th class="text-center">出库条数</th>
                        <th class="text-right">毛利</th>
                    </tr>
                </thead>
                <tbody id="resultTable"></tbody>
            </table>
        </div>
        <script>
            function resetDate(range) {
                const today = new Date();
                const fmt = d => d.toISOString().slice(0, 10);
                const s = document.getElementById('startDate');
                const e = document.getElementById('endDate');
                if (range === 'today') { s.value = fmt(today); e.value = fmt(today); }
                else if (range === '7d') { const t = new Date(today); t.setDate(today.getDate() - 6); s.value = fmt(t); e.value = fmt(today); }
                else if (range === '30d') { const t = new Date(today); t.setDate(today.getDate() - 29); s.value = fmt(t); e.value = fmt(today); }
                else if (range === 'month') { const t = new Date(today.getFullYear(), today.getMonth(), 1); s.value = fmt(t); e.value = fmt(today); }
                else { s.value = ''; e.value = ''; }
                searchStockSummary();
            }

            async function searchStockSummary() {
                const url = '/api/query/stock_summary?start_date=' + document.getElementById('startDate').value +
                    '&end_date=' + document.getElementById('endDate').value;
                const res = await fetch(url);
                const data = await res.json();
                const tbody = document.getElementById('resultTable');
                tbody.innerHTML = '';
                if (!data.rows || data.rows.length === 0) {
                    tbody.innerHTML = '<tr><td colspan="10" class="text-center text-muted">暂无数据</td></tr>';
                } else {
                    let prevDay = null;
                    data.rows.forEach(it => {
                        const profitCls = (it.gross_profit || 0) >= 0 ? 'text-success' : 'text-danger';
                        const rowStyle = it.is_summary ? 'font-weight:bold;background-color:#fff8e1;' : '';
                        const dayDisplay = it.day !== prevDay ? it.day : '';
                        prevDay = it.day;
                        tbody.innerHTML += '<tr style="' + rowStyle + '">' +
                            '<td>' + dayDisplay + '</td>' +
                            '<td>' + it.warehouse_name + '</td>' +
                            '<td class="text-right text-success">' + (it.in_amount || 0).toFixed(2) + '</td>' +
                            '<td class="text-center">' + (it.in_order_count || 0) + '</td>' +
                            '<td class="text-center">' + (it.in_item_count || 0) + '</td>' +
                            '<td class="text-right text-danger">' + (it.out_amount || 0).toFixed(2) + '</td>' +
                            '<td class="text-right text-danger">' + (it.discounted_out_amount || 0).toFixed(2) + '</td>' +
                            '<td class="text-center">' + (it.out_order_count || 0) + '</td>' +
                            '<td class="text-center">' + (it.out_item_count || 0) + '</td>' +
                            '<td class="text-right ' + profitCls + '">' + (it.gross_profit || 0).toFixed(2) + '</td>' +
                            '</tr>';
                    });
                }
                document.getElementById('totalIn').textContent = (data.total_in_amount || 0).toFixed(2);
                document.getElementById('totalOut').textContent = (data.total_out_amount || 0).toFixed(2);
                document.getElementById('totalDiscountedOut').textContent = (data.total_discounted_out_amount || 0).toFixed(2);
                const profit = data.total_gross_profit || 0;
                const profitEl = document.getElementById('totalGrossProfit');
                profitEl.textContent = profit.toFixed(2);
                profitEl.className = 'h4 mb-0 ' + (profit >= 0 ? 'text-success' : 'text-danger');
            }

            function exportStockSummary() {
                const url = '/api/query/stock_summary/export?start_date=' + document.getElementById('startDate').value +
                    '&end_date=' + document.getElementById('endDate').value;
                window.location.href = url;
            }

            resetDate('30d');
        </script>
    "#;
    Html(crate::layout_html("真实出入库统计", "/query/stock_summary", &content))
}

pub async fn page_query_stock_summary_reimburse(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/query/stock_summary_reimburse").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let content = r#"
        <div class="card p-4">
            <h3>报销出入库统计</h3>
            <p class="text-muted small">报销账套口径：出库金额 = 真实出库 + 分摊增项净额（目标单收到的分摊 − 来源耗材单金额）。入库金额与真实账套一致。下浮后出库金额按各销售订单的下浮率计算，毛利 = 下浮后出库金额 - 入库金额。</p>
            <div class="row mb-3">
                <div class="col-md-3">
                    <label>开始日期：</label>
                    <input type="date" id="startDate" class="form-control">
                </div>
                <div class="col-md-3">
                    <label>结束日期：</label>
                    <input type="date" id="endDate" class="form-control">
                </div>
                <div class="col-md-6 d-flex align-items-end">
                    <button onclick="searchStockSummary()" class="btn btn-primary">查询</button>
                    <button onclick="exportStockSummary()" class="btn btn-success ml-2">导出Excel</button>
                    <button onclick="resetDate('today')" class="btn btn-outline-secondary ml-2">今日</button>
                    <button onclick="resetDate('7d')" class="btn btn-outline-secondary ml-2">近7天</button>
                    <button onclick="resetDate('30d')" class="btn btn-outline-secondary ml-2">近30天</button>
                    <button onclick="resetDate('month')" class="btn btn-outline-secondary ml-2">本月</button>
                    <button onclick="resetDate('all')" class="btn btn-outline-secondary ml-2">全部</button>
                </div>
            </div>
        </div>
        <div class="row mt-3">
            <div class="col-md-3">
                <div class="card bg-light">
                    <div class="card-body py-2 text-center">
                        <div class="small text-muted">合计入库金额</div>
                        <div class="h4 text-success mb-0" id="totalIn">0.00</div>
                    </div>
                </div>
            </div>
            <div class="col-md-3">
                <div class="card bg-light">
                    <div class="card-body py-2 text-center">
                        <div class="small text-muted">合计出库金额</div>
                        <div class="h4 text-danger mb-0" id="totalOut">0.00</div>
                    </div>
                </div>
            </div>
            <div class="col-md-3">
                <div class="card bg-light">
                    <div class="card-body py-2 text-center">
                        <div class="small text-muted">下浮后合计出库金额</div>
                        <div class="h4 text-danger mb-0" id="totalDiscountedOut">0.00</div>
                    </div>
                </div>
            </div>
            <div class="col-md-3">
                <div class="card bg-light">
                    <div class="card-body py-2 text-center">
                        <div class="small text-muted">合计毛利</div>
                        <div class="h4 mb-0" id="totalGrossProfit">0.00</div>
                    </div>
                </div>
            </div>
        </div>
        <div class="card p-3 mt-3">
            <table class="table table-bordered table-sm">
                <thead class="thead-light">
                    <tr>
                        <th>日期</th>
                        <th>仓库</th>
                        <th class="text-right">入库金额</th>
                        <th class="text-center">入库单数</th>
                        <th class="text-center">入库条数</th>
                        <th class="text-right">出库金额</th>
                        <th class="text-right">下浮后出库金额</th>
                        <th class="text-center">出库单数</th>
                        <th class="text-center">出库条数</th>
                        <th class="text-right">毛利</th>
                    </tr>
                </thead>
                <tbody id="resultTable"></tbody>
            </table>
        </div>
        <script>
            function resetDate(range) {
                const today = new Date();
                const fmt = d => d.toISOString().slice(0, 10);
                const s = document.getElementById('startDate');
                const e = document.getElementById('endDate');
                if (range === 'today') { s.value = fmt(today); e.value = fmt(today); }
                else if (range === '7d') { const t = new Date(today); t.setDate(today.getDate() - 6); s.value = fmt(t); e.value = fmt(today); }
                else if (range === '30d') { const t = new Date(today); t.setDate(today.getDate() - 29); s.value = fmt(t); e.value = fmt(today); }
                else if (range === 'month') { const t = new Date(today.getFullYear(), today.getMonth(), 1); s.value = fmt(t); e.value = fmt(today); }
                else { s.value = ''; e.value = ''; }
                searchStockSummary();
            }

            async function searchStockSummary() {
                const url = '/api/query/stock_summary_reimburse?start_date=' + document.getElementById('startDate').value +
                    '&end_date=' + document.getElementById('endDate').value;
                const res = await fetch(url);
                const data = await res.json();
                const tbody = document.getElementById('resultTable');
                tbody.innerHTML = '';
                if (!data.rows || data.rows.length === 0) {
                    tbody.innerHTML = '<tr><td colspan="10" class="text-center text-muted">暂无数据</td></tr>';
                } else {
                    let prevDay = null;
                    data.rows.forEach(it => {
                        const profitCls = (it.gross_profit || 0) >= 0 ? 'text-success' : 'text-danger';
                        const rowStyle = it.is_summary ? 'font-weight:bold;background-color:#fff8e1;' : '';
                        const dayDisplay = it.day !== prevDay ? it.day : '';
                        prevDay = it.day;
                        tbody.innerHTML += '<tr style="' + rowStyle + '">' +
                            '<td>' + dayDisplay + '</td>' +
                            '<td>' + it.warehouse_name + '</td>' +
                            '<td class="text-right text-success">' + (it.in_amount || 0).toFixed(2) + '</td>' +
                            '<td class="text-center">' + (it.in_order_count || 0) + '</td>' +
                            '<td class="text-center">' + (it.in_item_count || 0) + '</td>' +
                            '<td class="text-right text-danger">' + (it.out_amount || 0).toFixed(2) + '</td>' +
                            '<td class="text-right text-danger">' + (it.discounted_out_amount || 0).toFixed(2) + '</td>' +
                            '<td class="text-center">' + (it.out_order_count || 0) + '</td>' +
                            '<td class="text-center">' + (it.out_item_count || 0) + '</td>' +
                            '<td class="text-right ' + profitCls + '">' + (it.gross_profit || 0).toFixed(2) + '</td>' +
                            '</tr>';
                    });
                }
                document.getElementById('totalIn').textContent = (data.total_in_amount || 0).toFixed(2);
                document.getElementById('totalOut').textContent = (data.total_out_amount || 0).toFixed(2);
                document.getElementById('totalDiscountedOut').textContent = (data.total_discounted_out_amount || 0).toFixed(2);
                const profit = data.total_gross_profit || 0;
                const profitEl = document.getElementById('totalGrossProfit');
                profitEl.textContent = profit.toFixed(2);
                profitEl.className = 'h4 mb-0 ' + (profit >= 0 ? 'text-success' : 'text-danger');
            }

            function exportStockSummary() {
                const url = '/api/query/stock_summary_reimburse/export?start_date=' + document.getElementById('startDate').value +
                    '&end_date=' + document.getElementById('endDate').value;
                window.location.href = url;
            }

            resetDate('30d');
        </script>
    "#;
    Html(crate::layout_html("报销出入库统计", "/query/stock_summary_reimburse", &content))
}

pub async fn page_query_stock_warning() -> Html<String> {
    let content = r#"
        <div class="card p-4">
            <h3>库存上下限预警</h3>
            <button onclick="searchStockWarning()" class="btn btn-primary">查询</button>
            <a href="/api/query/stock_warning/export" class="btn btn-success ml-2">导出Excel</a>
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
    Html(crate::layout_html("库存上下限预警", "/query/stock_warning", &content))
}

pub async fn page_query_slow_stock() -> Html<String> {
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
    Html(crate::layout_html("呆滞库存查询", "/query/slow_stock", &content))
}

pub async fn page_query_income_expense() -> Html<String> {
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
            <a href="/api/query/income_expense/export" class="btn btn-success ml-2">导出Excel</a>
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
    Html(crate::layout_html("收支流水查询", "/query/income_expense", &content))
}

pub async fn page_query_profit_detail() -> Html<String> {
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
            <a href="/api/query/profit_detail/export" class="btn btn-success ml-2">导出Excel</a>
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
    Html(crate::layout_html("毛利明细查询", "/query/profit_detail", &content))
}

pub async fn page_query_category_stats() -> Html<String> {
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
    Html(crate::layout_html("品类进销存统计", "/query/category_stats", &content))
}

pub async fn page_query_document_summary() -> Html<String> {
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
            <a href="/api/query/document_summary/export" class="btn btn-success ml-2">导出Excel</a>
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
    Html(crate::layout_html("单据汇总查询", "/query/document_summary", &content))
}

pub async fn page_system(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/system").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let rows = sqlx::query("SELECT key, value FROM system_config")
        .fetch_all(crate::pool())
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

    Html(crate::layout_html("系统参数", "/system", &content))
}

pub async fn page_operation_log(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/system/operation_log").await {
        Err(e) => return e,
        Ok(_) => {}
    }

    let content = r#"
        <div class="card p-4">
            <h3>操作日志</h3>
            <p class="text-muted">记录所有关键写操作（创建/修改/删除/审核/反审核/调价/状态流转等），全程留痕可追溯。</p>

            <div class="row g-2 mb-3">
                <div class="col-auto">
                    <input type="text" id="logUsername" class="form-control" placeholder="操作人">
                </div>
                <div class="col-auto">
                    <input type="text" id="logAction" class="form-control" placeholder="动作（如 审核）">
                </div>
                <div class="col-auto">
                    <input type="date" id="logStartDate" class="form-control">
                </div>
                <div class="col-auto">
                    <input type="date" id="logEndDate" class="form-control">
                </div>
                <div class="col-auto">
                    <button onclick="searchLogs()" class="btn btn-primary">查询</button>
                    <button onclick="resetLogs()" class="btn btn-secondary">重置</button>
                    <button onclick="exportLogs()" class="btn btn-success">导出Excel</button>
                </div>
            </div>

            <table class="table table-bordered table-sm">
                <thead>
                    <tr>
                        <th>ID</th>
                        <th>时间</th>
                        <th>操作人</th>
                        <th>动作</th>
                        <th>目标</th>
                        <th>详情</th>
                    </tr>
                </thead>
                <tbody id="logListBody"></tbody>
            </table>
            <div id="pagination"></div>
        </div>

        <script>
            let logPage = 1;

            function renderLogs(data) {
                const tbody = document.getElementById('logListBody');
                tbody.innerHTML = '';
                (data.data || []).forEach(row => {
                    const typeLabel = row.target_type === 'purchase_order' ? '采购单' : row.target_type === 'sales_order' ? '销售单' : row.target_type === 'purchase_document' ? '采购单据' : (row.target_type || '-');
                    tbody.innerHTML += '<tr>' +
                        '<td>' + row.id + '</td>' +
                        '<td>' + row.created_at + '</td>' +
                        '<td>' + (row.username || '-') + '</td>' +
                        '<td><span class="badge bg-info text-dark">' + row.action_label + '</span></td>' +
                        '<td>' + typeLabel + (row.target_id ? ' #' + row.target_id : '') + '</td>' +
                        '<td class="small">' + (row.detail || '') + '</td>' +
                        '</tr>';
                });
                if (!data.data || data.data.length === 0) {
                    tbody.innerHTML = '<tr><td colspan="6" class="text-center text-muted">暂无日志记录</td></tr>';
                }
                renderPagination(data);
            }

            function renderPagination(data) {
                const container = document.getElementById('pagination');
                if (!container) return;
                if (data.total_pages <= 1) {
                    container.innerHTML = '<p class="text-center text-muted mt-2">共 ' + data.total + ' 条记录</p>';
                    return;
                }
                let html = '<nav><ul class="pagination justify-content-center">';
                html += '<li class="page-item ' + (data.page <= 1 ? 'disabled' : '') + '"><a class="page-link" onclick="loadLogs(' + (data.page - 1) + ')">上一页</a></li>';
                for (let i = 1; i <= data.total_pages; i++) {
                    html += '<li class="page-item ' + (i === data.page ? 'active' : '') + '"><a class="page-link" onclick="loadLogs(' + i + ')">' + i + '</a></li>';
                }
                html += '<li class="page-item ' + (data.page >= data.total_pages ? 'disabled' : '') + '"><a class="page-link" onclick="loadLogs(' + (data.page + 1) + ')">下一页</a></li>';
                html += '</ul></nav>';
                html += '<p class="text-center text-muted mt-2">共 ' + data.total + ' 条记录，当前第 ' + data.page + '/' + data.total_pages + ' 页</p>';
                container.innerHTML = html;
            }

            function buildLogUrl(page) {
                let url = '/api/system/operation_log?page=' + page + '&page_size=20';
                const username = document.getElementById('logUsername').value.trim();
                const action = document.getElementById('logAction').value.trim();
                const startDate = document.getElementById('logStartDate').value;
                const endDate = document.getElementById('logEndDate').value;
                if (username) url += '&username=' + encodeURIComponent(username);
                if (action) url += '&action=' + encodeURIComponent(action);
                if (startDate) url += '&start_date=' + startDate;
                if (endDate) url += '&end_date=' + endDate;
                return url;
            }

            async function loadLogs(page) {
                if (page !== undefined) logPage = page;
                const res = await fetch(buildLogUrl(logPage));
                const data = await res.json();
                renderLogs(data);
            }

            function searchLogs() {
                logPage = 1;
                loadLogs(1);
            }

            function resetLogs() {
                document.getElementById('logUsername').value = '';
                document.getElementById('logAction').value = '';
                document.getElementById('logStartDate').value = '';
                document.getElementById('logEndDate').value = '';
                logPage = 1;
                loadLogs(1);
            }

            // 导出 Excel：应用当前筛选条件，下载全量匹配记录
            function exportLogs() {
                const params = new URLSearchParams();
                const username = document.getElementById('logUsername').value.trim();
                const action = document.getElementById('logAction').value.trim();
                const startDate = document.getElementById('logStartDate').value;
                const endDate = document.getElementById('logEndDate').value;
                if (username) params.set('username', username);
                if (action) params.set('action', action);
                if (startDate) params.set('start_date', startDate);
                if (endDate) params.set('end_date', endDate);
                const qs = params.toString();
                window.location.href = '/api/system/operation_log/export' + (qs ? '?' + qs : '');
            }

            loadLogs(1);
        </script>
    "#;

    Html(crate::layout_html("操作日志", "/system/operation_log", &content))
}

pub async fn page_user(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/user").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let rows = sqlx::query("SELECT u.id, u.username, u.nickname, u.role, u.status, u.last_login_time, u.create_at, COALESCE(u.supplier_id,0) as supplier_id, COALESCE(u.purchaser_id,0) as purchaser_id, COALESCE(s.name,'') as supplier_name, COALESCE(p.name,'') as purchaser_name FROM user_account u LEFT JOIN supplier s ON u.supplier_id = s.id LEFT JOIN purchaser p ON u.purchaser_id = p.id ORDER BY u.id")
        .fetch_all(crate::pool())
        .await
        .unwrap_or_default();

    // 绑定下拉选项：供应商 / 采购方
    let mut supplier_options = String::from("<option value=\"0\">未绑定</option>");
    let supplier_rows = sqlx::query("SELECT id, name FROM supplier ORDER BY name")
        .fetch_all(crate::pool())
        .await
        .unwrap_or_default();
    for sr in &supplier_rows {
        let sid: i64 = sr.get("id");
        let sname: String = sr.get("name");
        supplier_options.push_str(&format!(r#"<option value="{}">{}</option>"#, sid, sname));
    }
    let mut purchaser_options = String::from("<option value=\"0\">未绑定</option>");
    let purchaser_rows = sqlx::query("SELECT id, name FROM purchaser ORDER BY name")
        .fetch_all(crate::pool())
        .await
        .unwrap_or_default();
    for pr in &purchaser_rows {
        let pid: i64 = pr.get("id");
        let pname: String = pr.get("name");
        purchaser_options.push_str(&format!(r#"<option value="{}">{}</option>"#, pid, pname));
    }

    let mut table_html = String::new();
    for row in rows {
        let id: i64 = row.get("id");
        let username: String = row.get("username");
        let nickname: String = row.get("nickname");
        let role: String = row.get("role");
        let status: i32 = row.get("status");
        let last_login_time: Option<String> = row.get("last_login_time");
        let create_at: String = row.get("create_at");
        let supplier_id: i64 = row.get("supplier_id");
        let purchaser_id: i64 = row.get("purchaser_id");
        let supplier_name: String = row.get("supplier_name");
        let purchaser_name: String = row.get("purchaser_name");
        
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

        // 数据权限展示：供应商角色显示绑定的供应商，采购方角色显示绑定的采购单位
        let data_scope = match role.as_str() {
            "supplier" if supplier_id > 0 => format!("供应商：{}", supplier_name),
            "purchaser" if purchaser_id > 0 => format!("采购方：{}", purchaser_name),
            _ => "-".to_string(),
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
            data_scope,
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
                        <th>数据权限</th>
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
                                <select id="role" class="form-control" onchange="toggleDataScope()">
                                    <option value="admin">管理员</option>
                                    <option value="supplier">供应商</option>
                                    <option value="purchaser">采购方</option>
                                    <option value="user">普通用户</option>
                                </select>
                            </div>
                            <div class="mb-3" id="supplierBindRow">
                                <label class="form-label">绑定供应商（供应商角色数据权限）</label>
                                <select id="supplierId" class="form-control">{}</select>
                            </div>
                            <div class="mb-3" id="purchaserBindRow">
                                <label class="form-label">绑定采购单位（采购方角色数据权限）</label>
                                <select id="purchaserId" class="form-control">{}</select>
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

            // 根据角色显示/隐藏数据权限绑定行
            function toggleDataScope() {{
                const role = document.getElementById('role').value;
                document.getElementById('supplierBindRow').style.display = (role === 'supplier') ? '' : 'none';
                document.getElementById('purchaserBindRow').style.display = (role === 'purchaser') ? '' : 'none';
            }}

            function showAddModal() {{
                currentUserId = null;
                document.getElementById('modalTitle').textContent = '添加用户';
                document.getElementById('userForm').reset();
                document.getElementById('userId').value = '';
                document.getElementById('supplierId').value = '0';
                document.getElementById('purchaserId').value = '0';
                toggleDataScope();
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
                    document.getElementById('supplierId').value = String(data.user.supplier_id || 0);
                    document.getElementById('purchaserId').value = String(data.user.purchaser_id || 0);
                    document.getElementById('password').value = '';
                    toggleDataScope();
                    new bootstrap.Modal(document.getElementById('userModal')).show();
                }}
            }}

            async function saveUser() {{
                const id = document.getElementById('userId').value;
                const data = {{
                    username: document.getElementById('username').value,
                    nickname: document.getElementById('nickname').value,
                    password: document.getElementById('password').value,
                    role: document.getElementById('role').value,
                    supplier_id: parseInt(document.getElementById('supplierId').value || '0'),
                    purchaser_id: parseInt(document.getElementById('purchaserId').value || '0')
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
    "#, table_html, supplier_options, purchaser_options);

    Html(crate::layout_html("用户管理", "/user", &content))
}

pub async fn page_backup(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/backup").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let rows = sqlx::query("SELECT id, backup_time, file_name, size FROM backup_record ORDER BY backup_time DESC")
        .fetch_all(crate::pool())
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

    Html(crate::layout_html("数据备份", "/backup", &content))
}

pub async fn page_restore(headers: axum::http::HeaderMap) -> Html<String> {
    match crate::check_page_permission(&headers, "/restore").await {
        Err(e) => return e,
        Ok(_) => {}
    }
    let rows = sqlx::query("SELECT id, backup_time, file_name FROM backup_record ORDER BY backup_time DESC")
        .fetch_all(crate::pool())
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

    Html(crate::layout_html("数据恢复", "/restore", &content))
}

pub async fn page_mobile_sort() -> Html<String> {
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
        .history-bar { background: #ecfdf5; padding: 12px; border-bottom: 1px solid #eee; display: flex; gap: 8px; }
        .history-bar input { flex: 1; padding: 10px 14px; border: 1px solid #ddd; border-radius: 8px; font-size: 14px; }
        .history-bar button { padding: 10px 16px; border: none; border-radius: 8px; background: #10b981; color: white; font-size: 14px; white-space: nowrap; }
        .history-bar button.clear { background: #f3f4f6; color: #666; }
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
    
    <div class="history-bar">
        <input type="date" id="historyDate" title="选择日期检索该日期的历史分拣，留空显示当前待分拣">
        <button onclick="loadItems()">检索历史分拣</button>
        <button class="clear" onclick="clearHistory()">清除</button>
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
                const date = document.getElementById('historyDate').value;
                let url = '/api/sales_order/sort_items';
                if (date) url += '?date=' + encodeURIComponent(date);
                const res = await fetch(url);
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

        function clearHistory() {
            document.getElementById('historyDate').value = '';
            document.getElementById('searchInput').value = '';
            loadItems();
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
            const date = document.getElementById('historyDate').value;
            let url = '/api/sales_order/sort_items_excel';
            if (date) url += '?date=' + encodeURIComponent(date);
            window.location.href = url;
        }

        loadItems();
    </script>
</body>
</html>
    "#.to_string())
}

pub async fn page_mobile_sort_by_purchaser() -> Html<String> {
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
        .history-bar { background: #ecfdf5; padding: 12px; border-bottom: 1px solid #eee; display: flex; gap: 8px; }
        .history-bar input { flex: 1; padding: 10px 14px; border: 1px solid #ddd; border-radius: 8px; font-size: 14px; }
        .history-bar button { padding: 10px 16px; border: none; border-radius: 8px; background: #10b981; color: white; font-size: 14px; white-space: nowrap; }
        .history-bar button.clear { background: #f3f4f6; color: #666; }
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
    
    <div class="history-bar">
        <input type="date" id="historyDate" title="选择日期检索该日期的历史分拣，留空显示当前待分拣">
        <button onclick="loadItems()">检索历史分拣</button>
        <button class="clear" onclick="clearHistory()">清除</button>
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
                const date = document.getElementById('historyDate').value;
                let url = '/api/sales_order/sort_items_by_purchaser';
                if (date) url += '?date=' + encodeURIComponent(date);
                const res = await fetch(url);
                purchasers = await res.json();
                loadCheckedState();
                loadCorrectedQuantities();
                renderItems();
                updateStats();
            } catch (e) {
                console.error('加载失败:', e);
            }
        }

        function clearHistory() {
            document.getElementById('historyDate').value = '';
            document.getElementById('searchInput').value = '';
            loadItems();
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
            const date = document.getElementById('historyDate').value;
            let url = '/api/sales_order/sort_items_by_purchaser_excel';
            if (date) url += '?date=' + encodeURIComponent(date);
            window.location.href = url;
        }

        loadItems();
    </script>
</body>
</html>
    "#.to_string())
}

pub async fn page_mobile_sort_by_category() -> Html<String> {
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
        .history-bar { background: #ecfdf5; padding: 12px; border-bottom: 1px solid #eee; display: flex; gap: 8px; }
        .history-bar input { flex: 1; padding: 10px 14px; border: 1px solid #ddd; border-radius: 8px; font-size: 14px; }
        .history-bar button { padding: 10px 16px; border: none; border-radius: 8px; background: #10b981; color: white; font-size: 14px; white-space: nowrap; }
        .history-bar button.clear { background: #f3f4f6; color: #666; }
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
    
    <div class="history-bar">
        <input type="date" id="historyDate" title="选择日期检索该日期的历史分拣，留空显示当前待分拣">
        <button onclick="loadItems()">检索历史分拣</button>
        <button class="clear" onclick="clearHistory()">清除</button>
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
                const date = document.getElementById('historyDate').value;
                let url = '/api/sales_order/sort_items_by_category';
                if (date) url += '?date=' + encodeURIComponent(date);
                const res = await fetch(url);
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

        function clearHistory() {
            document.getElementById('historyDate').value = '';
            document.getElementById('searchInput').value = '';
            loadItems();
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
            const date = document.getElementById('historyDate').value;
            let url = '/api/sales_order/sort_items_by_category_excel';
            if (date) url += '?date=' + encodeURIComponent(date);
            window.location.href = url;
        }

        loadItems();
    </script>
</body>
</html>
    "#.to_string())
}

pub async fn page_mobile_sort_by_supplier() -> Html<String> {
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
        .history-bar { background: #ecfdf5; padding: 12px; border-bottom: 1px solid #eee; display: flex; gap: 8px; }
        .history-bar input { flex: 1; padding: 10px 14px; border: 1px solid #ddd; border-radius: 8px; font-size: 14px; }
        .history-bar button { padding: 10px 16px; border: none; border-radius: 8px; background: #10b981; color: white; font-size: 14px; white-space: nowrap; }
        .history-bar button.clear { background: #f3f4f6; color: #666; }
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
    
    <div class="history-bar">
        <input type="date" id="historyDate" title="选择日期检索该日期的历史分拣，留空显示当前待分拣">
        <button onclick="loadItems()">检索历史分拣</button>
        <button class="clear" onclick="clearHistory()">清除</button>
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
        <button class="btn-export" onclick="exportExcel(true)">导出(含数值)</button>
    </div>

    <script>
        let suppliers = [];
        let checkedIds = new Set();
        let correctedQuantities = {};

        async function loadItems() {
            try {
                const date = document.getElementById('historyDate').value;
                let url = '/api/sales_order/sort_items_by_supplier';
                if (date) url += '?date=' + encodeURIComponent(date);
                const res = await fetch(url);
                suppliers = await res.json();
                loadCheckedState();
                loadCorrectedQuantities();
                renderItems();
                updateStats();
            } catch (e) {
                console.error('加载失败:', e);
            }
        }

        function clearHistory() {
            document.getElementById('historyDate').value = '';
            loadItems();
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

        function exportExcel(withValues) {
            const date = document.getElementById('historyDate').value;
            let url = '/api/sales_order/sort_items_by_supplier_excel';
            let params = [];
            if (date) params.push('date=' + encodeURIComponent(date));
            if (withValues) params.push('print_values=1');
            if (params.length) url += '?' + params.join('&');
            window.location.href = url;
        }

        loadItems();
    </script>
</body>
</html>
    "#.to_string())
}

pub async fn page_mobile_sort_comprehensive() -> Html<String> {
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
    
    <div class="history-bar">
        <input type="date" id="historyDate" title="选择日期检索该日期的历史分拣，留空显示当前待分拣">
        <button onclick="loadItems()">检索历史分拣</button>
        <button class="clear" onclick="clearHistory()">清除</button>
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
                const date = document.getElementById('historyDate').value;
                let url = '/api/sales_order/sort_comprehensive';
                if (date) url += '?date=' + encodeURIComponent(date);
                const res = await fetch(url);
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

        function clearHistory() {
            document.getElementById('historyDate').value = '';
            document.getElementById('searchInput').value = '';
            loadItems();
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
            const date = document.getElementById('historyDate').value;
            let url = '/api/sales_order/sort_comprehensive_excel';
            if (date) url += '?date=' + encodeURIComponent(date);
            window.location.href = url;
        }

        loadItems();
    </script>
</body>
</html>
    "#.to_string())
}

pub async fn page_login() -> Html<String> {
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

pub async fn page_supplement() -> Html<String> {
    let content = r#"
        <div class="card mb-4">
            <div class="card-body">
                <h4>耗材分摊管理</h4>
                
                <div class="row" id="topSection">
                    <div class="col-md-4">
                        <label>采购单位</label>
                        <div class="position-relative">
                            <input type="text" id="purchaserInput" class="form-control" placeholder="单击选择 / 双击搜索" readonly>
                            <div id="purchaserDropdown" class="search-dropdown"></div>
                        </div>
                    </div>
                    <div class="col-md-4">
                        <label>耗材订单（来源）</label>
                        <div class="position-relative">
                            <input type="text" id="consumableOrderInput" class="form-control" placeholder="单击选择 / 双击搜索" readonly>
                            <div id="consumableOrderDropdown" class="search-dropdown"></div>
                        </div>
                    </div>
                    <div class="col-md-4">
                        <label>目标订单（分摊到）</label>
                        <div class="position-relative">
                            <input type="text" id="targetOrderInput" class="form-control" placeholder="单击选择 / 双击搜索" readonly>
                            <div id="targetOrderDropdown" class="search-dropdown"></div>
                        </div>
                    </div>
                </div>

                <div class="mt-3">
                    <div class="d-flex justify-content-between align-items-center mb-1">
                        <h6 class="mb-0">已进入分摊的订单列表 <span class="badge badge-info" id="allocatedOrdersCount">0</span></h6>
                        <button class="btn btn-xs btn-outline-secondary" onclick="loadAllocatedOrders()">刷新</button>
                    </div>
                    <div style="max-height:220px;overflow-y:auto;border:1px solid #eee;">
                        <table class="table table-sm table-bordered mb-0" id="allocatedOrdersTable">
                            <thead class="thead-light"><tr>
                                <th>订单号</th><th>采购单位</th><th>订单日期</th>
                                <th>总金额</th><th>已分摊</th><th>剩余余额</th>
                                <th>状态</th><th>创建时间</th><th>完成时间</th><th>操作</th>
                            </tr></thead>
                            <tbody><tr><td colspan="10" class="text-center text-muted small">暂无</td></tr></tbody>
                        </table>
                    </div>
                </div>

                <div class="row mt-4" id="bottomSection">
                    <div class="col-md-6">
                        <div class="card">
                            <div class="card-header bg-danger text-white d-flex justify-content-between align-items-center">
                                <h5>可分摊源订单列表</h5>
                                <div id="allocationStatusBadge" class="badge badge-light" style="display: none;">未分摊</div>
                            </div>
                            <div id="allocationSummary" class="card-body p-2" style="display: none; background-color: #fff3f3;">
                                <div class="row">
                                    <div class="col-md-3 text-center">
                                        <div class="small text-muted">总金额</div>
                                        <div class="font-bold" id="summaryTotal">0.00</div>
                                    </div>
                                    <div class="col-md-3 text-center">
                                        <div class="small text-muted">已分摊</div>
                                        <div class="font-bold text-success" id="summaryAllocated">0.00</div>
                                    </div>
                                    <div class="col-md-3 text-center">
                                        <div class="small text-muted">未分摊余额</div>
                                        <div class="font-bold text-danger" id="summaryRemaining">0.00</div>
                                    </div>
                                    <div class="col-md-3 text-center">
                                        <div class="small text-muted">分摊状态</div>
                                        <div class="font-bold" id="summaryStatus">未分摊</div>
                                    </div>
                                </div>
                            </div>
                            <div class="card-body" style="height: 250px; overflow-y: auto;">
                                <table class="table table-sm table-bordered" id="consumableOrderList">
                                    <thead><tr><th>订单号</th><th>日期</th><th>金额</th><th>状态</th></tr></thead>
                                    <tbody></tbody>
                                </table>
                            </div>
                            <div class="card-footer">
                                <div id="consumableOrderDetail">
                                    <p>请选择订单查看详情</p>
                                </div>
                                <div id="allocationInitActions" class="mt-2" style="display: none;">
                                    <button class="btn btn-sm btn-primary" onclick="createAllocation()">初始化分摊方案</button>
                                </div>
                                <div id="allocationManageActions" class="mt-2" style="display: none;">
                                    <button class="btn btn-sm btn-success" onclick="confirmCompleteAllocation()">确认完成分摊</button>
                                    <button class="btn btn-sm btn-warning" onclick="terminateAllocation()">终止分摊</button>
                                    <button class="btn btn-sm btn-secondary" onclick="cancelAllocation()" title="仅在未产生分摊记录时可用；取消后回到未分摊状态">取消分摊方案</button>
                                </div>
                            </div>
                        </div>
                    </div>
                    <div class="col-md-6">
                        <div class="card">
                            <div class="card-header bg-success text-white">
                                <h5>目标订单列表（可分摊到）</h5>
                            </div>
                            <div class="card-body" style="height: 250px; overflow-y: auto;">
                                <table class="table table-sm table-bordered" id="targetOrderList">
                                    <thead><tr><th>订单号</th><th>日期</th><th>金额</th><th>订单状态</th></tr></thead>
                                    <tbody></tbody>
                                </table>
                            </div>
                            <div class="card-footer">
                                <div id="targetOrderDetail">
                                    <p>请选择订单查看详情</p>
                                </div>
                                <div id="compareArea" style="display: none;">
                                    <hr>
                                    <h6>真实账套 vs 分摊账套 对比</h6>
                                    <div class="row">
                                        <div class="col-md-6">
                                            <div class="card bg-light">
                                                <div class="card-header py-1">
                                                    <small><strong>真实账套</strong> - <span id="realTotalLabel">0.00</span> 元</small>
                                                </div>
                                                <div class="card-body p-1" style="max-height: 300px; overflow-y: auto;">
                                                    <table class="table table-sm table-bordered mb-0" id="realTable">
                                                        <thead class="thead-light"><tr><th>商品</th><th>数量</th><th>金额</th></tr></thead>
                                                        <tbody></tbody>
                                                    </table>
                                                </div>
                                            </div>
                                        </div>
                                        <div class="col-md-6">
                                            <div class="card border-success">
                                                <div class="card-header bg-success text-white py-1">
                                                    <small><strong>分摊账套</strong> - <span id="allocTotalLabel">0.00</span> 元 <span class="badge badge-warning ml-1">差异 +<span id="diffTotalLabel">0.00</span></span></small>
                                                </div>
                                                <div class="card-body p-1" style="max-height: 300px; overflow-y: auto;">
                                                    <table class="table table-sm table-bordered mb-0" id="allocTable">
                                                        <thead class="thead-light"><tr><th>商品</th><th>数量</th><th>金额</th></tr></thead>
                                                        <tbody></tbody>
                                                    </table>
                                                </div>
                                            </div>
                                        </div>
                                    </div>
                                </div>
                                <div id="supplementActionArea" class="mt-3" style="display: none;">
                                    <hr>
                                    <h6>增项操作</h6>
                                    <div class="row mb-2">
                                        <div class="col-md-12">
                                            <label class="radio-inline mr-4">
                                                <input type="radio" name="operationType" value="new_item" checked onchange="toggleOperationType()"> 新增商品行
                                            </label>
                                            <label class="radio-inline mr-4">
                                                <input type="radio" name="operationType" value="increase_quantity" onchange="toggleOperationType()"> 追加已有商品数量
                                            </label>
                                            <label class="radio-inline">
                                                <input type="radio" name="operationType" value="replace_item" onchange="toggleOperationType()"> 替换明细
                                            </label>
                                        </div>
                                    </div>
                                    <div class="row mb-2" id="newItemSection">
                                        <div class="col-md-6">
                                            <label>选择商品</label>
                                            <div class="position-relative">
                                                <input type="text" id="suppProductInput" class="form-control form-control-sm" placeholder="点击选择商品" readonly>
                                                <div id="suppProductDropdown" class="search-dropdown"></div>
                                            </div>
                                        </div>
                                        <div class="col-md-2">
                                            <label>增项数量</label>
                                            <input type="number" step="0.01" id="addQtyInput" class="form-control form-control-sm" oninput="calcSupplementAmount()">
                                        </div>
                                        <div class="col-md-2">
                                            <label>单价</label>
                                            <input type="number" step="0.01" id="addPriceInput" class="form-control form-control-sm" oninput="calcSupplementAmount()">
                                        </div>
                                        <div class="col-md-2">
                                            <label>增项金额</label>
                                            <input type="number" step="0.01" id="addAmountInput" class="form-control form-control-sm" readonly>
                                        </div>
                                    </div>
                                    <div class="row mb-2" id="increaseQtySection" style="display: none;">
                                        <div class="col-md-4">
                                            <label>选择目标商品</label>
                                            <select id="increaseProductSelect" class="form-control form-control-sm"></select>
                                        </div>
                                        <div class="col-md-2">
                                            <label>追加数量</label>
                                            <input type="number" step="0.01" id="increaseQtyInput" class="form-control form-control-sm" oninput="calcIncreaseAmount()">
                                        </div>
                                        <div class="col-md-2">
                                            <label>单价</label>
                                            <input type="text" id="increasePriceInput" class="form-control form-control-sm" readonly>
                                        </div>
                                        <div class="col-md-2">
                                            <label>追加金额</label>
                                            <input type="text" id="increaseAmountInput" class="form-control form-control-sm" readonly>
                                        </div>
                                        <div class="col-md-2">
                                            <label>&nbsp;</label><br>
                                            <span class="text-muted small" id="increaseTotalHint">合计数量: 0</span>
                                        </div>
                                    </div>
                                    <div id="replaceSection" style="display: none;">
                                        <div class="row mb-2">
                                            <div class="col-md-6">
                                                <label>被替换的原明细（从真实明细选择）</label>
                                                <select id="replaceSourceSelect" class="form-control form-control-sm" onchange="onReplaceSourceChange()"></select>
                                            </div>
                                            <div class="col-md-6">
                                                <label>&nbsp;</label><br>
                                                <span class="text-muted small" id="replaceSourceHint">原明细金额: 0.00</span>
                                            </div>
                                        </div>
                                        <div class="row mb-2 align-items-end">
                                            <div class="col-md-5">
                                                <label>替换为商品</label>
                                                <div class="position-relative">
                                                    <input type="text" id="replaceProductInput" class="form-control form-control-sm" placeholder="点击选择商品" readonly>
                                                    <div id="replaceProductDropdown" class="search-dropdown"></div>
                                                </div>
                                            </div>
                                            <div class="col-md-2">
                                                <label>数量</label>
                                                <input type="number" step="0.01" id="replaceQtyInput" class="form-control form-control-sm" oninput="calcReplaceAmount()">
                                            </div>
                                            <div class="col-md-2">
                                                <label>单价</label>
                                                <input type="number" step="0.01" id="replacePriceInput" class="form-control form-control-sm" oninput="calcReplaceAmount()">
                                            </div>
                                            <div class="col-md-2">
                                                <label>金额</label>
                                                <input type="number" step="0.01" id="replaceAmountInput" class="form-control form-control-sm" readonly>
                                            </div>
                                            <div class="col-md-1">
                                                <button class="btn btn-sm btn-outline-primary" onclick="addReplaceLine()">加行</button>
                                            </div>
                                        </div>
                                        <table class="table table-sm table-bordered" id="replaceLineList">
                                            <thead><tr><th>替换为商品</th><th>数量</th><th>单价</th><th>金额</th><th>操作</th></tr></thead>
                                            <tbody></tbody>
                                        </table>
                                        <div class="mb-2">
                                            <span class="text-muted small">替换后合计: <span id="replaceLinesTotal">0.00</span> 元</span>
                                            <span class="ml-3 small" id="replaceDiffHint"></span>
                                        </div>
                                    </div>
                                    <div class="row">
                                        <div class="col-md-2">
                                            <label>分摊日期</label>
                                            <input type="date" id="allocateDateInput" class="form-control form-control-sm">
                                        </div>
                                        <div class="col-md-10">
                                            <label>&nbsp;</label><br>
                                            <button class="btn btn-sm btn-primary" onclick="dispatchAddOperation()">添加到分摊</button>
                                            <span class="text-danger ml-2" id="balanceWarning"></span>
                                        </div>
                                    </div>
                                    <table class="table table-sm table-bordered mt-2" id="supplementList">
                                        <thead><tr><th>操作类型</th><th>商品名称</th><th>数量</th><th>金额</th><th>来源订单</th><th>操作</th></tr></thead>
                                        <tbody></tbody>
                                    </table>
                                    <button class="btn btn-sm btn-success" onclick="saveAllSupplements()">保存增项</button>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </div>

        <script>
            let currentPurchaserId = null;
            let currentPurchaserName = '';
            let consumableOrders = [];
            let targetOrders = [];
            let selectedConsumableOrder = null;
            let selectedTargetOrder = null;
            let consumableOrderDetails = [];
            let targetOrderDetails = [];
            let pendingSupplements = [];
            let allocationSummary = null;
            let pendingReplaceLines = [];
            let selectedReplaceProduct = null;

            function initPurchaserSearch() {
                const input = document.getElementById('purchaserInput');
                input.addEventListener('click', function() {
                    this.readOnly = true;
                    showPurchaserDropdown('');
                });
                input.addEventListener('dblclick', function() {
                    this.readOnly = false;
                    this.value = '';
                    this.focus();
                });
                input.addEventListener('input', function() {
                    showPurchaserDropdown(this.value.trim());
                });
                input.addEventListener('blur', function() {
                    setTimeout(() => {
                        document.getElementById('purchaserDropdown').style.display = 'none';
                    }, 200);
                });
            }

            function initConsumableOrderSearch() {
                const input = document.getElementById('consumableOrderInput');
                input.addEventListener('click', function() {
                    this.readOnly = true;
                    showConsumableOrderDropdown('');
                });
                input.addEventListener('dblclick', function() {
                    this.readOnly = false;
                    this.value = '';
                    this.focus();
                });
                input.addEventListener('input', function() {
                    showConsumableOrderDropdown(this.value.trim());
                });
                input.addEventListener('blur', function() {
                    setTimeout(() => {
                        document.getElementById('consumableOrderDropdown').style.display = 'none';
                    }, 200);
                });
            }

            function initTargetOrderSearch() {
                const input = document.getElementById('targetOrderInput');
                input.addEventListener('click', function() {
                    this.readOnly = true;
                    showTargetOrderDropdown('');
                });
                input.addEventListener('dblclick', function() {
                    this.readOnly = false;
                    this.value = '';
                    this.focus();
                });
                input.addEventListener('input', function() {
                    showTargetOrderDropdown(this.value.trim());
                });
                input.addEventListener('blur', function() {
                    setTimeout(() => {
                        document.getElementById('targetOrderDropdown').style.display = 'none';
                    }, 200);
                });
            }

            async function showPurchaserDropdown(keyword) {
                const res = await fetch('/api/purchaser/list' + (keyword ? '?keyword=' + encodeURIComponent(keyword) : ''));
                const data = await res.json();
                const dropdown = document.getElementById('purchaserDropdown');
                dropdown.innerHTML = '';
                data.forEach(p => {
                    const li = document.createElement('li');
                    li.className = 'search-item';
                    li.textContent = p.name;
                    li.onclick = () => selectPurchaser(p.id, p.name);
                    dropdown.appendChild(li);
                });
                dropdown.style.display = data.length > 0 ? 'block' : 'none';
            }

            function showConsumableOrderDropdown(keyword) {
                const dropdown = document.getElementById('consumableOrderDropdown');
                dropdown.innerHTML = '';
                const filtered = consumableOrders.filter(o => 
                    o.order_no.toLowerCase().includes(keyword.toLowerCase())
                );
                filtered.forEach(order => {
                    const li = document.createElement('li');
                    li.className = 'search-item';
                    li.textContent = `${order.order_no} - ${order.order_date}`;
                    li.onclick = () => selectConsumableOrder(order);
                    dropdown.appendChild(li);
                });
                dropdown.style.display = filtered.length > 0 ? 'block' : 'none';
            }

            function showTargetOrderDropdown(keyword) {
                const dropdown = document.getElementById('targetOrderDropdown');
                dropdown.innerHTML = '';
                const filtered = targetOrders.filter(o => 
                    o.order_no.toLowerCase().includes(keyword.toLowerCase())
                );
                filtered.forEach(order => {
                    const li = document.createElement('li');
                    li.className = 'search-item';
                    li.textContent = `${order.order_no} - ${order.order_date}`;
                    li.onclick = () => selectTargetOrder(order);
                    dropdown.appendChild(li);
                });
                dropdown.style.display = filtered.length > 0 ? 'block' : 'none';
            }

            function selectPurchaser(id, name) {
                currentPurchaserId = id;
                currentPurchaserName = name;
                const input = document.getElementById('purchaserInput');
                input.value = name;
                input.readOnly = false;
                document.getElementById('purchaserDropdown').style.display = 'none';
                loadOrdersByPurchaser();
                resetOrderSelection();
            }

            async function loadOrdersByPurchaser() {
                if (!currentPurchaserId) return;
                console.log('Loading orders for purchaser_id:', currentPurchaserId);
                try {
                    const res = await fetch('/api/sales_order/by_purchaser/' + currentPurchaserId);
                    if (!res.ok) {
                        console.error('API error:', res.status, res.statusText);
                        alert('加载订单失败: ' + res.statusText);
                        return;
                    }
                    const data = await res.json();
                    console.log('API response:', data);
                    consumableOrders = data;
                    targetOrders = data;
                    console.log('源订单:', consumableOrders.length, '目标订单:', targetOrders.length);
                    renderOrderLists();
                } catch (e) {
                    console.error('Fetch error:', e);
                    alert('加载订单失败: ' + e.message);
                }
            }

            function renderOrderLists() {
                const consumableTbody = document.querySelector('#consumableOrderList tbody');
                consumableTbody.innerHTML = '';
                const allocationStatusMap = { '-1': '未初始化', '0': '未分摊', '1': '分摊中', '2': '已完成', '3': '已终止' };
                consumableOrders.forEach(order => {
                    const tr = document.createElement('tr');
                    tr.style.cursor = 'pointer';
                    const allocationStatus = order.allocation_status !== undefined ? order.allocation_status : -1;
                    const statusText = allocationStatusMap[allocationStatus] || '未知';
                    tr.innerHTML = `<td>${order.order_no}</td><td>${order.order_date}</td><td>${order.total_amount.toFixed(2)}</td><td>${statusText}</td>`;
                    tr.onclick = () => selectConsumableOrder(order);
                    consumableTbody.appendChild(tr);
                });

                const targetTbody = document.querySelector('#targetOrderList tbody');
                targetTbody.innerHTML = '';
                targetOrders.forEach(order => {
                    const tr = document.createElement('tr');
                    tr.style.cursor = 'pointer';
                    const statusText = ['accepted', 'settled'].includes(order.status) ? '已完成' : '未完成';
                    tr.innerHTML = `<td>${order.order_no}</td><td>${order.order_date}</td><td>${order.total_amount.toFixed(2)}</td><td>${statusText}</td>`;
                    tr.onclick = () => selectTargetOrder(order);
                    targetTbody.appendChild(tr);
                });
            }

            async function selectConsumableOrder(order) {
                selectedConsumableOrder = order;
                document.getElementById('consumableOrderInput').value = order.order_no;
                const res = await fetch('/api/sales_order/detail/' + order.id);
                const data = await res.json();
                consumableOrderDetails = data.items || [];
                await loadAllocationSummary(order.id);
                renderConsumableOrderDetail();
                await loadAllocationOrders(order.id);
            }

            function renderConsumableOrderDetail() {
                const detailDiv = document.getElementById('consumableOrderDetail');
                const hasScheme = allocationSummary && allocationSummary.exists;
                const schemeItemIds = (hasScheme && allocationSummary.source_item_ids) ? allocationSummary.source_item_ids : [];
                let html = `<h6>${selectedConsumableOrder.order_no} 明细</h6>`;
                html += `<table class="table table-sm table-bordered"><thead><tr><th style="width:36px;">选</th><th>商品</th><th>规格</th><th>单位</th><th>数量</th><th>金额</th><th>类别</th></tr></thead><tbody>`;
                consumableOrderDetails.forEach(item => {
                    // 已有方案：仅勾选并高亮已纳入的明细行，复选框禁用；无方案：默认全选可编辑
                    const inScheme = hasScheme ? schemeItemIds.includes(item.id) : true;
                    const disabled = hasScheme ? 'disabled' : '';
                    const rowStyle = (hasScheme && inScheme) ? ' style="background-color:#e8f4ff;"' : '';
                    const checked = inScheme ? 'checked' : '';
                    html += `<tr${rowStyle}><td class="text-center"><input type="checkbox" class="src-item-check" data-id="${item.id}" data-amount="${item.amount || 0}" ${checked} ${disabled} onchange="updateSelectedSourceTotal()"></td><td>${item.product_name || ''}</td><td>${item.spec || '-'}</td><td>${item.unit || ''}</td><td>${(item.quantity || 0).toFixed(2)}</td><td>${(item.amount || 0).toFixed(2)}</td><td>${item.category_name || ''}</td></tr>`;
                });
                html += '</tbody></table>';
                if (!hasScheme) {
                    html += '<div class="mb-2"><span class="text-muted small">已选明细合计: <span id="selectedSourceTotal">0.00</span> 元（全选=整单分摊）</span></div>';
                }
                html += '<div id="allocationOrdersSection"></div>';
                detailDiv.innerHTML = html;
                if (!hasScheme) updateSelectedSourceTotal();
            }

            function updateSelectedSourceTotal() {
                const el = document.getElementById('selectedSourceTotal');
                if (!el) return;
                let total = 0;
                document.querySelectorAll('.src-item-check:checked').forEach(cb => {
                    total += parseFloat(cb.getAttribute('data-amount')) || 0;
                });
                el.textContent = total.toFixed(2);
            }

            async function loadAllocationSummary(orderId) {
                const res = await fetch('/api/allocation/summary/' + orderId);
                const data = await res.json();
                allocationSummary = data;
                updateAllocationUI();
            }

            async function loadAllocationOrders(orderId) {
                const res = await fetch('/api/supplement/list_by_source/' + orderId);
                const supplements = await res.json();
                
                const section = document.getElementById('allocationOrdersSection');
                if (!section) return;
                
                if (!supplements || supplements.length === 0) {
                    section.innerHTML = '<p class="text-muted small">暂无分摊订单记录</p>';
                    return;
                }

                let html = '<hr><h6>关联分摊订单（<span class="text-danger small">可回滚</span>）</h6>';
                html += '<div style="max-height: 200px; overflow-y: auto;">';
                html += '<table class="table table-sm table-bordered"><thead><tr><th>目标订单</th><th>商品</th><th>数量</th><th>金额</th><th>操作</th></tr></thead><tbody>';
                
                supplements.forEach(supp => {
                    const opMap = { 'increase_quantity': '+追加', 'new_item': '+新增', 'replace_remove': '替换-冲减', 'replace_add': '替换-换入' };
                    const opTypeText = opMap[supp.operation_type] || '+新增';
                    html += `<tr><td>${supp.target_order_no || '未知'}</td><td>${opTypeText} ${supp.product_name}</td><td>${(supp.quantity || 0).toFixed(2)}</td><td>${(supp.amount || 0).toFixed(2)}</td>`;
                    html += `<td><button class="btn btn-xs btn-danger" onclick="rollbackSupplement(${supp.id})">回滚</button></td></tr>`;
                });
                
                html += '</tbody></table></div>';
                section.innerHTML = html;
            }

            async function rollbackSupplement(supplementId) {
                if (!confirm('确定回滚该分摊项？')) return;
                try {
                    const res = await fetch('/api/supplement/delete/' + supplementId, { method: 'DELETE' });
                    if (res.ok) {
                        alert('回滚成功');
                        if (selectedConsumableOrder) {
                            await loadAllocationSummary(selectedConsumableOrder.id);
                            renderConsumableOrderDetail();
                            await loadAllocationOrders(selectedConsumableOrder.id);
                            await loadOrdersByPurchaser();
                            await loadAllocatedOrders();
                        }
                    } else {
                        const text = await res.text();
                        alert('回滚失败: ' + text);
                    }
                } catch (e) {
                    alert('回滚失败: ' + e.message);
                }
            }

            async function cancelAllocation() {
                if (!selectedConsumableOrder) return;
                if (!allocationSummary || !allocationSummary.exists) {
                    alert('当前订单未初始化分摊方案');
                    return;
                }
                if ((allocationSummary.allocated_amount || 0) > 0.0001) {
                    alert('已存在分摊记录，请先回滚全部分摊后再取消');
                    return;
                }
                if (!confirm('确定取消该分摊方案？取消后订单将回到未分摊状态，可重新选择明细。')) return;
                const res = await fetch('/api/allocation/cancel', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ source_order_id: selectedConsumableOrder.id })
                });
                if (res.ok) {
                    await loadAllocationSummary(selectedConsumableOrder.id);
                    renderConsumableOrderDetail();
                    await loadAllocationOrders(selectedConsumableOrder.id);
                    await loadOrdersByPurchaser();
                    await loadAllocatedOrders();
                    alert('取消成功，已回到未分摊状态');
                } else {
                    const text = await res.text();
                    alert('取消失败: ' + text);
                }
            }

            async function loadAllocatedOrders() {
                const res = await fetch('/api/allocation/allocated_orders');
                const list = await res.json();
                window._allocatedOrdersMap = {};
                list.forEach(o => { window._allocatedOrdersMap[o.source_order_id] = o; });
                const tbody = document.querySelector('#allocatedOrdersTable tbody');
                document.getElementById('allocatedOrdersCount').textContent = list.length;
                tbody.innerHTML = '';
                if (!list.length) {
                    tbody.innerHTML = '<tr><td colspan="10" class="text-center text-muted small">暂无</td></tr>';
                    return;
                }
                const statusMap = { 0: ['未分摊', 'secondary'], 1: ['分摊中', 'info'], 2: ['已完成', 'success'], 3: ['已终止', 'warning'] };
                list.forEach(o => {
                    const tr = document.createElement('tr');
                    const st = statusMap[o.status] || ['未知', 'light'];
                    tr.innerHTML =
                        '<td><a href="javascript:void(0)" onclick="selectAllocatedOrder(' + o.source_order_id + ')">' + (o.order_no || '') + '</a></td>' +
                        '<td>' + (o.purchaser_name || '') + '</td>' +
                        '<td>' + (o.order_date || '') + '</td>' +
                        '<td class="text-right">' + (o.total_amount || 0).toFixed(2) + '</td>' +
                        '<td class="text-right text-success">' + (o.allocated_amount || 0).toFixed(2) + '</td>' +
                        '<td class="text-right text-danger">' + (o.remaining_balance || 0).toFixed(2) + '</td>' +
                        '<td><span class="badge badge-' + st[1] + '">' + st[0] + '</span></td>' +
                        '<td>' + (o.created_at || '') + '</td>' +
                        '<td>' + (o.completed_at || '') + '</td>' +
                        '<td><button class="btn btn-xs btn-outline-primary" onclick="selectAllocatedOrder(' + o.source_order_id + ')">查看</button></td>';
                    tbody.appendChild(tr);
                });
            }

            async function selectAllocatedOrder(sourceOrderId) {
                const o = window._allocatedOrdersMap && window._allocatedOrdersMap[sourceOrderId];
                if (!o) return;
                // 自动定位到该订单对应的采购单位并加载其订单列表
                if (o.purchaser_id && currentPurchaserId !== o.purchaser_id) {
                    currentPurchaserId = o.purchaser_id;
                    currentPurchaserName = o.purchaser_name || '';
                    document.getElementById('purchaserInput').value = currentPurchaserName;
                    await loadOrdersByPurchaser();
                }
                const order = consumableOrders.find(x => x.id === sourceOrderId) || {
                    id: sourceOrderId, order_no: o.order_no, order_date: o.order_date, total_amount: o.total_amount
                };
                await selectConsumableOrder(order);
            }

            function updateAllocationUI() {
                const summaryDiv = document.getElementById('allocationSummary');
                const initActions = document.getElementById('allocationInitActions');
                const manageActions = document.getElementById('allocationManageActions');
                const badge = document.getElementById('allocationStatusBadge');

                if (!allocationSummary || !allocationSummary.exists) {
                    summaryDiv.style.display = 'none';
                    initActions.style.display = 'block';
                    manageActions.style.display = 'none';
                    badge.style.display = 'none';
                    document.getElementById('balanceWarning').textContent = '';
                    return;
                }

                summaryDiv.style.display = 'block';
                initActions.style.display = 'none';
                
                document.getElementById('summaryTotal').textContent = allocationSummary.total_amount.toFixed(2);
                document.getElementById('summaryAllocated').textContent = allocationSummary.allocated_amount.toFixed(2);
                document.getElementById('summaryRemaining').textContent = allocationSummary.remaining_balance.toFixed(2);
                
                const statusMap = { 0: '未分摊', 1: '分摊中', 2: '已完成', 3: '已终止' };
                const statusText = statusMap[allocationSummary.status] || '未知';
                document.getElementById('summaryStatus').textContent = statusText;
                
                badge.textContent = statusText;
                badge.style.display = 'inline-block';
                
                if (allocationSummary.status === 2) {
                    badge.className = 'badge badge-success';
                    manageActions.style.display = 'none';
                } else if (allocationSummary.status === 3) {
                    badge.className = 'badge badge-warning';
                    manageActions.style.display = 'none';
                } else {
                    badge.className = allocationSummary.status === 1 ? 'badge badge-info' : 'badge badge-light';
                    manageActions.style.display = 'block';
                }
            }

            async function createAllocation() {
                if (!selectedConsumableOrder) return;
                const ids = [];
                document.querySelectorAll('.src-item-check:checked').forEach(cb => {
                    ids.push(parseInt(cb.getAttribute('data-id')));
                });
                if (ids.length === 0) {
                    alert('请至少勾选一条要分摊的明细');
                    return;
                }
                const remark = prompt('请输入分摊方案备注（选填）');
                const res = await fetch('/api/allocation/create', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        source_order_id: selectedConsumableOrder.id,
                        source_item_ids: ids,
                        remark: remark || ''
                    })
                });
                if (res.ok) {
                    await loadAllocationSummary(selectedConsumableOrder.id);
                    renderConsumableOrderDetail();
                    await loadAllocationOrders(selectedConsumableOrder.id);
                    await loadOrdersByPurchaser();
                    await loadAllocatedOrders();
                    alert('分摊方案创建成功');
                } else {
                    const text = await res.text();
                    alert('创建失败: ' + text);
                }
            }

            async function terminateAllocation() {
                if (!selectedConsumableOrder) return;
                const remark = prompt('请输入终止原因');
                if (!remark) return;
                const res = await fetch('/api/allocation/terminate', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        source_order_id: selectedConsumableOrder.id,
                        remark: remark
                    })
                });
                if (res.ok) {
                    await loadAllocationSummary(selectedConsumableOrder.id);
                    await loadOrdersByPurchaser();
                    await loadAllocatedOrders();
                    alert('终止成功');
                } else {
                    const text = await res.text();
                    alert(text);
                }
            }

            async function confirmCompleteAllocation() {
                if (!selectedConsumableOrder || !allocationSummary || !allocationSummary.exists) {
                    alert('请先初始化分摊方案');
                    return;
                }
                if (allocationSummary.status === 2) {
                    alert('分摊方案已完成');
                    return;
                }
                if (allocationSummary.status === 3) {
                    alert('分摊方案已终止');
                    return;
                }

                const remaining = allocationSummary.remaining_balance;
                const threshold = 5.0;

                let targetId = null;
                let autoTail = false;

                if (remaining > threshold) {
                    alert(`剩余 ${remaining.toFixed(2)} 元未分摊，超过尾差限额（±${threshold.toFixed(2)} 元），请继续分摊完成后确认。`);
                    return;
                }
                if (remaining < -threshold) {
                    alert(`已超额分摊 ${Math.abs(remaining).toFixed(2)} 元，超过尾差限额（±${threshold.toFixed(2)} 元），请回滚部分分摊后再确认。`);
                    return;
                }

                if (Math.abs(remaining) > 0.01) {
                    const msg = `尾差 ${remaining.toFixed(2)} 元（限额 ±${threshold.toFixed(2)} 元），
系统将自动在已选目标订单中创建一笔"分摊尾差"冲销项。是否继续？`;
                    if (!confirm(msg)) return;
                    autoTail = true;
                    if (selectedTargetOrder) {
                        targetId = selectedTargetOrder.id;
                    } else if (targetOrders.length > 0) {
                        targetId = targetOrders[0].id;
                    } else {
                        alert('没有可用的目标订单来创建尾差冲销项');
                        return;
                    }
                }

                try {
                    const res = await fetch('/api/allocation/complete', {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({
                            source_order_id: selectedConsumableOrder.id,
                            target_order_id: targetId,
                            auto_tail: autoTail,
                        })
                    });
                    const text = await res.text();
                    if (res.ok) {
                        const data = JSON.parse(text);
                        const msg = data.auto_tail ? '分摊完成，已自动创建尾差冲销项。' : '分摊完成！';
                        alert(msg);
                        await loadAllocationSummary(selectedConsumableOrder.id);
                        await loadOrdersByPurchaser();
                        await loadAllocatedOrders();
                    } else {
                        alert(text);
                    }
                } catch (e) {
                    alert('操作失败: ' + e.message);
                }
            }

            async function selectTargetOrder(order) {
                selectedTargetOrder = order;
                document.getElementById('targetOrderInput').value = order.order_no;
                const res = await fetch('/api/sales_order/detail/' + order.id);
                const data = await res.json();
                targetOrderDetails = data.items || [];
                renderTargetOrderDetail();
                document.getElementById('supplementActionArea').style.display = 'block';
                document.getElementById('compareArea').style.display = 'block';
                await loadCompareData(order.id);
                initIncreaseProductSelect();
                initSupplementProductSearch();
            }

            function renderTargetOrderDetail() {
                const detailDiv = document.getElementById('targetOrderDetail');
                let html = `<h6>${selectedTargetOrder.order_no} 明细</h6>`;
                html += `<table class="table table-sm table-bordered"><thead><tr><th>商品</th><th>规格</th><th>单位</th><th>数量</th><th>金额</th></tr></thead><tbody>`;
                targetOrderDetails.forEach((item, index) => {
                    html += `<tr data-index="${index}"><td>${item.product_name || ''}</td><td>${item.spec || '-'}</td><td>${item.unit || ''}</td><td>${(item.quantity || 0).toFixed(2)}</td><td>${(item.amount || 0).toFixed(2)}</td>`;
                });
                html += '</tbody></table>';
                detailDiv.innerHTML = html;
            }

            let compareData = null;
            let selectedSupplementProduct = null;

            async function loadCompareData(orderId) {
                const res = await fetch('/api/supplement/compare/' + orderId);
                compareData = await res.json();
                renderCompareTables();
                loadTargetSupplements(orderId);
            }

            function renderCompareTables() {
                if (!compareData) return;
                document.getElementById('realTotalLabel').textContent = compareData.real_total.toFixed(2);
                document.getElementById('allocTotalLabel').textContent = compareData.allocation_total.toFixed(2);
                document.getElementById('diffTotalLabel').textContent = (compareData.allocation_total - compareData.real_total).toFixed(2);

                const realTbody = document.querySelector('#realTable tbody');
                const allocTbody = document.querySelector('#allocTable tbody');
                realTbody.innerHTML = '';
                allocTbody.innerHTML = '';

                compareData.items.forEach(item => {
                    const realRow = document.createElement('tr');
                    const allocRow = document.createElement('tr');
                    const displayName = item.display_name || item.product_name;

                    if (item.is_new) {
                        realRow.innerHTML = `<td colspan="3" class="text-center text-muted small">—</td>`;
                        allocRow.style.backgroundColor = '#fff3cd';
                        allocRow.style.color = '#856404';
                        allocRow.innerHTML = `<td><strong>[增项]</strong> ${displayName}</td><td>${item.total_quantity.toFixed(2)}</td><td>${item.total_amount.toFixed(2)}</td>`;
                    } else if (item.is_replaced) {
                        realRow.innerHTML = `<td>${displayName}</td><td>${item.quantity.toFixed(2)}</td><td>${item.amount.toFixed(2)}</td>`;
                        allocRow.style.backgroundColor = '#f8d7da';
                        allocRow.style.color = '#721c24';
                        allocRow.innerHTML = `<td><del>${displayName}</del> <span class="badge badge-danger">已替换</span></td><td>${item.total_quantity.toFixed(2)}</td><td>${item.total_amount.toFixed(2)}</td>`;
                    } else if (item.is_increase) {
                        realRow.innerHTML = `<td>${displayName}</td><td>${item.quantity.toFixed(2)}</td><td>${item.amount.toFixed(2)}</td>`;
                        allocRow.style.backgroundColor = '#d4edda';
                        allocRow.style.color = '#155724';
                        allocRow.innerHTML = `<td>${displayName} <span class="badge badge-success">+${item.supplement_quantity.toFixed(2)}</span></td><td>${item.total_quantity.toFixed(2)}</td><td>${item.total_amount.toFixed(2)}</td>`;
                    } else {
                        realRow.innerHTML = `<td>${displayName}</td><td>${item.quantity.toFixed(2)}</td><td>${item.amount.toFixed(2)}</td>`;
                        allocRow.innerHTML = `<td>${displayName}</td><td>${item.total_quantity.toFixed(2)}</td><td>${item.total_amount.toFixed(2)}</td>`;
                    }
                    realTbody.appendChild(realRow);
                    allocTbody.appendChild(allocRow);
                });
            }

            function toggleOperationType() {
                const opType = document.querySelector('input[name="operationType"]:checked').value;
                document.getElementById('newItemSection').style.display = opType === 'new_item' ? 'flex' : 'none';
                document.getElementById('increaseQtySection').style.display = opType === 'increase_quantity' ? 'flex' : 'none';
                document.getElementById('replaceSection').style.display = opType === 'replace_item' ? 'block' : 'none';
                if (opType === 'replace_item') {
                    initReplaceSourceSelect();
                    initReplaceProductSearch();
                }
            }

            function initReplaceSourceSelect() {
                const select = document.getElementById('replaceSourceSelect');
                select.innerHTML = '';
                targetOrderDetails.forEach((item, index) => {
                    const opt = document.createElement('option');
                    opt.value = index;
                    opt.textContent = `${item.product_name} (数量${item.quantity.toFixed(2)} × ${item.unit_price.toFixed(2)} = ${item.amount.toFixed(2)}元)`;
                    select.appendChild(opt);
                });
                onReplaceSourceChange();
            }

            function onReplaceSourceChange() {
                const idx = parseInt(document.getElementById('replaceSourceSelect').value);
                if (!isNaN(idx) && targetOrderDetails[idx]) {
                    document.getElementById('replaceSourceHint').textContent = `原明细金额: ${targetOrderDetails[idx].amount.toFixed(2)} 元`;
                }
                updateReplaceDiffHint();
            }

            function initReplaceProductSearch() {
                const input = document.getElementById('replaceProductInput');
                const dropdown = document.getElementById('replaceProductDropdown');
                if (!input._init) {
                    input.addEventListener('click', function() { showReplaceProductDropdown(''); });
                    input.addEventListener('dblclick', function() { this.readOnly = false; this.value = ''; this.focus(); });
                    input.addEventListener('input', function() { showReplaceProductDropdown(this.value.trim()); });
                    input.addEventListener('blur', function() { setTimeout(() => { dropdown.style.display = 'none'; }, 200); });
                    input._init = true;
                }
            }

            async function showReplaceProductDropdown(keyword) {
                const res = await fetch('/api/product/list?keyword=' + encodeURIComponent(keyword || '') + '&page_size=50');
                const data = await res.json();
                const products = data.items || data.data || [];
                const nonConsumable = products.filter(p => !(p.category_name || '').includes('耗材'));
                const dropdown = document.getElementById('replaceProductDropdown');
                dropdown.innerHTML = '';
                nonConsumable.forEach(p => {
                    const li = document.createElement('li');
                    li.className = 'search-item';
                    const alias2 = p.alias2 ? `(${p.alias2})` : '';
                    const price = p.selling_price || p.base_price || 0;
                    li.textContent = `${p.name}${alias2} - ${price.toFixed(2)}元/${p.unit || ''}`;
                    li.onclick = () => selectReplaceProduct(p);
                    dropdown.appendChild(li);
                });
                dropdown.style.display = nonConsumable.length > 0 ? 'block' : 'none';
            }

            function selectReplaceProduct(p) {
                selectedReplaceProduct = p;
                const input = document.getElementById('replaceProductInput');
                const alias2 = p.alias2 ? `(${p.alias2})` : '';
                input.value = `${p.name}${alias2}`;
                input.readOnly = false;
                document.getElementById('replaceProductDropdown').style.display = 'none';
                const price = p.selling_price || p.base_price || 0;
                document.getElementById('replacePriceInput').value = price.toFixed(2);
                calcReplaceAmount();
            }

            function calcReplaceAmount() {
                const qty = parseFloat(document.getElementById('replaceQtyInput').value) || 0;
                const price = parseFloat(document.getElementById('replacePriceInput').value) || 0;
                document.getElementById('replaceAmountInput').value = (qty * price).toFixed(2);
            }

            function addReplaceLine() {
                if (!selectedReplaceProduct) { alert('请选择替换的商品'); return; }
                const qty = parseFloat(document.getElementById('replaceQtyInput').value) || 0;
                const price = parseFloat(document.getElementById('replacePriceInput').value) || 0;
                const amount = qty * price;
                if (qty <= 0 || amount <= 0) { alert('请输入有效的数量和单价'); return; }
                pendingReplaceLines.push({
                    product_id: selectedReplaceProduct.id,
                    product_name: selectedReplaceProduct.name,
                    alias1: selectedReplaceProduct.alias1 || '',
                    alias2: selectedReplaceProduct.alias2 || '',
                    spec: selectedReplaceProduct.spec || '',
                    unit: selectedReplaceProduct.unit || '',
                    unit_price: price,
                    quantity: qty,
                    amount: amount,
                });
                selectedReplaceProduct = null;
                document.getElementById('replaceProductInput').value = '';
                document.getElementById('replaceQtyInput').value = '';
                document.getElementById('replacePriceInput').value = '';
                document.getElementById('replaceAmountInput').value = '';
                renderReplaceLines();
            }

            function removeReplaceLine(index) {
                pendingReplaceLines.splice(index, 1);
                renderReplaceLines();
            }

            function renderReplaceLines() {
                const tbody = document.querySelector('#replaceLineList tbody');
                tbody.innerHTML = '';
                let total = 0;
                pendingReplaceLines.forEach((line, index) => {
                    total += line.amount;
                    const alias2 = line.alias2 ? `(${line.alias2})` : '';
                    const tr = document.createElement('tr');
                    tr.innerHTML = `<td>${line.product_name}${alias2}</td><td>${line.quantity.toFixed(2)}</td><td>${line.unit_price.toFixed(2)}</td><td>${line.amount.toFixed(2)}</td>` +
                        `<td><button class="btn btn-xs btn-danger" onclick="removeReplaceLine(${index})">删除</button></td>`;
                    tbody.appendChild(tr);
                });
                document.getElementById('replaceLinesTotal').textContent = total.toFixed(2);
                updateReplaceDiffHint();
            }

            function updateReplaceDiffHint() {
                const idx = parseInt(document.getElementById('replaceSourceSelect').value);
                const hint = document.getElementById('replaceDiffHint');
                if (isNaN(idx) || !targetOrderDetails[idx]) { hint.textContent = ''; return; }
                const origAmount = targetOrderDetails[idx].amount;
                const replaceTotal = pendingReplaceLines.reduce((s, l) => s + l.amount, 0);
                const diff = replaceTotal - origAmount;
                if (Math.abs(diff) <= 5.0) {
                    hint.textContent = `差额 ${diff.toFixed(2)} 元（在±5元内，可提交）`;
                    hint.className = 'text-success small';
                } else {
                    hint.textContent = `差额 ${diff.toFixed(2)} 元（超过±5元限制）`;
                    hint.className = 'text-danger small';
                }
            }

            function addReplacement() {
                if (!selectedConsumableOrder || !selectedTargetOrder) { alert('请先选择耗材订单和目标订单'); return; }
                if (!allocationSummary || !allocationSummary.exists) { alert('请先初始化分摊方案'); return; }
                if (allocationSummary.status === 2) { alert('分摊已完成，不可继续分摊'); return; }
                if (allocationSummary.status === 3) { alert('分摊已终止'); return; }

                const idx = parseInt(document.getElementById('replaceSourceSelect').value);
                if (isNaN(idx) || !targetOrderDetails[idx]) { alert('请选择被替换的原明细'); return; }
                if (pendingReplaceLines.length === 0) { alert('请至少添加一条替换商品'); return; }

                const src = targetOrderDetails[idx];
                // 添加时不立即校验替换差额，允许组合多条后统一在"保存增项"时检查

                const allocDate = document.getElementById('allocateDateInput').value;
                const groupTag = 'RPL' + Date.now();
                const srcRemark = `${selectedConsumableOrder.order_no} 替换[${src.product_name}]`;

                // 冲减原明细：负数记录
                pendingSupplements.push({
                    id: null,
                    source_order_id: selectedConsumableOrder.id,
                    target_order_id: selectedTargetOrder.id,
                    source_remark: srcRemark + ' 冲减',
                    product_id: src.product_id,
                    product_name: src.product_name,
                    alias1: src.alias1 || '',
                    alias2: src.alias2 || '',
                    spec: src.spec || '',
                    unit: src.unit || '',
                    unit_price: src.unit_price,
                    quantity: -src.quantity,
                    amount: -src.amount,
                    allocate_date: allocDate,
                    operation_type: 'replace_remove',
                    target_order_item_id: src.id,
                });

                // 新增替换商品：正数记录
                pendingReplaceLines.forEach(line => {
                    pendingSupplements.push({
                        id: null,
                        source_order_id: selectedConsumableOrder.id,
                        target_order_id: selectedTargetOrder.id,
                        source_remark: srcRemark + ' 换入',
                        product_id: line.product_id,
                        product_name: line.product_name,
                        alias1: line.alias1,
                        alias2: line.alias2,
                        spec: line.spec,
                        unit: line.unit,
                        unit_price: line.unit_price,
                        quantity: line.quantity,
                        amount: line.amount,
                        allocate_date: allocDate,
                        operation_type: 'replace_add',
                        target_order_item_id: src.id,
                    });
                });

                pendingReplaceLines = [];
                renderReplaceLines();
                renderPendingSupplements();
                updateBalanceWarning();
                renderLocalComparePreview();
            }


            function initIncreaseProductSelect() {
                const select = document.getElementById('increaseProductSelect');
                select.innerHTML = '';
                targetOrderDetails.forEach((item, index) => {
                    const opt = document.createElement('option');
                    opt.value = index;
                    opt.textContent = `${item.product_name} (${item.quantity.toFixed(2)} ${item.unit || ''})`;
                    select.appendChild(opt);
                });
                select.onchange = function() {
                    const idx = parseInt(this.value);
                    const item = targetOrderDetails[idx];
                    document.getElementById('increasePriceInput').value = item.unit_price.toFixed(2);
                    calcIncreaseAmount();
                };
                if (targetOrderDetails.length > 0) {
                    select.selectedIndex = 0;
                    select.onchange();
                }
            }

            function calcIncreaseAmount() {
                const qty = parseFloat(document.getElementById('increaseQtyInput').value) || 0;
                const price = parseFloat(document.getElementById('increasePriceInput').value) || 0;
                const amount = qty * price;
                document.getElementById('increaseAmountInput').value = amount.toFixed(2);
                const idx = parseInt(document.getElementById('increaseProductSelect').value);
                if (!isNaN(idx) && targetOrderDetails[idx]) {
                    const origQty = targetOrderDetails[idx].quantity;
                    document.getElementById('increaseTotalHint').textContent = `合计数量: ${(origQty + qty).toFixed(2)}`;
                }
            }

            function initSupplementProductSearch() {
                const input = document.getElementById('suppProductInput');
                const dropdown = document.getElementById('suppProductDropdown');
                if (!input._init) {
                    input.addEventListener('click', function() {
                        showSupplementProductDropdown('');
                    });
                    input.addEventListener('dblclick', function() {
                        this.readOnly = false;
                        this.value = '';
                        this.focus();
                    });
                    input.addEventListener('input', function() {
                        showSupplementProductDropdown(this.value.trim());
                    });
                    input.addEventListener('blur', function() {
                        setTimeout(() => { dropdown.style.display = 'none'; }, 200);
                    });
                    input._init = true;
                }
            }

            async function showSupplementProductDropdown(keyword) {
                const res = await fetch('/api/product/list?keyword=' + encodeURIComponent(keyword || '') + '&page_size=50');
                const data = await res.json();
                const products = data.items || data.data || [];
                const nonConsumable = products.filter(p => {
                    const cat = p.category_name || '';
                    return !cat.includes('耗材');
                });
                const dropdown = document.getElementById('suppProductDropdown');
                dropdown.innerHTML = '';
                nonConsumable.forEach(p => {
                    const li = document.createElement('li');
                    li.className = 'search-item';
                    const alias2 = p.alias2 ? `(${p.alias2})` : '';
                    const price = p.selling_price || p.base_price || 0;
                    li.textContent = `${p.name}${alias2} - ${price.toFixed(2)}元/${p.unit || ''}`;
                    li.onclick = () => selectSupplementProduct(p);
                    dropdown.appendChild(li);
                });
                dropdown.style.display = nonConsumable.length > 0 ? 'block' : 'none';
            }

            function selectSupplementProduct(p) {
                selectedSupplementProduct = p;
                const input = document.getElementById('suppProductInput');
                const alias2 = p.alias2 ? `(${p.alias2})` : '';
                input.value = `${p.name}${alias2}`;
                input.readOnly = false;
                document.getElementById('suppProductDropdown').style.display = 'none';
                const price = p.selling_price || p.base_price || 0;
                document.getElementById('addPriceInput').value = price.toFixed(2);
                calcSupplementAmount();
            }

            function calcSupplementAmount() {
                const qty = parseFloat(document.getElementById('addQtyInput').value) || 0;
                const price = parseFloat(document.getElementById('addPriceInput').value) || 0;
                document.getElementById('addAmountInput').value = (qty * price).toFixed(2);
            }

            function dispatchAddOperation() {
                const opType = document.querySelector('input[name="operationType"]:checked').value;
                if (opType === 'replace_item') {
                    addReplacement();
                } else {
                    addSupplement();
                }
            }

            function addSupplement() {
                if (!selectedConsumableOrder || !selectedTargetOrder) {
                    alert('请先选择耗材订单和目标订单');
                    return;
                }
                if (!allocationSummary || !allocationSummary.exists) {
                    alert('请先初始化分摊方案');
                    return;
                }
                if (allocationSummary.status === 2) {
                    alert('分摊已完成，不可继续分摊');
                    return;
                }
                if (allocationSummary.status === 3) {
                    alert('分摊已终止');
                    return;
                }
                const opType = document.querySelector('input[name="operationType"]:checked').value;
                let qty, amount, productId, productName, alias1, alias2, spec, unit, unitPrice, targetItemId;

                if (opType === 'new_item') {
                    if (!selectedSupplementProduct) {
                        alert('请选择要增项的商品');
                        return;
                    }
                    qty = parseFloat(document.getElementById('addQtyInput').value) || 0;
                    unitPrice = parseFloat(document.getElementById('addPriceInput').value) || 0;
                    amount = qty * unitPrice;
                    if (qty <= 0 || amount <= 0) {
                        alert('请输入有效的增项数量和单价');
                        return;
                    }
                    productId = selectedSupplementProduct.id;
                    productName = selectedSupplementProduct.name;
                    alias1 = selectedSupplementProduct.alias1 || '';
                    alias2 = selectedSupplementProduct.alias2 || '';
                    spec = selectedSupplementProduct.spec || '';
                    unit = selectedSupplementProduct.unit || '';
                    targetItemId = null;
                } else {
                    const idx = parseInt(document.getElementById('increaseProductSelect').value);
                    if (isNaN(idx) || !targetOrderDetails[idx]) {
                        alert('请选择要追加的商品');
                        return;
                    }
                    qty = parseFloat(document.getElementById('increaseQtyInput').value) || 0;
                    const item = targetOrderDetails[idx];
                    unitPrice = item.unit_price;
                    amount = qty * unitPrice;
                    if (qty <= 0) {
                        alert('请输入有效的追加数量');
                        return;
                    }
                    productId = item.product_id;
                    productName = item.product_name;
                    alias1 = item.alias1 || '';
                    alias2 = item.alias2 || '';
                    spec = item.spec || '';
                    unit = item.unit || '';
                    targetItemId = item.id;
                }

                // 添加时不立即校验金额，允许组合多条增项；统一在"保存增项"时校验差额（上下 5 元）

                pendingSupplements.push({
                    id: null,
                    source_order_id: selectedConsumableOrder.id,
                    target_order_id: selectedTargetOrder.id,
                    source_remark: selectedConsumableOrder.order_no + ' 耗材分摊',
                    product_id: productId,
                    product_name: productName,
                    alias1: alias1,
                    alias2: alias2,
                    spec: spec,
                    unit: unit,
                    unit_price: unitPrice,
                    quantity: qty,
                    amount: amount,
                    allocate_date: document.getElementById('allocateDateInput').value,
                    operation_type: opType,
                    target_order_item_id: targetItemId,
                });

                document.getElementById('addQtyInput').value = '';
                document.getElementById('increaseQtyInput').value = '';
                renderPendingSupplements();
                updateBalanceWarning();
                renderLocalComparePreview();
            }

            function updateBalanceWarning() {
                const pendingSum = pendingSupplements.filter(s => !s.id).reduce((sum, s) => sum + s.amount, 0);
                const total = allocationSummary ? allocationSummary.total_amount : 0;
                const allocated = allocationSummary ? allocationSummary.allocated_amount : 0;
                const remainingBalance = allocationSummary ? allocationSummary.remaining_balance : 0;
                // 预计总分摊 = 已分摊(历史已保存) + 本次待保存净额(含正负)
                const projected = allocated + pendingSum;
                // 超额 = 预计总分摊 - 耗材总额（等价于 pendingSum - remaining_balance）
                const over = projected - total;
                const warn = document.getElementById('balanceWarning');
                if (Math.abs(over) > 0.005) {
                    warn.textContent = `耗材总额 ${total.toFixed(2)}｜已分摊 ${allocated.toFixed(2)}｜剩余 ${remainingBalance.toFixed(2)}｜本次 ${pendingSum.toFixed(2)}｜预计总分摊 ${projected.toFixed(2)}（${over > 0 ? '超额' : '结余'} ${Math.abs(over).toFixed(2)} 元，保存时校验）`;
                    warn.className = 'text-muted ml-2';
                } else {
                    warn.textContent = `耗材总额 ${total.toFixed(2)}｜已分摊 ${allocated.toFixed(2)}｜本次 ${pendingSum.toFixed(2)}｜预计总分摊 ${projected.toFixed(2)}（平衡）`;
                    warn.className = 'text-success ml-2';
                }
            }

            function renderLocalComparePreview() {
                if (!compareData || !selectedTargetOrder) return;
                const pending = pendingSupplements.filter(s => !s.id);
                const preview = JSON.parse(JSON.stringify(compareData));
                const itemMap = {};
                preview.items.forEach(item => {
                    if (item.id > 0) itemMap[item.id] = item;
                });

                pending.forEach(s => {
                    if (s.operation_type === 'increase_quantity' && s.target_order_item_id && itemMap[s.target_order_item_id]) {
                        const it = itemMap[s.target_order_item_id];
                        it.supplement_quantity += s.quantity;
                        it.supplement_amount += s.amount;
                        it.total_quantity += s.quantity;
                        it.total_amount += s.amount;
                        it.is_increase = true;
                    } else if (s.operation_type === 'replace_remove' && s.target_order_item_id && itemMap[s.target_order_item_id]) {
                        const it = itemMap[s.target_order_item_id];
                        it.supplement_quantity += s.quantity;
                        it.supplement_amount += s.amount;
                        it.total_quantity += s.quantity;
                        it.total_amount += s.amount;
                        it.is_replaced = true;
                    } else if (s.operation_type === 'new_item' || s.operation_type === 'replace_add') {
                        preview.items.push({
                            id: -Math.random(),
                            product_id: s.product_id,
                            product_name: s.product_name,
                            display_name: s.product_name + (s.alias2 ? `(${s.alias2})` : ''),
                            quantity: 0,
                            amount: 0,
                            is_new: true,
                            is_increase: false,
                            supplement_quantity: s.quantity,
                            supplement_amount: s.amount,
                            total_quantity: s.quantity,
                            total_amount: s.amount,
                        });
                    }
                });

                preview.supplement_total = compareData.supplement_total + pending.reduce((sum, s) => sum + s.amount, 0);
                preview.allocation_total = compareData.allocation_total + pending.reduce((sum, s) => sum + s.amount, 0);

                const allocTbody = document.querySelector('#allocTable tbody');
                const realTbody = document.querySelector('#realTable tbody');
                realTbody.innerHTML = '';
                allocTbody.innerHTML = '';
                document.getElementById('allocTotalLabel').textContent = preview.allocation_total.toFixed(2);
                document.getElementById('diffTotalLabel').textContent = (preview.allocation_total - preview.real_total).toFixed(2);

                preview.items.forEach(item => {
                    const realRow = document.createElement('tr');
                    const allocRow = document.createElement('tr');
                    const displayName = item.display_name || item.product_name;

                    if (item.is_new) {
                        realRow.innerHTML = `<td colspan="3" class="text-center text-muted small">—</td>`;
                        allocRow.style.backgroundColor = '#fff3cd';
                        allocRow.style.color = '#856404';
                        allocRow.innerHTML = `<td><strong>[增项]</strong> ${displayName}</td><td>${item.total_quantity.toFixed(2)}</td><td>${item.total_amount.toFixed(2)}</td>`;
                    } else if (item.is_replaced) {
                        realRow.innerHTML = `<td>${displayName}</td><td>${item.quantity.toFixed(2)}</td><td>${item.amount.toFixed(2)}</td>`;
                        allocRow.style.backgroundColor = '#f8d7da';
                        allocRow.style.color = '#721c24';
                        allocRow.innerHTML = `<td><del>${displayName}</del> <span class="badge badge-danger">已替换</span></td><td>${item.total_quantity.toFixed(2)}</td><td>${item.total_amount.toFixed(2)}</td>`;
                    } else if (item.is_increase) {
                        realRow.innerHTML = `<td>${displayName}</td><td>${item.quantity.toFixed(2)}</td><td>${item.amount.toFixed(2)}</td>`;
                        allocRow.style.backgroundColor = '#d4edda';
                        allocRow.style.color = '#155724';
                        allocRow.innerHTML = `<td>${displayName} <span class="badge badge-success">+${item.supplement_quantity.toFixed(2)}</span></td><td>${item.total_quantity.toFixed(2)}</td><td>${item.total_amount.toFixed(2)}</td>`;
                    } else {
                        realRow.innerHTML = `<td>${displayName}</td><td>${item.quantity.toFixed(2)}</td><td>${item.amount.toFixed(2)}</td>`;
                        allocRow.innerHTML = `<td>${displayName}</td><td>${item.total_quantity.toFixed(2)}</td><td>${item.total_amount.toFixed(2)}</td>`;
                    }
                    realTbody.appendChild(realRow);
                    allocTbody.appendChild(allocRow);
                });
            }

            function renderPendingSupplements() {
                const tbody = document.querySelector('#supplementList tbody');
                tbody.innerHTML = '';
                const typeMap = {
                    'new_item': '新增商品',
                    'increase_quantity': '追加数量',
                    'replace_remove': '替换-冲减',
                    'replace_add': '替换-换入',
                };
                pendingSupplements.forEach((item, index) => {
                    const typeText = typeMap[item.operation_type] || item.operation_type;
                    const savedBadge = item.id ? '<span class="badge badge-info">已保存</span>' : '';
                    const isNeg = item.amount < 0;
                    const rowStyle = item.operation_type === 'replace_remove' ? ' style="color:#a94442;"' : (item.operation_type === 'replace_add' ? ' style="color:#3c763d;"' : '');
                    tbody.innerHTML += `<tr${rowStyle}><td>${typeText} ${savedBadge}</td><td>${item.product_name}</td><td>${item.quantity.toFixed(2)}</td><td>${item.amount.toFixed(2)}</td><td>${item.source_order_no || (selectedConsumableOrder?.order_no || '')}</td><td>${!item.id ? '<button class="btn btn-xs btn-danger" onclick="removePendingSupplement(' + index + ')">删除</button>' : ''}</td></tr>`;
                });
            }

            function removePendingSupplement(index) {
                pendingSupplements.splice(index, 1);
                renderPendingSupplements();
                updateBalanceWarning();
                renderLocalComparePreview();
            }

            async function loadTargetSupplements(orderId) {
                const res = await fetch('/api/supplement/list_by_target/' + orderId);
                const data = await res.json();
                pendingSupplements = data;
                renderPendingSupplements();
                updateBalanceWarning();
            }

            async function saveAllSupplements() {
                const toSave = pendingSupplements.filter(s => !s.id);
                if (toSave.length === 0) {
                    alert('没有待保存的增项');
                    return;
                }
                // 保存前校验分摊总额（上下 5 元尾差）
                const pendingSum = toSave.reduce((sum, s) => sum + s.amount, 0);
                const total_amount = allocationSummary ? allocationSummary.total_amount : 0;
                const allocated_amount = allocationSummary ? allocationSummary.allocated_amount : 0;
                const remaining_balance = allocationSummary ? allocationSummary.remaining_balance : 0;
                const projected = allocated_amount + pendingSum;  // 预计总分摊
                const diff = projected - total_amount;            // 与耗材总额的差额
                if (Math.abs(diff) > 5.0) {
                    alert(`保存失败：预计总分摊金额超出耗材总额 ${diff.toFixed(2)} 元（超出±5元限制）。` +
                          `\n\n耗材总额: ${total_amount.toFixed(2)} 元` +
                          `\n已分摊: ${allocated_amount.toFixed(2)} 元` +
                          `\n剩余余额: ${remaining_balance.toFixed(2)} 元` +
                          `\n本次待保存: ${pendingSum.toFixed(2)} 元` +
                          `\n预计总分摊: ${projected.toFixed(2)} 元` +
                          `\n超额: ${diff.toFixed(2)} 元` +
                          `\n\n请调整增项后再保存。`);
                    return;
                }
                for (const item of toSave) {
                    await fetch('/api/supplement/create', {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify(item),
                    });
                }
                alert('增项保存成功');
                // 保存成功后立即清空 pending 列表，避免残留条目被重复计入或重复保存
                pendingSupplements = [];
                renderPendingSupplements();
                if (selectedConsumableOrder) {
                    await loadAllocationSummary(selectedConsumableOrder.id);
                    await loadAllocationOrders(selectedConsumableOrder.id);
                }
                if (selectedTargetOrder) {
                    await loadCompareData(selectedTargetOrder.id);
                }
                await loadOrdersByPurchaser();
                // 数据刷新后更新标签，显示保存后的整体分摊状态
                updateBalanceWarning();
            }

            function resetOrderSelection() {
                selectedConsumableOrder = null;
                selectedTargetOrder = null;
                allocationSummary = null;
                compareData = null;
                selectedSupplementProduct = null;
                document.getElementById('consumableOrderInput').value = '';
                document.getElementById('targetOrderInput').value = '';
                document.getElementById('consumableOrderDetail').innerHTML = '<p>请选择订单查看详情</p>';
                document.getElementById('targetOrderDetail').innerHTML = '<p>请选择订单查看详情</p>';
                document.getElementById('supplementActionArea').style.display = 'none';
                document.getElementById('compareArea').style.display = 'none';
                document.getElementById('allocationSummary').style.display = 'none';
                document.getElementById('allocationInitActions').style.display = 'none';
                document.getElementById('allocationManageActions').style.display = 'none';
                document.getElementById('allocationStatusBadge').style.display = 'none';
                pendingSupplements = [];
                renderPendingSupplements();
            }

            document.addEventListener('click', function(e) {
                if (!e.target.closest('#purchaserInput') && !e.target.closest('#purchaserDropdown')) {
                    document.getElementById('purchaserDropdown').style.display = 'none';
                }
            });

            initPurchaserSearch();
            initConsumableOrderSearch();
            initTargetOrderSearch();
            
            const allocateDateInput = document.getElementById('allocateDateInput');
            if (allocateDateInput) {
                allocateDateInput.value = new Date().toISOString().split('T')[0];
            }
            loadAllocatedOrders();
        </script>
    "#;
    Html(crate::layout_html("耗材分摊管理", "supplement", content))
}

