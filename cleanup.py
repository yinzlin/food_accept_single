import sqlite3
conn = sqlite3.connect('D:\\projects\\food_accept_single\\food_accept_v3.db')
c = conn.cursor()

queries = [
    'DELETE FROM sales_order_item WHERE unit_price IS NULL OR quantity IS NULL OR quantity = 0 OR amount = 0',
    'DELETE FROM purchase_order_item WHERE unit_price IS NULL OR quantity IS NULL OR quantity = 0 OR amount = 0',
    'DELETE FROM sales_order_item WHERE order_id NOT IN (SELECT id FROM sales_order)',
    'DELETE FROM purchase_order_item WHERE order_id NOT IN (SELECT id FROM purchase_order)',
    'DELETE FROM sales_order_item WHERE product_id NOT IN (SELECT id FROM product)',
    'DELETE FROM purchase_order_item WHERE product_id NOT IN (SELECT id FROM product)',
    'DELETE FROM sales_order WHERE id NOT IN (SELECT DISTINCT order_id FROM sales_order_item)',
    'DELETE FROM purchase_order WHERE id NOT IN (SELECT DISTINCT order_id FROM purchase_order_item)',
    'DELETE FROM sales_order WHERE purchaser_id NOT IN (SELECT id FROM purchaser)',
    'DELETE FROM purchase_order WHERE supplier_id NOT IN (SELECT id FROM supplier)',
    'DELETE FROM food_item WHERE accept_id NOT IN (SELECT id FROM food_accept)',
    'DELETE FROM food_accept WHERE supplier_id NOT IN (SELECT id FROM supplier) OR purchaser_id NOT IN (SELECT id FROM purchaser)',
    'DELETE FROM inventory WHERE product_id NOT IN (SELECT id FROM product) OR warehouse_id NOT IN (SELECT id FROM warehouse)',
    'DELETE FROM product_unit WHERE product_id NOT IN (SELECT id FROM product)',
    'DELETE FROM product_price WHERE product_id NOT IN (SELECT id FROM product)',
]
total = 0
for q in queries:
    c.execute(q)
    total += c.rowcount
    print(f'Deleted {c.rowcount} rows')

conn.commit()
c.execute('VACUUM')
conn.close()
print(f'\nTotal deleted: {total} rows')
