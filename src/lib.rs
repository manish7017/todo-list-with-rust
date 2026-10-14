use askama::Template;
use axum::{
    Router,
    extract::{Form, Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use serde::Deserialize;
use sqlx::{FromRow, sqlite::SqlitePool};
use tracing::error;

const MAX_TITLE_LENGTH: usize = 200;

pub fn app(pool: SqlitePool) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/todos", get(list_todos).post(create_todo))
        .route("/todos/new", get(new_todo))
        .route("/todos/{id}/edit", get(edit_todo))
        .route("/todos/{id}", axum::routing::post(update_todo))
        .route("/todos/{id}/toggle", axum::routing::post(toggle_todo))
        .route("/todos/{id}/delete", axum::routing::post(delete_todo))
        .route("/styles.css", get(stylesheet))
        .with_state(pool)
}

pub async fn initialize_database(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS todos (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            completed BOOLEAN NOT NULL DEFAULT FALSE
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, FromRow)]
struct Todo {
    id: i64,
    title: String,
    completed: bool,
}

#[derive(Template)]
#[template(path = "todo_list.html")]
struct TodoListTemplate {
    todos: Vec<Todo>,
    search: String,
    status: String,
}

#[derive(Template)]
#[template(path = "todo_form.html")]
struct TodoFormTemplate {
    title: String,
    action: String,
    heading: String,
    error: Option<String>,
}

#[derive(Deserialize)]
struct ListQuery {
    q: Option<String>,
    status: Option<String>,
}

#[derive(Deserialize)]
struct TodoForm {
    title: String,
}

#[derive(Debug)]
enum AppError {
    Database(sqlx::Error),
    Template(askama::Error),
}

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl From<askama::Error> for AppError {
    fn from(error: askama::Error) -> Self {
        Self::Template(error)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            Self::Database(cause) => error!(%cause, "database request failed"),
            Self::Template(cause) => error!(%cause, "template rendering failed"),
        }
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html("<h1>Something went wrong</h1><p>Please try again.</p>".to_owned()),
        )
            .into_response()
    }
}

async fn root() -> Redirect {
    Redirect::to("/todos")
}

async fn list_todos(
    State(pool): State<SqlitePool>,
    Query(query): Query<ListQuery>,
) -> Result<Response, AppError> {
    let search = query.q.unwrap_or_default().trim().to_owned();
    let status = query.status.unwrap_or_else(|| "all".to_owned());
    if !matches!(status.as_str(), "all" | "active" | "completed") {
        return Ok((StatusCode::BAD_REQUEST, "Invalid completion filter").into_response());
    }

    let todos = sqlx::query_as::<_, Todo>(
        "SELECT id, title, completed FROM todos
         WHERE (?1 = '' OR title LIKE ?2)
           AND (?3 = 'all'
                OR (?3 = 'active' AND completed = FALSE)
                OR (?3 = 'completed' AND completed = TRUE))
         ORDER BY completed ASC, id DESC",
    )
    .bind(&search)
    .bind(format!("%{search}%"))
    .bind(&status)
    .fetch_all(&pool)
    .await?;

    let html = TodoListTemplate {
        todos,
        search,
        status,
    }
    .render()?;
    Ok(Html(html).into_response())
}

async fn new_todo() -> Result<Response, AppError> {
    render_form(TodoFormTemplate {
        title: String::new(),
        action: "/todos".to_owned(),
        heading: "Add a todo".to_owned(),
        error: None,
    })
}

async fn create_todo(
    State(pool): State<SqlitePool>,
    Form(form): Form<TodoForm>,
) -> Result<Response, AppError> {
    let title = form.title.trim();
    if let Some(message) = validate_title(title) {
        return render_form(TodoFormTemplate {
            title: title.to_owned(),
            action: "/todos".to_owned(),
            heading: "Add a todo".to_owned(),
            error: Some(message),
        });
    }
    sqlx::query("INSERT INTO todos (title) VALUES (?)")
        .bind(title)
        .execute(&pool)
        .await?;
    Ok(Redirect::to("/todos").into_response())
}

async fn edit_todo(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let Some(todo) =
        sqlx::query_as::<_, Todo>("SELECT id, title, completed FROM todos WHERE id = ?")
            .bind(id)
            .fetch_optional(&pool)
            .await?
    else {
        return Ok(not_found());
    };

    render_form(TodoFormTemplate {
        title: todo.title,
        action: format!("/todos/{id}"),
        heading: "Edit a todo".to_owned(),
        error: None,
    })
}

async fn update_todo(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Form(form): Form<TodoForm>,
) -> Result<Response, AppError> {
    let title = form.title.trim();
    if let Some(message) = validate_title(title) {
        return render_form(TodoFormTemplate {
            title: title.to_owned(),
            action: format!("/todos/{id}"),
            heading: "Edit a todo".to_owned(),
            error: Some(message),
        });
    }
    let result = sqlx::query("UPDATE todos SET title = ? WHERE id = ?")
        .bind(title)
        .bind(id)
        .execute(&pool)
        .await?;
    if result.rows_affected() == 0 {
        return Ok(not_found());
    }
    Ok(Redirect::to("/todos").into_response())
}

async fn toggle_todo(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let result = sqlx::query("UPDATE todos SET completed = NOT completed WHERE id = ?")
        .bind(id)
        .execute(&pool)
        .await?;
    if result.rows_affected() == 0 {
        return Ok(not_found());
    }
    Ok(Redirect::to("/todos").into_response())
}

async fn delete_todo(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let result = sqlx::query("DELETE FROM todos WHERE id = ?")
        .bind(id)
        .execute(&pool)
        .await?;
    if result.rows_affected() == 0 {
        return Ok(not_found());
    }
    Ok(Redirect::to("/todos").into_response())
}

fn validate_title(title: &str) -> Option<String> {
    if title.is_empty() {
        Some("A todo title is required.".to_owned())
    } else if title.chars().count() > MAX_TITLE_LENGTH {
        Some(format!(
            "A todo title must be {MAX_TITLE_LENGTH} characters or fewer."
        ))
    } else {
        None
    }
}

fn render_form(template: TodoFormTemplate) -> Result<Response, AppError> {
    let html = template.render()?;
    let status = if template.error.is_some() {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::OK
    };
    Ok((status, Html(html)).into_response())
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "Todo not found").into_response()
}

async fn stylesheet() -> impl IntoResponse {
    (
        [("content-type", "text/css; charset=utf-8")],
        include_str!("../static/styles.css"),
    )
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
    };
    use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
    use tempfile::tempdir;
    use tower::ServiceExt;

    use super::{app, initialize_database};

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory SQLite should connect");
        initialize_database(&pool)
            .await
            .expect("schema should initialize");
        pool
    }

    async fn post_form(app: axum::Router, path: &str, body: &str) -> axum::response::Response {
        app.oneshot(
            Request::post(path)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body.to_owned()))
                .expect("request should build"),
        )
        .await
        .expect("router should respond")
    }

    #[tokio::test]
    async fn supports_todo_crud_toggle_filter_and_search() {
        let pool = test_pool().await;
        let response = post_form(app(pool.clone()), "/todos", "title=Write+tests").await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);

        let id: i64 = sqlx::query_scalar("SELECT id FROM todos WHERE title = 'Write tests'")
            .fetch_one(&pool)
            .await
            .expect("created todo should be stored");

        let response = app(pool.clone())
            .oneshot(
                Request::get("/todos?q=Write&status=active")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        assert!(String::from_utf8_lossy(&body).contains("Write tests"));

        let response = post_form(app(pool.clone()), &format!("/todos/{id}/toggle"), "").await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let completed: bool = sqlx::query_scalar("SELECT completed FROM todos WHERE id = ?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("todo should still exist");
        assert!(completed);

        let response = app(pool.clone())
            .oneshot(
                Request::get("/todos?status=completed")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        assert!(String::from_utf8_lossy(&body).contains("Write tests"));

        let response = app(pool.clone())
            .oneshot(
                Request::get("/todos?status=active")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        assert!(!String::from_utf8_lossy(&body).contains("Write tests"));

        let response = post_form(
            app(pool.clone()),
            &format!("/todos/{id}"),
            "title=Updated+todo",
        )
        .await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let title: String = sqlx::query_scalar("SELECT title FROM todos WHERE id = ?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("updated todo should exist");
        assert_eq!(title, "Updated todo");

        let response = post_form(app(pool.clone()), &format!("/todos/{id}/delete"), "").await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM todos WHERE id = ?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("todo count should be queryable");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn rejects_blank_titles() {
        let pool = test_pool().await;
        let response = post_form(app(pool.clone()), "/todos", "title=+++").await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM todos")
            .fetch_one(&pool)
            .await
            .expect("todo count should be queryable");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn rejects_unknown_completion_filters() {
        let response = app(test_pool().await)
            .oneshot(
                Request::get("/todos?status=archived")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn sqlite_data_survives_reopening_the_database() {
        let directory = tempdir().expect("temporary directory should be created");
        let database_path = directory.path().join("todos.sqlite");
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&database_path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options.clone())
            .await
            .expect("file-backed SQLite should connect");
        initialize_database(&pool)
            .await
            .expect("schema should initialize");
        sqlx::query("INSERT INTO todos (title) VALUES (?)")
            .bind("Persist me")
            .execute(&pool)
            .await
            .expect("todo should be inserted");
        pool.close().await;

        let reopened = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("database should reopen");
        let title: String = sqlx::query_scalar("SELECT title FROM todos")
            .fetch_one(&reopened)
            .await
            .expect("stored todo should remain available");
        assert_eq!(title, "Persist me");
    }
}
