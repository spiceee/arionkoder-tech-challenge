# arionkoder-tech-challenge

## 2. Resource Manager

Implement a Rust struct that manages:
- Connections to an external database.
- Automatic cleanup when going out of scope or upon error.

Constraints:
- Use Rust's built-in RAII (Resource Acquisition Is Initialization) pattern.
- Clearly log when resources are acquired and released.

## Assumptions

I've taken for granted that the struct would also manage the pool of connections instead of receiving it from upstream.

## Requirements

- Needs postgresql installed and running.
- Needs .env file with the proper values (which you can find in the Dockerfile-pg-dev)

```
docker compose up -d
```
