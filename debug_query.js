const { DatabaseSync } = require('node:sqlite');
const db = new DatabaseSync('d:\\projects\\in_out\\target\\x86_64-win7-windows-msvc\\release\\food_accept_v3.db', { readOnly: true });
console.log('--- product_unit for 1,2,3,4 ---');
console.table(db.prepare("SELECT product_id, unit_name, ratio FROM product_unit WHERE product_id IN (1,2,3,4)").all());
console.log('--- product base_unit ---');
console.table(db.prepare("SELECT id, name, unit, base_unit FROM product WHERE id IN (1,2,3,4)").all());
db.close();
