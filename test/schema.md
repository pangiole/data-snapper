# Test schema
This relational database schema is specifically engineered to stress-test our 
graph traversal, cycle detection, and composite key handling.


| Table | Scenario Tested |
| --- | --- |
| `users` & `billing_profiles` | **Cross-table cycle.** If our deduplication logic fails, the `data-snapper` will infinitely loop extracting User 1 -> Billing 5 -> User 1. |
| `employees` | **Self-referential cycle.** Ensures the graph crawler doesn't crash when a table is its own parent. |
| `orders` | **Nullable FKs.** Your engine must know to ignore `NULL` values instead of pushing them into the queue. |
| `order_items` | **Composite PK.** Tests if your `HashSet` can store and deduplicate tuples `(Int, String)` instead of scalar IDs. |
| `shipments` | **Composite FK.** Tests if your `pg_catalog` SQL accurately maps multi-column foreign keys together as a single referential constraint. |


```mermaid
erDiagram
    PROMOTIONS {
        uuid id PK
        varchar discount_code
    }

    USERS {
        int id PK
        varchar username
        int default_billing_id FK
    }

    BILLING_PROFILES {
        int id PK
        int user_id FK
        varchar card_last_four
    }

    EMPLOYEES {
        int id PK
        varchar name
        int manager_id FK
    }

    ORDERS {
        int id PK
        int user_id FK
        uuid promotion_id FK
        varchar status
    }

    PRODUCTS {
        varchar sku PK
        varchar name
    }

    ORDER_ITEMS {
        int order_id PK,FK
        varchar product_sku PK,FK
        int quantity
    }

    SHIPMENTS {
        int tracking_id PK
        int order_id FK
        varchar product_sku FK
        timestamp shipped_at
    }

    BILLING_PROFILES }o--|| USERS : "belongs to"
    USERS |o--o| BILLING_PROFILES : "defaults to"
    EMPLOYEES }o--o| EMPLOYEES : "reports to"
    ORDERS }o--|| USERS : "belongs to"
    ORDERS }o--o| PROMOTIONS : "references"
    ORDER_ITEMS }o--|| ORDERS : "belongs to"
    ORDER_ITEMS }o--|| PRODUCTS : "references"
    SHIPMENTS }o--|| ORDER_ITEMS : "fulfills"
```