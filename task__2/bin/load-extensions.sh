#!/bin/sh
set -e

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<-EOSQL
    create extension if not exists "pg_trgm";
    create extension if not exists "plpgsql";
    create extension if not exists "btree_gin";
    create extension if not exists "btree_gist";
    select * FROM pg_extension;
EOSQL
