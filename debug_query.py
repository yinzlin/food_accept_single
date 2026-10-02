import sqlite3
con = sqlite3.connect(r'd:\projects\in_out\food_accept_v3.db')
cur = con.cursor()
print('--- PO20261002002 order ---')
for r in cur.execute("SELECT id, order_no, order_date, status FROM purchase_order WHERE order_no='PO20261002002'"):
    print(r)
print('--- items ---')
for r in cur.execute("SELECT id, product_id, product_name, quantity, unit, base_quantity FROM purchase_order_item WHERE order_id=(SELECT id FROM purchase_order WHERE order_no='PO20261002002')"):
    print(r)
print('--- banana product ---')
for r in cur.execute("SELECT id, name, unit, base_unit FROM product WHERE name LIKE '%香蕉%'"):
    print(r)
print('--- banana product_unit ---')
for r in cur.execute("SELECT product_id, unit_name, ratio FROM product_unit WHERE product_id IN (SELECT id FROM product WHERE name LIKE '%香蕉%')"):
    print(r)
print('--- stock_movement for this PO ---')
for r in cur.execute("SELECT id, direction, base_quantity, orig_quantity, orig_unit, ref_no, snapshot_version, created_at FROM stock_movement WHERE ref_no='PO20261002002'"):
    print(r)
print('--- inventory banana ---')
for r in cur.execute("SELECT product_id, quantity FROM inventory WHERE product_id IN (SELECT id FROM product WHERE name LIKE '%香蕉%')"):
    print(r)
con.close()
