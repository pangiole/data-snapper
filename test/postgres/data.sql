SET
search_path TO snapper;

-- Clean up safely in reverse dependency order if re-running
TRUNCATE shipments, order_items, orders, products, employees, billing_profiles, users, promotions RESTART IDENTITY CASCADE;

-- 1. Independent Table (UUID primary keys & string codes)
INSERT INTO promotions (id, discount_code)
VALUES ('a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11', 'SUMMER2026'),
       ('b0eebc99-9c0b-4ef8-bb6d-6bb9bd380a22', 'WELCOME10');

-- 2. Independent Table (VARCHAR primary keys)
INSERT INTO products (sku, name)
VALUES ('LAPTOP-PRO-15', 'Pro Laptop 15-inch'),
       ('MOUSE-WIRELESS', 'Ergonomic Wireless Mouse'),
       ('KEYBOARD-MECH', 'Mechanical RGB Keyboard');

-- 3. Circular Dependency Step 1: Insert Users with NULL foreign keys to break the chicken-and-egg creation lock
INSERT INTO users (id, username, default_billing_id)
VALUES (1, 'alice_dev', NULL),
       (2, 'bob_tester', NULL),
       (3, 'charlie_tester', NULL);

-- 4. Insert Billing Profiles pointing back to Users
INSERT INTO billing_profiles (id, user_id, card_last_four)
VALUES (101, 1, '4242'),
       (102, 2, '1234'),
       (103, 2, '8901');

-- 5. Circular Dependency Step 2: Complete the cycle by updating Users.default_billing_id
UPDATE users
SET default_billing_id = 101
WHERE id = 1;
UPDATE users
SET default_billing_id = 102
WHERE id = 2;

-- 6. Self-Referential Cycle (Hierarchical manager/subordinate relationship)
INSERT INTO employees (id, name, manager_id)
VALUES (1, 'Big Boss CEO', NULL),           -- Root node (manager_id is NULL)
       (2, 'Alice Engineering Manager', 1), -- Subordinate to CEO
       (3, 'Charlie Code Monkey', 2);
-- Subordinate to Alice

-- 7. Orders with Nullable Foreign Keys (Order 1002 has a NULL promotion_id)
INSERT INTO orders (id, user_id, promotion_id, status)
VALUES (1001, 1, 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11', 'completed'),
       (1002, 1, NULL, 'pending'), -- Tests skipped/null foreign key logic
       (1003, 2, 'b0eebc99-9c0b-4ef8-bb6d-6bb9bd380a22', 'shipped');

-- 8. Composite Primary Key Table (order_id + product_sku)
INSERT INTO order_items (order_id, product_sku, quantity)
VALUES (1001, 'LAPTOP-PRO-15', 1),
       (1001, 'MOUSE-WIRELESS', 2),
       (1002, 'KEYBOARD-MECH', 1),
       (1003, 'LAPTOP-PRO-15', 1);

-- 9. Composite Foreign Key Table (referencing composite PKs from order_items)
INSERT INTO shipments (tracking_id, order_id, product_sku, shipped_at)
VALUES (5001, 1001, 'LAPTOP-PRO-15', '2026-06-01 10:00:00'),
       (5002, 1001, 'MOUSE-WIRELESS', '2026-06-01 10:00:00'),
       (5003, 1003, 'LAPTOP-PRO-15', '2026-06-02 14:30:00');