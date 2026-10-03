CREATE SCHEMA IF NOT EXISTS snapper;
SET search_path TO snapper;

-- 1. Base Tables (UUIDs and Integer PKs)
CREATE TABLE promotions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    discount_code VARCHAR(50) NOT NULL
);

-- 2. Cross-Table Circular Dependency Setup
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    username VARCHAR(50) NOT NULL,
    default_billing_id INT -- Nullable initially to avoid chicken-and-egg creation
);

CREATE TABLE billing_profiles (
    id SERIAL PRIMARY KEY,
    user_id INT NOT NULL REFERENCES users(id),
    card_last_four VARCHAR(4)
);

-- Close the cycle: users -> billing_profiles -> users
ALTER TABLE users
ADD CONSTRAINT fk_user_billing
FOREIGN KEY (default_billing_id) REFERENCES billing_profiles(id);

-- 3. Self-Referential Table (Self-Cycle)
CREATE TABLE employees (
    id SERIAL PRIMARY KEY,
    name VARCHAR(50) NOT NULL,
    manager_id INT REFERENCES employees(id) -- Nullable for top-level bosses
);

-- 4. Standard 1:N & Nullable FKs
CREATE TABLE orders (
    id SERIAL PRIMARY KEY,
    user_id INT NOT NULL REFERENCES users(id),
    promotion_id UUID REFERENCES promotions(id), -- Nullable FK UUID testing
    status VARCHAR(20)
);

-- 5. Varchar Primary Key
CREATE TABLE products (
    sku VARCHAR(50) PRIMARY KEY,
    name VARCHAR(100) NOT NULL
);

-- 6. Composite PK + Multiple FKs
CREATE TABLE order_items (
    order_id INT REFERENCES orders(id),
    product_sku VARCHAR(50) REFERENCES products(sku),
    quantity INT NOT NULL,
    PRIMARY KEY (order_id, product_sku)
);

-- 7. Composite FK referencing a Composite PK
CREATE TABLE shipments (
    tracking_id SERIAL PRIMARY KEY,
    order_id INT NOT NULL,
    product_sku VARCHAR(50) NOT NULL,
    shipped_at TIMESTAMP DEFAULT now(),
    -- The critical test: A multi-column foreign key
    FOREIGN KEY (order_id, product_sku) REFERENCES order_items(order_id, product_sku)
);