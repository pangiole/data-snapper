# data-snapper
A tool able to extract a slice of data from a relational database.

## build

```sh
$ cargo build
```

## test

```sh
$ cargo test
```

### database
A Dockerized Postgres server, seeded with the `snapper` schema and test data, 
is defined in `docker-compose.yml`.

```
$ docker compose up -d        # start Postgres and seed it on first boot
$ docker compose down         # stop the server
$ docker compose down -v      # stop AND delete the data volume (re-seeds next start)
```

On first startup the scripts in `test/` are applied in order:

1. `test/postgres/system.sql`  — configure the Postgres system
2. `test/postgres/schema.sql`  — creates the `snapper` schema and tables.
3. `test/postgres/data.sql`    — truncates (safety) and inserts the seed rows.

The server listens on `localhost:5432` with the following credentials:

```
dbname   = snapper 
user     = snapper
password = snapper
```

> **Note:** the test tables live in the `snapper` schema (not `public`), so
> point the tool at e.g. `snapper.orders` rather than `public.orders`.
