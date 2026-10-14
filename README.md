# Rust Todos

A small, single-user todo web app built in Rust. Todos are stored in SQLite and remain available after restarting the app.

## Features

- Create, edit, and delete todos
- Mark todos complete and reopen them
- Search titles and filter by all, active, or completed status
- Server-rendered HTML pages
- SQLite persistence

## Run locally

Install a current stable Rust toolchain, then run:

```sh
cargo run
```

Open [http://127.0.0.1:3000](http://127.0.0.1:3000). The app creates `todos.db` in the current directory on first run.

This is a local, single-user app without authentication or CSRF protection. Keep the default loopback bind address; do not expose it to a network without adding appropriate protections.

Optional environment variables:

- `DATABASE_URL`: SQLx SQLite URL; defaults to `sqlite://todos.db`
- `APP_ADDRESS`: bind address; defaults to `127.0.0.1:3000`
- `RUST_LOG`: tracing filter; defaults to `info`

## Tests

```sh
cargo test
```

The tests exercise CRUD, completion toggling, filtering/search, input validation, and persistence across database connections.
